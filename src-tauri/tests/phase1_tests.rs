use tempfile::tempdir;

use reposhelf_lib::db::database::{init_pool, run_migrations, verify_foreign_keys};
use reposhelf_lib::db::favorites::{
    delete_favorite_by_id, find_favorite_by_id, find_favorite_by_repo_id, find_favorite_by_url,
    insert_favorite,
};
use reposhelf_lib::db::models::{
    NewFavoriteRecord, NewFavoriteTagRecord, NewSelectionHistoryRecord,
};
use reposhelf_lib::db::selection_history::{
    count_selection_history, has_selected_repo_id, has_selected_url, insert_selection,
};
use reposhelf_lib::db::tags::{delete_tag_by_id, insert_tag, list_tags_by_favorite_id};
use reposhelf_lib::error::AppError;
use reposhelf_lib::secrets::{InMemorySecretStore, OsSecretStore, SecretStore};
use reposhelf_lib::settings::{
    AiProvider, AiSettings, GitHubSettings, Settings, SettingsStore, TrendingPeriod,
};

// ---------------------------------------------------------------------------
// 1. Database Schema & Migration Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_database_migration_and_foreign_keys() {
    let dir = tempdir().expect("Failed to create tempdir");
    let db_path = dir.path().join("test_reposhelf.db");

    // Initialize pool which runs migrations and enables foreign keys
    let pool = init_pool(&db_path)
        .await
        .expect("Failed to initialize database pool");

    // Verify foreign keys are ON
    verify_foreign_keys(&pool)
        .await
        .expect("Foreign keys should be active");

    // Verify required tables exist
    let tables: Vec<(String,)> = sqlx::query_as(
        "SELECT name FROM sqlite_master WHERE type='table' AND name IN ('favorites', 'favorite_tags', 'selection_history') ORDER BY name",
    )
    .fetch_all(&pool)
    .await
    .expect("Failed to query tables");

    let table_names: Vec<String> = tables.into_iter().map(|(n,)| n).collect();
    assert_eq!(
        table_names,
        vec!["favorite_tags", "favorites", "selection_history"]
    );

    // Verify 7 required indexes exist
    let expected_indexes = vec![
        "idx_favorites_name",
        "idx_favorites_category",
        "idx_favorites_rating",
        "idx_favorites_stars",
        "idx_favorite_tags_tag",
        "idx_selection_repo_id",
        "idx_selection_url",
    ];

    for idx in expected_indexes {
        let count: (i64,) =
            sqlx::query_as("SELECT COUNT(1) FROM sqlite_master WHERE type='index' AND name = ?")
                .bind(idx)
                .fetch_one(&pool)
                .await
                .expect("Failed to check index existence");
        assert_eq!(count.0, 1, "Index {} must exist", idx);
    }

    // Verify repeated migration does not corrupt or fail
    run_migrations(&pool)
        .await
        .expect("Repeated migration run should succeed idempotently");
}

