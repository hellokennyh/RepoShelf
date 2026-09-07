use sqlx::{QueryBuilder, Sqlite, SqlitePool};
use std::collections::HashSet;

use super::models::{NewSelectionHistoryRecord, SelectionHistoryRecord};
use crate::error::AppError;
use crate::github::{parse_github_repo_url, GitHubRepository};

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

pub async fn insert_selection_history_batch(
    pool: &SqlitePool,
    items: &[GitHubRepository],
) -> Result<(), AppError> {
    if items.is_empty() {
        return Ok(());
    }

    let now_utc = chrono::Utc::now().to_rfc3339();
    let mut tx = pool.begin().await?;

    for item in items {
        let canonical_url = match parse_github_repo_url(&item.github_url) {
            Ok((_, _, canonical)) => canonical,
            Err(_) => item.github_url.clone(),
        };

        sqlx::query(
            r#"
            INSERT INTO selection_history (github_repo_id, github_url, selected_at)
            VALUES (?, ?, ?)
            "#,
        )
        .bind(item.github_repo_id)
        .bind(&canonical_url)
        .bind(&now_utc)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(())
}

pub async fn find_selected_identities(
    pool: &SqlitePool,
    repo_ids: &[i64],
    canonical_urls: &[String],
) -> Result<(HashSet<i64>, HashSet<String>), AppError> {
    let mut matched_repo_ids = HashSet::new();
    if !repo_ids.is_empty() {
        let mut qb: QueryBuilder<Sqlite> = QueryBuilder::new(
            "SELECT DISTINCT github_repo_id FROM selection_history WHERE github_repo_id IN (",
        );
        let mut separated = qb.separated(", ");
        for id in repo_ids {
            separated.push_bind(id);
        }
        separated.push_unseparated(")");
        let rows: Vec<(Option<i64>,)> = qb.build_query_as().fetch_all(pool).await?;
        for (id_opt,) in rows {
            if let Some(id) = id_opt {
                matched_repo_ids.insert(id);
            }
        }
    }

    let mut matched_urls = HashSet::new();
    if !canonical_urls.is_empty() {
        let mut qb: QueryBuilder<Sqlite> = QueryBuilder::new(
            "SELECT DISTINCT github_url FROM selection_history WHERE github_url IN (",
        );
        let mut separated = qb.separated(", ");
        for url in canonical_urls {
            separated.push_bind(url);
        }
        separated.push_unseparated(")");
        let rows: Vec<(String,)> = qb.build_query_as().fetch_all(pool).await?;
        for (url,) in rows {
            matched_urls.insert(url);
        }
    }

    Ok((matched_repo_ids, matched_urls))
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
