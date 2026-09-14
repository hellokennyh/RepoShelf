use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};
use tempfile::tempdir;

use reposhelf_lib::db::database::init_pool;
use reposhelf_lib::github::GitHubRepository;
use reposhelf_lib::library::favorite_service::{
    favorite_repository, get_favorite, update_favorite_metadata,
};
use reposhelf_lib::library::repository::{filter_supplemental_ai_tags, normalize_tag_for_dedup};
use reposhelf_lib::library::{FavoriteInput, UpdateFavoriteUserMetadataInput};

async fn setup_clean_test_db() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempdir().expect("Failed to create tempdir");
    let db_path = dir.path().join("test_dedup.db");
    let pool = init_pool(&db_path)
        .await
        .expect("Failed to initialize test DB pool");
    (dir, pool)
}

// =============================================================================
// 1. Data Migration 003 Acceptance Tests
// =============================================================================

#[tokio::test]
async fn test_migration_003_cleans_up_ai_github_duplicates_safely() {
    let dir = tempdir().expect("Failed to create tempdir");
    let db_path = dir.path().join("migration_test.db");

    let options = SqliteConnectOptions::new()
        .filename(&db_path)
        .create_if_missing(true)
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .connect_with(options)
        .await
        .expect("Failed to connect to test db");

    // Execute migration 001 and 002 manually
    let m001 = include_str!("../migrations/001_initial_schema.sql");
    sqlx::raw_sql(m001)
        .execute(&pool)
        .await
        .expect("Migration 001 failed");

    let m002 = include_str!("../migrations/002_rating_suggestions.sql");
    sqlx::raw_sql(m002)
        .execute(&pool)
        .await
        .expect("Migration 002 failed");

    // Insert a test favorite
    sqlx::query(
        r#"
        INSERT INTO favorites (
            id, github_repo_id, github_url, name, description, category,
            stars, license, pushed_at, rating, notes, favorited_at, metadata_refreshed_at
        ) VALUES (
            1, 1001, 'https://github.com/test/repo', 'repo', 'desc', 'AI & Agents',
            500, 'MIT', '2026-03-01T00:00:00Z', 4, 'my notes', '2026-03-01T00:00:00Z', '2026-03-01T00:00:00Z'
        )
        "#
    )
    .execute(&pool)
    .await
    .expect("Failed to insert favorite");

    // Insert fixtures per Section 29:
    // github: llm, ai-video-generation
    // ai: llm, ai-video-generation, creator-tools
    // user: llm
    sqlx::query("INSERT INTO favorite_tags (favorite_id, tag, source) VALUES (1, 'llm', 'github')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO favorite_tags (favorite_id, tag, source) VALUES (1, 'ai-video-generation', 'github')")
        .execute(&pool).await.unwrap();

    sqlx::query("INSERT INTO favorite_tags (favorite_id, tag, source) VALUES (1, 'llm', 'ai')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO favorite_tags (favorite_id, tag, source) VALUES (1, 'ai-video-generation', 'ai')")
        .execute(&pool).await.unwrap();
    sqlx::query(
        "INSERT INTO favorite_tags (favorite_id, tag, source) VALUES (1, 'creator-tools', 'ai')",
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query("INSERT INTO favorite_tags (favorite_id, tag, source) VALUES (1, 'llm', 'user')")
        .execute(&pool)
        .await
        .unwrap();

    // Verify duplicate count BEFORE migration
    let dup_count_before: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*)
        FROM favorite_tags a
        JOIN favorite_tags g ON g.favorite_id = a.favorite_id
        WHERE a.source = 'ai' AND g.source = 'github' AND trim(lower(a.tag)) = trim(lower(g.tag))
        "#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        dup_count_before, 2,
        "Expected 2 duplicate pairs before migration"
    );

    // Run migration 003
    let m003 = include_str!("../migrations/003_ai_tag_dedup_cleanup.sql");
    sqlx::raw_sql(m003)
        .execute(&pool)
        .await
        .expect("Migration 003 failed");

    // Verify duplicate count AFTER migration
    let dup_count_after: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*)
        FROM favorite_tags a
        JOIN favorite_tags g ON g.favorite_id = a.favorite_id
        WHERE a.source = 'ai' AND g.source = 'github' AND trim(lower(a.tag)) = trim(lower(g.tag))
        "#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        dup_count_after, 0,
        "Expected 0 duplicate pairs after migration"
    );

    // Verify remaining tags:
    let remaining_tags: Vec<(String, String)> = sqlx::query(
        "SELECT tag, source FROM favorite_tags WHERE favorite_id = 1 ORDER BY source, tag",
    )
    .fetch_all(&pool)
    .await
    .unwrap()
    .into_iter()
    .map(|r| (r.get::<String, _>("tag"), r.get::<String, _>("source")))
    .collect();

    // ai: ONLY creator-tools
    let ai_tags: Vec<String> = remaining_tags
        .iter()
        .filter(|(_, s)| s == "ai")
        .map(|(t, _)| t.clone())
        .collect();
    assert_eq!(ai_tags, vec!["creator-tools"]);

    // github: llm, ai-video-generation
    let gh_tags: Vec<String> = remaining_tags
        .iter()
        .filter(|(_, s)| s == "github")
        .map(|(t, _)| t.clone())
        .collect();
    assert_eq!(gh_tags, vec!["ai-video-generation", "llm"]);

    // user: llm (user tag must NOT be deleted!)
    let user_tags: Vec<String> = remaining_tags
        .iter()
        .filter(|(_, s)| s == "user")
        .map(|(t, _)| t.clone())
        .collect();
    assert_eq!(user_tags, vec!["llm"]);

    // Verify favorite record was preserved
    let fav_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM favorites")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(fav_count, 1);
}

