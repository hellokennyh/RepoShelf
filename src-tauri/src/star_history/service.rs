use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;
use tokio::sync::{watch, Mutex, Semaphore};

use crate::error::AppError;
use crate::github::client::GitHubClient;
use crate::star_history::client::fetch_star_history_svg;
use crate::star_history::models::StarHistoryPreviewResult;
use crate::star_history::parser::parse_readme_for_star_history;
use crate::star_history::sanitizer::validate_and_sanitize_svg;

pub const MAX_STAR_HISTORY_PREVIEW_CONCURRENCY: usize = 2;

pub struct StarHistoryService {
    github_client: Arc<GitHubClient>,
    limiter: Arc<Semaphore>,
    in_flight: Arc<Mutex<HashMap<String, watch::Sender<Option<StarHistoryPreviewResult>>>>>,
    mock_dns: Option<HashMap<String, Vec<IpAddr>>>,
    allowed_hosts: Option<Vec<String>>,
    star_history_base_url: Option<String>,
}

impl StarHistoryService {
    pub fn new(github_client: Arc<GitHubClient>) -> Self {
        Self {
            github_client,
            limiter: Arc::new(Semaphore::new(MAX_STAR_HISTORY_PREVIEW_CONCURRENCY)),
            in_flight: Arc::new(Mutex::new(HashMap::new())),
            mock_dns: None,
            allowed_hosts: None,
            star_history_base_url: None,
        }
    }

    pub fn with_test_config(
        github_client: Arc<GitHubClient>,
        star_history_base_url: Option<String>,
        mock_dns: Option<HashMap<String, Vec<IpAddr>>>,
        allowed_hosts: Option<Vec<String>>,
    ) -> Self {
        Self {
            github_client,
            limiter: Arc::new(Semaphore::new(MAX_STAR_HISTORY_PREVIEW_CONCURRENCY)),
            in_flight: Arc::new(Mutex::new(HashMap::new())),
            mock_dns,
            allowed_hosts,
            star_history_base_url,
        }
    }

    /// Discovers and returns a transient Star History preview for a repository.
    ///
    /// Pipeline steps:
    /// 1. Coalesces concurrent in-flight requests for the same repository.
    /// 2. Bounded concurrency: acquires 1 of 2 global semaphore permits.
    /// 3. Fetches default README via GitHub API (budget <= 1 MiB).
    /// 4. Parses README with CommonMark & HTML parser for supported embed.
    /// 5. Fetches SVG asset using credential-free client (budget <= 2 MiB, 15s timeout, SSRF protected).
    /// 6. Validates & sanitizes SVG with XML parser (disallows scripts, foreignObject, external refs, etc.).
    /// 7. Returns StarHistoryPreviewResult without persisting anything to disk or DB.
    pub async fn get_preview(
        &self,
        owner: &str,
        repo: &str,
        token: Option<&str>,
    ) -> Result<StarHistoryPreviewResult, AppError> {
        let key = format!(
            "{}/{}",
            owner.to_ascii_lowercase(),
            repo.to_ascii_lowercase()
        );

        // Check if there is already an in-flight request for this repo
        let (sender_opt, receiver_opt) = {
            let mut guard = self.in_flight.lock().await;
            if let Some(tx) = guard.get(&key) {
                (None, Some(tx.subscribe()))
            } else {
                let (tx, _) = watch::channel(None);
                guard.insert(key.clone(), tx.clone());
                (Some(tx), None)
            }
        };

        if let Some(mut rx) = receiver_opt {
            // Await existing leader's result
            while rx.borrow().is_none() {
                if rx.changed().await.is_err() {
                    break;
                }
            }
            if let Some(ref res) = *rx.borrow() {
                return Ok(res.clone());
            }
            return Ok(StarHistoryPreviewResult::none());
        }

        let tx = sender_opt.expect("Leader must have a sender");

        // Leader: execute pipeline under the global concurrency limiter
        let result = self.execute_preview_pipeline(owner, repo, token).await;

        // Broadcast result to any coalesced waiters and clean up in-flight map
        let _ = tx.send(Some(result.clone()));
        {
            let mut guard = self.in_flight.lock().await;
            guard.remove(&key);
        }

        Ok(result)
    }

    async fn execute_preview_pipeline(
        &self,
        owner: &str,
        repo: &str,
        token: Option<&str>,
    ) -> StarHistoryPreviewResult {
        // Acquire permit from global concurrency limiter (max 2)
        let _permit = match self.limiter.acquire().await {
            Ok(permit) => permit,
            Err(_) => return StarHistoryPreviewResult::none(),
        };

        // Step 1: Fetch default README
        let readme_opt = match self
            .github_client
            .get_repository_readme(owner, repo, token)
            .await
        {
            Ok(Some(content)) => content,
            _ => return StarHistoryPreviewResult::none(),
        };

        // Step 2: Parse README for supported Star History embed
        let embed_url = match parse_readme_for_star_history(&readme_opt, owner, repo) {
            Some(url) => url,
            None => return StarHistoryPreviewResult::none(),
        };

        let fetch_url = if let Some(ref base) = self.star_history_base_url {
            embed_url.replace("https://api.star-history.com", base)
        } else {
            embed_url.clone()
        };

        // Step 3: Fetch SVG asset using credential-free client
        let raw_svg = match fetch_star_history_svg(
            &fetch_url,
            self.mock_dns.as_ref(),
            self.allowed_hosts.as_deref(),
        )
        .await
        {
            Ok(svg) => svg,
            Err(_) => return StarHistoryPreviewResult::none(),
        };

        // Step 4: Validate and sanitize SVG
        match validate_and_sanitize_svg(&raw_svg) {
            Ok(sanitized_svg) => StarHistoryPreviewResult::ready(sanitized_svg, embed_url),
            Err(_) => StarHistoryPreviewResult::none(),
        }
    }
}
