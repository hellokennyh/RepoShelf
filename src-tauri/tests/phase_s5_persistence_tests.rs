use tempfile::tempdir;

use reposhelf_lib::db::database::init_pool;
use reposhelf_lib::db::translation::{
    compute_source_hash, parse_db_translation, DescriptionTranslation, DescriptionTranslationInput,
    TranslationPatchInput, TranslationStatus,
};
use reposhelf_lib::github::models::GitHubRepository;
use reposhelf_lib::library::favorite_repository;
use reposhelf_lib::library::models::{
    FavoriteInput, MetadataPatch, UpdateFavoriteUserMetadataInput,
};
use reposhelf_lib::library::repository::{
    find_favorite_by_id, insert_favorite_tx, update_github_owned_metadata_tx,
    update_user_metadata_tx,
};

async fn setup_test_db() -> (tempfile::TempDir, sqlx::SqlitePool) {
    let dir = tempdir().expect("Failed to create tempdir");
    let db_path = dir.path().join("test_phase_s5.db");
    let pool = init_pool(&db_path)
        .await
        .expect("Failed to initialize test pool");
    (dir, pool)
}

// -----------------------------------------------------------------------------
// 1. Migration 005 Schema & Fresh / Fixture Upgrade
// -----------------------------------------------------------------------------

#[tokio::test]
async fn test_migration_005_applies_to_fresh_test_db() {
    let (_dir, pool) = setup_test_db().await;

    // Verify migration 005 is applied
    let migrations: Vec<(i64,)> =
        sqlx::query_as("SELECT version FROM _sqlx_migrations ORDER BY version ASC")
            .fetch_all(&pool)
            .await
            .unwrap();

    assert!(migrations.len() >= 5);
    assert_eq!(migrations[4].0, 5);

    // Verify columns exist on favorites table
    let columns: Vec<(i64, String, String, i64, Option<String>, i64)> =
        sqlx::query_as("PRAGMA table_info(favorites)")
            .fetch_all(&pool)
            .await
            .unwrap();

    let col_names: Vec<String> = columns.into_iter().map(|c| c.1).collect();
    assert!(col_names.contains(&"description_translations".to_string()));
    assert!(col_names.contains(&"description_translation_source_hash".to_string()));
}

