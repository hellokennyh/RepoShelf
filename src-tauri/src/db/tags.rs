use sqlx::SqlitePool;

use super::models::{FavoriteTagRecord, NewFavoriteTagRecord};
use crate::error::AppError;

pub async fn insert_tag(
    pool: &SqlitePool,
    tag: &NewFavoriteTagRecord,
) -> Result<FavoriteTagRecord, AppError> {
    let result = sqlx::query(
        r#"
        INSERT INTO favorite_tags (favorite_id, tag, source)
        VALUES (?, ?, ?)
        "#,
    )
    .bind(tag.favorite_id)
    .bind(&tag.tag)
    .bind(&tag.source)
    .execute(pool)
    .await?;

    let id = result.last_insert_rowid();

    let record = sqlx::query_as::<_, FavoriteTagRecord>(
        r#"
        SELECT id, favorite_id, tag, source
        FROM favorite_tags
        WHERE id = ?
        "#,
    )
    .bind(id)
    .fetch_one(pool)
    .await?;

    Ok(record)
}

pub async fn delete_tag_by_id(pool: &SqlitePool, id: i64) -> Result<bool, AppError> {
    let result = sqlx::query("DELETE FROM favorite_tags WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;

    Ok(result.rows_affected() > 0)
}

pub async fn list_tags_by_favorite_id(
    pool: &SqlitePool,
    favorite_id: i64,
) -> Result<Vec<FavoriteTagRecord>, AppError> {
    let tags = sqlx::query_as::<_, FavoriteTagRecord>(
        r#"
        SELECT id, favorite_id, tag, source
        FROM favorite_tags
        WHERE favorite_id = ?
        ORDER BY id ASC
        "#,
    )
    .bind(favorite_id)
    .fetch_all(pool)
    .await?;

    Ok(tags)
}
