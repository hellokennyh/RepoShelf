use sqlx::{Column, Row};
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use tempfile::tempdir;

use reposhelf_lib::db::database::init_pool;
use reposhelf_lib::library::models::FavoriteInput;
use reposhelf_lib::library::repository::insert_favorite_tx;
use reposhelf_lib::settings::SettingsStore;

#[test]
fn test_no_migration_006_exists() {
    let migrations_dir = Path::new("migrations");
    let entries = fs::read_dir(migrations_dir).expect("Failed to read migrations directory");

    let mut migration_files = Vec::new();
    for entry in entries {
        let entry = entry.expect("Valid migration dir entry");
        let name = entry.file_name().to_string_lossy().to_string();
        if name.ends_with(".sql") {
            migration_files.push(name);
        }
    }
    migration_files.sort();

    // Verify 001 through 005 only
    assert_eq!(
        migration_files,
        vec![
            "001_initial_schema.sql".to_string(),
            "002_rating_suggestions.sql".to_string(),
            "003_ai_tag_dedup_cleanup.sql".to_string(),
            "004_repository_metadata.sql".to_string(),
            "005_description_translation.sql".to_string(),
        ]
    );

    // Migration 006 must be strictly absent
    assert!(
        !migration_files.iter().any(|m| m.contains("006")),
        "Migration 006 must NOT exist in S6"
    );
}

#[tokio::test]
async fn test_favorites_schema_has_no_star_history_columns() {
    let temp_dir = tempdir().expect("tempdir");
    let db_path = temp_dir.path().join("test_schema.db");

    let pool = init_pool(&db_path).await.expect("init_pool");

    let rows = sqlx::query("PRAGMA table_info(favorites)")
        .fetch_all(&pool)
        .await
        .expect("PRAGMA table_info");

    let mut column_names: HashSet<String> = HashSet::new();
    for row in rows {
        let col_name: String = row.get("name");
        column_names.insert(col_name);
    }

    // Verify expected columns from 001-005 exist
    assert!(column_names.contains("id"));
    assert!(column_names.contains("name"));
    assert!(column_names.contains("description"));
    assert!(column_names.contains("releases"));
    assert!(column_names.contains("languages"));
    assert!(column_names.contains("description_translations"));
    assert!(column_names.contains("description_translation_source_hash"));

    // Verify NO star history columns exist in favorites table
    for col in &column_names {
        let lower = col.to_ascii_lowercase();
        assert!(
            !lower.contains("star_history"),
            "Disallowed column '{}' found in favorites table",
            col
        );
        assert!(
            !lower.contains("preview"),
            "Disallowed column '{}' found in favorites table",
            col
        );
        assert!(
            !lower.contains("svg"),
            "Disallowed column '{}' found in favorites table",
            col
        );
        assert!(
            !lower.contains("chart"),
            "Disallowed column '{}' found in favorites table",
            col
        );
        assert!(
            !lower.contains("readme"),
            "Disallowed column '{}' found in favorites table",
            col
        );
    }
}

#[tokio::test]
async fn test_favorite_save_does_not_persist_star_history() {
    let temp_dir = tempdir().expect("tempdir");
    let db_path = temp_dir.path().join("test_fav.db");

    let pool = init_pool(&db_path).await.expect("init_pool");

    let input = FavoriteInput {
        github_repo_id: Some(123456),
        name: "test-repo".to_string(),
        github_url: "https://github.com/owner/test-repo".to_string(),
        description: Some("A great repository".to_string()),
        category: "Developer Tools".to_string(),
        stars: 1234,
        license: Some("MIT".to_string()),
        topics: vec!["rust".to_string()],
        pushed_at: None,
        releases: Some(5),
        languages: Some(std::collections::HashMap::from([(
            "Rust".to_string(),
            100.0,
        )])),
        translation: None,
    };

    let item = insert_favorite_tx(&pool, &input)
        .await
        .expect("insert_favorite_tx");

    // Query all columns from the newly created row
    let raw_row = sqlx::query("SELECT * FROM favorites WHERE id = ?")
        .bind(item.id)
        .fetch_one(&pool)
        .await
        .expect("fetch favorite row");

    // Verify column values: verify no Star History columns or values exist
    let columns = raw_row.columns();
    for col in columns {
        let col_name = col.name().to_ascii_lowercase();
        assert!(!col_name.contains("star_history"));
        assert!(!col_name.contains("svg"));
        assert!(!col_name.contains("readme"));
    }
}

#[tokio::test]
async fn test_settings_store_contains_no_star_history_cache() {
    let temp_dir = tempdir().expect("tempdir");
    let settings_path = temp_dir.path().join("settings.json");

    let store = SettingsStore::new(settings_path.clone()).expect("SettingsStore::new");
    let settings = store.get().expect("store.get");

    let settings_json = serde_json::to_string(&settings).expect("serialize settings");
    assert!(
        !settings_json.contains("star_history"),
        "Settings must not contain star_history cache"
    );
    assert!(
        !settings_json.contains("svg"),
        "Settings must not contain svg data"
    );

    // Read raw file on disk
    let disk_raw = fs::read_to_string(&settings_path).expect("read settings file");
    assert!(!disk_raw.contains("star_history"));
}

#[tokio::test]
async fn test_no_business_disk_cache_directory_created() {
    let temp_dir = tempdir().expect("tempdir");
    let data_dir = temp_dir.path().join("data");
    let config_dir = temp_dir.path().join("config");
    fs::create_dir_all(&data_dir).expect("create data_dir");
    fs::create_dir_all(&config_dir).expect("create config_dir");

    // Check data_dir contents
    let data_entries: Vec<_> = fs::read_dir(&data_dir)
        .expect("read data_dir")
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();

    assert!(
        !data_entries
            .iter()
            .any(|name| name.contains("star_history") || name.contains("cache")),
        "No business disk cache directory should exist in data_dir"
    );
}
