use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, Semaphore};

use crate::error::AppError;
use crate::github::classification::classify_repository;
use crate::github::GitHubClient;
use crate::secrets::{SecretStore, SECRET_KEY_GITHUB_TOKEN};
use crate::trending::models::{
    EnrichmentSummary, ParsedTrendingRepository, TrendingPeriod, TrendingRepositoryItem,
    TrendingResponse,
};
use crate::trending::parser::parse_trending_html;
use crate::trending::provider::TrendingProvider;

const DEFAULT_TRENDING_BASE_URL: &str = "https://github.com/trending";
const HTML_TIMEOUT_SECS: u64 = 15;
const MAX_ENRICHMENT_CONCURRENCY: usize = 4;

pub struct GitHubTrendingProvider {
    http_client: reqwest::Client,
    base_url: String,
    github_client: Arc<GitHubClient>,
    secret_store: Arc<dyn SecretStore>,
    max_observed_concurrency: Arc<AtomicUsize>,
    active_concurrency: Arc<AtomicUsize>,
}

impl GitHubTrendingProvider {
    pub fn new(github_client: Arc<GitHubClient>, secret_store: Arc<dyn SecretStore>) -> Self {
        Self::with_base_url(DEFAULT_TRENDING_BASE_URL, github_client, secret_store)
    }

    pub fn with_base_url(
        base_url: impl Into<String>,
        github_client: Arc<GitHubClient>,
        secret_store: Arc<dyn SecretStore>,
    ) -> Self {
        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(HTML_TIMEOUT_SECS))
            .redirect(reqwest::redirect::Policy::limited(5))
            .build()
            .expect("failed to construct reqwest client for trending");

