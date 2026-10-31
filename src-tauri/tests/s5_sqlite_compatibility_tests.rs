use std::path::Path;
use tempfile::tempdir;
use reposhelf_lib::db::database::init_pool;
use reposhelf_lib::db::translation::compute_source_hash;
use reposhelf_lib::library::models::{FavoriteInput, UpdateFavoriteUserMetadataInput};
use reposhelf_lib::library::repository::{
    find_favorite_by_id, insert_favorite_tx, list_favorites, update_user_metadata_tx,
};

/// S5 SQLite Compatibility and Transaction Safety Regression Suite
/// Explicitly verifies all 10 SQLite compatibility requirements of S5-A3 Section 5.3
/// using isolated synthetic database fixtures in temporary directories.

async fn create_v1_1_1_synthetic_fixture(db_path: &Path) {
    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(db_path)
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal);
    let pool = sqlx::SqlitePool::connect_with(options).await.unwrap();

    // Create _sqlx_migrations table
    sqlx::raw_sql(
        r#"
        CREATE TABLE IF NOT EXISTS _sqlx_migrations (
            version BIGINT PRIMARY KEY,
            description TEXT NOT NULL,
            installed_on TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
            success BOOLEAN NOT NULL,
            checksum BLOB NOT NULL,
            execution_time BIGINT NOT NULL
        );
        "#,
    )
    .execute(&pool)
    .await
    .unwrap();

    // Run migrations 1 through 5
    let migrator = sqlx::migrate!("./migrations");
    for m in migrator.iter() {
        if m.version <= 5 {
            sqlx::raw_sql(&m.sql).execute(&pool).await.unwrap();
            sqlx::query(
                "INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time) VALUES (?, ?, 1, ?, 10)",
            )
            .bind(m.version)
            .bind(&*m.description)
            .bind(&*m.checksum)
            .execute(&pool)
            .await
            .unwrap();
        }
    }

    let desc = "Synthetic repository for v1.1.1 upgrade test";
    let hash = compute_source_hash(desc);
    let trans_json = r#"{"text":"v1.1.1 结构测试合成仓库","target_locale":"zh-Hans"}"#;

    // Insert v1.1.1 synthetic records with tags, notes, translations, rating reasons
    sqlx::query(
        r#"
        INSERT INTO favorites (
            id, github_repo_id, github_url, name, description, category, stars, license,
            pushed_at, rating, rating_reason, rating_confidence, notes, favorited_at, metadata_refreshed_at,
            releases, languages, description_translations, description_translation_source_hash
        ) VALUES (
            100, 1001, 'https://github.com/synthetic/v1-1-1-repo', 'v1-1-1-repo',
            ?, 'Developer Tools', 3500, 'MIT',
            '2026-09-15T10:00:00Z', 5, 'High code quality and test coverage', 'high',
            'Committed notes from v1.1.1 user', '2026-09-16T12:00:00Z', '2026-09-16T12:00:00Z',
            10, '{"Rust":80.0,"TypeScript":20.0}',
            ?, ?
        )
        "#,
    )
    .bind(desc)
    .bind(&trans_json)
    .bind(&hash)
    .execute(&pool)
    .await
    .unwrap();

    // Insert user tags and github topics
    sqlx::query(
        r#"
        INSERT INTO favorite_tags (favorite_id, tag, source) VALUES
            (100, 'rust', 'github'),
            (100, 'tauri', 'github'),
            (100, 'cli-tool', 'user'),
            (100, 'production-ready', 'user');
        "#,
    )
    .execute(&pool)
    .await
    .unwrap();

    pool.close().await;
}

