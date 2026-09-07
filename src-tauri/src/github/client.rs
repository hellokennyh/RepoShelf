use std::sync::{Arc, RwLock};
use std::time::Duration;

use reqwest::header::{HeaderMap, ACCEPT, AUTHORIZATION, USER_AGENT};
use reqwest::StatusCode;

use crate::error::AppError;
use crate::github::classification::classify_repository;
use crate::github::models::{
    GitHubApiRateLimitResponse, GitHubApiRepo, GitHubApiSearchResponse, GitHubConnectionStatus,
    GitHubRateLimitStatus, GitHubRepository, RateLimitBucket, RepositorySearchRequest,
    RepositorySearchResponse,
};
use crate::github::rate_limit::{
    format_reset_time, handle_rate_limit_error, parse_rate_limit_headers, update_rate_limit_status,
};
use crate::github::search::build_search_query;

pub const GITHUB_API_VERSION: &str = "2026-03-10";
pub const GITHUB_BASE_URL: &str = "https://api.github.com";
pub const GITHUB_REQUEST_TIMEOUT_SECS: u64 = 15;

#[derive(Clone)]
pub struct GitHubClient {
    client: reqwest::Client,
    base_url: String,
    rate_limit_state: Arc<RwLock<GitHubRateLimitStatus>>,
}

impl GitHubClient {
    /// Creates a production GitHub client pointing to https://api.github.com.
    pub fn new() -> Self {
        Self::with_base_url(GITHUB_BASE_URL)
    }

    /// Creates a GitHub client with a custom base URL (used for HTTP mock servers in tests).
    pub fn with_base_url(base_url: &str) -> Self {
        Self::with_base_url_and_timeout(base_url, Duration::from_secs(GITHUB_REQUEST_TIMEOUT_SECS))
    }

