use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use tempfile::tempdir;

use reposhelf_lib::db::database::{ensure_pre_v1_1_backup, init_pool, validate_backup_file};
use reposhelf_lib::secrets::{InMemorySecretStore, SecretStore};
use reposhelf_lib::settings::{SettingsStore, UiLocale, WindowsUiLanguageDetector};

struct MockDetector {
    result: Result<Vec<String>, String>,
}

impl MockDetector {
    fn with_languages(langs: Vec<&str>) -> Self {
        Self {
            result: Ok(langs.into_iter().map(|s| s.to_string()).collect()),
        }
    }

    fn failing() -> Self {
        Self {
            result: Err("Mock detector failure".to_string()),
        }
    }
}

impl WindowsUiLanguageDetector for MockDetector {
    fn detect_user_preferred_ui_languages(&self) -> Result<Vec<String>, String> {
        self.result.clone()
    }
}

async fn create_v1_0_0_fixture(db_path: &Path) -> sqlx::SqlitePool {
    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(db_path)
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal);
    let pool = sqlx::SqlitePool::connect_with(options).await.unwrap();

    // Create _sqlx_migrations tracking table
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

    // Apply migrations 1, 2, 3 with exact sqlx checksums
    let migrator = sqlx::migrate!("./migrations");
    for m in migrator.iter() {
        if m.version <= 3 {
            sqlx::raw_sql(&m.sql).execute(&pool).await.unwrap();
            sqlx::query(
                "INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time) VALUES (?, ?, 1, ?, 10)"
            )
            .bind(m.version)
            .bind(&*m.description)
            .bind(&*m.checksum)
            .execute(&pool)
            .await
            .unwrap();
        }
    }

    // Insert representative v1.0.0 user records
    sqlx::query(
        r#"
        INSERT INTO favorites (
            id, github_repo_id, github_url, name, description, category, stars, license,
            pushed_at, rating, rating_reason, rating_confidence, notes, favorited_at, metadata_refreshed_at
        ) VALUES (
            1, 9901, 'https://github.com/fixture-owner/fixture-repo', 'fixture-repo',
            'Representative v1.0.0 description', 'Developer Tools', 1250, 'Apache-2.0',
            '2026-09-01T12:00:00Z', 5, 'Robust architecture and complete documentation', 'high',
            'User notes on fixture repo', '2026-09-02T10:00:00Z', '2026-09-02T10:00:00Z'
        )
        "#,
    )
    .execute(&pool)
    .await
    .unwrap();

    // Insert tags
    sqlx::query(
        r#"
        INSERT INTO favorite_tags (favorite_id, tag, source) VALUES
            (1, 'rust', 'github'),
            (1, 'tauri', 'github'),
            (1, 'productivity', 'user');
        "#,
    )
    .execute(&pool)
    .await
    .unwrap();

    // Insert selection_history
    sqlx::query(
        r#"
        INSERT INTO selection_history (github_repo_id, github_url, selected_at)
        VALUES (9901, 'https://github.com/fixture-owner/fixture-repo', '2026-09-02T10:05:00Z');
        "#,
    )
    .execute(&pool)
    .await
    .unwrap();

    pool
}

// -----------------------------------------------------------------------------
// 1. Fresh Database Migration 001 - 005 & Second Launch
// -----------------------------------------------------------------------------
#[tokio::test]
async fn test_fresh_database_migration_001_to_005_and_idempotence() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("fresh_reposhelf.db");

    // Launch 1: Fresh database initialization
    let pool = init_pool(&db_path)
        .await
        .expect("Fresh init_pool must succeed");

    // Verify migrations 1..5 applied
    let versions: Vec<(i64,)> = sqlx::query_as(
        "SELECT version FROM _sqlx_migrations WHERE success = 1 ORDER BY version ASC",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(versions.len(), 5);
    assert_eq!(versions[0].0, 1);
    assert_eq!(versions[1].0, 2);
    assert_eq!(versions[2].0, 3);
    assert_eq!(versions[3].0, 4);
    assert_eq!(versions[4].0, 5);

    // Verify NO backup was created for a fresh database
    let backup_path = dir.path().join("backups").join("reposhelf-pre-v1.1.0.db");
    assert!(
        !backup_path.exists(),
        "Fresh DB must not create pre-v1.1 backup"
    );

    pool.close().await;

    // Launch 2: Second launch on the same database
    let pool2 = init_pool(&db_path)
        .await
        .expect("Second init_pool must succeed");

    let versions2: Vec<(i64,)> = sqlx::query_as(
        "SELECT version FROM _sqlx_migrations WHERE success = 1 ORDER BY version ASC",
    )
    .fetch_all(&pool2)
    .await
    .unwrap();
    assert_eq!(versions2.len(), 5);
    assert!(!backup_path.exists());

    pool2.close().await;
}

