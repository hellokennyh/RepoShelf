use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use tempfile::tempdir;

use reposhelf_lib::db::database::{ensure_pre_v1_1_backup, init_pool, validate_backup_file};
use reposhelf_lib::error::AppError;
use reposhelf_lib::secrets::{InMemorySecretStore, SecretStore};
use reposhelf_lib::settings::{SettingsStore, UiLocale, WindowsUiLanguageDetector};
use sha2::{Digest, Sha256, Sha384};

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
    assert_eq!(versions.len(), 6);
    assert_eq!(versions[0].0, 1);
    assert_eq!(versions[1].0, 2);
    assert_eq!(versions[2].0, 3);
    assert_eq!(versions[3].0, 4);
    assert_eq!(versions[4].0, 5);
    assert_eq!(versions[5].0, 6);

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
    assert_eq!(versions2.len(), 6);
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
    assert_eq!(versions.len(), 6);

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
    assert_eq!(versions2.len(), 6);
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
// 5b. True Migration 005 Failure Preserves Backup & Partial History
// -----------------------------------------------------------------------------
#[tokio::test]
async fn test_true_migration_005_failure_preserves_backup_and_partial_history() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("reposhelf.db");

    // 1. Create a valid pre-v1.1 database fixture (migrations 1..3 with representative data)
    let initial_pool = create_v1_0_0_fixture(&db_path).await;

    // 2. Fault injection: add conflicting column specifically for migration 005
    // Leaving _sqlx_migrations at versions 1, 2, 3 without manual 004/005 entries
    sqlx::query("ALTER TABLE favorites ADD COLUMN description_translations TEXT")
        .execute(&initial_pool)
        .await
        .unwrap();
    initial_pool.close().await;

    // 3. First startup attempt via production init_pool
    let res1 = init_pool(&db_path).await;
    assert!(
        res1.is_err(),
        "First init_pool must fail due to migration 005 failure"
    );

    // 4. Verify error is attributable to migration execution, NOT backup validation
    let err1 = res1.unwrap_err();
    match &err1 {
        AppError::DatabaseMigration(msg) => {
            assert!(
                msg.contains("description_translations"),
                "Error must identify migration 005 column conflict: {}",
                msg
            );
            assert!(
                !msg.contains("Existing pre-v1.1 backup")
                    && !msg.contains("backup validation failed"),
                "Error must be from migration execution, not backup validation: {}",
                msg
            );
        }
        other => panic!("Expected DatabaseMigration error, got: {:?}", other),
    }

    // 5. Verify pre-v1.1 backup was created and independently passes validation
    let backup_path = dir.path().join("backups").join("reposhelf-pre-v1.1.0.db");
    assert!(backup_path.exists(), "Pre-v1.1 backup must exist");
    validate_backup_file(&backup_path)
        .await
        .expect("Pre-v1.1 backup must independently pass validation");

    // 6. Verify backup represents pre-upgrade state (versions 1..3 and representative data)
    let backup_options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(&backup_path)
        .read_only(true);
    let backup_pool = sqlx::SqlitePool::connect_with(backup_options)
        .await
        .unwrap();

    let backup_versions: Vec<(i64,)> = sqlx::query_as(
        "SELECT version FROM _sqlx_migrations WHERE success = 1 ORDER BY version ASC",
    )
    .fetch_all(&backup_pool)
    .await
    .unwrap();
    assert_eq!(
        backup_versions.iter().map(|(v,)| *v).collect::<Vec<_>>(),
        vec![1, 2, 3],
        "Backup migration history must strictly remain 1, 2, 3"
    );

    let backup_fav: (String, Option<String>, Option<i64>) =
        sqlx::query_as("SELECT name, description, rating FROM favorites WHERE id = 1")
            .fetch_one(&backup_pool)
            .await
            .unwrap();
    assert_eq!(backup_fav.0, "fixture-repo");
    assert_eq!(
        backup_fav.1.as_deref(),
        Some("Representative v1.0.0 description")
    );
    assert_eq!(backup_fav.2, Some(5));

    let backup_tags: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM favorite_tags WHERE favorite_id = 1")
            .fetch_one(&backup_pool)
            .await
            .unwrap();
    assert_eq!(backup_tags.0, 3);

    let backup_hist: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM selection_history WHERE github_repo_id = 9901")
            .fetch_one(&backup_pool)
            .await
            .unwrap();
    assert_eq!(backup_hist.0, 1);
    backup_pool.close().await;

    // 7. Verify active database: migration 004 committed, 005 not committed, DB inspectable
    let active_options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(&db_path)
        .read_only(true);
    let active_pool = sqlx::SqlitePool::connect_with(active_options)
        .await
        .unwrap();

    let (active_integrity,): (String,) = sqlx::query_as("PRAGMA integrity_check")
        .fetch_one(&active_pool)
        .await
        .unwrap();
    assert_eq!(active_integrity, "ok");

    let active_versions: Vec<(i64,)> = sqlx::query_as(
        "SELECT version FROM _sqlx_migrations WHERE success = 1 ORDER BY version ASC",
    )
    .fetch_all(&active_pool)
    .await
    .unwrap();
    assert_eq!(
        active_versions.iter().map(|(v,)| *v).collect::<Vec<_>>(),
        vec![1, 2, 3, 4],
        "Active DB migration history must reflect committed migrations 1..4 without 5"
    );

    let columns: Vec<(i64, String, String, i64, Option<String>, i64)> =
        sqlx::query_as("PRAGMA table_info(favorites)")
            .fetch_all(&active_pool)
            .await
            .unwrap();
    let col_names: Vec<String> = columns.into_iter().map(|c| c.1).collect();
    assert!(
        col_names.contains(&"releases".to_string()),
        "releases column added by migration 004"
    );
    assert!(
        col_names.contains(&"languages".to_string()),
        "languages column added by migration 004"
    );
    assert!(
        !col_names.contains(&"description_translation_source_hash".to_string()),
        "description_translation_source_hash must not exist since migration 005 failed"
    );
    active_pool.close().await;

    // 8. Calculate SHA-256 of backup before retry
    let backup_hash_before = {
        let bytes = fs::read(&backup_path).expect("read backup bytes before retry");
        let mut hasher = Sha256::new();
        Digest::update(&mut hasher, &bytes);
        format!("{:x}", Digest::finalize(hasher))
    };

    // 9. Second startup attempt: must fail with same error without overwriting backup or replaying 004
    let res2 = init_pool(&db_path).await;
    assert!(res2.is_err(), "Second init_pool must fail on retry");
    let err2 = res2.unwrap_err();
    match &err2 {
        AppError::DatabaseMigration(msg) => {
            assert!(
                msg.contains("description_translations"),
                "Second startup error must identify migration 005 column conflict: {}",
                msg
            );
        }
        other => panic!(
            "Expected DatabaseMigration error on second launch, got: {:?}",
            other
        ),
    }

    // 10. Active migration history remains stable (no duplicate 004, no 005)
    let active_pool2 = sqlx::SqlitePool::connect_with(
        sqlx::sqlite::SqliteConnectOptions::new()
            .filename(&db_path)
            .read_only(true),
    )
    .await
    .unwrap();
    let active_versions2: Vec<(i64,)> = sqlx::query_as(
        "SELECT version FROM _sqlx_migrations WHERE success = 1 ORDER BY version ASC",
    )
    .fetch_all(&active_pool2)
    .await
    .unwrap();
    assert_eq!(
        active_versions2.iter().map(|(v,)| *v).collect::<Vec<_>>(),
        vec![1, 2, 3, 4],
        "Active DB migration history must remain exactly 1..4 with no duplicates"
    );
    let row_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM _sqlx_migrations")
        .fetch_one(&active_pool2)
        .await
        .unwrap();
    assert_eq!(
        row_count.0, 4,
        "Exact row count in _sqlx_migrations must remain 4"
    );
    active_pool2.close().await;

    // 11. Backup after retry: hash is identical, still valid, representative data preserved
    let backup_hash_after = {
        let bytes = fs::read(&backup_path).expect("read backup bytes after retry");
        let mut hasher = Sha256::new();
        Digest::update(&mut hasher, &bytes);
        format!("{:x}", Digest::finalize(hasher))
    };
    assert_eq!(
        backup_hash_after, backup_hash_before,
        "Backup SHA-256 must remain identical across retry (no overwrite)"
    );
    validate_backup_file(&backup_path)
        .await
        .expect("Backup must still pass validation after retry");

    let backup_pool2 = sqlx::SqlitePool::connect_with(
        sqlx::sqlite::SqliteConnectOptions::new()
            .filename(&backup_path)
            .read_only(true),
    )
    .await
    .unwrap();
    let backup_fav2: (String, Option<String>, Option<i64>) =
        sqlx::query_as("SELECT name, description, rating FROM favorites WHERE id = 1")
            .fetch_one(&backup_pool2)
            .await
            .unwrap();
    assert_eq!(backup_fav2.0, "fixture-repo");
    assert_eq!(
        backup_fav2.1.as_deref(),
        Some("Representative v1.0.0 description")
    );
    assert_eq!(backup_fav2.2, Some(5));
    backup_pool2.close().await;
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
        pkg_version, "1.1.3",
        "Cargo package version must resolve to 1.1.3: found {}",
        pkg_version
    );
}