// ---------------------------------------------------------------------------
// 2. Favorites Persistence & Constraints Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_favorites_constraints() {
    let dir = tempdir().expect("Failed to create tempdir");
    let db_path = dir.path().join("test_favorites.db");
    let pool = init_pool(&db_path).await.expect("Failed to init db");

    // Valid insert
    let fav1 = NewFavoriteRecord {
        github_repo_id: Some(12345),
        github_url: "https://github.com/rust-lang/rust".to_string(),
        name: "rust".to_string(),
        description: Some(
            "Empowering everyone to build reliable and efficient software.".to_string(),
        ),
        category: Some("System Utilities".to_string()),
        stars: Some(95000),
        license: Some("MIT OR Apache-2.0".to_string()),
        pushed_at: Some("2026-09-04T00:00:00Z".to_string()),
        rating: Some(5),
        rating_reason: Some("High quality language".to_string()),
        rating_confidence: Some("high".to_string()),
        notes: Some("Core system language".to_string()),
        favorited_at: "2026-09-04T01:00:00Z".to_string(),
        metadata_refreshed_at: "2026-09-04T01:00:00Z".to_string(),
        releases: None,
        languages: None,
        description_translations: None,
        description_translation_source_hash: None,
        repo_created_at: None,
        ai_analysis: None,
    };

    let inserted = insert_favorite(&pool, &fav1)
        .await
        .expect("Valid favorite insert should succeed");
    assert_eq!(inserted.id, 1);
    assert_eq!(inserted.name, "rust");
    assert_eq!(inserted.rating, Some(5));
    assert_eq!(
        inserted.rating_reason.as_deref(),
        Some("High quality language")
    );
    assert_eq!(inserted.rating_confidence.as_deref(), Some("high"));

    // Lookup by id, repo_id, url
    let by_id = find_favorite_by_id(&pool, inserted.id)
        .await
        .expect("Lookup by id should succeed");
    assert!(by_id.is_some());
    let found = by_id.unwrap();
    assert_eq!(found.github_url, fav1.github_url);
    assert_eq!(
        found.rating_reason.as_deref(),
        Some("High quality language")
    );
    assert_eq!(found.rating_confidence.as_deref(), Some("high"));

    let by_repo_id = find_favorite_by_repo_id(&pool, 12345)
        .await
        .expect("Lookup by repo_id should succeed");
    assert!(by_repo_id.is_some());

    let by_url = find_favorite_by_url(&pool, "https://github.com/rust-lang/rust")
        .await
        .expect("Lookup by url should succeed");
    assert!(by_url.is_some());

    // Duplicate github_url constraint violation
    let fav_dup_url = NewFavoriteRecord {
        github_repo_id: Some(99999),
        github_url: "https://github.com/rust-lang/rust".to_string(),
        name: "rust-duplicate".to_string(),
        description: None,
        category: None,
        stars: None,
        license: None,
        pushed_at: None,
        rating: None,
        rating_reason: None,
        rating_confidence: None,
        notes: None,
        favorited_at: "2026-09-04T02:00:00Z".to_string(),
        metadata_refreshed_at: "2026-09-04T02:00:00Z".to_string(),
        releases: None,
        languages: None,
        description_translations: None,
        description_translation_source_hash: None,
        repo_created_at: None,
        ai_analysis: None,
    };
    let err = insert_favorite(&pool, &fav_dup_url).await;
    assert!(err.is_err(), "Duplicate github_url must fail");

    // Duplicate github_repo_id constraint violation
    let fav_dup_repo_id = NewFavoriteRecord {
        github_repo_id: Some(12345),
        github_url: "https://github.com/other-org/other-repo".to_string(),
        name: "other-repo".to_string(),
        description: None,
        category: None,
        stars: None,
        license: None,
        pushed_at: None,
        rating: None,
        rating_reason: None,
        rating_confidence: None,
        notes: None,
        favorited_at: "2026-09-04T02:00:00Z".to_string(),
        metadata_refreshed_at: "2026-09-04T02:00:00Z".to_string(),
        releases: None,
        languages: None,
        description_translations: None,
        description_translation_source_hash: None,
        repo_created_at: None,
        ai_analysis: None,
    };
    let err = insert_favorite(&pool, &fav_dup_repo_id).await;
    assert!(err.is_err(), "Duplicate github_repo_id must fail");

    // Nullable github_repo_id: Multiple entries with NULL repo_id are permitted
    let fav_null_id1 = NewFavoriteRecord {
        github_repo_id: None,
        github_url: "https://github.com/null-org/repo1".to_string(),
        name: "repo1".to_string(),
        description: None,
        category: None,
        stars: None,
        license: None,
        pushed_at: None,
        rating: None,
        rating_reason: None,
        rating_confidence: None,
        notes: None,
        favorited_at: "2026-09-04T03:00:00Z".to_string(),
        metadata_refreshed_at: "2026-09-04T03:00:00Z".to_string(),
        releases: None,
        languages: None,
        description_translations: None,
        description_translation_source_hash: None,
        repo_created_at: None,
        ai_analysis: None,
    };
    let fav_null_id2 = NewFavoriteRecord {
        github_repo_id: None,
        github_url: "https://github.com/null-org/repo2".to_string(),
        name: "repo2".to_string(),
        description: None,
        category: None,
        stars: None,
        license: None,
        pushed_at: None,
        rating: None,
        rating_reason: None,
        rating_confidence: None,
        notes: None,
        favorited_at: "2026-09-04T03:00:00Z".to_string(),
        metadata_refreshed_at: "2026-09-04T03:00:00Z".to_string(),
        releases: None,
        languages: None,
        description_translations: None,
        description_translation_source_hash: None,
        repo_created_at: None,
        ai_analysis: None,
    };
    assert!(insert_favorite(&pool, &fav_null_id1).await.is_ok());
    assert!(insert_favorite(&pool, &fav_null_id2).await.is_ok());

    // Rating check constraint: Valid ratings: NULL, 1..=5
    for valid_rating in [None, Some(1), Some(2), Some(3), Some(4), Some(5)] {
        let f = NewFavoriteRecord {
            github_repo_id: None,
            github_url: format!(
                "https://github.com/test-org/rating-{}",
                valid_rating.unwrap_or(0)
            ),
            name: "test".to_string(),
            description: None,
            category: None,
            stars: None,
            license: None,
            pushed_at: None,
            rating: valid_rating,
            rating_reason: None,
            rating_confidence: None,
            notes: None,
            favorited_at: "2026-09-04T04:00:00Z".to_string(),
            metadata_refreshed_at: "2026-09-04T04:00:00Z".to_string(),
            releases: None,
            languages: None,
            description_translations: None,
            description_translation_source_hash: None,
        repo_created_at: None,
        ai_analysis: None,
        };
        assert!(
            insert_favorite(&pool, &f).await.is_ok(),
            "Rating {:?} should be valid",
            valid_rating
        );
    }

    // Rating check constraint: Invalid ratings: 0, 6, -1
    for invalid_rating in [0, 6, -1] {
        let f = NewFavoriteRecord {
            github_repo_id: None,
            github_url: format!(
                "https://github.com/test-org/invalid-rating-{}",
                invalid_rating
            ),
            name: "test".to_string(),
            description: None,
            category: None,
            stars: None,
            license: None,
            pushed_at: None,
            rating: Some(invalid_rating),
            rating_reason: None,
            rating_confidence: None,
            notes: None,
            favorited_at: "2026-09-04T04:00:00Z".to_string(),
            metadata_refreshed_at: "2026-09-04T04:00:00Z".to_string(),
            releases: None,
            languages: None,
            description_translations: None,
            description_translation_source_hash: None,
        repo_created_at: None,
        ai_analysis: None,
        };
        assert!(
            insert_favorite(&pool, &f).await.is_err(),
            "Rating {} must be rejected by check constraint",
            invalid_rating
        );
    }

    // Rating confidence check constraint: Valid values: NULL, 'low', 'medium', 'high'
    for valid_conf in [None, Some("low"), Some("medium"), Some("high")] {
        let f = NewFavoriteRecord {
            github_repo_id: None,
            github_url: format!(
                "https://github.com/test-org/conf-{}",
                valid_conf.unwrap_or("none")
            ),
            name: "test".to_string(),
            description: None,
            category: None,
            stars: None,
            license: None,
            pushed_at: None,
            rating: Some(4),
            rating_reason: Some("Valid reason".to_string()),
            rating_confidence: valid_conf.map(|s| s.to_string()),
            notes: None,
            favorited_at: "2026-09-04T05:00:00Z".to_string(),
            metadata_refreshed_at: "2026-09-04T05:00:00Z".to_string(),
            releases: None,
            languages: None,
            description_translations: None,
            description_translation_source_hash: None,
        repo_created_at: None,
        ai_analysis: None,
        };
        assert!(
            insert_favorite(&pool, &f).await.is_ok(),
            "Rating confidence {:?} should be valid",
            valid_conf
        );
    }

    // Rating confidence check constraint: Invalid values
    for invalid_conf in ["invalid", "very_high", "LOW", "MEDIUM"] {
        let f = NewFavoriteRecord {
            github_repo_id: None,
            github_url: format!("https://github.com/test-org/invalid-conf-{}", invalid_conf),
            name: "test".to_string(),
            description: None,
            category: None,
            stars: None,
            license: None,
            pushed_at: None,
            rating: Some(4),
            rating_reason: Some("Reason".to_string()),
            rating_confidence: Some(invalid_conf.to_string()),
            notes: None,
            favorited_at: "2026-09-04T05:00:00Z".to_string(),
            metadata_refreshed_at: "2026-09-04T05:00:00Z".to_string(),
            releases: None,
            languages: None,
            description_translations: None,
            description_translation_source_hash: None,
        repo_created_at: None,
        ai_analysis: None,
        };
        assert!(
            insert_favorite(&pool, &f).await.is_err(),
            "Rating confidence '{}' must be rejected by check constraint",
            invalid_conf
        );
    }
}