// =============================================================================
// 2. Pure Tag Normalization & Deduplication Logic Tests (Section 27)
// =============================================================================

#[test]
fn test_tag_dedup_case_a_exact_match() {
    let gh = vec!["ai-video-generation".to_string()];
    let ai = vec![
        "ai-video-generation".to_string(),
        "creator-tools".to_string(),
    ];
    let filtered = filter_supplemental_ai_tags(&ai, &gh);
    assert_eq!(filtered, vec!["creator-tools"]);
}

#[test]
fn test_tag_dedup_case_b_case_insensitive() {
    let gh = vec!["LLM".to_string()];
    let ai = vec!["llm".to_string()];
    let filtered = filter_supplemental_ai_tags(&ai, &gh);
    assert!(
        filtered.is_empty(),
        "Case-insensitive duplicate must be removed"
    );
}

#[test]
fn test_tag_dedup_case_c_whitespace_normalization() {
    let gh = vec!["automation".to_string()];
    let ai = vec![" automation ".to_string()];
    let filtered = filter_supplemental_ai_tags(&ai, &gh);
    assert!(filtered.is_empty(), "Whitespace duplicate must be removed");
}

#[test]
fn test_tag_dedup_case_d_similar_not_identical_both_retained() {
    let gh = vec!["video-generation".to_string()];
    let ai = vec!["ai-video-generation".to_string()];
    let filtered = filter_supplemental_ai_tags(&ai, &gh);
    assert_eq!(
        filtered,
        vec!["ai-video-generation"],
        "Similar but distinct tags must both be retained"
    );
}

#[test]
fn test_tag_normalization_contract() {
    assert_eq!(normalize_tag_for_dedup("  LLM  "), "llm");
    assert_eq!(normalize_tag_for_dedup("Rust-Lang"), "rust-lang");
}

// =============================================================================
// 3. Backend Enforcement & Frontend Bypass Protection Tests (Section 8 & 27)
// =============================================================================

