use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::Semaphore;

use crate::article::models::ExtractedRepoCandidate;
use crate::error::AppError;
use crate::github::{GitHubClient, GitHubRepository};

const MAX_VALIDATION_CONCURRENCY: usize = 4;

pub struct ValidationOutcome {
    pub repositories: Vec<GitHubRepository>,
    pub valid_count: usize,
    pub invalid_count: usize,
    pub skipped_count: usize,
    pub rate_limited: bool,
}

pub async fn validate_candidates(
    github: &Arc<GitHubClient>,
    token: Option<&str>,
    candidates: &[ExtractedRepoCandidate],
) -> ValidationOutcome {
    let semaphore = Arc::new(Semaphore::new(MAX_VALIDATION_CONCURRENCY));
    let rate_limited = Arc::new(AtomicBool::new(false));

    let mut tasks = Vec::with_capacity(candidates.len());

    for (idx, candidate) in candidates.iter().enumerate() {
        let github = Arc::clone(github);
        let token_opt = token.map(|s| s.to_string());
        let owner = candidate.owner.clone();
        let repo = candidate.repo.clone();
        let sem = Arc::clone(&semaphore);
        let rl = Arc::clone(&rate_limited);

        tasks.push(tokio::spawn(async move {
            if rl.load(Ordering::SeqCst) {
                return (
                    idx,
                    Err(AppError::GitHubRateLimit {
                        reset_at: None,
                        message: "Rate limit reached in earlier candidate".to_string(),
                    }),
                );
            }

            let _permit = match sem.acquire().await {
                Ok(p) => p,
                Err(_) => {
                    return (idx, Err(AppError::Unknown("Semaphore closed".to_string())));
                }
            };

            if rl.load(Ordering::SeqCst) {
                return (
                    idx,
                    Err(AppError::GitHubRateLimit {
                        reset_at: None,
                        message: "Rate limit reached in earlier candidate".to_string(),
                    }),
                );
            }

            // Fetch repository base metadata
            let repo_res = github
                .get_repository(&owner, &repo, token_opt.as_deref())
                .await;

            match repo_res {
                Ok(mut repo_model) => {
                    // Enrich with S2 Releases count & Languages
                    let releases = github
                        .get_repository_releases_count(&owner, &repo, token_opt.as_deref())
                        .await
                        .ok();
                    let languages = github
                        .get_repository_languages(&owner, &repo, token_opt.as_deref())
                        .await
                        .ok();

                    repo_model.releases = releases;
                    repo_model.languages = languages;

                    (idx, Ok(repo_model))
                }
                Err(err) => {
                    if let AppError::GitHubRateLimit { .. } = &err {
                        rl.store(true, Ordering::SeqCst);
                    }
                    (idx, Err(err))
                }
            }
        }));
    }

    let mut results = vec![None; candidates.len()];
    let mut invalid_count = 0;
    let mut skipped_count = 0;

    for task in tasks {
        if let Ok((idx, res)) = task.await {
            match res {
                Ok(repo) => {
                    results[idx] = Some(repo);
                }
                Err(AppError::GitHubRateLimit { .. }) => {
                    skipped_count += 1;
                }
                Err(_) => {
                    invalid_count += 1;
                }
            }
        } else {
            invalid_count += 1;
        }
    }

    let was_rate_limited = rate_limited.load(Ordering::SeqCst);
    let repositories: Vec<GitHubRepository> = results.into_iter().flatten().collect();
    let valid_count = repositories.len();

    ValidationOutcome {
        repositories,
        valid_count,
        invalid_count,
        skipped_count,
        rate_limited: was_rate_limited,
    }
}