// -----------------------------------------------------------------------------
// 2. v1.0.0 Fixture -> 004/005 Upgrade & NULL Semantics
// -----------------------------------------------------------------------------
#[tokio::test]
async fn test_v1_0_0_fixture_upgrade_to_005_and_null_semantics() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("reposhelf.db");

    // Create v1.0.0 database fixture
    let initial_pool = create_v1_0_0_fixture(&db_path).await;
    initial_pool.close().await;

    // Run v1.1 initialization
    let pool = init_pool(&db_path)
        .await
        .expect("init_pool on v1.0.0 fixture must succeed");

    // 1. Verify pre-v1.1 backup exists and is valid
    let backup_path = dir.path().join("backups").join("reposhelf-pre-v1.1.0.db");
    assert!(backup_path.exists(), "Pre-v1.1 backup must exist");
    validate_backup_file(&backup_path)
        .await
        .expect("Pre-v1.1 backup must pass validation");

    // 2. Verify all 5 migrations are applied
    let versions: Vec<(i64,)> = sqlx::query_as(
        "SELECT version FROM _sqlx_migrations WHERE success = 1 ORDER BY version ASC",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(versions.len(), 5);

    // 3. Verify old data is preserved and new columns are NULL
    let row = sqlx::query(
        r#"
        SELECT id, name, description, category, rating, rating_reason, rating_confidence, notes,
               releases, languages, description_translations, description_translation_source_hash
        FROM favorites WHERE id = 1
        "#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    use sqlx::Row;
    let name: String = row.get("name");
    let desc: Option<String> = row.get("description");
    let cat: Option<String> = row.get("category");
    let rating: Option<i64> = row.get("rating");
    let rating_reason: Option<String> = row.get("rating_reason");
    let rating_conf: Option<String> = row.get("rating_confidence");
    let notes: Option<String> = row.get("notes");
    let releases: Option<i64> = row.get("releases");
    let languages: Option<String> = row.get("languages");
    let trans: Option<String> = row.get("description_translations");
    let trans_hash: Option<String> = row.get("description_translation_source_hash");

    assert_eq!(name, "fixture-repo");
    assert_eq!(desc.as_deref(), Some("Representative v1.0.0 description"));
    assert_eq!(cat.as_deref(), Some("Developer Tools"));
    assert_eq!(rating, Some(5));
    assert_eq!(
        rating_reason.as_deref(),
        Some("Robust architecture and complete documentation")
    );
    assert_eq!(rating_conf.as_deref(), Some("high"));
    assert_eq!(notes.as_deref(), Some("User notes on fixture repo"));

    // NULL semantics for new columns
    assert!(releases.is_none(), "releases must be NULL for old rows");
    assert!(languages.is_none(), "languages must be NULL for old rows");
    assert!(
        trans.is_none(),
        "description_translations must be NULL for old rows"
    );
    assert!(
        trans_hash.is_none(),
        "description_translation_source_hash must be NULL for old rows"
    );

    // 4. Verify tags and history preserved
    let tag_count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM favorite_tags WHERE favorite_id = 1")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(tag_count.0, 3);

    let hist_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM selection_history")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(hist_count.0, 1);

    pool.close().await;

    // 5. Second launch: verify no duplicate backup or migration
    let pool2 = init_pool(&db_path).await.unwrap();
    let versions2: Vec<(i64,)> = sqlx::query_as(
        "SELECT version FROM _sqlx_migrations WHERE success = 1 ORDER BY version ASC",
    )
    .fetch_all(&pool2)
    .await
    .unwrap();
    assert_eq!(versions2.len(), 5);
    pool2.close().await;
}

// -----------------------------------------------------------------------------
// 3. Consistent Pre-Migration Backup & WAL Consistency
// -----------------------------------------------------------------------------
#[tokio::test]
async fn test_wal_committed_data_backup_consistency() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("wal_test.db");

    let initial_pool = create_v1_0_0_fixture(&db_path).await;

    // Insert more records into WAL mode without checkpointing
    for i in 2..=5 {
        sqlx::query(
            &format!(
                "INSERT INTO favorites (id, github_repo_id, github_url, name, favorited_at, metadata_refreshed_at) VALUES ({}, {}, 'https://github.com/o/r{}', 'repo{}', '2026-09-02T12:00:00Z', '2026-09-02T12:00:00Z')",
                i, 9900 + i, i, i
            )
        )
        .execute(&initial_pool)
        .await
        .unwrap();
    }

    // Call backup directly
    let backup = ensure_pre_v1_1_backup(&initial_pool, &db_path)
        .await
        .expect("Backup creation must succeed");
    let backup_path = backup.expect("Backup path must be returned");

    // Close initial pool to leave WAL uncheckpointed or intact
    initial_pool.close().await;

    // Open backup database independently in a read-only connection
    let backup_options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(&backup_path)
        .read_only(true);
    let backup_pool = sqlx::SqlitePool::connect_with(backup_options)
        .await
        .unwrap();

    // Verify all 5 committed records exist in the standalone backup file
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM favorites")
        .fetch_one(&backup_pool)
        .await
        .unwrap();
    assert_eq!(
        count.0, 5,
        "Committed WAL data must be present in the backup"
    );

    // Verify integrity and foreign keys on the standalone backup
    let (integrity,): (String,) = sqlx::query_as("PRAGMA integrity_check")
        .fetch_one(&backup_pool)
        .await
        .unwrap();
    assert_eq!(integrity, "ok");

    let fk_rows = sqlx::query("PRAGMA foreign_key_check")
        .fetch_all(&backup_pool)
        .await
        .unwrap();
    assert!(fk_rows.is_empty());

    backup_pool.close().await;
}

