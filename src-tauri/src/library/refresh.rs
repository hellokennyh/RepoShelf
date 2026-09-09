use chrono::Utc;
use sqlx::SqlitePool;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use tokio::sync::Semaphore;

use super::models::{BatchRefreshProgress, BatchRefreshResult, FavoriteItem, RefreshResult};
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
    refresh_batch_with_progress(pool, github, token, favorite_ids, None).await
}

pub async fn refresh_batch_with_progress(
    pool: &SqlitePool,
    github: Arc<GitHubClient>,
    token: Option<String>,
    favorite_ids: Vec<i64>,
    on_progress: Option<Arc<dyn Fn(BatchRefreshProgress) + Send + Sync>>,
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
    let completed_counter = Arc::new(AtomicU32::new(0));
    let mut tasks = Vec::with_capacity(total);

    for id in favorite_ids {
        let pool_clone = pool.clone();
        let github_clone = Arc::clone(&github);
        let token_clone = token.clone();
        let sem_clone = Arc::clone(&semaphore);
        let rl_flag = Arc::clone(&rate_limit_flag);
        let counter_clone = Arc::clone(&completed_counter);
        let progress_cb = on_progress.clone();

        tasks.push(tokio::spawn(async move {
            let result = if rl_flag.load(Ordering::Relaxed) {
                RefreshResult {
                    favorite_id: id,
                    status: "rate_limited".to_string(),
                    message: Some(
                        "Skipped request because GitHub API rate limit was reached".to_string(),
                    ),
                    favorite: None,
                }
            } else {
                let permit = match sem_clone.acquire().await {
                    Ok(p) => p,
                    Err(_) => {
                        let res = RefreshResult {
                            favorite_id: id,
                            status: "failed".to_string(),
                            message: Some("Semaphore acquisition failed".to_string()),
                            favorite: None,
                        };
                        let count = counter_clone.fetch_add(1, Ordering::SeqCst) + 1;
                        if let Some(ref cb) = progress_cb {
                            cb(BatchRefreshProgress {
                                completed: count.min(total as u32),
                                total: total as u32,
                            });
                        }
                        return res;
                    }
                };

                // Double check after acquiring permit
                if rl_flag.load(Ordering::Relaxed) {
                    drop(permit);
                    RefreshResult {
                        favorite_id: id,
                        status: "rate_limited".to_string(),
                        message: Some(
                            "Skipped request because GitHub API rate limit was reached".to_string(),
                        ),
                        favorite: None,
                    }
                } else {
                    let res =
                        refresh_single(&pool_clone, &github_clone, token_clone.as_deref(), id)
                            .await;
                    drop(permit);

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
                }
            };

            let count = counter_clone.fetch_add(1, Ordering::SeqCst) + 1;
            if let Some(ref cb) = progress_cb {
                cb(BatchRefreshProgress {
                    completed: count.min(total as u32),
                    total: total as u32,
                });
            }

            result
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
                let count = completed_counter.fetch_add(1, Ordering::SeqCst) + 1;
                if let Some(ref cb) = on_progress {
                    cb(BatchRefreshProgress {
                        completed: count.min(total as u32),
                        total: total as u32,
                    });
                }
                results.push(RefreshResult {
                    favorite_id: -1,
                    status: "failed".to_string(),
                    message: Some(format!("Join error: {}", e)),
                    favorite: None,
                });
            }
        }
    }

    let final_completed = completed_counter.load(Ordering::SeqCst);
    if final_completed < total as u32 {
        if let Some(ref cb) = on_progress {
            cb(BatchRefreshProgress {
                completed: total as u32,
                total: total as u32,
            });
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