// ---------------------------------------------------------------------------
// 3. Tags & Foreign Key Cascade Delete Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_tags_and_cascade_delete() {
    let dir = tempdir().expect("Failed to create tempdir");
    let db_path = dir.path().join("test_tags.db");
    let pool = init_pool(&db_path).await.expect("Failed to init db");

    let fav = insert_favorite(
        &pool,
        &NewFavoriteRecord {
            github_repo_id: Some(1001),
            github_url: "https://github.com/tauri-apps/tauri".to_string(),
            name: "tauri".to_string(),
            description: Some(
                "Build smaller, faster, and more secure desktop and mobile apps".to_string(),
            ),
            category: Some("Developer Tools".to_string()),
            stars: Some(85000),
            license: Some("Apache-2.0 OR MIT".to_string()),
            pushed_at: Some("2026-09-04T00:00:00Z".to_string()),
            rating: Some(5),
            rating_reason: None,
            rating_confidence: None,
            notes: Some("Primary GUI toolkit".to_string()),
            favorited_at: "2026-09-04T00:00:00Z".to_string(),
            metadata_refreshed_at: "2026-09-04T00:00:00Z".to_string(),
            releases: None,
            languages: None,
            description_translations: None,
            description_translation_source_hash: None,
        repo_created_at: None,
        ai_analysis: None,
        },
    )
    .await
    .expect("Failed to insert favorite");

    // Insert valid tags for allowed sources: 'github', 'ai', 'user'
    let t_github = insert_tag(
        &pool,
        &NewFavoriteTagRecord {
            favorite_id: fav.id,
            tag: "rust".to_string(),
            source: "github".to_string(),
        },
    )
    .await
    .expect("Source github should be valid");

    let _t_ai = insert_tag(
        &pool,
        &NewFavoriteTagRecord {
            favorite_id: fav.id,
            tag: "desktop-app".to_string(),
            source: "ai".to_string(),
        },
    )
    .await
    .expect("Source ai should be valid");

    let _t_user = insert_tag(
        &pool,
        &NewFavoriteTagRecord {
            favorite_id: fav.id,
            tag: "favorite-framework".to_string(),
            source: "user".to_string(),
        },
    )
    .await
    .expect("Source user should be valid");

    // Verify invalid sources are rejected by check constraint
    for invalid_source in ["system", "unknown", "admin", ""] {
        let err = insert_tag(
            &pool,
            &NewFavoriteTagRecord {
                favorite_id: fav.id,
                tag: "invalid-source-tag".to_string(),
                source: invalid_source.to_string(),
            },
        )
        .await;
        assert!(err.is_err(), "Source '{}' must be rejected", invalid_source);
    }

    // Verify compound uniqueness (favorite_id, tag, source)
    let dup_tag = insert_tag(
        &pool,
        &NewFavoriteTagRecord {
            favorite_id: fav.id,
            tag: "rust".to_string(),
            source: "github".to_string(),
        },
    )
    .await;
    assert!(
        dup_tag.is_err(),
        "Duplicate (favorite_id, tag, source) must be rejected"
    );

    // Verify NOCASE collation uniqueness (e.g. 'RUST' with same source and favorite_id)
    let dup_tag_nocase = insert_tag(
        &pool,
        &NewFavoriteTagRecord {
            favorite_id: fav.id,
            tag: "RUST".to_string(),
            source: "github".to_string(),
        },
    )
    .await;
    assert!(
        dup_tag_nocase.is_err(),
        "Duplicate tag under NOCASE must be rejected"
    );

    // Verify list tags
    let tags = list_tags_by_favorite_id(&pool, fav.id)
        .await
        .expect("Failed to list tags");
    assert_eq!(tags.len(), 3);

    // Delete single tag
    assert!(delete_tag_by_id(&pool, t_github.id).await.unwrap());
    let tags_after_one_delete = list_tags_by_favorite_id(&pool, fav.id).await.unwrap();
    assert_eq!(tags_after_one_delete.len(), 2);

    // HARD GATE: Verify CASCADE DELETE
    // Deleting the favorite must automatically cascade-delete all its remaining tags
    let deleted = delete_favorite_by_id(&pool, fav.id)
        .await
        .expect("Failed to delete favorite");
    assert!(deleted);

    let remaining_tags = list_tags_by_favorite_id(&pool, fav.id)
        .await
        .expect("Querying tags after parent delete should succeed");
    assert_eq!(
        remaining_tags.len(),
        0,
        "ON DELETE CASCADE must remove all child tags when favorite is deleted"
    );

    // Verify in database directly
    let direct_count: (i64,) =
        sqlx::query_as("SELECT COUNT(1) FROM favorite_tags WHERE favorite_id = ?")
            .bind(fav.id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(direct_count.0, 0, "No orphaned tags may exist");
}