#[tokio::test]
async fn test_backend_silently_filters_ai_github_duplicates_on_save() {
    let (_db_dir, pool) = setup_clean_test_db().await;

    let fav = favorite_repository(
        &pool,
        FavoriteInput {
            github_repo_id: Some(2001),
            github_url: "https://github.com/test/dedup-save".to_string(),
            name: "dedup-save".to_string(),
            description: Some("test".to_string()),
            category: "AI & Agents".to_string(),
            stars: 100,
            license: Some("MIT".to_string()),
            pushed_at: Some("2026-03-01T00:00:00Z".to_string()),
            releases: None,
            languages: None,
            topics: vec!["llm".to_string(), "ai-video-generation".to_string()],
        },
    )
    .await
    .unwrap();

    // Attempt to save AI tags that duplicate GitHub topics
    let updated = update_favorite_metadata(
        &pool,
        UpdateFavoriteUserMetadataInput {
            favorite_id: fav.id,
            category: "AI & Agents".to_string(),
            rating: Some(4),
            rating_reason: Some("good".to_string()),
            rating_confidence: Some("high".to_string()),
            notes: "notes".to_string(),
            user_tags: vec!["llm".to_string()], // user tag same as github tag is allowed!
            ai_tags: Some(vec![
                "LLM".to_string(),                 // duplicate of github 'llm' (case-insensitive)
                "ai-video-generation".to_string(), // duplicate of github
                "creator-tools".to_string(),       // valid supplemental
            ]),
        },
    )
    .await
    .unwrap();

    let ai_tags: Vec<String> = updated
        .tags
        .iter()
        .filter(|t| t.source == "ai")
        .map(|t| t.tag.clone())
        .collect();
    assert_eq!(
        ai_tags,
        vec!["creator-tools"],
        "Backend must have filtered out LLM and ai-video-generation"
    );

    let user_tags: Vec<String> = updated
        .tags
        .iter()
        .filter(|t| t.source == "user")
        .map(|t| t.tag.clone())
        .collect();
    assert_eq!(
        user_tags,
        vec!["llm"],
        "User tag must NOT be affected by AI tag deduplication"
    );
}

// =============================================================================
// 4. GitHub Refresh Invariant Tests (Section 9 & 28)
// =============================================================================

#[tokio::test]
async fn test_github_refresh_invariant_github_tag_wins_over_ai_tag() {
    let (_db_dir, pool) = setup_clean_test_db().await;

    let fav = favorite_repository(
        &pool,
        FavoriteInput {
            github_repo_id: Some(3001),
            github_url: "https://github.com/test/refresh-dedup".to_string(),
            name: "refresh-dedup".to_string(),
            description: Some("test".to_string()),
            category: "Automation".to_string(),
            stars: 50,
            license: Some("Apache-2.0".to_string()),
            pushed_at: Some("2026-03-01T00:00:00Z".to_string()),
            releases: None,
            languages: None,
            topics: vec!["automation".to_string()],
        },
    )
    .await
    .unwrap();

    // Add initial AI tags: video-generation and creator-tools
    // and User tag: automation
    let with_ai = update_favorite_metadata(
        &pool,
        UpdateFavoriteUserMetadataInput {
            favorite_id: fav.id,
            category: "Automation".to_string(),
            rating: Some(3),
            rating_reason: None,
            rating_confidence: None,
            notes: "".to_string(),
            user_tags: vec!["automation".to_string()],
            ai_tags: Some(vec![
                "video-generation".to_string(),
                "creator-tools".to_string(),
            ]),
        },
    )
    .await
    .unwrap();

    assert_eq!(with_ai.tags.iter().filter(|t| t.source == "ai").count(), 2);

    // Simulate GitHub refresh with new topics: ["automation", "video-generation"]
    let refreshed_repo = GitHubRepository {
        github_repo_id: 3001,
        github_url: "https://github.com/test/refresh-dedup".to_string(),
        name: "refresh-dedup".to_string(),
        description: Some("test".to_string()),
        stars: 55,
        license: Some("Apache-2.0".to_string()),
        pushed_at: Some("2026-03-02T00:00:00Z".to_string()),
        releases: None,
        languages: None,
        topics: vec!["automation".to_string(), "video-generation".to_string()],
        category: "Automation".to_string(),
    };

    let refreshed = reposhelf_lib::library::repository::update_github_owned_metadata_tx(
        &pool,
        fav.id,
        &refreshed_repo,
        "2026-03-02T12:00:00Z",
        reposhelf_lib::library::models::MetadataPatch::Keep,
        reposhelf_lib::library::models::MetadataPatch::Keep,
    )
    .await
    .unwrap();

    let gh_tags: Vec<String> = refreshed
        .tags
        .iter()
        .filter(|t| t.source == "github")
        .map(|t| t.tag.clone())
        .collect();
    assert_eq!(gh_tags, vec!["automation", "video-generation"]);

    let ai_tags: Vec<String> = refreshed
        .tags
        .iter()
        .filter(|t| t.source == "ai")
        .map(|t| t.tag.clone())
        .collect();
    // GitHub tag 'video-generation' won; conflicting AI tag was removed!
    assert_eq!(ai_tags, vec!["creator-tools"]);

    // User tags preserved
    let user_tags: Vec<String> = refreshed
        .tags
        .iter()
        .filter(|t| t.source == "user")
        .map(|t| t.tag.clone())
        .collect();
    assert_eq!(user_tags, vec!["automation"]);
}

