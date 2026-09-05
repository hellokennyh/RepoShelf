use chrono::Utc;
use sqlx::SqlitePool;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::Semaphore;

use super::models::{BatchRefreshResult, FavoriteItem, RefreshResult};
use super::repository::{find_favorite_by_id, update_github_owned_metadata_tx};
use crate::error::AppError;
use crate::github::{parse_github_repo_url, GitHubClient};

pub const MAX_REFRESH_CONCURRENCY: usize = 4;

pub async fn refresh_single(
    pool: &SqlitePool,
    github: &GitHubClient,
    token: Option<&str>,
    favorite_id: i64,
) -> Result<RefreshResult, AppError> {
    let current_fav: FavoriteItem = match find_favorite_by_id(pool, favorite_id).await? {
        Some(f) => f,
        None => {
            return Err(AppError::FavoriteNotFound(format!(
                "Favorite {} not found",
                favorite_id
            )));
        }
    };

    let (owner, repo, _) = match parse_github_repo_url(&current_fav.github_url) {
        Ok(res) => res,
        Err(e) => {
            return Ok(RefreshResult {
                favorite_id,
                status: "failed".to_string(),
                message: Some(format!("Invalid GitHub URL: {}", e)),
                favorite: None,
            });
        }
    };

    let fetch_result = github.get_repository(&owner, &repo, token).await;

    match fetch_result {
        Ok(new_repo) => {
            let now_utc = Utc::now().to_rfc3339();
            match update_github_owned_metadata_tx(pool, favorite_id, &new_repo, &now_utc).await {
                Ok(updated) => Ok(RefreshResult {
                    favorite_id,
                    status: "succeeded".to_string(),
                    message: None,
                    favorite: Some(updated),
                }),
                Err(AppError::FavoriteIdentityConflict(msg)) => Ok(RefreshResult {
                    favorite_id,
                    status: "conflict".to_string(),
                    message: Some(msg),
                    favorite: None,
                }),
                Err(e) => Ok(RefreshResult {
                    favorite_id,
                    status: "failed".to_string(),
                    message: Some(e.to_string()),
                    favorite: None,
                }),
            }
        }
        Err(AppError::GitHubNotFound(msg)) => Ok(RefreshResult {
            favorite_id,
            status: "not_found".to_string(),
            message: Some(msg),
            favorite: None,
        }),
        Err(AppError::GitHubRateLimit { message, .. }) => Ok(RefreshResult {
            favorite_id,
            status: "rate_limited".to_string(),
            message: Some(message),
            favorite: None,
        }),
        Err(e) => Ok(RefreshResult {
            favorite_id,
            status: "failed".to_string(),
            message: Some(e.to_string()),
            favorite: None,
        }),
    }
}

pub async fn refresh_batch(
    pool: &SqlitePool,
    github: Arc<GitHubClient>,
    token: Option<String>,
    favorite_ids: Vec<i64>,
) -> Result<BatchRefreshResult, AppError> {
    let total = favorite_ids.len();
    if total == 0 {
        return Ok(BatchRefreshResult {
            total: 0,
            succeeded: 0,
            failed: 0,
            rate_limited: 0,
            not_found: 0,
            results: Vec::new(),
        });
    }

    let semaphore = Arc::new(Semaphore::new(MAX_REFRESH_CONCURRENCY));
    let rate_limit_flag = Arc::new(AtomicBool::new(false));
    let mut tasks = Vec::with_capacity(total);

    for id in favorite_ids {
        let pool_clone = pool.clone();
        let github_clone = Arc::clone(&github);
        let token_clone = token.clone();
        let sem_clone = Arc::clone(&semaphore);
        let rl_flag = Arc::clone(&rate_limit_flag);

        tasks.push(tokio::spawn(async move {
            // If primary rate limit was previously hit, stop starting queued requests
            if rl_flag.load(Ordering::Relaxed) {
                return RefreshResult {
                    favorite_id: id,
                    status: "rate_limited".to_string(),
                    message: Some(
                        "Skipped request because GitHub API rate limit was reached".to_string(),
                    ),
                    favorite: None,
                };
            }

            let _permit = match sem_clone.acquire().await {
                Ok(p) => p,
                Err(_) => {
                    return RefreshResult {
                        favorite_id: id,
                        status: "failed".to_string(),
                        message: Some("Semaphore acquisition failed".to_string()),
                        favorite: None,
                    };
                }
            };

            // Double check after acquiring permit
            if rl_flag.load(Ordering::Relaxed) {
                return RefreshResult {
                    favorite_id: id,
                    status: "rate_limited".to_string(),
                    message: Some(
                        "Skipped request because GitHub API rate limit was reached".to_string(),
                    ),
                    favorite: None,
                };
            }

            let res = refresh_single(&pool_clone, &github_clone, token_clone.as_deref(), id).await;

            match res {
                Ok(refresh_res) => {
                    if refresh_res.status == "rate_limited" {
                        rl_flag.store(true, Ordering::Relaxed);
                    }
                    refresh_res
                }
                Err(e) => RefreshResult {
                    favorite_id: id,
                    status: "failed".to_string(),
                    message: Some(e.to_string()),
                    favorite: None,
                },
            }
        }));
    }

    let mut succeeded = 0;
    let mut failed = 0;
    let mut rate_limited = 0;
    let mut not_found = 0;
    let mut results = Vec::with_capacity(total);

    for t in tasks {
        match t.await {
            Ok(item) => {
                match item.status.as_str() {
                    "succeeded" => succeeded += 1,
                    "not_found" => not_found += 1,
                    "rate_limited" => rate_limited += 1,
                    _ => failed += 1,
                }
                results.push(item);
            }
            Err(e) => {
                failed += 1;
                results.push(RefreshResult {
                    favorite_id: -1,
                    status: "failed".to_string(),
                    message: Some(format!("Join error: {}", e)),
                    favorite: None,
                });
            }
        }
    }

    Ok(BatchRefreshResult {
        total,
        succeeded,
        failed,
        rate_limited,
        not_found,
        results,
    })
}