// ---------------------------------------------------------------------------
// 4. Selection History Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_selection_history_persistence() {
    let dir = tempdir().expect("Failed to create tempdir");
    let db_path = dir.path().join("test_selection.db");
    let pool = init_pool(&db_path).await.expect("Failed to init db");

    let repo_id = 998877;
    let repo_url = "https://github.com/tokio-rs/tokio";

    assert!(!has_selected_url(&pool, repo_url).await.unwrap());
    assert!(!has_selected_repo_id(&pool, repo_id).await.unwrap());

    // Insert first selection event
    let sel1 = insert_selection(
        &pool,
        &NewSelectionHistoryRecord {
            github_repo_id: Some(repo_id),
            github_url: repo_url.to_string(),
            selected_at: "2026-09-01T10:00:00Z".to_string(),
        },
    )
    .await
    .expect("Failed to insert selection");
    assert_eq!(sel1.github_url, repo_url);

    assert!(has_selected_url(&pool, repo_url).await.unwrap());
    assert!(has_selected_repo_id(&pool, repo_id).await.unwrap());

    // Selection history permits multiple entries for the same repository at different times
    let sel2 = insert_selection(
        &pool,
        &NewSelectionHistoryRecord {
            github_repo_id: Some(repo_id),
            github_url: repo_url.to_string(),
            selected_at: "2026-09-04T12:00:00Z".to_string(),
        },
    )
    .await
    .expect("Same repo should be allowed to be recorded multiple times in selection history");
    assert_ne!(sel1.id, sel2.id);

    let total = count_selection_history(&pool).await.unwrap();
    assert_eq!(total, 2);
}