// -----------------------------------------------------------------------------
// 4. Backup Validation (Integrity Check & Foreign Key Check)
// -----------------------------------------------------------------------------
#[tokio::test]
async fn test_backup_validation_gates() {
    let dir = tempdir().expect("tempdir");

    // 1. Non-existent file fails
    let missing_path = dir.path().join("non_existent.db");
    let res = validate_backup_file(&missing_path).await;
    assert!(res.is_err());

    // 2. Corrupt file fails
    let corrupt_path = dir.path().join("corrupt.db");
    {
        let mut f = File::create(&corrupt_path).unwrap();
        f.write_all(b"not a valid sqlite database header at all")
            .unwrap();
    }
    let res_corrupt = validate_backup_file(&corrupt_path).await;
    assert!(
        res_corrupt.is_err(),
        "Corrupt SQLite file must fail validation"
    );

    // 3. Valid database passes
    let valid_path = dir.path().join("valid.db");
    let pool = create_v1_0_0_fixture(&valid_path).await;
    pool.close().await;
    let res_valid = validate_backup_file(&valid_path).await;
    assert!(res_valid.is_ok(), "Valid SQLite file must pass validation");
}

// -----------------------------------------------------------------------------
// 5. Backup Failure Prevents Migration
// -----------------------------------------------------------------------------
#[tokio::test]
async fn test_backup_failure_prevents_migration() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("reposhelf.db");

    let pool = create_v1_0_0_fixture(&db_path).await;
    pool.close().await;

    // Create corrupt file at deterministic backup path
    let backup_dir = dir.path().join("backups");
    fs::create_dir_all(&backup_dir).unwrap();
    let backup_path = backup_dir.join("reposhelf-pre-v1.1.0.db");
    {
        let mut f = File::create(&backup_path).unwrap();
        f.write_all(b"CORRUPT_EXISTING_BACKUP_CONTENT").unwrap();
    }

    // Launch init_pool: it should detect corrupt existing backup and fail before running migrations
    let res = init_pool(&db_path).await;
    assert!(res.is_err(), "init_pool must fail when backup is corrupt");

    // Connect raw to check migration state of original DB
    let raw_options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(&db_path)
        .read_only(true);
    let check_pool = sqlx::SqlitePool::connect_with(raw_options).await.unwrap();

    let max_v: (Option<i64>,) =
        sqlx::query_as("SELECT MAX(version) FROM _sqlx_migrations WHERE success = 1")
            .fetch_one(&check_pool)
            .await
            .unwrap();

    assert_eq!(
        max_v.0,
        Some(3),
        "Migrations 004 and 005 must NOT be run if backup fails"
    );
    check_pool.close().await;
}

