use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use std::path::Path;

use crate::error::AppError;

pub async fn init_pool(db_path: &Path) -> Result<SqlitePool, AppError> {
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            AppError::Database(format!("Failed to create database directory: {}", e))
        })?;
    }

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
    run_migrations(&pool).await?;

    Ok(pool)
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