        Self {
            http_client,
            base_url: base_url.into(),
            github_client,
            secret_store,
            max_observed_concurrency: Arc::new(AtomicUsize::new(0)),
            active_concurrency: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub fn max_observed_concurrency(&self) -> usize {
        self.max_observed_concurrency.load(Ordering::SeqCst)
    }

    pub fn reset_concurrency_stats(&self) {
        self.max_observed_concurrency.store(0, Ordering::SeqCst);
        self.active_concurrency.store(0, Ordering::SeqCst);
    }

    /// Fetch HTML from GitHub Trending anonymously
    pub async fn fetch_html(&self, period: TrendingPeriod) -> Result<String, AppError> {
        let url = format!("{}?since={}", self.base_url, period.as_str());

        // Anonymous public fetch only: never send Authorization or cookies
        let request = self
            .http_client
            .get(&url)
            .header("Accept", "text/html")
            .header(
                "User-Agent",
                format!("RepoShelf/{}", env!("CARGO_PKG_VERSION")),
            )
            .header("Accept-Language", "en-US,en;q=0.9");

        let response = match request.send().await {
            Ok(resp) => resp,
            Err(e) => {
                if e.is_timeout() {
                    return Err(AppError::TrendingTimeout(format!(
                        "Request to {} timed out after {}s",
                        url, HTML_TIMEOUT_SECS
                    )));
                }
                return Err(AppError::TrendingNetwork(format!(
                    "Failed to fetch trending HTML from {}: {}",
                    url, e
                )));
            }
        };

        let status = response.status();
        if !status.is_success() {
            return Err(AppError::TrendingHttp {
                status: status.as_u16(),
                message: format!("GitHub Trending returned HTTP status {}", status),
            });
        }

        let body = response.text().await.map_err(|e| {
            AppError::TrendingNetwork(format!("Failed to read trending response body: {}", e))
        })?;

        Ok(body)
    }

    /// Execute get_trending with bounded concurrency REST enrichment
    pub async fn execute_get_trending(
        &self,
        period: TrendingPeriod,
        limit: usize,
    ) -> Result<TrendingResponse, AppError> {
        let requested_limit = limit.clamp(1, 50);

        // 1. Fetch raw HTML
        let html = self.fetch_html(period).await?;

        // 2. Parse HTML deterministically
        let parsed_cards = parse_trending_html(&html)?;
        let total_parsed = parsed_cards.len();
        let is_short_source = total_parsed < requested_limit;

        // Take up to requested limit
        let target_cards: Vec<ParsedTrendingRepository> =
            parsed_cards.into_iter().take(requested_limit).collect();

        if target_cards.is_empty() {
            return Ok(TrendingResponse {
                period,
                requested_limit,
                returned_count: 0,
                items: Vec::new(),
                enrichment_summary: EnrichmentSummary::default(),
                is_short_source,
                partial_warning: None,
            });
        }

        // 3. REST metadata enrichment with max concurrency 4
        let token = self
            .secret_store
            .get_secret(SECRET_KEY_GITHUB_TOKEN)
            .unwrap_or(None);

        let semaphore = Arc::new(Semaphore::new(MAX_ENRICHMENT_CONCURRENCY));
        let rate_limited_flag = Arc::new(AtomicBool::new(false));
        let rate_limit_reset = Arc::new(Mutex::new(None));
        let succeeded_count = Arc::new(AtomicUsize::new(0));
        let failed_count = Arc::new(AtomicUsize::new(0));

        let mut tasks = Vec::with_capacity(target_cards.len());

        for card in target_cards {
            let sem = Arc::clone(&semaphore);
            let rl_flag = Arc::clone(&rate_limited_flag);
            let rl_reset = Arc::clone(&rate_limit_reset);
            let succ = Arc::clone(&succeeded_count);
            let fail = Arc::clone(&failed_count);
            let gh_client = Arc::clone(&self.github_client);
            let token_opt = token.clone();
            let active_conc = Arc::clone(&self.active_concurrency);
            let max_conc = Arc::clone(&self.max_observed_concurrency);

            tasks.push(tauri::async_runtime::spawn(async move {
                // If rate limited has been triggered, immediately degrade without API call
                if rl_flag.load(Ordering::SeqCst) {
                    fail.fetch_add(1, Ordering::SeqCst);
                    return create_degraded_item(&card);
                }

                // Acquire semaphore permit (max 4 concurrent)
                let _permit = match sem.acquire().await {
                    Ok(p) => p,
                    Err(_) => {
                        fail.fetch_add(1, Ordering::SeqCst);
                        return create_degraded_item(&card);
                    }
                };

                // Check again after acquiring permit in case another task tripped rate limit
                if rl_flag.load(Ordering::SeqCst) {
                    fail.fetch_add(1, Ordering::SeqCst);
                    return create_degraded_item(&card);
                }

                let cur = active_conc.fetch_add(1, Ordering::SeqCst) + 1;
                max_conc.fetch_max(cur, Ordering::SeqCst);

                // Call Phase 2 GitHubClient
                let fetch_result = gh_client
                    .get_repository(&card.owner, &card.repo, token_opt.as_deref())
                    .await;

                active_conc.fetch_sub(1, Ordering::SeqCst);

                match fetch_result {
                    Ok(repo) => {
                        succ.fetch_add(1, Ordering::SeqCst);
                        let category = classify_repository(
                            &repo.name,
                            repo.description.as_deref(),
                            &repo.topics,
                        );
                        TrendingRepositoryItem {
                            github_repo_id: Some(repo.github_repo_id),
                            github_url: repo.github_url,
                            name: repo.name,
                            description: repo.description.or(card.description),
                            category: category.to_string(),
                            stars: Some(repo.stars),
                            license: repo.license,
                            topics: repo.topics,
                            pushed_at: repo.pushed_at,
                            trending_rank: card.trending_rank,
                            period_stars: card.period_stars,
                        }
                    }
                    Err(AppError::GitHubRateLimit { reset_at, .. }) => {
                        rl_flag.store(true, Ordering::SeqCst);
                        if let Some(reset) = reset_at {
                            let mut lock = rl_reset.lock().await;
                            *lock = Some(reset);
                        }
                        fail.fetch_add(1, Ordering::SeqCst);
                        create_degraded_item(&card)
                    }
                    Err(_) => {
                        // 404, network error, timeout, or malformed JSON: gracefully degrade
                        fail.fetch_add(1, Ordering::SeqCst);
                        create_degraded_item(&card)
                    }
                }
            }));
        }

        let mut enriched_items = Vec::with_capacity(tasks.len());
        for task in tasks {
            match task.await {
                Ok(item) => enriched_items.push(item),
                Err(_) => {
                    // Task panicked or cancelled
                }
            }
        }

        // Strictly preserve GitHub Trending DOM rank order
        enriched_items.sort_by_key(|item| item.trending_rank);

        let is_rl = rate_limited_flag.load(Ordering::SeqCst);
        let succ_total = succeeded_count.load(Ordering::SeqCst);
        let fail_total = failed_count.load(Ordering::SeqCst);
        let rl_reset_val = *rate_limit_reset.lock().await;

        let partial_warning = if is_rl {
            Some(
                "部分 GitHub 项目信息因 GitHub API 访问限额未能补充，热门排名仍来自 GitHub Trending。"
                    .to_string(),
            )
        } else if fail_total > 0 {
            Some("部分 GitHub 项目信息未能补充，热门排名仍来自 GitHub Trending。".to_string())
        } else {
            None
        };

        let summary = EnrichmentSummary {
            attempted: succ_total + fail_total,
            succeeded: succ_total,
            failed: fail_total,
            rate_limited: is_rl,
            rate_limit_reset: rl_reset_val,
        };

        Ok(TrendingResponse {
            period,
            requested_limit,
            returned_count: enriched_items.len(),
            items: enriched_items,
            enrichment_summary: summary,
            is_short_source,
            partial_warning,
        })
    }
}

/// Fallback helper when REST enrichment fails or is skipped
fn create_degraded_item(card: &ParsedTrendingRepository) -> TrendingRepositoryItem {
    let category = classify_repository(&card.name, card.description.as_deref(), &[]);
    TrendingRepositoryItem {
        github_repo_id: None,
        github_url: card.github_url.clone(),
        name: card.name.clone(),
        description: card.description.clone(),
        category: category.to_string(),
        stars: card.stars,
        license: None,
        topics: Vec::new(),
        pushed_at: None,
        trending_rank: card.trending_rank,
        period_stars: card.period_stars,
    }
}

impl TrendingProvider for GitHubTrendingProvider {
    fn get_trending<'a>(
        &'a self,
        period: TrendingPeriod,
        limit: usize,
    ) -> Pin<Box<dyn Future<Output = Result<TrendingResponse, AppError>> + Send + 'a>> {
        Box::pin(async move { self.execute_get_trending(period, limit).await })
    }
}
