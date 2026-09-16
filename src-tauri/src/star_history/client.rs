use std::net::IpAddr;
use std::time::Duration;
use url::Url;

use crate::article::security::resolve_and_validate_host;
use crate::error::AppError;

pub const DEFAULT_STAR_HISTORY_TIMEOUT_SECS: u64 = 15;
pub const MAX_STAR_HISTORY_REDIRECTS: usize = 3;
pub const MAX_STAR_HISTORY_RESPONSE_BYTES: usize = 2 * 1024 * 1024; // 2 MiB (2,097,152 bytes)

/// Safely fetch official Star History SVG asset with SSRF protection, manual redirect validation,
/// content-type verification, and strict response size budget.
///
/// Security constraints:
/// - Client is strictly credential-free (no GitHub token, no AI key, no cookies, no Authorization header)
/// - Target scheme must be HTTPS
/// - Host must be exact `api.star-history.com` (case-insensitive) on default port 443
/// - Up to 3 manual redirects; each hop is re-validated against SSRF address rules and host constraints
/// - 4th redirect is rejected
/// - Content-Type must be `image/svg+xml`
/// - Response size strictly capped at 2 MiB (2,097,152 bytes) with streaming enforcement
/// - Request timeout strictly 15 seconds
pub async fn fetch_star_history_svg(
    target_url: &str,
    mock_dns: Option<&std::collections::HashMap<String, Vec<IpAddr>>>,
    allowed_hosts: Option<&[String]>,
) -> Result<String, AppError> {
    let trimmed = target_url.trim();
    if trimmed.is_empty() {
        return Err(AppError::InvalidInput(
            "Star History URL cannot be empty".to_string(),
        ));
    }

    let mut current_url =
        Url::parse(trimmed).map_err(|e| AppError::InvalidInput(format!("Invalid URL: {}", e)))?;

    // Build credential-free client with no automatic redirects
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(DEFAULT_STAR_HISTORY_TIMEOUT_SECS))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| AppError::GitHubNetwork(format!("Failed to build HTTP client: {}", e)))?;

    let mut redirect_count = 0;

    loop {
        let host = current_url
            .host_str()
            .ok_or_else(|| AppError::InvalidInput("URL is missing hostname".to_string()))?;

        // Host must be exact api.star-history.com (or test allowed_host)
        let is_explicitly_allowed = allowed_hosts
            .map(|hosts| hosts.iter().any(|h| h.eq_ignore_ascii_case(host)))
            .unwrap_or(false);

        // Scheme must be HTTPS (except for explicitly allowed test hosts like WireMock)
        if !is_explicitly_allowed && current_url.scheme() != "https" {
            return Err(AppError::ArticleUnsupportedScheme(format!(
                "Unsupported scheme '{}'. Star History preview requires HTTPS.",
                current_url.scheme()
            )));
        }

        if !is_explicitly_allowed && !host.eq_ignore_ascii_case("api.star-history.com") {
            return Err(AppError::ArticleBlockedAddress(format!(
                "Host '{}' is not supported for Star History fetch",
                host
            )));
        }

        // Port must be default 443
        let port = current_url.port_or_known_default().unwrap_or(443);
        if !is_explicitly_allowed && current_url.port().is_some() {
            return Err(AppError::ArticleBlockedAddress(
                "Explicit non-default port is rejected for Star History fetch".to_string(),
            ));
        }

        // Address security validation
        if !is_explicitly_allowed {
            resolve_and_validate_host(host, port, mock_dns).await?;
        }

        // Send credential-free request: NO Authorization, NO Cookies
        let response = client
            .get(current_url.as_str())
            .header(
                "User-Agent",
                format!("RepoShelf/{}", env!("CARGO_PKG_VERSION")),
            )
            .header("Accept", "image/svg+xml")
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    AppError::ArticleTimeout(format!(
                        "Star History request timed out after {}s",
                        DEFAULT_STAR_HISTORY_TIMEOUT_SECS
                    ))
                } else {
                    AppError::GitHubNetwork(format!("Failed to fetch Star History SVG: {}", e))
                }
            })?;

        let status = response.status();

        // Handle redirects (301, 302, 303, 307, 308)
        if status.is_redirection() {
            redirect_count += 1;
            if redirect_count > MAX_STAR_HISTORY_REDIRECTS {
                return Err(AppError::ArticleTooManyRedirects(format!(
                    "Exceeded maximum limit of {} redirects",
                    MAX_STAR_HISTORY_REDIRECTS
                )));
            }

            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .ok_or_else(|| AppError::ArticleHttp {
                    status: status.as_u16(),
                    message: "Redirect response missing Location header".to_string(),
                })?
                .to_str()
                .map_err(|_| AppError::ArticleHttp {
                    status: status.as_u16(),
                    message: "Invalid characters in Location header".to_string(),
                })?;

            current_url = current_url
                .join(location)
                .map_err(|e| AppError::InvalidInput(format!("Invalid redirect location: {}", e)))?;

            continue;
        }

        if !status.is_success() {
            return Err(AppError::ArticleHttp {
                status: status.as_u16(),
                message: format!("HTTP {}", status),
            });
        }

        // Check Content-Type header: must be image/svg+xml
        if let Some(ct_val) = response.headers().get(reqwest::header::CONTENT_TYPE) {
            let ct_str = ct_val.to_str().unwrap_or("").to_lowercase();
            let mime = ct_str.split(';').next().unwrap_or("").trim();
            if mime != "image/svg+xml" {
                return Err(AppError::ArticleUnsupportedContentType(format!(
                    "Unsupported content type '{}'. Star History requires image/svg+xml.",
                    mime
                )));
            }
        }

        // Check Content-Length header for early budget rejection
        if let Some(cl_val) = response.headers().get(reqwest::header::CONTENT_LENGTH) {
            if let Ok(cl_str) = cl_val.to_str() {
                if let Ok(len) = cl_str.parse::<usize>() {
                    if len > MAX_STAR_HISTORY_RESPONSE_BYTES {
                        return Err(AppError::ArticleResponseTooLarge(format!(
                            "Content-Length {} exceeds maximum Star History budget of 2 MiB",
                            len
                        )));
                    }
                }
            }
        }

        // Stream response body up to 2 MiB (2,097,152 bytes)
        let mut response = response;
        let mut body_bytes = Vec::new();

        while let Some(chunk) = response.chunk().await.map_err(|e| {
            AppError::GitHubNetwork(format!(
                "Failed to read Star History SVG response body: {}",
                e
            ))
        })? {
            body_bytes.extend_from_slice(&chunk);
            if body_bytes.len() > MAX_STAR_HISTORY_RESPONSE_BYTES {
                return Err(AppError::ArticleResponseTooLarge(
                    "Star History SVG response body exceeded maximum budget of 2 MiB".to_string(),
                ));
            }
        }

        let svg_text = String::from_utf8(body_bytes).map_err(|_| {
            AppError::InvalidInput("Star History SVG contains invalid UTF-8".to_string())
        })?;

        return Ok(svg_text);
    }
}