// =============================================================================
// 5. Atomic Save & Exit Transaction Tests (Section 20 & 21)
// =============================================================================

#[tokio::test]
async fn test_atomic_save_updates_all_fields_or_none() {
    let (_db_dir, pool) = setup_clean_test_db().await;

    let fav = favorite_repository(
        &pool,
        FavoriteInput {
            github_repo_id: Some(4001),
            github_url: "https://github.com/test/atomic-save".to_string(),
            name: "atomic-save".to_string(),
            description: Some("desc".to_string()),
            category: "Other".to_string(),
            stars: 200,
            license: Some("MIT".to_string()),
            pushed_at: Some("2026-03-01T00:00:00Z".to_string()),
            releases: None,
            languages: None,
            topics: vec!["rust".to_string()],
        },
    )
    .await
    .unwrap();

    // Successful atomic save
    let updated = update_favorite_metadata(
        &pool,
        UpdateFavoriteUserMetadataInput {
            favorite_id: fav.id,
            category: "Developer Tools".to_string(),
            rating: Some(5),
            rating_reason: Some("Excellent dev tool".to_string()),
            rating_confidence: Some("high".to_string()),
            notes: "Deep notes".to_string(),
            user_tags: vec!["cli".to_string()],
            ai_tags: Some(vec!["productivity".to_string()]),
        },
    )
    .await
    .unwrap();

    assert_eq!(updated.category, "Developer Tools");
    assert_eq!(updated.rating, Some(5));
    assert_eq!(updated.rating_reason.as_deref(), Some("Excellent dev tool"));
    assert_eq!(updated.rating_confidence.as_deref(), Some("high"));
    assert_eq!(updated.notes, "Deep notes");
    assert_eq!(
        updated
            .tags
            .iter()
            .filter(|t| t.source == "user")
            .map(|t| t.tag.as_str())
            .collect::<Vec<_>>(),
        vec!["cli"]
    );
    assert_eq!(
        updated
            .tags
            .iter()
            .filter(|t| t.source == "ai")
            .map(|t| t.tag.as_str())
            .collect::<Vec<_>>(),
        vec!["productivity"]
    );

    // Failure case: invalid category causes entire transaction to roll back
    let err = update_favorite_metadata(
        &pool,
        UpdateFavoriteUserMetadataInput {
            favorite_id: fav.id,
            category: "InvalidNonExistentCategory".to_string(),
            rating: Some(1),
            rating_reason: Some("should not be saved".to_string()),
            rating_confidence: Some("low".to_string()),
            notes: "failed notes".to_string(),
            user_tags: vec!["fail-tag".to_string()],
            ai_tags: Some(vec!["fail-ai".to_string()]),
        },
    )
    .await;

    assert!(err.is_err());

    // Verify DB was NOT partially modified
    let reloaded = get_favorite(&pool, fav.id).await.unwrap();
    assert_eq!(reloaded.category, "Developer Tools");
    assert_eq!(reloaded.rating, Some(5));
    assert_eq!(reloaded.notes, "Deep notes");
    assert_eq!(
        reloaded
            .tags
            .iter()
            .filter(|t| t.source == "user")
            .map(|t| t.tag.as_str())
            .collect::<Vec<_>>(),
        vec!["cli"]
    );
    assert_eq!(
        reloaded
            .tags
            .iter()
            .filter(|t| t.source == "ai")
            .map(|t| t.tag.as_str())
            .collect::<Vec<_>>(),
        vec!["productivity"]
    );
}
