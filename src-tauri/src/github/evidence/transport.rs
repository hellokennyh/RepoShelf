use std::sync::Arc;
use std::time::Duration;

use reqwest::header::{ACCEPT, AUTHORIZATION, LOCATION, USER_AGENT};
use reqwest::StatusCode;
use url::Url;

use crate::error::AppError;
use crate::github::client::{GITHUB_API_VERSION, GITHUB_BASE_URL};
use crate::github::evidence::budget::{
    AcquisitionBudgetTracker, BUDGET_MAX_REDIRECTS_PER_REQUEST, BUDGET_MAX_SINGLE_RESPONSE_BYTES,
};
use crate::github::evidence::identity::validate_redirect_target;
use crate::github::rate_limit::handle_rate_limit_error;
use crate::github::GitHubClient;

/// Safe HTTP transport for repository evidence acquisition.
///
/// Guarantees:
/// - Request and total bytes accounting before and during network stream
/// - Rejection of responses > 2 MiB (Content-Length precheck + streaming chunk check)
/// - Rejection of total responses > 8 MiB across acquisition session
/// - Manual redirect handling with max 3 hops
/// - Strict redirect host safety checks (prevent SSRF / private IP / cloud metadata)
/// - Token stripping if redirected across different hosts
/// - At most 1 bounded retry on transient 5xx errors or network drops (counts towards budget & deadline)
/// - Cancellation and remaining deadline checks before dispatch and between streaming chunks
pub struct AcquisitionTransport {
    http_client: reqwest::Client,
    base_url: String,
    github_client: Arc<GitHubClient>,
}

