use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::Semaphore;

use crate::github::url::parse_github_repo_url;
use crate::github::{GitHubClient, GitHubRepository};

pub const MAX_METADATA_ENRICHMENT_CONCURRENCY: usize = 4;

/// Enriches a batch of GitHubRepository items with Releases count and Languages breakdown.
///
/// Ensures:
/// 1. Deduplication within the batch: identical canonical repos are fetched only once.
/// 2. Bounded concurrency: at most MAX_METADATA_ENRICHMENT_CONCURRENCY tasks.
/// 3. Rate limit protection: halts further enrichment immediately if rate limit is encountered.
/// 4. Graceful degradation: failure to fetch metadata leaves fields as None without breaking the batch.
pub async fn enrich_repositories(
    github: &GitHubClient,
    items: &mut [GitHubRepository],
    token: Option<&str>,
) {
    if items.is_empty() {
        return;
    }

    // 1. Group item indices by canonical (owner, repo)
    let mut repo_groups: HashMap<(String, String), Vec<usize>> = HashMap::new();
    for (idx, item) in items.iter().enumerate() {
        if let Ok((owner, repo, _)) = parse_github_repo_url(&item.github_url) {
            repo_groups
                .entry((owner.to_lowercase(), repo.to_lowercase()))
                .or_default()
                .push(idx);
        }
    }

    if repo_groups.is_empty() {
        return;
    }

    let semaphore = Arc::new(Semaphore::new(MAX_METADATA_ENRICHMENT_CONCURRENCY));
    let rate_limited = Arc::new(AtomicBool::new(false));

    let mut handles = Vec::with_capacity(repo_groups.len());

    for ((owner, repo), indices) in repo_groups {
        let sem = Arc::clone(&semaphore);
        let rl_flag = Arc::clone(&rate_limited);
        let gh = github.clone();
        let tok = token.map(|t| t.to_string());

        handles.push(tauri::async_runtime::spawn(async move {
            if rl_flag.load(Ordering::SeqCst) {
                return (indices, None, None);
            }

            let _permit = match sem.acquire().await {
                Ok(p) => p,
                Err(_) => return (indices, None, None),
            };

            if rl_flag.load(Ordering::SeqCst) {
                return (indices, None, None);
            }

            let token_ref = tok.as_deref();

            // Fetch releases count
            let releases = match gh
                .get_repository_releases_count(&owner, &repo, token_ref)
                .await
            {
                Ok(c) => Some(c),
                Err(crate::error::AppError::GitHubRateLimit { .. }) => {
                    rl_flag.store(true, Ordering::SeqCst);
                    None
                }
                Err(_) => None,
            };

            // Fetch languages if not rate-limited
            let languages = if rl_flag.load(Ordering::SeqCst) {
                None
            } else {
                match gh.get_repository_languages(&owner, &repo, token_ref).await {
                    Ok(langs) => Some(langs),
                    Err(crate::error::AppError::GitHubRateLimit { .. }) => {
                        rl_flag.store(true, Ordering::SeqCst);
                        None
                    }
                    Err(_) => None,
                }
            };

            (indices, releases, languages)
        }));
    }

    // 2. Populate enriched results back to items
    for handle in handles {
        if let Ok((indices, releases, languages)) = handle.await {
            for idx in indices {
                if let Some(item) = items.get_mut(idx) {
                    item.releases = releases;
                    item.languages = languages.clone();
                }
            }
        }
    }
}