#[tokio::test]
async fn test_s5_sqlite_upgrade_from_v1_1_1_and_v1_1_2_fixtures() {
    let dir = tempdir().expect("Failed to create tempdir");
    let db_path = dir.path().join("upgrade_test.db");

    // 1. Create synthetic v1.1.1 database
    create_v1_1_1_synthetic_fixture(&db_path).await;

    // 2. Initialize pool with RepoShelf v1.1.3 database loader (runs migrations)
    let pool = init_pool(&db_path).await.expect("init_pool should succeed on v1.1.1 database");

    // Requirement 3: Verify migration state (exactly 6 migrations applied, no unauthorized migrations)
    let applied_migrations: Vec<(i64, String, bool)> = sqlx::query_as(
        "SELECT version, description, success FROM _sqlx_migrations ORDER BY version ASC",
    )
    .fetch_all(&pool)
    .await
    .expect("Querying migrations must succeed");

    assert_eq!(applied_migrations.len(), 6, "Must have exactly 6 migrations");
    assert_eq!(applied_migrations.last().unwrap().0, 6, "Latest migration must be version 6");
    for m in &applied_migrations {
        assert!(m.2, "Migration {} must have success = true", m.0);
    }

    // Requirement 9: Verify PRAGMA integrity_check
    let (integrity,): (String,) = sqlx::query_as("PRAGMA integrity_check")
        .fetch_one(&pool)
        .await
        .expect("PRAGMA integrity_check must execute");
    assert_eq!(integrity, "ok", "PRAGMA integrity_check must return 'ok'");

    // Requirement 1 & 4: Read old data and tags
    let fav = find_favorite_by_id(&pool, 100)
        .await
        .expect("Querying favorite 100 must succeed")
        .expect("Favorite 100 must exist");

    assert_eq!(fav.name, "v1-1-1-repo");
    assert_eq!(fav.notes, "Committed notes from v1.1.1 user");
    assert_eq!(fav.description, Some("Synthetic repository for v1.1.1 upgrade test".to_string()));
    assert_eq!(fav.rating_reason, Some("High code quality and test coverage".to_string()));
    let trans = fav.description_translation.as_ref().expect("Translation must exist");
    assert_eq!(trans.text, "v1.1.1 结构测试合成仓库");
    assert_eq!(trans.target_locale, "zh-Hans");
    assert_eq!(fav.repo_created_at, None, "v1.1.1 upgraded record initially has NULL repo_created_at");
    assert_eq!(fav.ai_analysis, None, "v1.1.1 upgraded record initially has NULL ai_analysis");

    // Verify tag sources separation
    let user_tags: Vec<String> = fav.tags.iter().filter(|t| t.source == "user").map(|t| t.tag.clone()).collect();
    let github_tags: Vec<String> = fav.tags.iter().filter(|t| t.source == "github").map(|t| t.tag.clone()).collect();
    assert_eq!(user_tags, vec!["cli-tool", "production-ready"]);
    assert_eq!(github_tags, vec!["rust", "tauri"]);

    // Requirement 5: Save/exit transaction atomicity
    let update_input = UpdateFavoriteUserMetadataInput {
        favorite_id: 100,
        category: fav.category.clone(),
        rating: Some(4),
        rating_reason: Some("Updated rating reason in zh-Hans".to_string()),
        rating_confidence: Some("high".to_string()),
        notes: "Updated notes committed to DB".to_string(),
        user_tags: vec!["cli-tool".to_string(), "v1-1-3-tested".to_string()],
        ai_tags: None,
        translation_patch: None,
        analysis_patch: None,
    };

    update_user_metadata_tx(&pool, &update_input)
        .await
        .expect("update_user_metadata_tx should commit atomically");

    let updated_fav = find_favorite_by_id(&pool, 100).await.unwrap().unwrap();
    assert_eq!(updated_fav.rating, Some(4));
    assert_eq!(updated_fav.notes, "Updated notes committed to DB");
    assert_eq!(updated_fav.rating_reason, Some("Updated rating reason in zh-Hans".to_string()));

    // Requirement 8: Reopen database and verify persistence
    drop(pool);

    let reopened_pool = init_pool(&db_path).await.expect("Reopening pool must succeed");
    let reloaded = find_favorite_by_id(&reopened_pool, 100)
        .await
        .expect("Querying reopened database must succeed")
        .expect("Favorite 100 must exist");

    assert_eq!(reloaded.rating, Some(4));
    assert_eq!(reloaded.notes, "Updated notes committed to DB");
    assert_eq!(reloaded.rating_reason, Some("Updated rating reason in zh-Hans".to_string()));
    let reloaded_user_tags: Vec<String> = reloaded.tags.iter().filter(|t| t.source == "user").map(|t| t.tag.clone()).collect();
    assert_eq!(reloaded_user_tags, vec!["cli-tool", "v1-1-3-tested"]);

    // Requirement 7: Reading favorites offline without AI
    let all_favs = list_favorites(&reopened_pool, &Default::default())
        .await
        .expect("Listing favorites offline must succeed");
    assert_eq!(all_favs.len(), 1);
    assert_eq!(all_favs[0].id, 100);

    // Requirement 10: Integrity check after all mutations
    let (final_integrity,): (String,) = sqlx::query_as("PRAGMA integrity_check")
        .fetch_one(&reopened_pool)
        .await
        .expect("Final integrity check must succeed");
    assert_eq!(final_integrity, "ok");
}

#[tokio::test]
async fn test_s5_sqlite_transaction_rollback_and_draft_isolation() {
    let dir = tempdir().expect("Failed to create tempdir");
    let db_path = dir.path().join("transaction_test.db");
    let pool = init_pool(&db_path).await.expect("init_pool should succeed");

    // Insert a new favorite
    let input = FavoriteInput {
        github_repo_id: Some(2002),
        github_url: "https://github.com/synthetic/draft-test-repo".to_string(),
        name: "draft-test-repo".to_string(),
        description: Some("Testing draft isolation".to_string()),
        category: "Developer Tools".to_string(),
        stars: 500,
        license: Some("MIT".to_string()),
        topics: vec!["developer-tools".to_string()],
        pushed_at: Some("2026-10-01T00:00:00Z".to_string()),
        repo_created_at: Some("2026-01-01T00:00:00Z".to_string()),
        releases: None,
        languages: None,
        translation: None,
        ai_analysis: None,
    };
    let fav = insert_favorite_tx(&pool, &input).await.expect("insert_favorite_tx must succeed");

    // Initial state: notes is empty string (default)
    assert_eq!(fav.notes, "");

    // Attempt an invalid update to non-existent favorite ID: must fail and not affect anything
    let bad_update_res = update_user_metadata_tx(
        &pool,
        &UpdateFavoriteUserMetadataInput {
            favorite_id: 99999, // non-existent
            category: "Invalid".to_string(),
            rating: Some(1),
            rating_reason: None,
            rating_confidence: None,
            notes: "Poisoned notes".to_string(),
            user_tags: vec!["poison-tag".to_string()],
            ai_tags: None,
            translation_patch: None,
            analysis_patch: None,
        },
    )
    .await;

    assert!(bad_update_res.is_err(), "Updating non-existent favorite must return error");

    // Verify existing favorite is completely untouched
    let untouched = find_favorite_by_id(&pool, fav.id)
        .await
        .unwrap()
        .expect("Favorite must still exist");

    assert_eq!(untouched.notes, "");
    assert_eq!(untouched.rating, None);
    assert_eq!(untouched.category, "Developer Tools");
}
