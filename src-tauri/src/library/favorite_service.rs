use sqlx::SqlitePool;

use super::models::{
    BatchFavoriteIdentitiesResult, BatchFavoriteItemResult, BatchFavoriteResult, FavoriteInput,
    FavoriteItem, FavoriteStatus, LibraryQuery, UpdateFavoriteUserMetadataInput,
};
use super::repository::{
    apply_favorite_ai_metadata_tx, delete_favorite_by_id, find_favorite_by_id,
    find_favorite_by_identity, find_favorited_identities, insert_favorite_tx, list_distinct_tags,
    list_favorites, update_user_metadata_tx, validate_category,
};

use crate::ai::{
    check_ai_search_readiness, create_provider_client, generate_favorite_suggestion,
    AiFavoriteMetadataSuggestion, AiProviderClient,
};
use crate::error::AppError;
use crate::github::parse_github_repo_url;
use crate::secrets::{SecretStore, SECRET_KEY_AI_API_KEY};
use crate::settings::{AiProvider, SettingsStore};

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

pub async fn check_favorite_identities_batch(
    pool: &SqlitePool,
    repo_ids: &[i64],
    raw_urls: &[String],
) -> Result<BatchFavoriteIdentitiesResult, AppError> {
    let mut canonical_urls = Vec::with_capacity(raw_urls.len());
    for url in raw_urls {
        if let Ok((owner, repo, _)) = parse_github_repo_url(url) {
            canonical_urls.push(format!("https://github.com/{}/{}", owner, repo));
        } else {
            canonical_urls.push(url.clone());
        }
    }

    let (matched_ids, matched_urls) =
        find_favorited_identities(pool, repo_ids, &canonical_urls).await?;

    Ok(BatchFavoriteIdentitiesResult {
        matched_repo_ids: matched_ids.into_iter().collect(),
        matched_urls: matched_urls.into_iter().collect(),
    })
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

pub async fn suggest_favorite_ai_metadata_with_client(
    pool: &SqlitePool,
    client: &dyn AiProviderClient,
    favorite_id: i64,
) -> Result<AiFavoriteMetadataSuggestion, AppError> {
    // 1. Verify existence before AI call
    let favorite = find_favorite_by_id(pool, favorite_id)
        .await?
        .ok_or_else(|| {
            AppError::FavoriteNotFound(format!("Favorite with id {} not found", favorite_id))
        })?;

    // 2. Filter ONLY source='github' topics
    let github_topics: Vec<String> = favorite
        .tags
        .iter()
        .filter(|t| t.source == "github")
        .map(|t| t.tag.clone())
        .collect();

    // 3. Generate suggestion
    generate_favorite_suggestion(
        client,
        &favorite.name,
        favorite.description.as_deref(),
        favorite.stars,
        favorite.license.as_deref(),
        favorite.pushed_at.as_deref(),
        &github_topics,
    )
    .await
}

pub async fn suggest_favorite_ai_metadata(
    pool: &SqlitePool,
    settings_store: &SettingsStore,
    secret_store: &dyn SecretStore,
    favorite_id: i64,
) -> Result<AiFavoriteMetadataSuggestion, AppError> {
    // 1. Check readiness
    let settings = settings_store.get()?;
    let readiness = check_ai_search_readiness(&settings, secret_store);
    if !readiness.ready {
        return Err(AppError::SettingsValidation(
            readiness
                .reason
                .unwrap_or_else(|| "AI is not ready".to_string()),
        ));
    }

    // 2. Verify favorite existence BEFORE client creation or AI call
    let favorite = find_favorite_by_id(pool, favorite_id)
        .await?
        .ok_or_else(|| {
            AppError::FavoriteNotFound(format!("Favorite with id {} not found", favorite_id))
        })?;

    let provider = match settings.ai.provider {
        Some(p) => p,
        None => {
            return Err(AppError::SettingsValidation(
                "No AI provider configured".to_string(),
            ))
        }
    };

    let model = match settings
        .ai
        .model
        .as_deref()
        .map(str::trim)
        .filter(|m| !m.is_empty())
    {
        Some(m) => m,
        None => return Err(AppError::AIModel("No AI model configured".to_string())),
    };

    let base_url = settings.ai.base_url.as_deref();
    let api_key = if provider == AiProvider::Ollama {
        None
    } else {
        secret_store.get_secret(SECRET_KEY_AI_API_KEY)?
    };

    let client = create_provider_client(provider, base_url, model, api_key.as_deref(), None)?;

    let github_topics: Vec<String> = favorite
        .tags
        .iter()
        .filter(|t| t.source == "github")
        .map(|t| t.tag.clone())
        .collect();

    generate_favorite_suggestion(
        &*client,
        &favorite.name,
        favorite.description.as_deref(),
        favorite.stars,
        favorite.license.as_deref(),
        favorite.pushed_at.as_deref(),
        &github_topics,
    )
    .await
}

pub async fn apply_favorite_ai_metadata(
    pool: &SqlitePool,
    favorite_id: i64,
    category: String,
    tags: Vec<String>,
) -> Result<FavoriteItem, AppError> {
    apply_favorite_ai_metadata_tx(pool, favorite_id, &category, &tags).await
}
