use sqlx::SqlitePool;
use std::collections::HashSet;

use crate::db::selection_history::insert_selection_history_batch;
use crate::error::AppError;
use crate::github::{
    build_search_query, parse_github_repo_url, GitHubClient, GitHubRepository,
    RepositorySearchRequest, RepositorySearchResponse,
};

/// Orchestrates repository search with optional exclusions, bounded candidate replenishment,
/// cross-page deduplication, and atomic selection history recording.
pub async fn execute_search(
    pool: &SqlitePool,
    github: &GitHubClient,
    token: Option<&str>,
    request: &RepositorySearchRequest,
) -> Result<RepositorySearchResponse, AppError> {
    let formatted_query = build_search_query(request)?;

    let has_exclusions = request.exclude_selected || request.exclude_favorites;

    // Phase 2 no-exclusion path: single request, no auto-pagination
    if !has_exclusions {
        let resp = github.search_repositories(request, token).await?;
        if !resp.items.is_empty() {
            insert_selection_history_batch(pool, &resp.items).await?;
        }
        return Ok(resp);
    }

    // Bounded replenishment path:
    // candidate_budget = min(max(limit * 3, 100), 300)
    let candidate_budget = (request.limit * 3).clamp(100, 300);
    let (page_size, max_pages) = if candidate_budget == 100 {
        (100u32, 1u32)
    } else {
        (request.limit, 3u32)
    };

    let mut page: u32 = 1;
    let mut final_items: Vec<GitHubRepository> = Vec::new();
    let mut total_count: u64 = 0;
    let mut seen_repo_ids: HashSet<i64> = HashSet::new();
    let mut seen_canonical_urls: HashSet<String> = HashSet::new();

    while page <= max_pages && final_items.len() < request.limit as usize {
        let page_result = github
            .search_repositories_page(&formatted_query, page, page_size, token)
            .await;

        let page_resp = match page_result {
            Ok(resp) => resp,
            Err(e) => {
                if page == 1 {
                    return Err(e);
                }
                // Later-page failure degradation semantics:
                match e {
                    AppError::GitHubRateLimit { .. }
                    | AppError::GitHubTimeout(_)
                    | AppError::GitHubNetwork(_)
                        if !final_items.is_empty() =>
                    {
                        eprintln!("Search replenishment degraded on page {}: {}", page, e);
                        break;
                    }
                    _ => return Err(e),
                }
            }
        };

        if page == 1 {
            total_count = page_resp.total_count;
        }

        let raw_item_count = page_resp.raw_item_count;
        let page_items = page_resp.items;

        if raw_item_count == 0 {
            break;
        }

        // 1. Cross-page defensive deduplication (repo ID first, canonical URL fallback)
        let mut deduped_candidates: Vec<(GitHubRepository, String)> = Vec::new();
        for candidate in page_items {
            let canonical_url = match parse_github_repo_url(&candidate.github_url) {
                Ok((_, _, canonical)) => canonical,
                Err(_) => candidate.github_url.clone(),
            };

            if seen_repo_ids.contains(&candidate.github_repo_id)
                || seen_canonical_urls.contains(&canonical_url)
            {
                continue;
            }

            seen_repo_ids.insert(candidate.github_repo_id);
            seen_canonical_urls.insert(canonical_url.clone());
            deduped_candidates.push((candidate, canonical_url));
        }

        if deduped_candidates.is_empty() {
            if raw_item_count < page_size as usize {
                break;
            }
            let requested_through = (page as u64).saturating_mul(page_size as u64);
            if requested_through >= total_count {
                break;
            }
            page += 1;
            continue;
        }

        // Collect candidate IDs and canonical URLs for bulk DB checks
        let candidate_ids: Vec<i64> = deduped_candidates
            .iter()
            .map(|(c, _)| c.github_repo_id)
            .collect();
        let candidate_urls: Vec<String> = deduped_candidates
            .iter()
            .map(|(_, url)| url.clone())
            .collect();

        // 2. Exclude previously selected (against selection_history)
        let (selected_repo_ids, selected_urls) = if request.exclude_selected {
            crate::db::selection_history::find_selected_identities(
                pool,
                &candidate_ids,
                &candidate_urls,
            )
            .await?
        } else {
            (HashSet::new(), HashSet::new())
        };

        // 3. Exclude favorites (against favorites)
        let (fav_repo_ids, fav_urls) = if request.exclude_favorites {
            crate::library::repository::find_favorited_identities(
                pool,
                &candidate_ids,
                &candidate_urls,
            )
            .await?
        } else {
            (HashSet::new(), HashSet::new())
        };

        // 4. Filter candidates and append survivors up to limit
        for (candidate, canonical_url) in deduped_candidates {
            if request.exclude_selected
                && (selected_repo_ids.contains(&candidate.github_repo_id)
                    || selected_urls.contains(&canonical_url))
            {
                continue;
            }

            if request.exclude_favorites
                && (fav_repo_ids.contains(&candidate.github_repo_id)
                    || fav_urls.contains(&canonical_url))
            {
                continue;
            }

            final_items.push(candidate);
            if final_items.len() == request.limit as usize {
                break;
            }
        }

        if final_items.len() == request.limit as usize {
            break;
        }

        if raw_item_count < page_size as usize {
            break;
        }

        let requested_through = (page as u64).saturating_mul(page_size as u64);
        if requested_through >= total_count {
            break;
        }

        page += 1;
    }

    // Atomic history transaction for final displayed items
    if !final_items.is_empty() {
        insert_selection_history_batch(pool, &final_items).await?;
    }

    Ok(RepositorySearchResponse {
        items: final_items,
        total_count,
    })
}