fn decode_hex_to_bytes(hex_str: &str) -> Vec<u8> {
    (0..hex_str.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex_str[i..i + 2], 16).expect("valid hex byte"))
        .collect()
}

// -----------------------------------------------------------------------------
// 10. Historical Migration Checksum Regression Test (Raw SHA-384)
// -----------------------------------------------------------------------------
#[test]
fn test_historical_migration_bytes_match_released_v1_0_checksums() {
    let mig_001_bytes =
        fs::read("migrations/001_initial_schema.sql").expect("Read 001_initial_schema.sql");
    assert_eq!(
        mig_001_bytes.len(),
        1691,
        "001 byte length must be exactly 1691"
    );
    let mut hasher1 = Sha384::new();
    hasher1.update(&mig_001_bytes);
    let hash_001 = format!("{:X}", hasher1.finalize());
    assert_eq!(
        hash_001,
        "0662BEC024141D4F372A41ECB889660DA7923E3EB9727827CDA71AD84EF72846F981C7B64CE169BBAA6272C95E37B46F",
        "001 SHA-384 must match historical v1.0 CRLF checksum"
    );

    let mig_002_bytes =
        fs::read("migrations/002_rating_suggestions.sql").expect("Read 002_rating_suggestions.sql");
    assert_eq!(
        mig_002_bytes.len(),
        268,
        "002 byte length must be exactly 268"
    );
    let mut hasher2 = Sha384::new();
    hasher2.update(&mig_002_bytes);
    let hash_002 = format!("{:X}", hasher2.finalize());
    assert_eq!(
        hash_002,
        "774927ADFDD38967533618F1ED0111EF008373E9A4F42EA650CAE4A0A8C047FD1DA782C944C1DA7A9F044B45066BA2AC",
        "002 SHA-384 must match historical v1.0 CRLF checksum"
    );

    let mig_003_bytes = fs::read("migrations/003_ai_tag_dedup_cleanup.sql")
        .expect("Read 003_ai_tag_dedup_cleanup.sql");
    assert_eq!(
        mig_003_bytes.len(),
        354,
        "003 byte length must be exactly 354"
    );
    let mut hasher3 = Sha384::new();
    hasher3.update(&mig_003_bytes);
    let hash_003 = format!("{:X}", hasher3.finalize());
    assert_eq!(
        hash_003,
        "6CCFC5615A447D2CBDB12C8FBA77FAB8870DDB91BED981BAF11182C0ED2C4C956D0150D625C01A9BF2BCB9CB9B08F6B3",
        "003 SHA-384 must match historical v1.0 LF checksum"
    );
}