// -----------------------------------------------------------------------------
// 6. Restore Drill
// -----------------------------------------------------------------------------
#[tokio::test]
async fn test_backup_restore_drill() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("active.db");

    // 1. Create historical fixture
    let pool = create_v1_0_0_fixture(&db_path).await;
    pool.close().await;

    // 2. Perform v1.1 migration via init_pool
    let pool_v11 = init_pool(&db_path).await.unwrap();

    // 3. Update v1.1 database with new features
    sqlx::query(
        "UPDATE favorites SET releases = 42, languages = '{\"Rust\":90.0,\"HTML\":10.0}' WHERE id = 1"
    )
    .execute(&pool_v11)
    .await
    .unwrap();

    let active_releases: (Option<i64>,) =
        sqlx::query_as("SELECT releases FROM favorites WHERE id = 1")
            .fetch_one(&pool_v11)
            .await
            .unwrap();
    assert_eq!(active_releases.0, Some(42));
    pool_v11.close().await;

    // 4. Restore backup to separate isolated path
    let backup_path = dir.path().join("backups").join("reposhelf-pre-v1.1.0.db");
    assert!(backup_path.exists());

    let restore_dir = tempdir().expect("restore tempdir");
    let restored_db_path = restore_dir.path().join("restored_reposhelf.db");
    fs::copy(&backup_path, &restored_db_path).expect("Copy backup to restore path");

    // 5. Open restored backup
    validate_backup_file(&restored_db_path)
        .await
        .expect("Restored backup must pass validation");

    let restore_options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(&restored_db_path)
        .read_only(true);
    let restored_pool = sqlx::SqlitePool::connect_with(restore_options)
        .await
        .unwrap();

    // 6. Verify representative pre-upgrade data and migration history
    let max_v: (Option<i64>,) =
        sqlx::query_as("SELECT MAX(version) FROM _sqlx_migrations WHERE success = 1")
            .fetch_one(&restored_pool)
            .await
            .unwrap();
    assert_eq!(
        max_v.0,
        Some(3),
        "Restored backup must reflect v1.0.0 migration state (max 3)"
    );

    let restored_fav: (String, Option<String>, Option<i64>) =
        sqlx::query_as("SELECT name, description, rating FROM favorites WHERE id = 1")
            .fetch_one(&restored_pool)
            .await
            .unwrap();
    assert_eq!(restored_fav.0, "fixture-repo");
    assert_eq!(
        restored_fav.1.as_deref(),
        Some("Representative v1.0.0 description")
    );
    assert_eq!(restored_fav.2, Some(5));

    // Verify releases column does not even exist in restored v1.0.0 schema
    let columns: Vec<(i64, String, String, i64, Option<String>, i64)> =
        sqlx::query_as("PRAGMA table_info(favorites)")
            .fetch_all(&restored_pool)
            .await
            .unwrap();
    let col_names: Vec<String> = columns.into_iter().map(|c| c.1).collect();
    assert!(!col_names.contains(&"releases".to_string()));
    assert!(!col_names.contains(&"languages".to_string()));
    assert!(!col_names.contains(&"description_translations".to_string()));

    restored_pool.close().await;
}

