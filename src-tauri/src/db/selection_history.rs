use sqlx::SqlitePool;

use super::models::{NewSelectionHistoryRecord, SelectionHistoryRecord};
use crate::error::AppError;

pub async fn insert_selection(
    pool: &SqlitePool,
    sel: &NewSelectionHistoryRecord,
) -> Result<SelectionHistoryRecord, AppError> {
    let result = sqlx::query(
        r#"
        INSERT INTO selection_history (github_repo_id, github_url, selected_at)
        VALUES (?, ?, ?)
        "#,
    )
    .bind(sel.github_repo_id)
    .bind(&sel.github_url)
    .bind(&sel.selected_at)
    .execute(pool)
    .await?;

    let id = result.last_insert_rowid();

    let record = sqlx::query_as::<_, SelectionHistoryRecord>(
        r#"
        SELECT id, github_repo_id, github_url, selected_at
        FROM selection_history
        WHERE id = ?
        "#,
    )
    .bind(id)
    .fetch_one(pool)
    .await?;

    Ok(record)
}

pub async fn has_selected_url(pool: &SqlitePool, url: &str) -> Result<bool, AppError> {
    let row: (i64,) = sqlx::query_as(
        r#"
        SELECT COUNT(1)
        FROM selection_history
        WHERE github_url = ?
        "#,
    )
    .bind(url)
    .fetch_one(pool)
    .await?;

    Ok(row.0 > 0)
}

pub async fn has_selected_repo_id(pool: &SqlitePool, repo_id: i64) -> Result<bool, AppError> {
    let row: (i64,) = sqlx::query_as(
        r#"
        SELECT COUNT(1)
        FROM selection_history
        WHERE github_repo_id = ?
        "#,
    )
    .bind(repo_id)
    .fetch_one(pool)
    .await?;

    Ok(row.0 > 0)
}

pub async fn count_selection_history(pool: &SqlitePool) -> Result<i64, AppError> {
    let row: (i64,) = sqlx::query_as("SELECT COUNT(1) FROM selection_history")
        .fetch_one(pool)
        .await?;

    Ok(row.0)
}