// -----------------------------------------------------------------------------
// 11. Real Historical v1.0 Checksums Upgrade Cleanly to 005
// -----------------------------------------------------------------------------
#[tokio::test]
async fn test_real_historical_v1_0_checksums_upgrade_cleanly_to_005() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("historical_v1_0.db");

    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(&db_path)
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal);
    let pool = sqlx::SqlitePool::connect_with(options).await.unwrap();

    // 1. Create _sqlx_migrations tracking table
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

    // Historical raw SHA-384 bytes matching SQLx checksum semantics
    let c001 = decode_hex_to_bytes("0662BEC024141D4F372A41ECB889660DA7923E3EB9727827CDA71AD84EF72846F981C7B64CE169BBAA6272C95E37B46F");
    let c002 = decode_hex_to_bytes("774927ADFDD38967533618F1ED0111EF008373E9A4F42EA650CAE4A0A8C047FD1DA782C944C1DA7A9F044B45066BA2AC");
    let c003 = decode_hex_to_bytes("6CCFC5615A447D2CBDB12C8FBA77FAB8870DDB91BED981BAF11182C0ED2C4C956D0150D625C01A9BF2BCB9CB9B08F6B3");

    // Apply migrations 1, 2, 3 SQL schemas and insert historical checksums
    let migrator = sqlx::migrate!("./migrations");
    for m in migrator.iter() {
        if m.version == 1 {
            sqlx::raw_sql(&m.sql).execute(&pool).await.unwrap();
            sqlx::query(
                "INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time) VALUES (1, ?, 1, ?, 10)",
            )
            .bind(&*m.description)
            .bind(&c001[..])
            .execute(&pool)
            .await
            .unwrap();
        } else if m.version == 2 {
            sqlx::raw_sql(&m.sql).execute(&pool).await.unwrap();
            sqlx::query(
                "INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time) VALUES (2, ?, 1, ?, 10)",
            )
            .bind(&*m.description)
            .bind(&c002[..])
            .execute(&pool)
            .await
            .unwrap();
        } else if m.version == 3 {
            sqlx::raw_sql(&m.sql).execute(&pool).await.unwrap();
            sqlx::query(
                "INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time) VALUES (3, ?, 1, ?, 10)",
            )
            .bind(&*m.description)
            .bind(&c003[..])
            .execute(&pool)
            .await
            .unwrap();
        }
    }

    // Populate representative synthetic user data
    sqlx::query(
        r#"
        INSERT INTO favorites (
            id, github_repo_id, github_url, name, description, category, stars, license,
            pushed_at, rating, rating_reason, rating_confidence, notes, favorited_at, metadata_refreshed_at
        ) VALUES (
            1, 9901, 'https://github.com/historical-owner/historical-repo', 'historical-repo',
            'Historical v1.0.0 user description', 'Productivity', 888, 'MIT',
            '2026-08-15T12:00:00Z', 5, 'Historical high rating reason', 'high',
            'Critical historical user notes that must be preserved', '2026-08-16T10:00:00Z', '2026-08-16T10:00:00Z'
        )
        "#,
    )
    .execute(&pool)
    .await
    .unwrap();

    // Populate GitHub tags and user tag
    sqlx::query(
        r#"
        INSERT INTO favorite_tags (favorite_id, tag, source) VALUES
            (1, 'rust', 'github'),
            (1, 'tauri', 'github'),
            (1, 'offline-first', 'user');
        "#,
    )
    .execute(&pool)
    .await
    .unwrap();

    // Populate selection history
    sqlx::query(
        r#"
        INSERT INTO selection_history (github_repo_id, github_url, selected_at)
        VALUES (9901, 'https://github.com/historical-owner/historical-repo', '2026-08-16T10:05:00Z');
        "#,
    )
    .execute(&pool)
    .await
    .unwrap();

    // Before upgrade assertion: assert migration versions 1, 2, 3 and exact raw checksum values
    let pre_rows: Vec<(i64, Vec<u8>)> = sqlx::query_as(
        "SELECT version, checksum FROM _sqlx_migrations WHERE success = 1 ORDER BY version ASC",
    )
    .fetch_all(&pool)
    .await
    .unwrap();

    assert_eq!(pre_rows.len(), 3);
    assert_eq!(pre_rows[0].0, 1);
    assert_eq!(pre_rows[0].1, c001);
    assert_eq!(pre_rows[1].0, 2);
    assert_eq!(pre_rows[1].1, c002);
    assert_eq!(pre_rows[2].0, 3);
    assert_eq!(pre_rows[2].1, c003);

    pool.close().await;

    // Invoke production init_pool: must succeed with exact historical checksums
    let pool_v11 = init_pool(&db_path)
        .await
        .expect("init_pool on real historical v1.0 checksums must succeed");

    // After upgrade assert: migration versions 1,2,3,4,5, all success
    let post_rows: Vec<(i64, bool)> =
        sqlx::query_as("SELECT version, success FROM _sqlx_migrations ORDER BY version ASC")
            .fetch_all(&pool_v11)
            .await
            .unwrap();

    assert_eq!(post_rows.len(), 6);
    for (idx, &(v, success)) in post_rows.iter().enumerate() {
        assert_eq!(v, (idx + 1) as i64);
        assert!(success, "Migration version {} must have success = 1", v);
    }

    // Historical user data preserved
    use sqlx::Row;
    let fav = sqlx::query(
        r#"
        SELECT name, description, category, rating, rating_reason, rating_confidence, notes,
               releases, languages, description_translations, description_translation_source_hash
        FROM favorites WHERE id = 1
        "#,
    )
    .fetch_one(&pool_v11)
    .await
    .unwrap();

    let name: String = fav.get("name");
    let desc: Option<String> = fav.get("description");
    let cat: Option<String> = fav.get("category");
    let rating: Option<i64> = fav.get("rating");
    let rating_reason: Option<String> = fav.get("rating_reason");
    let rating_conf: Option<String> = fav.get("rating_confidence");
    let notes: Option<String> = fav.get("notes");
    let releases: Option<i64> = fav.get("releases");
    let languages: Option<String> = fav.get("languages");
    let trans: Option<String> = fav.get("description_translations");
    let trans_hash: Option<String> = fav.get("description_translation_source_hash");

    assert_eq!(name, "historical-repo");
    assert_eq!(desc.as_deref(), Some("Historical v1.0.0 user description"));
    assert_eq!(cat.as_deref(), Some("Productivity"));
    assert_eq!(rating, Some(5));
    assert_eq!(
        rating_reason.as_deref(),
        Some("Historical high rating reason")
    );
    assert_eq!(rating_conf.as_deref(), Some("high"));
    assert_eq!(
        notes.as_deref(),
        Some("Critical historical user notes that must be preserved")
    );

    // Migration 004 fields remain NULL for old rows
    assert!(
        releases.is_none(),
        "releases must remain NULL for historical rows"
    );
    assert!(
        languages.is_none(),
        "languages must remain NULL for historical rows"
    );

    // Migration 005 fields remain NULL for old rows
    assert!(
        trans.is_none(),
        "description_translations must remain NULL for historical rows"
    );
    assert!(
        trans_hash.is_none(),
        "description_translation_source_hash must remain NULL for historical rows"
    );

    // Tags preserved
    let tag_count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM favorite_tags WHERE favorite_id = 1")
            .fetch_one(&pool_v11)
            .await
            .unwrap();
    assert_eq!(tag_count.0, 3);

    // Selection history preserved
    let hist_count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM selection_history WHERE github_repo_id = 9901")
            .fetch_one(&pool_v11)
            .await
            .unwrap();
    assert_eq!(hist_count.0, 1);

    // Pre-v1.1 backup exists and is valid
    let backup_path = dir.path().join("backups").join("reposhelf-pre-v1.1.0.db");
    assert!(backup_path.exists(), "Pre-v1.1 backup must exist");
    validate_backup_file(&backup_path)
        .await
        .expect("Pre-v1.1 backup must be valid SQLite database");

    // Backup migration history remains strictly 1, 2, 3 (not 4 or 5)
    let backup_pool = sqlx::SqlitePool::connect_with(
        sqlx::sqlite::SqliteConnectOptions::new()
            .filename(&backup_path)
            .read_only(true),
    )
    .await
    .unwrap();

    let backup_versions: Vec<(i64,)> = sqlx::query_as(
        "SELECT version FROM _sqlx_migrations WHERE success = 1 ORDER BY version ASC",
    )
    .fetch_all(&backup_pool)
    .await
    .unwrap();

    assert_eq!(backup_versions.len(), 3);
    assert_eq!(backup_versions[0].0, 1);
    assert_eq!(backup_versions[1].0, 2);
    assert_eq!(backup_versions[2].0, 3);

    backup_pool.close().await;
    pool_v11.close().await;

    // Second launch verification
    let pool2 = init_pool(&db_path)
        .await
        .expect("Second launch on historical upgraded DB must succeed");
    let v2: Vec<(i64,)> = sqlx::query_as(
        "SELECT version FROM _sqlx_migrations WHERE success = 1 ORDER BY version ASC",
    )
    .fetch_all(&pool2)
    .await
    .unwrap();
    assert_eq!(v2.len(), 6);
    pool2.close().await;
}

