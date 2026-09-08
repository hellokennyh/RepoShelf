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

/// Computes static candidate budgets for generated queries.
/// Formula: candidate_budget = min(max(limit * 3, 100), 300)
/// base = candidate_budget / N, remainder = candidate_budget % N
/// The first `remainder` queries get `base + 1`, and the rest get `base`.
pub fn calculate_query_budgets(candidate_budget: u32, query_count: usize) -> Vec<u32> {
    if query_count == 0 {
        return Vec::new();
    }
    let n = query_count as u32;
    let base = candidate_budget / n;
    let remainder = candidate_budget % n;
    (0..query_count)
        .map(|i| {
            if (i as u32) < remainder {
                base + 1
            } else {
                base
            }
        })
        .collect()
}

/// Computes per-query pagination parameters: (page_size, max_pages).
/// If budget <= 100: page_size = budget, max_pages = 1.
/// If budget > 100: page_size = floor(budget / 3), max_pages = 3.
pub fn calculate_pagination_params(query_budget: u32) -> (u32, u32) {
    if query_budget <= 100 {
        (query_budget, 1)
    } else {
        (query_budget / 3, 3)
    }
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
    let query_budgets = calculate_query_budgets(candidate_budget, generated_queries.len());

    let mut partial = false;
    let mut total_candidates_collected = 0;
    // Store per-query candidates: Vec<Vec<(GitHubRepository, usize)>> (repo, raw_rank)
    let mut query_results: Vec<Vec<(GitHubRepository, usize)>> = Vec::new();

    'query_loop: for (i, q) in generated_queries.iter().enumerate() {
        let budget = query_budgets[i];
        let (page_size, max_pages) = calculate_pagination_params(budget);
        let formatted_query =
            format_query_with_filters(q, request.min_stars, request.pushed_after.as_deref());

        let mut query_raw_items: Vec<(GitHubRepository, usize)> = Vec::new();
        let mut requested_slots = 0u32;
        let mut first_total_count: Option<u64> = None;
        let mut stop_queries = false;

        for page in 1..=max_pages {
            if requested_slots >= budget {
                break;
            }

            requested_slots += page_size;

            let page_result = github
                .search_repositories_page(&formatted_query, page, page_size, token)
                .await;

            match page_result {
                Ok(res) => {
                    if page == 1 {
                        first_total_count = Some(res.total_count);
                    }

                    // Compute raw rank for each item: ((page - 1) * page_size) + item.raw_position
                    for ranked in res.ranked_items {
                        let raw_rank =
                            (((page - 1) as usize) * (page_size as usize)) + ranked.raw_position;
                        query_raw_items.push((ranked.repository, raw_rank));
                    }

                    // Stop conditions:
                    // 1. Raw page contains zero rows
                    if res.raw_item_count == 0 {
                        break;
                    }

                    // 2. Raw short page: raw_item_count < page_size
                    if res.raw_item_count < page_size as usize {
                        break;
                    }

                    // 3. Requested range reaches first_total_count
                    if let Some(first_total) = first_total_count {
                        let requested_through = (page as u64) * (page_size as u64);
                        if requested_through >= first_total {
                            break;
                        }
                    }

                    // 4. Requested slot budget reached
                    if requested_slots >= budget {
                        break;
                    }
                }
                Err(e) => {
                    let candidates_so_far = total_candidates_collected + query_raw_items.len();
                    if candidates_so_far == 0 {
                        return Err(e);
                    }
                    match e {
                        AppError::GitHubRateLimit { .. }
                        | AppError::GitHubTimeout(_)
                        | AppError::GitHubNetwork(_) => {
                            eprintln!("AI search query {} page {} degraded: {}", i, page, e);
                            partial = true;
                            stop_queries = true;
                            // Break out of page loop to preserve query_raw_items collected so far
                            break;
                        }
                        _ => return Err(e),
                    }
                }
            }
        }

        // Deduplicate intra-query: first occurrence wins, later duplicates in the SAME query are ignored completely
        let mut deduped_query_items: Vec<(GitHubRepository, usize)> = Vec::new();
        let mut query_seen_ids: HashSet<i64> = HashSet::new();
        let mut query_seen_urls: HashSet<String> = HashSet::new();

        for (repo, raw_rank) in query_raw_items {
            let canonical_url = match parse_github_repo_url(&repo.github_url) {
                Ok((_, _, u)) => u,
                Err(_) => repo.github_url.clone(),
            };

            if query_seen_ids.contains(&repo.github_repo_id)
                || query_seen_urls.contains(&canonical_url)
            {
                continue;
            }

            query_seen_ids.insert(repo.github_repo_id);
            query_seen_urls.insert(canonical_url);
            deduped_query_items.push((repo, raw_rank));
        }

        total_candidates_collected += deduped_query_items.len();
        query_results.push(deduped_query_items);

        if stop_queries {
            break 'query_loop;
        }
    }

    // Dedicated Single-Query Path (R7)
    if generated_queries.len() == 1 {
        let single_candidates: Vec<GitHubRepository> = query_results
            .into_iter()
            .next()
            .unwrap_or_default()
            .into_iter()
            .map(|(repo, _)| repo)
            .collect();

        let candidate_ids: Vec<i64> = single_candidates.iter().map(|e| e.github_repo_id).collect();
        let candidate_urls: Vec<String> = single_candidates
            .iter()
            .map(|e| match parse_github_repo_url(&e.github_url) {
                Ok((_, _, u)) => u,
                Err(_) => e.github_url.clone(),
            })
            .collect();

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
            crate::library::repository::find_favorited_identities(
                pool,
                &candidate_ids,
                &candidate_urls,
            )
            .await?
        } else {
            (HashSet::new(), HashSet::new())
        };

        let mut final_items: Vec<GitHubRepository> = Vec::new();
        for repo in single_candidates {
            let canonical_url = match parse_github_repo_url(&repo.github_url) {
                Ok((_, _, u)) => u,
                Err(_) => repo.github_url.clone(),
            };

            if request.exclude_selected
                && (selected_repo_ids.contains(&repo.github_repo_id)
                    || selected_urls.contains(&canonical_url))
            {
                continue;
            }

            if request.exclude_favorites
                && (fav_repo_ids.contains(&repo.github_repo_id)
                    || fav_urls.contains(&canonical_url))
            {
                continue;
            }

            final_items.push(repo);
            if final_items.len() == request.limit as usize {
                break;
            }
        }

        if !final_items.is_empty() {
            insert_selection_history_batch(pool, &final_items).await?;
        }

        return Ok(AiRepositorySearchResponse {
            items: final_items,
            generated_queries,
            partial,
        });
    }

    // Multi-Query Path with RRF Ranking
    let mut entries: Vec<CandidateEntry> = Vec::new();
    let mut seen_repo_ids: HashMap<i64, usize> = HashMap::new();
    let mut seen_canonical_urls: HashMap<String, usize> = HashMap::new();
    let mut seen_counter: usize = 0;

    for query_items in query_results {
        for (item, raw_rank) in query_items {
            let score_contrib = 1.0 / (60.0 + (raw_rank as f64));
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

    // Sort entries by RRF score descending, stars descending, then first-seen ascending
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
