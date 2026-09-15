use sqlx::SqlitePool;

use super::models::{FavoriteRecord, NewFavoriteRecord};
use crate::error::AppError;

pub async fn insert_favorite(
    pool: &SqlitePool,
    fav: &NewFavoriteRecord,
) -> Result<FavoriteRecord, AppError> {
    let category = fav.category.as_deref().unwrap_or("Other");
    let stars = fav.stars.unwrap_or(0);
    let notes = fav.notes.as_deref().unwrap_or("");

    let result = sqlx::query(
        r#"
        INSERT INTO favorites (
            github_repo_id,
            github_url,
            name,
            description,
            category,
            stars,
            license,
            pushed_at,
            rating,
            rating_reason,
            rating_confidence,
            notes,
            favorited_at,
            metadata_refreshed_at,
            releases,
            languages,
            description_translations,
            description_translation_source_hash
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(fav.github_repo_id)
    .bind(&fav.github_url)
    .bind(&fav.name)
    .bind(&fav.description)
    .bind(category)
    .bind(stars)
    .bind(&fav.license)
    .bind(&fav.pushed_at)
    .bind(fav.rating)
    .bind(&fav.rating_reason)
    .bind(&fav.rating_confidence)
    .bind(notes)
    .bind(&fav.favorited_at)
    .bind(&fav.metadata_refreshed_at)
    .bind(fav.releases)
    .bind(&fav.languages)
    .bind(&fav.description_translations)
    .bind(&fav.description_translation_source_hash)
    .execute(pool)
    .await?;

    let id = result.last_insert_rowid();

    let record = find_favorite_by_id(pool, id)
        .await?
        .ok_or_else(|| AppError::Database("Inserted favorite not found".to_string()))?;

    Ok(record)
}

pub async fn find_favorite_by_id(
    pool: &SqlitePool,
    id: i64,
) -> Result<Option<FavoriteRecord>, AppError> {
    let fav = sqlx::query_as::<_, FavoriteRecord>(
        r#"
        SELECT
            id,
            github_repo_id,
            github_url,
            name,
            description,
            category,
            stars,
            license,
            pushed_at,
            rating,
            rating_reason,
            rating_confidence,
            notes,
            favorited_at,
            metadata_refreshed_at,
            releases,
            languages,
            description_translations,
            description_translation_source_hash
        FROM favorites
        WHERE id = ?
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    Ok(fav)
}

pub async fn find_favorite_by_repo_id(
    pool: &SqlitePool,
    repo_id: i64,
) -> Result<Option<FavoriteRecord>, AppError> {
    let fav = sqlx::query_as::<_, FavoriteRecord>(
        r#"
        SELECT
            id,
            github_repo_id,
            github_url,
            name,
            description,
            category,
            stars,
            license,
            pushed_at,
            rating,
            rating_reason,
            rating_confidence,
            notes,
            favorited_at,
            metadata_refreshed_at,
            releases,
            languages,
            description_translations,
            description_translation_source_hash
        FROM favorites
        WHERE github_repo_id = ?
        "#,
    )
    .bind(repo_id)
    .fetch_optional(pool)
    .await?;

    Ok(fav)
}

pub async fn find_favorite_by_url(
    pool: &SqlitePool,
    url: &str,
) -> Result<Option<FavoriteRecord>, AppError> {
    let fav = sqlx::query_as::<_, FavoriteRecord>(
        r#"
        SELECT
            id,
            github_repo_id,
            github_url,
            name,
            description,
            category,
            stars,
            license,
            pushed_at,
            rating,
            rating_reason,
            rating_confidence,
            notes,
            favorited_at,
            metadata_refreshed_at,
            releases,
            languages,
            description_translations,
            description_translation_source_hash
        FROM favorites
        WHERE github_url = ?
        "#,
    )
    .bind(url)
    .fetch_optional(pool)
    .await?;

    Ok(fav)
}

pub async fn delete_favorite_by_id(pool: &SqlitePool, id: i64) -> Result<bool, AppError> {
    let result = sqlx::query("DELETE FROM favorites WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;

    Ok(result.rows_affected() > 0)
}