// -----------------------------------------------------------------------------
// 12. Negative Test: SQLx Rejects Tampered Migration 001 Checksum
// -----------------------------------------------------------------------------
#[tokio::test]
async fn test_tampered_migration_checksum_is_rejected_by_sqlx() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("tampered.db");

    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(&db_path)
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal);
    let pool = sqlx::SqlitePool::connect_with(options).await.unwrap();

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

    let migrator = sqlx::migrate!("./migrations");
    for m in migrator.iter() {
        if m.version == 1 {
            sqlx::raw_sql(&m.sql).execute(&pool).await.unwrap();
            // Intentionally corrupt migration 1 checksum
            let tampered_checksum = vec![0xFFu8; 48];
            sqlx::query(
                "INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time) VALUES (1, ?, 1, ?, 10)",
            )
            .bind(&*m.description)
            .bind(&tampered_checksum[..])
            .execute(&pool)
            .await
            .unwrap();
        }
    }
    pool.close().await;

    // Attempting init_pool must fail because SQLx rejects the mismatched checksum
    let res = init_pool(&db_path).await;
    assert!(
        res.is_err(),
        "init_pool must fail when migration checksum does not match"
    );
    match res.err().unwrap() {
        AppError::DatabaseMigration(msg) => {
            assert!(
                msg.contains("migration") || msg.contains("modified") || msg.contains("version 1"),
                "Error message should reflect migration checksum/version mismatch: {}",
                msg
            );
        }
        other => panic!("Expected DatabaseMigration error, got: {:?}", other),
    }
}
