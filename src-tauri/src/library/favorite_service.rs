use sqlx::SqlitePool;

use super::models::{
    BatchFavoriteItemResult, BatchFavoriteResult, FavoriteInput, FavoriteItem, FavoriteStatus,
    LibraryQuery, UpdateFavoriteUserMetadataInput,
};
use super::repository::{
    delete_favorite_by_id, find_favorite_by_id, find_favorite_by_identity, insert_favorite_tx,
    list_distinct_tags, list_favorites, update_user_metadata_tx, validate_category,
};
use crate::error::AppError;
use crate::github::parse_github_repo_url;

pub async fn check_favorite_status(
    pool: &SqlitePool,
    repo_id: Option<i64>,
    raw_url: &str,
) -> Result<FavoriteStatus, AppError> {
    let existing = find_favorite_by_identity(pool, repo_id, raw_url).await?;
    match existing {
        Some(fav) => Ok(FavoriteStatus {
            is_favorited: true,
            favorite_id: Some(fav.id),
        }),
        None => Ok(FavoriteStatus {
            is_favorited: false,
            favorite_id: None,
        }),
    }
}

pub async fn favorite_repository(
    pool: &SqlitePool,
    input: FavoriteInput,
) -> Result<FavoriteItem, AppError> {
    let trimmed_name = input.name.trim();
    if trimmed_name.is_empty() {
        return Err(AppError::InvalidFavoriteInput(
            "Repository name cannot be empty".to_string(),
        ));
    }

    validate_category(&input.category)?;
    parse_github_repo_url(&input.github_url)?;

    // Duplicate detection: Priority 1: repo_id, Priority 2: canonical URL
    if let Some(existing) =
        find_favorite_by_identity(pool, input.github_repo_id, &input.github_url).await?
    {
        return Err(AppError::FavoriteAlreadyExists {
            favorite_id: existing.id,
        });
    }

    // Atomic insert transaction with fallback to catch database constraint
    match insert_favorite_tx(pool, &input).await {
        Ok(fav) => Ok(fav),
        Err(AppError::Database(ref msg)) if msg.contains("UNIQUE constraint failed") => {
            if let Some(existing) =
                find_favorite_by_identity(pool, input.github_repo_id, &input.github_url).await?
            {
                Err(AppError::FavoriteAlreadyExists {
                    favorite_id: existing.id,
                })
            } else {
                Err(AppError::Database(msg.clone()))
            }
        }
        Err(e) => Err(e),
    }
}

pub async fn favorite_repositories_batch(
    pool: &SqlitePool,
    inputs: Vec<FavoriteInput>,
) -> Result<BatchFavoriteResult, AppError> {
    let mut succeeded = 0;
    let mut already_exists = 0;
    let mut failed = 0;
    let mut items = Vec::with_capacity(inputs.len());

    for input in inputs {
        let url = input.github_url.clone();
        match favorite_repository(pool, input).await {
            Ok(fav) => {
                succeeded += 1;
                items.push(BatchFavoriteItemResult {
                    github_url: url,
                    status: "succeeded".to_string(),
                    favorite_id: Some(fav.id),
                    error: None,
                });
            }
            Err(AppError::FavoriteAlreadyExists { favorite_id }) => {
                already_exists += 1;
                items.push(BatchFavoriteItemResult {
                    github_url: url,
                    status: "already_exists".to_string(),
                    favorite_id: Some(favorite_id),
                    error: None,
                });
            }
            Err(e) => {
                failed += 1;
                items.push(BatchFavoriteItemResult {
                    github_url: url,
                    status: "failed".to_string(),
                    favorite_id: None,
                    error: Some(e.to_string()),
                });
            }
        }
    }

    Ok(BatchFavoriteResult {
        succeeded,
        already_exists,
        failed,
        items,
    })
}

pub async fn get_favorite(pool: &SqlitePool, id: i64) -> Result<FavoriteItem, AppError> {
    find_favorite_by_id(pool, id)
        .await?
        .ok_or_else(|| AppError::FavoriteNotFound(format!("Favorite {} not found", id)))
}

pub async fn get_favorites(
    pool: &SqlitePool,
    query: LibraryQuery,
) -> Result<Vec<FavoriteItem>, AppError> {
    list_favorites(pool, &query).await
}

pub async fn get_distinct_tags(pool: &SqlitePool) -> Result<Vec<String>, AppError> {
    list_distinct_tags(pool).await
}

pub async fn update_favorite_metadata(
    pool: &SqlitePool,
    input: UpdateFavoriteUserMetadataInput,
) -> Result<FavoriteItem, AppError> {
    update_user_metadata_tx(pool, &input).await
}

pub async fn delete_favorite(pool: &SqlitePool, id: i64) -> Result<bool, AppError> {
    delete_favorite_by_id(pool, id).await
}
