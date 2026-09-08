use sqlx::SqlitePool;
use std::collections::{HashMap, HashSet};

use crate::ai::{
    check_ai_search_readiness, create_provider_client, generate_search_queries,
    validate_ai_search_input, AiProviderClient, AiRepositorySearchRequest,
    AiRepositorySearchResponse,
};
use crate::db::selection_history::insert_selection_history_batch;
use crate::error::AppError;
use crate::github::{
    build_search_query, parse_github_repo_url, GitHubClient, GitHubRepository,
    RepositorySearchRequest, RepositorySearchResponse,
};
use crate::secrets::{SecretStore, SECRET_KEY_AI_API_KEY};
use crate::settings::{AiProvider, SettingsStore};

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

fn format_query_with_filters(
    base: &str,
    min_stars: Option<i64>,
    pushed_after: Option<&str>,
) -> String {
    let mut parts = vec![base.trim().to_string()];
    if let Some(stars) = min_stars {
        if stars > 0 {
            parts.push(format!("stars:>={}", stars));
        }
    }
    if let Some(date_str) = pushed_after {
        let trimmed = date_str.trim();
        if !trimmed.is_empty() {
            parts.push(format!("pushed:>={}", trimmed));
        }
    }
    parts.join(" ")
}

struct CandidateEntry {
    repo: GitHubRepository,
    canonical_url: String,
    rrf_score: f64,
    first_seen: usize,
}

pub async fn execute_ai_search_with_client(
    pool: &SqlitePool,
    client: &dyn AiProviderClient,
    github: &GitHubClient,
    token: Option<&str>,
    request: &AiRepositorySearchRequest,
) -> Result<AiRepositorySearchResponse, AppError> {
    let valid_input = validate_ai_search_input(request)?;

    let generated_queries = generate_search_queries(client, &valid_input).await?;

    let candidate_budget = (request.limit * 3).clamp(100, 300);
    let mut remaining_budget = candidate_budget;
    let mut partial = false;
    let k = generated_queries.len();
    let mut results_per_query: Vec<Vec<GitHubRepository>> = Vec::new();

    for (i, q) in generated_queries.iter().enumerate() {
        if remaining_budget == 0 {
            partial = true;
            break;
        }

        let queries_left = (k - i) as u32;
        let query_target = (remaining_budget / queries_left).clamp(1, 100);
        let formatted_query =
            format_query_with_filters(q, request.min_stars, request.pushed_after.as_deref());

        let page_result = github
            .search_repositories_page(&formatted_query, 1, query_target, token)
            .await;

        match page_result {
            Ok(res) => {
                let count = res.items.len() as u32;
                remaining_budget = remaining_budget.saturating_sub(count);
                results_per_query.push(res.items);
            }
            Err(e) => {
                let total_collected: usize = results_per_query.iter().map(|v| v.len()).sum();
                if total_collected == 0 {
                    return Err(e);
                }
                match e {
                    AppError::GitHubRateLimit { .. }
                    | AppError::GitHubTimeout(_)
                    | AppError::GitHubNetwork(_) => {
                        eprintln!("AI search query {} degraded: {}", i, e);
                        partial = true;
                        break;
                    }
                    _ => return Err(e),
                }
            }
        }
    }

    // Deduplicate across queries and compute RRF scores
    let mut entries: Vec<CandidateEntry> = Vec::new();
    let mut seen_repo_ids: HashMap<i64, usize> = HashMap::new();
    let mut seen_canonical_urls: HashMap<String, usize> = HashMap::new();
    let mut seen_counter: usize = 0;

    for query_items in results_per_query {
        for (rank_0, item) in query_items.into_iter().enumerate() {
            let rank = rank_0 + 1;
            let score_contrib = 1.0 / (60.0 + rank as f64);
            let canonical_url = match parse_github_repo_url(&item.github_url) {
                Ok((_, _, u)) => u,
                Err(_) => item.github_url.clone(),
            };

            let existing_idx = seen_repo_ids
                .get(&item.github_repo_id)
                .copied()
                .or_else(|| seen_canonical_urls.get(&canonical_url).copied());

            if let Some(idx) = existing_idx {
                entries[idx].rrf_score += score_contrib;
                seen_repo_ids.insert(item.github_repo_id, idx);
                seen_canonical_urls.insert(canonical_url, idx);
            } else {
                let idx = entries.len();
                seen_repo_ids.insert(item.github_repo_id, idx);
                seen_canonical_urls.insert(canonical_url.clone(), idx);
                entries.push(CandidateEntry {
                    repo: item,
                    canonical_url,
                    rrf_score: score_contrib,
                    first_seen: seen_counter,
                });
                seen_counter += 1;
            }
        }
    }

    // Sort entries by RRF score descending, stars descending, then first-seen
    entries.sort_by(|a, b| match b.rrf_score.partial_cmp(&a.rrf_score) {
        Some(std::cmp::Ordering::Equal) | None => match b.repo.stars.cmp(&a.repo.stars) {
            std::cmp::Ordering::Equal => a.first_seen.cmp(&b.first_seen),
            other => other,
        },
        Some(other) => other,
    });

    // Exclusions
    let candidate_ids: Vec<i64> = entries.iter().map(|e| e.repo.github_repo_id).collect();
    let candidate_urls: Vec<String> = entries.iter().map(|e| e.canonical_url.clone()).collect();

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

    let (fav_repo_ids, fav_urls) = if request.exclude_favorites {
        crate::library::repository::find_favorited_identities(pool, &candidate_ids, &candidate_urls)
            .await?
    } else {
        (HashSet::new(), HashSet::new())
    };

    let mut final_items: Vec<GitHubRepository> = Vec::new();
    for entry in entries {
        if request.exclude_selected
            && (selected_repo_ids.contains(&entry.repo.github_repo_id)
                || selected_urls.contains(&entry.canonical_url))
        {
            continue;
        }

        if request.exclude_favorites
            && (fav_repo_ids.contains(&entry.repo.github_repo_id)
                || fav_urls.contains(&entry.canonical_url))
        {
            continue;
        }

        final_items.push(entry.repo);
        if final_items.len() == request.limit as usize {
            break;
        }
    }

    if !final_items.is_empty() {
        insert_selection_history_batch(pool, &final_items).await?;
    }

    Ok(AiRepositorySearchResponse {
        items: final_items,
        generated_queries,
        partial,
    })
}

pub async fn execute_ai_search(
    pool: &SqlitePool,
    settings_store: &SettingsStore,
    secret_store: &dyn SecretStore,
    github: &GitHubClient,
    token: Option<&str>,
    request: &AiRepositorySearchRequest,
) -> Result<AiRepositorySearchResponse, AppError> {
    let settings = settings_store.get()?;
    let readiness = check_ai_search_readiness(&settings, secret_store);
    if !readiness.ready {
        return Err(AppError::SettingsValidation(
            readiness
                .reason
                .unwrap_or_else(|| "AI search is not configured".to_string()),
        ));
    }

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
    execute_ai_search_with_client(pool, &*client, github, token, request).await
}
