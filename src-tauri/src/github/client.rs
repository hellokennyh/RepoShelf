use std::sync::{Arc, RwLock};
use std::time::Duration;

use reqwest::header::{HeaderMap, ACCEPT, AUTHORIZATION, USER_AGENT};
use reqwest::StatusCode;

use crate::error::AppError;
use crate::github::classification::classify_repository;
use crate::github::models::{
    GitHubApiRateLimitResponse, GitHubApiRelease, GitHubApiRepo, GitHubApiSearchResponse,
    GitHubConnectionStatus, GitHubRateLimitStatus, GitHubRepository, RateLimitBucket,
    RepositorySearchRequest, RepositorySearchResponse, SearchPageResult,
};
use crate::github::rate_limit::{
    format_reset_time, handle_rate_limit_error, parse_rate_limit_headers, update_rate_limit_status,
};
use crate::github::search::build_search_query;

pub const GITHUB_API_VERSION: &str = "2026-03-10";
pub const GITHUB_BASE_URL: &str = "https://api.github.com";
pub const GITHUB_REQUEST_TIMEOUT_SECS: u64 = 15;

type SearchPageHook = Arc<dyn Fn(u32) -> Option<AppError> + Send + Sync>;

#[derive(Clone)]
pub struct GitHubClient {
    client: reqwest::Client,
    base_url: String,
    rate_limit_state: Arc<RwLock<GitHubRateLimitStatus>>,
    search_page_hook: Option<SearchPageHook>,
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
            search_page_hook: None,
        }
    }

    /// Creates a GitHub client with a search page failure hook for deterministic integration testing.
    pub fn with_search_page_hook<F>(base_url: &str, hook: F) -> Self
    where
        F: Fn(u32) -> Option<AppError> + Send + Sync + 'static,
    {
        let mut client = Self::with_base_url(base_url);
        client.search_page_hook = Some(Arc::new(hook));
        client
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

    async fn execute_search_page_request(
        &self,
        query_params: &[(&str, &str)],
        token: Option<&str>,
    ) -> Result<SearchPageResult, AppError> {
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

        let raw_item_count = api_response.items.len();

        let mut items = Vec::new();
        let mut ranked_items = Vec::new();

        for (idx, repo) in api_response.items.into_iter().enumerate() {
            let raw_position = idx + 1;
            if !repo.private {
                let category =
                    classify_repository(&repo.name, repo.description.as_deref(), &repo.topics)
                        .to_string();
                let domain_repo = repo.into_domain(category);
                items.push(domain_repo.clone());
                ranked_items.push(crate::github::RankedGitHubRepository {
                    repository: domain_repo,
                    raw_position,
                });
            }
        }

        Ok(SearchPageResult {
            items,
            ranked_items,
            total_count: api_response.total_count,
            raw_item_count,
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
        let page_res = self
            .execute_search_page_request(&query_params, token)
            .await?;
        Ok(RepositorySearchResponse {
            items: page_res.items,
            total_count: page_res.total_count,
        })
    }

    /// Executes GET /search/repositories with explicit page and per_page pagination parameters.
    pub async fn search_repositories_page(
        &self,
        formatted_query: &str,
        page: u32,
        per_page: u32,
        token: Option<&str>,
    ) -> Result<SearchPageResult, AppError> {
        if let Some(ref hook) = self.search_page_hook {
            if let Some(err) = hook(page) {
                return Err(err);
            }
        }
        let page_str = page.to_string();
        let per_page_str = per_page.to_string();
        let query_params = [
            ("q", formatted_query),
            ("page", page_str.as_str()),
            ("per_page", per_page_str.as_str()),
        ];
        self.execute_search_page_request(&query_params, token).await
    }

    /// Executes GET /repos/{owner}/{repo}/releases and counts public published releases (excluding drafts).
    ///
    /// Respects pagination with per_page=100 up to 10 pages maximum.
    /// All-or-nothing: any page failure returns Err (partial count is never returned).
    pub async fn get_repository_releases_count(
        &self,
        owner: &str,
        repo: &str,
        token: Option<&str>,
    ) -> Result<i64, AppError> {
        let mut page: u32 = 1;
        let mut total_count: i64 = 0;
        let max_pages: u32 = 10;

        loop {
            let path = format!("/repos/{}/{}/releases", owner, repo);
            let page_str = page.to_string();
            let query_params = [("per_page", "100"), ("page", page_str.as_str())];

            let req = self
                .prepare_request(reqwest::Method::GET, &path, token)
                .query(&query_params);

            let resp = req.send().await.map_err(|e| {
                if e.is_timeout() {
                    AppError::GitHubTimeout(format!(
                        "Releases request timed out after {}s",
                        GITHUB_REQUEST_TIMEOUT_SECS
                    ))
                } else {
                    AppError::GitHubNetwork(format!("Failed to reach GitHub releases API: {}", e))
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
                    "GitHub releases API returned HTTP {} for '{}/{}'",
                    status, owner, repo
                )));
            }

            let releases: Vec<GitHubApiRelease> = resp.json().await.map_err(|e| {
                AppError::GitHubInvalidResponse(format!(
                    "Failed to deserialize GitHub releases JSON: {}",
                    e
                ))
            })?;

            let page_items_count = releases.len();
            for release in releases {
                if !release.draft {
                    total_count += 1;
                }
            }

            if page_items_count < 100 {
                break;
            }

            let has_next = if let Some(link) = headers.get("link").and_then(|h| h.to_str().ok()) {
                link.split(',').any(|part| part.contains("rel=\"next\""))
            } else {
                page_items_count == 100
            };

            if has_next && page < max_pages {
                page += 1;
            } else if has_next && page >= max_pages {
                return Err(AppError::GitHubInvalidResponse(
                    "Releases pagination exceeded maximum allowed budget".to_string(),
                ));
            } else {
                break;
            }
        }

        Ok(total_count)
    }

    /// Executes GET /repos/{owner}/{repo}/languages and calculates language code byte percentages.
    ///
    /// percentage = language_bytes / total_language_bytes * 100
    /// Only bytes > 0 are counted.
    /// If total_bytes is 0 or result is empty, returns Ok(HashMap::new()).
    pub async fn get_repository_languages(
        &self,
        owner: &str,
        repo: &str,
        token: Option<&str>,
    ) -> Result<std::collections::HashMap<String, f64>, AppError> {
        let path = format!("/repos/{}/{}/languages", owner, repo);
        let req = self.prepare_request(reqwest::Method::GET, &path, token);

        let resp = req.send().await.map_err(|e| {
            if e.is_timeout() {
                AppError::GitHubTimeout(format!(
                    "Languages request timed out after {}s",
                    GITHUB_REQUEST_TIMEOUT_SECS
                ))
            } else {
                AppError::GitHubNetwork(format!("Failed to reach GitHub languages API: {}", e))
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
                "GitHub languages API returned HTTP {} for '{}/{}'",
                status, owner, repo
            )));
        }

        let raw_languages: std::collections::HashMap<String, u64> =
            resp.json().await.map_err(|e| {
                AppError::GitHubInvalidResponse(format!(
                    "Failed to deserialize GitHub languages JSON: {}",
                    e
                ))
            })?;

        let valid_entries: Vec<(String, u64)> = raw_languages
            .into_iter()
            .filter(|(_, bytes)| *bytes > 0)
            .collect();

        let total_bytes: u64 = valid_entries.iter().map(|(_, b)| *b).sum();

        if total_bytes == 0 || valid_entries.is_empty() {
            return Ok(std::collections::HashMap::new());
        }

        let mut result = std::collections::HashMap::new();
        for (lang, bytes) in valid_entries {
            let pct = (bytes as f64) / (total_bytes as f64) * 100.0;
            if pct.is_finite() && pct >= 0.0 {
                result.insert(lang, pct);
            }
        }

        Ok(result)
    }

    /// Executes GET /repos/{owner}/{repo}/readme and returns decoded README string.
    ///
    /// Respects MAX_README_BYTES (1 MiB = 1,048,576 bytes).
    /// If GitHub metadata declares a larger README or decoded bytes exceed limit, returns Ok(None).
    /// If repository has no README (404), returns Ok(None).
    pub async fn get_repository_readme(
        &self,
        owner: &str,
        repo: &str,
        token: Option<&str>,
    ) -> Result<Option<String>, AppError> {
        let max_readme_bytes = 1024 * 1024; // 1 MiB (1,048,576 bytes)
        let path = format!("/repos/{}/{}/readme", owner, repo);
        let req = self.prepare_request(reqwest::Method::GET, &path, token);

        let resp = req.send().await.map_err(|e| {
            if e.is_timeout() {
                AppError::GitHubTimeout(format!(
                    "README request timed out after {}s",
                    GITHUB_REQUEST_TIMEOUT_SECS
                ))
            } else {
                AppError::GitHubNetwork(format!("Failed to reach GitHub README API: {}", e))
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
            return Ok(None);
        }

        if status == StatusCode::FORBIDDEN || status == StatusCode::TOO_MANY_REQUESTS {
            return Err(handle_rate_limit_error(status.as_u16(), &headers));
        }

        if !status.is_success() {
            return Err(AppError::GitHubInvalidResponse(format!(
                "GitHub README API returned HTTP {} for '{}/{}'",
                status, owner, repo
            )));
        }

        let body_bytes = resp.bytes().await.map_err(|e| {
            AppError::GitHubInvalidResponse(format!("Failed to read README response bytes: {}", e))
        })?;

        // Attempt to parse standard GitHub contents API JSON response
        #[derive(serde::Deserialize)]
        struct GitHubApiReadmeJson {
            size: Option<usize>,
            content: Option<String>,
            encoding: Option<String>,
        }

        if let Ok(json_obj) = serde_json::from_slice::<GitHubApiReadmeJson>(&body_bytes) {
            if let Some(size) = json_obj.size {
                if size > max_readme_bytes {
                    return Ok(None);
                }
            }

            if let Some(ref content) = json_obj.content {
                if json_obj.encoding.as_deref() == Some("base64") {
                    use base64::Engine;
                    let clean_b64: String =
                        content.chars().filter(|c| !c.is_whitespace()).collect();
                    let decoded = base64::engine::general_purpose::STANDARD
                        .decode(&clean_b64)
                        .map_err(|e| {
                            AppError::GitHubInvalidResponse(format!(
                                "Failed to decode base64 README: {}",
                                e
                            ))
                        })?;

                    if decoded.len() > max_readme_bytes {
                        return Ok(None);
                    }

                    return Ok(Some(String::from_utf8_lossy(&decoded).to_string()));
                } else {
                    if content.len() > max_readme_bytes {
                        return Ok(None);
                    }
                    return Ok(Some(content.clone()));
                }
            }
        }

        // Fallback for raw text / markdown response body (e.g. application/vnd.github.raw or mock servers)
        if body_bytes.len() > max_readme_bytes {
            return Ok(None);
        }

        Ok(Some(String::from_utf8_lossy(&body_bytes).to_string()))
    }
}

impl Default for GitHubClient {
    fn default() -> Self {
        Self::new()
    }
}