// ---------------------------------------------------------------------------
// 5. Settings Store Tests
// ---------------------------------------------------------------------------

#[test]
fn test_settings_defaults_and_roundtrip() {
    let dir = tempdir().expect("Failed to create tempdir");
    let config_path = dir.path().join("settings.json");

    // 1. Missing file returns defaults
    let store = SettingsStore::new(config_path.clone()).expect("Failed to init settings store");
    let default_settings = store.get().expect("Failed to get settings");
    assert_eq!(default_settings.schema_version, 1);
    assert_eq!(default_settings.github.default_search_limit, 20);
    assert_eq!(
        default_settings.github.default_trending_period,
        TrendingPeriod::Daily
    );
    assert!(!default_settings.ai.enabled);
    assert_eq!(default_settings.ai.provider, None);
    assert_eq!(default_settings.ai.base_url, None);
    assert_eq!(default_settings.ai.model, None);

    // 2. Save settings and round trip
    let updated = Settings {
        schema_version: 1,
        github: GitHubSettings {
            default_search_limit: 50,
            default_trending_period: TrendingPeriod::Weekly,
        },
        ai: AiSettings {
            enabled: true,
            provider: Some(AiProvider::OpenaiCompatible),
            base_url: Some("https://api.openai.com/v1".to_string()),
            model: Some("gpt-4o-mini".to_string()),
            ..Default::default()
        },
        ui_language: default_settings.ui_language,
        ..Default::default()
    };

    let saved = store.save(&updated).expect("Failed to save settings");
    assert_eq!(saved, updated);

    // Re-instantiate from disk and verify
    let reload_store =
        SettingsStore::new(config_path.clone()).expect("Failed to reload settings store");
    let reloaded = reload_store
        .get()
        .expect("Failed to read reloaded settings");
    assert_eq!(reloaded, updated);
}

#[test]
fn test_settings_validation() {
    let dir = tempdir().expect("Failed to create tempdir");
    let config_path = dir.path().join("settings.json");
    let store = SettingsStore::new(config_path).expect("Failed to init settings store");

    // Invalid search limit = 0
    let invalid_zero = Settings {
        github: GitHubSettings {
            default_search_limit: 0,
            ..Default::default()
        },
        ..Default::default()
    };
    assert!(matches!(
        store.save(&invalid_zero),
        Err(AppError::SettingsValidation(_))
    ));

    // Invalid search limit = 101
    let invalid_large = Settings {
        github: GitHubSettings {
            default_search_limit: 101,
            ..Default::default()
        },
        ..Default::default()
    };
    assert!(matches!(
        store.save(&invalid_large),
        Err(AppError::SettingsValidation(_))
    ));

    // Invalid schema version = 2
    let invalid_version = Settings {
        schema_version: 2,
        ..Default::default()
    };
    assert!(matches!(
        store.save(&invalid_version),
        Err(AppError::SettingsValidation(_))
    ));
}