    /// Creates a GitHub client with a custom base URL and timeout (used for mock testing timeouts).
    pub fn with_base_url_and_timeout(base_url: &str, timeout: Duration) -> Self {
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .redirect(reqwest::redirect::Policy::limited(5))
            .build()
            .expect("Failed to build reqwest HTTP client");

        Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
            rate_limit_state: Arc::new(RwLock::new(GitHubRateLimitStatus::default())),
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn get_rate_limit_status(&self) -> GitHubRateLimitStatus {
        self.rate_limit_state
            .read()
            .map(|s| s.clone())
            .unwrap_or_default()
    }

    /// Prepares a request with required headers according to the GitHub REST API Version contract.
    fn prepare_request(
        &self,
        method: reqwest::Method,
        path: &str,
        token: Option<&str>,
    ) -> reqwest::RequestBuilder {
        let url = format!("{}{}", self.base_url, path);
        let mut builder = self
            .client
            .request(method, &url)
            .header(ACCEPT, "application/vnd.github+json")
            .header("X-GitHub-Api-Version", GITHUB_API_VERSION)
            .header(
                USER_AGENT,
                format!("RepoShelf/{}", env!("CARGO_PKG_VERSION")),
            );

        if let Some(t) = token {
            let trimmed = t.trim();
            if !trimmed.is_empty() {
                builder = builder.header(AUTHORIZATION, format!("Bearer {}", trimmed));
            }
        }

        builder
    }

    /// Updates internal rate limit state from HTTP response headers.
    fn record_rate_limit_headers(&self, headers: &HeaderMap) {
        if let Some(bucket) = parse_rate_limit_headers(headers) {
            if let Ok(mut guard) = self.rate_limit_state.write() {
                update_rate_limit_status(&mut guard, bucket);
            }
        }
    }

    /// Executes GET /rate_limit for connection testing.
    pub async fn test_connection(
        &self,
        token: Option<&str>,
    ) -> Result<GitHubConnectionStatus, AppError> {
        let req = self.prepare_request(reqwest::Method::GET, "/rate_limit", token);

        let resp = req.send().await.map_err(|e| {
            if e.is_timeout() {
                AppError::GitHubTimeout(format!(
                    "Connection test timed out after {}s",
                    GITHUB_REQUEST_TIMEOUT_SECS
                ))
            } else {
                AppError::GitHubNetwork(format!("Failed to connect to GitHub API: {}", e))
            }
        })?;

        let status = resp.status();
        let headers = resp.headers().clone();
        self.record_rate_limit_headers(&headers);

        if status == StatusCode::UNAUTHORIZED {
            return Err(AppError::GitHubAuth(
                "Invalid or expired GitHub token".to_string(),
            ));
        }

        if status == StatusCode::FORBIDDEN || status == StatusCode::TOO_MANY_REQUESTS {
            return Err(handle_rate_limit_error(status.as_u16(), &headers));
        }

        if !status.is_success() {
            return Err(AppError::GitHubInvalidResponse(format!(
                "GitHub connection test returned HTTP {}",
                status
            )));
        }

        let body: GitHubApiRateLimitResponse = resp.json().await.map_err(|e| {
            AppError::GitHubInvalidResponse(format!(
                "Failed to deserialize GitHub rate limit response: {}",
                e
            ))
        })?;

        let core_bucket = RateLimitBucket {
            limit: body.resources.core.limit,
            remaining: body.resources.core.remaining,
            used: body.resources.core.used,
            reset_at: format_reset_time(body.resources.core.reset),
            resource: "core".to_string(),
        };

        let search_bucket = RateLimitBucket {
            limit: body.resources.search.limit,
            remaining: body.resources.search.remaining,
            used: body.resources.search.used,
            reset_at: format_reset_time(body.resources.search.reset),
            resource: "search".to_string(),
        };

        let rate_limit_status = GitHubRateLimitStatus {
            core: core_bucket,
            search: search_bucket,
        };

        if let Ok(mut guard) = self.rate_limit_state.write() {
            *guard = rate_limit_status.clone();
        }

        let mode = if token.map(|t| !t.trim().is_empty()).unwrap_or(false) {
            "Authenticated".to_string()
        } else {
            "Anonymous".to_string()
        };

        Ok(GitHubConnectionStatus {
            connected: true,
            mode,
            rate_limit: rate_limit_status,
        })
    }

    /// Executes GET /repos/{owner}/{repo} and returns transient domain model.
    pub async fn get_repository(
        &self,
        owner: &str,
        repo: &str,
        token: Option<&str>,
    ) -> Result<GitHubRepository, AppError> {
        let path = format!("/repos/{}/{}", owner, repo);
        let req = self.prepare_request(reqwest::Method::GET, &path, token);

        let resp = req.send().await.map_err(|e| {
            if e.is_timeout() {
                AppError::GitHubTimeout(format!(
                    "Repository request timed out after {}s",
                    GITHUB_REQUEST_TIMEOUT_SECS
                ))
            } else {
                AppError::GitHubNetwork(format!("Failed to reach GitHub API: {}", e))
            }
        })?;

        let status = resp.status();
        let headers = resp.headers().clone();
        self.record_rate_limit_headers(&headers);

        if status == StatusCode::UNAUTHORIZED {
            return Err(AppError::GitHubAuth(
                "Invalid or expired GitHub token".to_string(),
            ));
        }

        if status == StatusCode::NOT_FOUND {
            return Err(AppError::GitHubNotFound(format!(
                "Repository '{}/{}' was not found on GitHub",
                owner, repo
            )));
        }

        if status == StatusCode::FORBIDDEN || status == StatusCode::TOO_MANY_REQUESTS {
            return Err(handle_rate_limit_error(status.as_u16(), &headers));
        }

        if !status.is_success() {
            return Err(AppError::GitHubInvalidResponse(format!(
                "GitHub API returned HTTP {} for repository '{}/{}'",
                status, owner, repo
            )));
        }

        let api_repo: GitHubApiRepo = resp.json().await.map_err(|e| {
            AppError::GitHubInvalidResponse(format!(
                "Failed to deserialize GitHub repository JSON: {}",
                e
            ))
        })?;

        if api_repo.private {
            return Err(AppError::PrivateRepositoryUnsupported(
                "Private repositories are unsupported in RepoShelf V1".to_string(),
            ));
        }

        let category = classify_repository(
            &api_repo.name,
            api_repo.description.as_deref(),
            &api_repo.topics,
        )
        .to_string();

        Ok(api_repo.into_domain(category))
    }

    async fn execute_search_request(
        &self,
        query_params: &[(&str, &str)],
        token: Option<&str>,
    ) -> Result<RepositorySearchResponse, AppError> {
        let req = self
            .prepare_request(reqwest::Method::GET, "/search/repositories", token)
            .query(query_params);

        let resp = req.send().await.map_err(|e| {
            if e.is_timeout() {
                AppError::GitHubTimeout(format!(
                    "Search request timed out after {}s",
                    GITHUB_REQUEST_TIMEOUT_SECS
                ))
            } else {
                AppError::GitHubNetwork(format!("Failed to reach GitHub search API: {}", e))
            }
        })?;

        let status = resp.status();
        let headers = resp.headers().clone();
        self.record_rate_limit_headers(&headers);

        if status == StatusCode::UNAUTHORIZED {
            return Err(AppError::GitHubAuth(
                "Invalid or expired GitHub token".to_string(),
            ));
        }

        if status == StatusCode::FORBIDDEN || status == StatusCode::TOO_MANY_REQUESTS {
            return Err(handle_rate_limit_error(status.as_u16(), &headers));
        }

        if !status.is_success() {
            return Err(AppError::GitHubInvalidResponse(format!(
                "GitHub search API returned HTTP {}",
                status
            )));
        }

        let api_response: GitHubApiSearchResponse = resp.json().await.map_err(|e| {
            AppError::GitHubInvalidResponse(format!(
                "Failed to deserialize GitHub search JSON: {}",
                e
            ))
        })?;

        // Filter private == false, preserve API order, and assign local category
        let items: Vec<GitHubRepository> = api_response
            .items
            .into_iter()
            .filter(|repo| !repo.private)
            .map(|repo| {
                let category =
                    classify_repository(&repo.name, repo.description.as_deref(), &repo.topics)
                        .to_string();
                repo.into_domain(category)
            })
            .collect();

        Ok(RepositorySearchResponse {
            items,
            total_count: api_response.total_count,
        })
    }

    /// Executes GET /search/repositories and returns mapped search results.
    pub async fn search_repositories(
        &self,
        request: &RepositorySearchRequest,
        token: Option<&str>,
    ) -> Result<RepositorySearchResponse, AppError> {
        let formatted_query = build_search_query(request)?;
        let limit_str = request.limit.to_string();
        let query_params = [
            ("q", formatted_query.as_str()),
            ("per_page", limit_str.as_str()),
        ];
        self.execute_search_request(&query_params, token).await
    }

    /// Executes GET /search/repositories with explicit page and per_page pagination parameters.
    pub async fn search_repositories_page(
        &self,
        formatted_query: &str,
        page: u32,
        per_page: u32,
        token: Option<&str>,
    ) -> Result<RepositorySearchResponse, AppError> {
        let page_str = page.to_string();
        let per_page_str = per_page.to_string();
        let query_params = [
            ("q", formatted_query),
            ("page", page_str.as_str()),
            ("per_page", per_page_str.as_str()),
        ];
        self.execute_search_request(&query_params, token).await
    }
}

impl Default for GitHubClient {
    fn default() -> Self {
        Self::new()
    }
}