#[tokio::test]
async fn test_migration_005_applies_after_001_to_004_fixture_and_preserves_old_rows() {
    let dir = tempdir().expect("Failed to create tempdir");
    let db_path = dir.path().join("test_migration_004_to_005.db");

    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(&db_path)
        .create_if_missing(true)
        .foreign_keys(true);
    let pool = sqlx::SqlitePool::connect_with(options).await.unwrap();

    // 1. Apply migrations 001 - 004 manually
    let m1 = include_str!("../migrations/001_initial_schema.sql");
    let m2 = include_str!("../migrations/002_rating_suggestions.sql");
    let m3 = include_str!("../migrations/003_ai_tag_dedup_cleanup.sql");
    let m4 = include_str!("../migrations/004_repository_metadata.sql");

    sqlx::raw_sql(m1).execute(&pool).await.unwrap();
    sqlx::raw_sql(m2).execute(&pool).await.unwrap();
    sqlx::raw_sql(m3).execute(&pool).await.unwrap();
    sqlx::raw_sql(m4).execute(&pool).await.unwrap();

    // 2. Insert an old favorite into 004 schema
    sqlx::query(
        r#"
        INSERT INTO favorites (
            github_repo_id, github_url, name, description, category, stars, license,
            pushed_at, rating, notes, favorited_at, metadata_refreshed_at, releases, languages
        ) VALUES (
            101, 'https://github.com/owner/old-repo', 'old-repo', 'An old description',
            'Developer Tools', 500, 'MIT', '2026-09-01T00:00:00Z', 4, 'My note',
            '2026-09-01T00:00:00Z', '2026-09-01T00:00:00Z', 5, '{"Rust":100.0}'
        )
        "#,
    )
    .execute(&pool)
    .await
    .unwrap();

    // 3. Apply migration 005 and 006
    let m5 = include_str!("../migrations/005_description_translation.sql");
    let m6 = include_str!("../migrations/006_repo_created_at_and_ai_analysis.sql");
    sqlx::raw_sql(m5).execute(&pool).await.unwrap();
    sqlx::raw_sql(m6).execute(&pool).await.unwrap();

    // 4. Verify old favorite preserved and new columns are NULL
    let row = sqlx::query(
        "SELECT id, name, description, releases, languages, description_translations, description_translation_source_hash FROM favorites WHERE github_repo_id = 101"
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    use sqlx::Row;
    let name: String = row.get("name");
    let desc: Option<String> = row.get("description");
    let releases: Option<i64> = row.get("releases");
    let languages: Option<String> = row.get("languages");
    let trans: Option<String> = row.get("description_translations");
    let hash: Option<String> = row.get("description_translation_source_hash");

    assert_eq!(name, "old-repo");
    assert_eq!(desc.as_deref(), Some("An old description"));
    assert_eq!(releases, Some(5));
    assert_eq!(languages.as_deref(), Some("{\"Rust\":100.0}"));
    assert!(
        trans.is_none(),
        "New translation column must be NULL for existing row"
    );
    assert!(
        hash.is_none(),
        "New source hash column must be NULL for existing row"
    );

    // 5. Querying through find_favorite_by_id produces FavoriteItem with None translation
    let fav_item = find_favorite_by_id(&pool, row.get("id"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(fav_item.description_translation, None);
    assert_eq!(fav_item.description_translation_source_hash, None);
    assert_eq!(fav_item.translation_status, None);
    assert_eq!(fav_item.translation_error, None);
}

// -----------------------------------------------------------------------------
// 2. Hash & UTF-8 Behavior
// -----------------------------------------------------------------------------

#[test]
fn test_source_hash_exact_utf8_behavior() {
    let d = "Clean description";
    let h = compute_source_hash(d);
    assert_eq!(h.len(), 64);
    assert_eq!(h, compute_source_hash(d));

    // Whitespace changes hash
    assert_ne!(h, compute_source_hash(" Clean description"));
    assert_ne!(h, compute_source_hash("Clean description "));
    assert_ne!(h, compute_source_hash("Clean  description"));

    // Newline changes hash
    assert_ne!(
        compute_source_hash("line1\nline2"),
        compute_source_hash("line1\r\nline2")
    );

    // Unicode byte differences change hash
    let s1 = "Unicode 测试";
    let s2 = "Unicode 測試";
    assert_ne!(compute_source_hash(s1), compute_source_hash(s2));
}

// -----------------------------------------------------------------------------
// 3. Translation Validation & Locales
// -----------------------------------------------------------------------------

#[test]
fn test_translation_locales_and_validation() {
    // en, zh-Hans, zh-Hant accepted
    assert!(DescriptionTranslation::new("Hello".to_string(), "en".to_string()).is_ok());
    assert!(DescriptionTranslation::new("你好".to_string(), "zh-Hans".to_string()).is_ok());
    assert!(DescriptionTranslation::new("你好".to_string(), "zh-Hant".to_string()).is_ok());

    // invalid locale rejected
    assert!(DescriptionTranslation::new("Bonjour".to_string(), "fr".to_string()).is_err());
    assert!(DescriptionTranslation::new("Hello".to_string(), "zh".to_string()).is_err());
    assert!(DescriptionTranslation::new("Hello".to_string(), "en-US".to_string()).is_err());

    // empty text rejected
    assert!(DescriptionTranslation::new("".to_string(), "en".to_string()).is_err());
    assert!(DescriptionTranslation::new("   ".to_string(), "en".to_string()).is_err());
}

#[test]
fn test_parse_db_translation_unpaired_and_malformed() {
    // One-sided translation without hash -> Corrupt
    let (_, _, status1, err1) = parse_db_translation(
        Some(r#"{"text":"foo","target_locale":"en"}"#),
        None,
        Some("source"),
    );
    assert_eq!(status1, Some(TranslationStatus::Corrupt));
    assert!(err1.is_some());

    // One-sided hash without translation -> Corrupt
    let (_, _, status2, err2) =
        parse_db_translation(None, Some("a".repeat(64).as_str()), Some("source"));
    assert_eq!(status2, Some(TranslationStatus::Corrupt));
    assert!(err2.is_some());

    // Malformed JSON string -> Corrupt
    let (_, _, status3, err3) = parse_db_translation(
        Some("{broken json"),
        Some("a".repeat(64).as_str()),
        Some("source"),
    );
    assert_eq!(status3, Some(TranslationStatus::Corrupt));
    assert!(err3.is_some());

    // Empty string DB value is not treated as NULL
    let (_, _, status4, err4) =
        parse_db_translation(Some(""), Some("a".repeat(64).as_str()), Some("source"));
    assert_eq!(status4, Some(TranslationStatus::Corrupt));
    assert!(err4.is_some());

    // Literal string "null" DB value is not treated as NULL
    let (_, _, status5, err5) =
        parse_db_translation(Some("null"), Some("a".repeat(64).as_str()), Some("source"));
    assert_eq!(status5, Some(TranslationStatus::Corrupt));
    assert!(err5.is_some());
}

// -----------------------------------------------------------------------------
// 4. Staleness & Refresh Interaction
// -----------------------------------------------------------------------------

#[tokio::test]
async fn test_staleness_on_description_change_and_null() {
    let desc_orig = "Initial description";
    let hash_orig = compute_source_hash(desc_orig);
    let json = r#"{"text":"初始描述","target_locale":"zh-Hans"}"#;

    // Normal valid
    let (t, h, status, _) = parse_db_translation(Some(json), Some(&hash_orig), Some(desc_orig));
    assert_eq!(status, Some(TranslationStatus::Valid));
    assert_eq!(h.as_deref(), Some(hash_orig.as_str()));
    assert!(t.is_some());

    // Description changed -> Stale
    let (t_stale, _, status_stale, _) =
        parse_db_translation(Some(json), Some(&hash_orig), Some("Changed description"));
    assert_eq!(status_stale, Some(TranslationStatus::Stale));
    assert!(t_stale.is_some());

    // Description became NULL -> Stale
    let (t_null, _, status_null, _) = parse_db_translation(Some(json), Some(&hash_orig), None);
    assert_eq!(status_null, Some(TranslationStatus::Stale));
    assert!(t_null.is_some());
}

#[tokio::test]
async fn test_metadata_refresh_preserves_old_translation_and_source_hash() {
    let (_dir, pool) = setup_test_db().await;

    let desc1 = "First description";
    let fav = insert_favorite_tx(
        &pool,
        &FavoriteInput {
            github_repo_id: Some(201),
            github_url: "https://github.com/owner/refresh-repo".to_string(),
            name: "refresh-repo".to_string(),
            description: Some(desc1.to_string()),
            category: "Developer Tools".to_string(),
            stars: 10,
            license: Some("MIT".to_string()),
            topics: vec![],
            pushed_at: None,
            releases: Some(1),
            languages: None,
            translation: Some(DescriptionTranslationInput {
                text: "第一版描述翻译".to_string(),
                target_locale: "zh-Hans".to_string(),
            }),
                    repo_created_at: None,
            ai_analysis: None,
},
    )
    .await
    .unwrap();

    assert_eq!(fav.translation_status, Some(TranslationStatus::Valid));
    let saved_hash = fav.description_translation_source_hash.clone().unwrap();

    // 1. Refresh with byte-identical description -> keeps Valid
    let repo_same = GitHubRepository {
        github_repo_id: 201,
        github_url: "https://github.com/owner/refresh-repo".to_string(),
        name: "refresh-repo".to_string(),
        description: Some(desc1.to_string()),
        stars: 20,
        license: Some("MIT".to_string()),
        pushed_at: None,
        category: "Developer Tools".to_string(),
        topics: vec![],
        releases: Some(1),
        languages: None,
            created_at: None,
};

    let refreshed_same = update_github_owned_metadata_tx(
        &pool,
        fav.id,
        &repo_same,
        "2026-09-15T12:00:00Z",
        MetadataPatch::Keep,
        MetadataPatch::Keep,
    )
    .await
    .unwrap();

    assert_eq!(
        refreshed_same.translation_status,
        Some(TranslationStatus::Valid)
    );
    assert_eq!(
        refreshed_same
            .description_translation_source_hash
            .as_deref(),
        Some(saved_hash.as_str())
    );

    // 2. Refresh with modified description -> turns Stale, KEEPS old translation and old hash
    let desc2 = "Second description";
    let repo_changed = GitHubRepository {
        github_repo_id: 201,
        github_url: "https://github.com/owner/refresh-repo".to_string(),
        name: "refresh-repo".to_string(),
        description: Some(desc2.to_string()),
        stars: 25,
        license: Some("MIT".to_string()),
        pushed_at: None,
        category: "Developer Tools".to_string(),
        topics: vec![],
        releases: Some(1),
        languages: None,
            created_at: None,
};

    let refreshed_diff = update_github_owned_metadata_tx(
        &pool,
        fav.id,
        &repo_changed,
        "2026-09-15T12:30:00Z",
        MetadataPatch::Keep,
        MetadataPatch::Keep,
    )
    .await
    .unwrap();

    assert_eq!(
        refreshed_diff.translation_status,
        Some(TranslationStatus::Stale)
    );
    assert_eq!(
        refreshed_diff
            .description_translation_source_hash
            .as_deref(),
        Some(saved_hash.as_str())
    );
    assert_eq!(
        refreshed_diff
            .description_translation
            .as_ref()
            .unwrap()
            .text,
        "第一版描述翻译"
    );
}

// -----------------------------------------------------------------------------
// 5. Patch Semantics (Keep, Replace, Clear) & Unrelated Saves
// -----------------------------------------------------------------------------

#[tokio::test]
async fn test_unrelated_saves_keep_translation_pair_even_when_stale() {
    let (_dir, pool) = setup_test_db().await;

    let desc = "Stale test description";
    let fav = insert_favorite_tx(
        &pool,
        &FavoriteInput {
            github_repo_id: Some(301),
            github_url: "https://github.com/owner/stale-unrelated".to_string(),
            name: "stale-unrelated".to_string(),
            description: Some(desc.to_string()),
            category: "Developer Tools".to_string(),
            stars: 10,
            license: Some("MIT".to_string()),
            topics: vec![],
            pushed_at: None,
            releases: None,
            languages: None,
            translation: Some(DescriptionTranslationInput {
                text: "翻译文本".to_string(),
                target_locale: "zh-Hans".to_string(),
            }),
                    repo_created_at: None,
            ai_analysis: None,
},
    )
    .await
    .unwrap();

    // Invalidate hash by updating description directly in DB
    sqlx::query("UPDATE favorites SET description = 'Updated description' WHERE id = ?")
        .bind(fav.id)
        .execute(&pool)
        .await
        .unwrap();

    let stale_fav = find_favorite_by_id(&pool, fav.id).await.unwrap().unwrap();
    assert_eq!(stale_fav.translation_status, Some(TranslationStatus::Stale));

    // Now save unrelated metadata (notes, tags, rating) without translation patch
    let update_input = UpdateFavoriteUserMetadataInput {
        favorite_id: fav.id,
        category: "Web & SaaS".to_string(),
        rating: Some(5),
        rating_reason: Some("Great project".to_string()),
        rating_confidence: Some("high".to_string()),
        notes: "My updated note".to_string(),
        user_tags: vec!["tool".to_string()],
        ai_tags: None,
        translation_patch: None, // Defaults to Keep
            analysis_patch: None,
};

    let updated = update_user_metadata_tx(&pool, &update_input).await.unwrap();
    assert_eq!(updated.category, "Web & SaaS");
    assert_eq!(updated.rating, Some(5));
    assert_eq!(updated.notes, "My updated note");
    // Translation remains stale and untouched!
    assert_eq!(updated.translation_status, Some(TranslationStatus::Stale));
    assert_eq!(
        updated.description_translation.as_ref().unwrap().text,
        "翻译文本"
    );
}

#[tokio::test]
async fn test_explicit_replace_and_clear_translation_patch() {
    let (_dir, pool) = setup_test_db().await;

    let desc = "Patch description";
    let fav = insert_favorite_tx(
        &pool,
        &FavoriteInput {
            github_repo_id: Some(401),
            github_url: "https://github.com/owner/patch-repo".to_string(),
            name: "patch-repo".to_string(),
            description: Some(desc.to_string()),
            category: "Developer Tools".to_string(),
            stars: 10,
            license: None,
            topics: vec![],
            pushed_at: None,
            releases: None,
            languages: None,
            translation: None,
                    repo_created_at: None,
            ai_analysis: None,
},
    )
    .await
    .unwrap();

    assert_eq!(fav.description_translation, None);

    // 1. Explicit REPLACE
    let replace_input = UpdateFavoriteUserMetadataInput {
        favorite_id: fav.id,
        category: fav.category.clone(),
        rating: None,
        rating_reason: None,
        rating_confidence: None,
        notes: String::new(),
        user_tags: vec![],
        ai_tags: None,
        translation_patch: Some(TranslationPatchInput::Replace {
            text: "补上的翻译".to_string(),
            target_locale: "zh-Hans".to_string(),
            source_text: desc.to_string(),
        }),
            analysis_patch: None,
};

    let replaced = update_user_metadata_tx(&pool, &replace_input)
        .await
        .unwrap();
    assert_eq!(replaced.translation_status, Some(TranslationStatus::Valid));
    assert_eq!(
        replaced.description_translation.as_ref().unwrap().text,
        "补上的翻译"
    );
    assert_eq!(
        replaced.description_translation_source_hash.as_deref(),
        Some(compute_source_hash(desc).as_str())
    );

    // 2. Explicit CLEAR
    let clear_input = UpdateFavoriteUserMetadataInput {
        favorite_id: fav.id,
        category: fav.category.clone(),
        rating: None,
        rating_reason: None,
        rating_confidence: None,
        notes: String::new(),
        user_tags: vec![],
        ai_tags: None,
        translation_patch: Some(TranslationPatchInput::Clear),
            analysis_patch: None,
};

    let cleared = update_user_metadata_tx(&pool, &clear_input).await.unwrap();
    assert_eq!(cleared.description_translation, None);
    assert_eq!(cleared.description_translation_source_hash, None);
    assert_eq!(cleared.translation_status, None);
}

#[tokio::test]
async fn test_source_conflict_at_save_rolls_back_entire_transaction() {
    let (_dir, pool) = setup_test_db().await;

    let desc = "Old source text";
    let fav = insert_favorite_tx(
        &pool,
        &FavoriteInput {
            github_repo_id: Some(501),
            github_url: "https://github.com/owner/conflict-repo".to_string(),
            name: "conflict-repo".to_string(),
            description: Some(desc.to_string()),
            category: "Developer Tools".to_string(),
            stars: 10,
            license: None,
            topics: vec![],
            pushed_at: None,
            releases: None,
            languages: None,
            translation: None,
                    repo_created_at: None,
            ai_analysis: None,
},
    )
    .await
    .unwrap();

    // Now user in frontend created a translation draft based on "Old source text",
    // but in DB description was changed to "New changed source text":
    sqlx::query("UPDATE favorites SET description = 'New changed source text' WHERE id = ?")
        .bind(fav.id)
        .execute(&pool)
        .await
        .unwrap();

    // User attempts to Save & Exit with draft tied to "Old source text"
    let conflict_input = UpdateFavoriteUserMetadataInput {
        favorite_id: fav.id,
        category: "Security & Privacy".to_string(),
        rating: Some(5),
        rating_reason: None,
        rating_confidence: None,
        notes: "Should not be saved due to conflict".to_string(),
        user_tags: vec!["security".to_string()],
        ai_tags: None,
        translation_patch: Some(TranslationPatchInput::Replace {
            text: "基于旧原文的翻译".to_string(),
            target_locale: "zh-Hans".to_string(),
            source_text: desc.to_string(), // Mismatches current DB description
        }),
            analysis_patch: None,
};

    let res = update_user_metadata_tx(&pool, &conflict_input).await;
    assert!(res.is_err());
    let err = res.unwrap_err();
    assert_eq!(err.code(), "TranslationSourceConflict");

    // Verify full rollback: category, rating, notes, and translation were NOT modified!
    let fav_after = find_favorite_by_id(&pool, fav.id).await.unwrap().unwrap();
    assert_eq!(fav_after.category, "Developer Tools");
    assert_eq!(fav_after.rating, None);
    assert_eq!(fav_after.notes, "");
    assert_eq!(fav_after.description_translation, None);
}

#[tokio::test]
async fn test_new_favorite_atomic_save_and_rollback() {
    let (_dir, pool) = setup_test_db().await;

    // Normal insertion with valid translation
    let valid_input = FavoriteInput {
        github_repo_id: Some(601),
        github_url: "https://github.com/owner/atomic-new".to_string(),
        name: "atomic-new".to_string(),
        description: Some("Description for atomic new".to_string()),
        category: "Developer Tools".to_string(),
        stars: 100,
        license: Some("MIT".to_string()),
        topics: vec!["rust".to_string()],
        pushed_at: None,
        releases: Some(2),
        languages: None,
        translation: Some(DescriptionTranslationInput {
            text: "原子性新收藏翻译".to_string(),
            target_locale: "zh-Hans".to_string(),
        }),
            repo_created_at: None,
        ai_analysis: None,
};

    let created = favorite_repository(&pool, valid_input).await.unwrap();
    assert_eq!(created.translation_status, Some(TranslationStatus::Valid));
    assert_eq!(
        created.description_translation.as_ref().unwrap().text,
        "原子性新收藏翻译"
    );
    assert_eq!(
        created.description_translation_source_hash.as_deref(),
        Some(compute_source_hash("Description for atomic new").as_str())
    );

    // Attempt insert with invalid locale -> fails before DB write
    let invalid_input = FavoriteInput {
        github_repo_id: Some(602),
        github_url: "https://github.com/owner/atomic-fail".to_string(),
        name: "atomic-fail".to_string(),
        description: Some("Some description".to_string()),
        category: "Developer Tools".to_string(),
        stars: 10,
        license: None,
        topics: vec![],
        pushed_at: None,
        releases: None,
        languages: None,
        translation: Some(DescriptionTranslationInput {
            text: "Bonjour".to_string(),
            target_locale: "fr".to_string(), // Invalid locale
        }),
            repo_created_at: None,
        ai_analysis: None,
};

    let fail_res = favorite_repository(&pool, invalid_input).await;
    assert!(fail_res.is_err());

    // Verify row was not created
    let check = find_favorite_by_id(&pool, 602).await.unwrap();
    assert!(check.is_none());
}