impl AcquisitionTransport {
    pub fn new(github_client: Arc<GitHubClient>) -> Self {
        let http_client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("Failed to build acquisition reqwest client");

        let base_url = github_client.base_url().to_string();
        Self {
            http_client,
            base_url,
            github_client,
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Executes a GET request with strict streaming size bounds, redirect handling, and retry logic.
    pub async fn get_bounded_json<T: serde::de::DeserializeOwned>(
        &self,
        endpoint_path: &str,
        token: Option<&str>,
        budget: &AcquisitionBudgetTracker,
    ) -> Result<T, AppError> {
        let bytes = self.get_bounded_bytes(endpoint_path, token, budget).await?;
        let val: T = serde_json::from_slice(&bytes).map_err(|e| {
            AppError::GitHubInvalidResponse(format!(
                "Failed to deserialize GitHub API JSON response from '{}': {}",
                endpoint_path, e
            ))
        })?;
        Ok(val)
    }

    /// Fetches bounded raw bytes for an endpoint.
    pub async fn get_bounded_bytes(
        &self,
        endpoint_path: &str,
        token: Option<&str>,
        budget: &AcquisitionBudgetTracker,
    ) -> Result<Vec<u8>, AppError> {
        let initial_url = if endpoint_path.starts_with("http://")
            || endpoint_path.starts_with("https://")
        {
            Url::parse(endpoint_path).map_err(|e| {
                AppError::InvalidRepositoryUrl(format!(
                    "Invalid URL '{}': {}",
                    endpoint_path, e
                ))
            })?
        } else {
            let full_url_str = format!("{}{}", self.base_url, endpoint_path);
            Url::parse(&full_url_str).map_err(|e| {
                AppError::InvalidRepositoryUrl(format!(
                    "Invalid constructed URL '{}': {}",
                    full_url_str, e
                ))
            })?
        };

        // Attempt request with at most 1 retry for transient server/network errors
        let mut attempts = 0;
        let max_attempts = 2;

        loop {
            attempts += 1;
            match self
                .execute_request_with_redirects(&initial_url, token, budget)
                .await
            {
                Ok(bytes) => return Ok(bytes),
                Err(err) => {
                    let is_transient = match &err {
                        AppError::GitHubNetwork(_) => true,
                        AppError::GitHubInvalidResponse(msg)
                            if msg.contains("HTTP 500")
                                || msg.contains("HTTP 502")
                                || msg.contains("HTTP 503")
                                || msg.contains("HTTP 504") =>
                        {
                            true
                        }
                        _ => false,
                    };

                    if is_transient && attempts < max_attempts {
                        // Check if we still have deadline and request budget before retrying
                        if budget.check_cancel_and_deadline().is_ok() {
                            // Small backoff before single retry
                            tokio::time::sleep(Duration::from_millis(200)).await;
                            continue;
                        }
                    }

                    return Err(err);
                }
            }
        }
    }

    async fn execute_request_with_redirects(
        &self,
        initial_url: &Url,
        token: Option<&str>,
        budget: &AcquisitionBudgetTracker,
    ) -> Result<Vec<u8>, AppError> {
        let mut current_url = initial_url.clone();
        let mut current_token = token.map(|t| t.trim().to_string()).filter(|t| !t.is_empty());
        let mut redirect_hops = 0;

        let test_base_opt = if self.base_url != GITHUB_BASE_URL {
            Some(self.base_url.as_str())
        } else {
            None
        };

        loop {
            // Count this attempt towards the global 40 requests budget and check deadline
            budget.record_request()?;
            let timeout = budget.remaining_request_timeout()?;

            let mut req_builder = self
                .http_client
                .get(current_url.as_str())
                .timeout(timeout)
                .header(ACCEPT, "application/vnd.github+json")
                .header("X-GitHub-Api-Version", GITHUB_API_VERSION)
                .header(
                    USER_AGENT,
                    format!("RepoShelf/{}", env!("CARGO_PKG_VERSION")),
                );

            if let Some(ref tok) = current_token {
                req_builder = req_builder.header(AUTHORIZATION, format!("Bearer {}", tok));
            }

            let resp = req_builder.send().await.map_err(|e| {
                if e.is_timeout() {
                    AppError::GitHubTimeout(format!(
                        "Request to '{}' timed out after {:?}",
                        current_url, timeout
                    ))
                } else {
                    AppError::GitHubNetwork(format!(
                        "Failed to connect to '{}': {}",
                        current_url, e
                    ))
                }
            })?;

            let status = resp.status();
            let headers = resp.headers().clone();

            // Handle rate limit headers on any status
            self.github_client.get_rate_limit_status(); // touch
            if status == StatusCode::FORBIDDEN || status == StatusCode::TOO_MANY_REQUESTS {
                return Err(handle_rate_limit_error(status.as_u16(), &headers));
            }

            // Handle Redirects (301, 302, 307, 308)
            if status.is_redirection() {
                redirect_hops += 1;
                if redirect_hops > BUDGET_MAX_REDIRECTS_PER_REQUEST {
                    return Err(AppError::AcquisitionBudgetExceeded(format!(
                        "Redirect limit ({}) exceeded for request to '{}'",
                        BUDGET_MAX_REDIRECTS_PER_REQUEST, initial_url
                    )));
                }

                let loc_header = headers.get(LOCATION).ok_or_else(|| {
                    AppError::GitHubInvalidResponse(format!(
                        "Redirect status {} missing Location header from '{}'",
                        status, current_url
                    ))
                })?;

                let loc_str = loc_header.to_str().map_err(|_| {
                    AppError::GitHubInvalidResponse(
                        "Invalid non-ASCII Location header in redirect".to_string(),
                    )
                })?;

                let next_url = current_url.join(loc_str).map_err(|e| {
                    AppError::InvalidRepositoryUrl(format!(
                        "Failed to parse redirect Location '{}': {}",
                        loc_str, e
                    ))
                })?;

                let should_forward_token =
                    validate_redirect_target(&current_url, &next_url, test_base_opt)?;

                if !should_forward_token {
                    // Strip credentials when redirecting across hosts
                    current_token = None;
                }

                current_url = next_url;
                continue;
            }

            // Authentication error
            if status == StatusCode::UNAUTHORIZED {
                return Err(AppError::GitHubAuth(
                    "GitHub authentication failed: invalid or expired token".to_string(),
                ));
            }

            // Not found
            if status == StatusCode::NOT_FOUND {
                return Err(AppError::GitHubNotFound(format!(
                    "Resource not found at '{}'",
                    current_url
                )));
            }

            // Other HTTP failures
            if !status.is_success() {
                return Err(AppError::GitHubInvalidResponse(format!(
                    "GitHub API returned HTTP {} for '{}'",
                    status, current_url
                )));
            }

            // Content-Length Precheck: immediately reject if header states > 2 MiB
            if let Some(content_length) = resp.content_length() {
                if content_length as usize > BUDGET_MAX_SINGLE_RESPONSE_BYTES {
                    return Err(AppError::GitHubInvalidResponse(format!(
                        "Content-Length {} exceeds single response budget limit ({} bytes) for '{}'",
                        content_length, BUDGET_MAX_SINGLE_RESPONSE_BYTES, current_url
                    )));
                }
            }

            // Stream body with strict byte-by-byte bounded checks
            let mut stream_resp = resp;
            let mut buffer = Vec::new();

            while let Some(chunk) = stream_resp.chunk().await.map_err(|e| {
                AppError::GitHubNetwork(format!(
                    "Failed to read response body chunk from '{}': {}",
                    current_url, e
                ))
            })? {
                budget.check_cancel_and_deadline()?;

                let new_len = buffer.len() + chunk.len();
                if new_len > BUDGET_MAX_SINGLE_RESPONSE_BYTES {
                    return Err(AppError::GitHubInvalidResponse(format!(
                        "Single response exceeded {} bytes limit (streamed {} bytes so far) for '{}'",
                        BUDGET_MAX_SINGLE_RESPONSE_BYTES, new_len, current_url
                    )));
                }

                buffer.extend_from_slice(&chunk);
            }

            // Record bytes in global budget (also checks global 8 MiB limit)
            budget.record_bytes(buffer.len())?;

            return Ok(buffer);
        }
    }
}