#[test]
fn test_settings_corrupted_json_handling() {
    let dir = tempdir().expect("Failed to create tempdir");
    let config_path = dir.path().join("settings.json");

    // Write corrupted JSON content
    std::fs::write(&config_path, "{ broken json ...")
        .expect("Failed to write corrupted settings file");

    let result = SettingsStore::new(config_path.clone());
    assert!(
        result.is_err(),
        "Corrupted JSON must return SettingsRead error"
    );
    assert!(
        config_path.exists(),
        "Corrupted settings file must NOT be deleted"
    );
}

// ---------------------------------------------------------------------------
// 6. Secret Store Contract & Leak Tests
// ---------------------------------------------------------------------------

#[test]
fn test_secret_store_contract_in_memory() {
    let store = InMemorySecretStore::new();

    assert!(!store.has_secret("github_token").unwrap());
    assert_eq!(store.get_secret("github_token").unwrap(), None);

    // Set secret
    store
        .set_secret("github_token", "ghp_mock_token_12345")
        .unwrap();
    assert!(store.has_secret("github_token").unwrap());
    assert_eq!(
        store.get_secret("github_token").unwrap(),
        Some("ghp_mock_token_12345".to_string())
    );

    // Overwrite secret
    store
        .set_secret("github_token", "ghp_new_token_67890")
        .unwrap();
    assert_eq!(
        store.get_secret("github_token").unwrap(),
        Some("ghp_new_token_67890".to_string())
    );

    // Delete secret
    store.delete_secret("github_token").unwrap();
    assert!(!store.has_secret("github_token").unwrap());
    assert_eq!(store.get_secret("github_token").unwrap(), None);

    // Delete non-existent secret is safe
    assert!(store.delete_secret("github_token").is_ok());
}

#[tokio::test]
async fn test_secret_leak_gate() {
    let dir = tempdir().expect("Failed to create tempdir");
    let config_path = dir.path().join("settings.json");
    let db_path = dir.path().join("reposhelf.db");

    let canary_secret = "REPOSHELF_TEST_SECRET_NOT_REAL_CANARY_VALUE_98765";

    // Setup database
    let pool = init_pool(&db_path).await.expect("Failed to init pool");
    insert_favorite(
        &pool,
        &NewFavoriteRecord {
            github_repo_id: Some(55555),
            github_url: "https://github.com/canary/repo".to_string(),
            name: "canary-repo".to_string(),
            description: Some("Canary test repository".to_string()),
            category: Some("Other".to_string()),
            stars: Some(10),
            license: None,
            pushed_at: None,
            rating: None,
            rating_reason: None,
            rating_confidence: None,
            notes: Some("Safe notes without secrets".to_string()),
            favorited_at: "2026-09-04T00:00:00Z".to_string(),
            metadata_refreshed_at: "2026-09-04T00:00:00Z".to_string(),
            releases: None,
            languages: None,
            description_translations: None,
            description_translation_source_hash: None,
            repo_created_at: None,
            ai_analysis: None,
        },
    )
    .await
    .unwrap();

    // Setup settings store
    let settings_store = SettingsStore::new(config_path.clone()).unwrap();
    settings_store.save(&Settings::default()).unwrap();

    // Save secret into secret store abstraction (NOT settings or SQLite)
    let secret_store = InMemorySecretStore::new();
    secret_store
        .set_secret("github_token", canary_secret)
        .unwrap();

    // Check settings.json content
    let settings_content = std::fs::read_to_string(&config_path).unwrap();
    assert!(
        !settings_content.contains(canary_secret),
        "CRITICAL LEAK: Canary secret found in settings.json!"
    );

    // Check SQLite database raw binary content
    let db_bytes = std::fs::read(&db_path).unwrap();
    let db_str = String::from_utf8_lossy(&db_bytes);
    assert!(
        !db_str.contains(canary_secret),
        "CRITICAL LEAK: Canary secret found in reposhelf.db SQLite database!"
    );
}

#[test]
fn test_os_secret_store_construction() {
    // Verifies that OsSecretStore constructs cleanly with the com.reposhelf.desktop namespace
    let os_store = OsSecretStore::new();
    assert_eq!(os_store.service(), "com.reposhelf.desktop");

    let custom_store = OsSecretStore::with_service("com.reposhelf.custom");
    assert_eq!(custom_store.service(), "com.reposhelf.custom");
}