// -----------------------------------------------------------------------------
// 7. Settings Upgrade Validation (v1.0 -> v1.1)
// -----------------------------------------------------------------------------
#[test]
fn test_v1_0_settings_upgrade_without_ui_language() {
    let dir = tempdir().expect("tempdir");
    let settings_path = dir.path().join("settings.json");

    // Synthetic v1.0-style settings JSON (missing ui_language)
    let v1_0_json = r#"{
        "schema_version": 1,
        "github": {
            "default_search_limit": 25,
            "default_trending_period": "weekly"
        },
        "ai": {
            "enabled": true,
            "provider": "ollama",
            "base_url": "http://127.0.0.1:11434",
            "model": "qwen2.5-coder:7b"
        }
    }"#;
    fs::write(&settings_path, v1_0_json).unwrap();

    // 1. Initial launch with English system language
    let en_detector = MockDetector::with_languages(vec!["en-US", "en"]);
    let store = SettingsStore::new_with_detector(settings_path.clone(), &en_detector).unwrap();

    let settings = store.get().unwrap();
    assert_eq!(settings.ui_language, Some(UiLocale::En));
    assert_eq!(settings.github.default_search_limit, 25);
    assert_eq!(
        settings.ai.provider,
        Some(reposhelf_lib::settings::AiProvider::Ollama)
    );
    assert_eq!(
        settings.ai.base_url.as_deref(),
        Some("http://127.0.0.1:11434")
    );
    assert_eq!(settings.ai.model.as_deref(), Some("qwen2.5-coder:7b"));

    // 2. Unsupported system language falls back to English
    let unsupported_path = dir.path().join("settings_unsupported.json");
    fs::write(&unsupported_path, v1_0_json).unwrap();
    let fr_detector = MockDetector::with_languages(vec!["fr-FR", "es-ES"]);
    let store_fr =
        SettingsStore::new_with_detector(unsupported_path.clone(), &fr_detector).unwrap();
    assert_eq!(store_fr.get().unwrap().ui_language, Some(UiLocale::En));

    // 3. Failing detector falls back to English
    let failing_path = dir.path().join("settings_failing.json");
    fs::write(&failing_path, v1_0_json).unwrap();
    let fail_detector = MockDetector::failing();
    let store_fail =
        SettingsStore::new_with_detector(failing_path.clone(), &fail_detector).unwrap();
    assert_eq!(store_fail.get().unwrap().ui_language, Some(UiLocale::En));
}

#[test]
fn test_settings_preservation_of_valid_ui_languages() {
    let dir = tempdir().expect("tempdir");

    for locale_str in &["en", "zh-Hans", "zh-Hant"] {
        let path = dir.path().join(format!("settings_{}.json", locale_str));
        let json = format!(
            r#"{{
                "schema_version": 1,
                "github": {{
                    "default_search_limit": 30,
                    "default_trending_period": "daily"
                }},
                "ai": {{
                    "enabled": true,
                    "provider": "openai_compatible",
                    "base_url": "https://api.openai.com/v1",
                    "model": "gpt-4o-mini"
                }},
                "ui_language": "{}"
            }}"#,
            locale_str
        );
        fs::write(&path, json).unwrap();

        // Detector returns an entirely different language
        let detector = MockDetector::with_languages(vec!["ja-JP"]);
        let store = SettingsStore::new_with_detector(path.clone(), &detector).unwrap();

        let settings = store.get().unwrap();
        match *locale_str {
            "en" => assert_eq!(settings.ui_language, Some(UiLocale::En)),
            "zh-Hans" => assert_eq!(settings.ui_language, Some(UiLocale::ZhHans)),
            "zh-Hant" => assert_eq!(settings.ui_language, Some(UiLocale::ZhHant)),
            _ => unreachable!(),
        }
        assert_eq!(
            settings.ai.provider,
            Some(reposhelf_lib::settings::AiProvider::OpenaiCompatible)
        );
        assert_eq!(settings.ai.model.as_deref(), Some("gpt-4o-mini"));
    }
}

// -----------------------------------------------------------------------------
// 8. Dummy SecretStore Namespace Preservation
// -----------------------------------------------------------------------------
#[test]
fn test_dummy_secretstore_preservation() {
    let store = InMemorySecretStore::new();
    let test_key = "dummy_test_key";
    let test_val = "dummy_non_sensitive_value_12345";

    store.set_secret(test_key, test_val).unwrap();
    assert!(store.has_secret(test_key).unwrap());
    assert_eq!(
        store.get_secret(test_key).unwrap().as_deref(),
        Some(test_val)
    );

    // Simulate v1.1 store access and ensure secret is unchanged
    assert_eq!(
        store.get_secret(test_key).unwrap().as_deref(),
        Some(test_val)
    );
}

// -----------------------------------------------------------------------------
// 9. Product Version Consistency Verification
// -----------------------------------------------------------------------------
#[test]
fn test_product_version_cargo_package() {
    let pkg_version = env!("CARGO_PKG_VERSION");
    assert_eq!(
        pkg_version, "1.1.0",
        "Cargo package version must resolve to 1.1.0: found {}",
        pkg_version
    );
}
