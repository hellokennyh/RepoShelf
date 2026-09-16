use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use std::path::{Path, PathBuf};

use crate::error::AppError;

pub async fn init_pool(db_path: &Path) -> Result<SqlitePool, AppError> {
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            AppError::Database(format!("Failed to create database directory: {}", e))
        })?;
    }

    let db_existed = db_path.exists()
        && std::fs::metadata(db_path)
            .map(|m| m.len() > 0)
            .unwrap_or(false);

    let options = SqliteConnectOptions::new()
        .filename(db_path)
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .busy_timeout(std::time::Duration::from_secs(10));

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await
        .map_err(|e| AppError::Database(format!("Failed to connect to SQLite: {}", e)))?;

    verify_foreign_keys(&pool).await?;

    if db_existed {
        ensure_pre_v1_1_backup(&pool, db_path).await?;
    }

    run_migrations(&pool).await?;

    Ok(pool)
}

pub async fn ensure_pre_v1_1_backup(
    pool: &SqlitePool,
    db_path: &Path,
) -> Result<Option<PathBuf>, AppError> {
    let tables: Vec<(String,)> =
        sqlx::query_as("SELECT name FROM sqlite_master WHERE type='table'")
            .fetch_all(pool)
            .await
            .map_err(|e| AppError::Database(format!("Failed to inspect database tables: {}", e)))?;

    if tables.is_empty() {
        return Ok(None);
    }

    let has_migrations_table = tables.iter().any(|(t,)| t == "_sqlx_migrations");
    let needs_backup = if has_migrations_table {
        let row: (Option<i64>,) =
            sqlx::query_as("SELECT MAX(version) FROM _sqlx_migrations WHERE success = 1")
                .fetch_one(pool)
                .await
                .map_err(|e| {
                    AppError::Database(format!("Failed to query _sqlx_migrations: {}", e))
                })?;
        match row.0 {
            Some(max_version) => max_version < 5,
            None => tables.iter().any(|(t,)| t == "favorites"),
        }
    } else {
        tables.iter().any(|(t,)| t == "favorites")
    };

    if !needs_backup {
        return Ok(None);
    }

    let data_dir = db_path.parent().unwrap_or_else(|| Path::new("."));
    let backup_dir = data_dir.join("backups");
    let backup_path = backup_dir.join("reposhelf-pre-v1.1.0.db");

    if backup_path.exists() {
        validate_backup_file(&backup_path).await.map_err(|e| {
            AppError::DatabaseMigration(format!(
                "Existing pre-v1.1 backup at '{}' is corrupt or invalid: {}. Halting migration.",
                backup_path.display(),
                e
            ))
        })?;
        return Ok(Some(backup_path));
    }

    std::fs::create_dir_all(&backup_dir).map_err(|e| {
        AppError::DatabaseMigration(format!(
            "Failed to create backup directory '{}': {}",
            backup_dir.display(),
            e
        ))
    })?;

    let backup_path_str = backup_path.to_str().ok_or_else(|| {
        AppError::DatabaseMigration("Backup path contains invalid UTF-8".to_string())
    })?;
    let escaped_path = backup_path_str.replace('\'', "''");
    let vacuum_sql = format!("VACUUM INTO '{}'", escaped_path);

    sqlx::query(&vacuum_sql).execute(pool).await.map_err(|e| {
        AppError::DatabaseMigration(format!(
            "Failed to create pre-v1.1 backup via VACUUM INTO: {}",
            e
        ))
    })?;

    validate_backup_file(&backup_path).await.map_err(|e| {
        AppError::DatabaseMigration(format!(
            "Newly created pre-v1.1 backup validation failed: {}",
            e
        ))
    })?;

    Ok(Some(backup_path))
}

pub async fn validate_backup_file(backup_path: &Path) -> Result<(), AppError> {
    if !backup_path.exists() {
        return Err(AppError::DatabaseMigration(format!(
            "Backup file does not exist: {}",
            backup_path.display()
        )));
    }

    let options = SqliteConnectOptions::new()
        .filename(backup_path)
        .read_only(true);

    let backup_pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .map_err(|e| {
            AppError::DatabaseMigration(format!(
                "Failed to open backup database '{}' read-only: {}",
                backup_path.display(),
                e
            ))
        })?;

    let integrity_row: (String,) = sqlx::query_as("PRAGMA integrity_check")
        .fetch_one(&backup_pool)
        .await
        .map_err(|e| {
            AppError::DatabaseMigration(format!(
                "Failed to execute integrity_check on backup: {}",
                e
            ))
        })?;

    if integrity_row.0 != "ok" {
        return Err(AppError::DatabaseMigration(format!(
            "Backup integrity check failed: {}",
            integrity_row.0
        )));
    }

    let fk_rows = sqlx::query("PRAGMA foreign_key_check")
        .fetch_all(&backup_pool)
        .await
        .map_err(|e| {
            AppError::DatabaseMigration(format!(
                "Failed to execute foreign_key_check on backup: {}",
                e
            ))
        })?;

    if !fk_rows.is_empty() {
        return Err(AppError::DatabaseMigration(format!(
            "Backup foreign_key_check failed: {} violation(s)",
            fk_rows.len()
        )));
    }

    backup_pool.close().await;
    Ok(())
}

pub async fn run_migrations(pool: &SqlitePool) -> Result<(), AppError> {
    sqlx::migrate!("./migrations")
        .run(pool)
        .await
        .map_err(|e| AppError::DatabaseMigration(e.to_string()))?;
    Ok(())
}

pub async fn verify_foreign_keys(pool: &SqlitePool) -> Result<(), AppError> {
    let row: (i64,) = sqlx::query_as("PRAGMA foreign_keys")
        .fetch_one(pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to query PRAGMA foreign_keys: {}", e)))?;

    if row.0 != 1 {
        return Err(AppError::Database(
            "SQLite foreign keys are not enabled".to_string(),
        ));
    }
    Ok(())
}
