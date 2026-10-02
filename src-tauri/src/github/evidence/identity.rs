use std::net::IpAddr;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::error::AppError;
use crate::github::url::parse_github_repo_url;

/// Validated public GitHub repository identity.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ValidatedRepoIdentity {
    pub owner: String,
    pub repo: String,
    pub canonical_url: String,
    pub expected_repo_id: Option<i64>,
}

/// Validates a repository target input (URL or "owner/repo" slug) and returns a normalized identity.
///
/// Strictly rejects:
/// - Empty/whitespace inputs
/// - Non-GitHub hosts (gitlab, bitbucket, arbitrary domains)
/// - Localhost, loopback (127.0.0.1, ::1), private IP ranges, cloud metadata (169.254.169.254)
/// - Subpages (/issues, /pulls, /commits, etc.)
pub fn validate_repository_target(
    input: &str,
    expected_repo_id: Option<i64>,
) -> Result<ValidatedRepoIdentity, AppError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(AppError::InvalidRepositoryUrl(
            "Repository target cannot be empty".to_string(),
        ));
    }

    // Check if input looks like an IP address or contains localhost
    if let Ok(ip) = trimmed.parse::<IpAddr>() {
        return Err(AppError::AcquisitionSecurity(format!(
            "Target cannot be an IP address: {}",
            ip
        )));
    }

    let trimmed_lower = trimmed.to_lowercase();
    if trimmed_lower.contains("localhost")
        || trimmed_lower.contains("127.0.0.1")
        || trimmed_lower.contains("169.254.169.254")
        || trimmed_lower.contains("0.0.0.0")
        || trimmed_lower.contains("[::1]")
    {
        return Err(AppError::AcquisitionSecurity(format!(
            "Target '{}' contains restricted host or loopback address",
            trimmed
        )));
    }

    // Try parsing as full URL first
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        let parsed_url = Url::parse(trimmed).map_err(|e| {
            AppError::InvalidRepositoryUrl(format!("Failed to parse URL '{}': {}", trimmed, e))
        })?;

        // Validate scheme
        if parsed_url.scheme() != "http" && parsed_url.scheme() != "https" {
            return Err(AppError::InvalidRepositoryUrl(format!(
                "Unsupported scheme '{}'",
                parsed_url.scheme()
            )));
        }

        // Validate host
        let host = parsed_url.host_str().unwrap_or("").to_lowercase();
        if host != "github.com" && host != "www.github.com" {
            return Err(AppError::AcquisitionSecurity(format!(
                "Target host '{}' is not supported; only public GitHub repositories are permitted",
                host
            )));
        }

        let (owner, repo, canonical_url) = parse_github_repo_url(trimmed)?;
        Ok(ValidatedRepoIdentity {
            owner,
            repo,
            canonical_url,
            expected_repo_id,
        })
    } else {
        // Try parsing as "owner/repo" slug
        let segments: Vec<&str> = trimmed
            .split('/')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();

        if segments.len() != 2 {
            return Err(AppError::InvalidRepositoryUrl(format!(
                "Target '{}' is neither a valid GitHub URL nor an owner/repo slug",
                trimmed
            )));
        }

        let owner = segments[0];
        let mut repo = segments[1];
        if let Some(stripped) = repo.strip_suffix(".git") {
            repo = stripped;
        }

        if !is_valid_github_id(owner) || !is_valid_github_id(repo) {
            return Err(AppError::InvalidRepositoryUrl(format!(
                "Invalid characters in owner '{}' or repo '{}'",
                owner, repo
            )));
        }

        let canonical_url = format!("https://github.com/{}/{}", owner, repo);
        Ok(ValidatedRepoIdentity {
            owner: owner.to_string(),
            repo: repo.to_string(),
            canonical_url,
            expected_repo_id,
        })
    }
}

/// Checks whether a host is an allowed GitHub API host, taking optional mock test base_url into account.
pub fn is_allowed_request_host(target_url: &Url, test_base_url: Option<&str>) -> bool {
    let host = match target_url.host_str() {
        Some(h) => h.to_lowercase(),
        None => return false,
    };

    // If mock base URL is configured in tests, allow matching origin
    if let Some(base) = test_base_url {
        if let Ok(base_parsed) = Url::parse(base) {
            if let Some(base_host) = base_parsed.host_str() {
                if host == base_host.to_lowercase()
                    && target_url.port_or_known_default()
                        == base_parsed.port_or_known_default()
                {
                    return true;
                }
            }
        }
    }

    // Production hosts allowed for GitHub operations
    matches!(
        host.as_str(),
        "api.github.com"
            | "github.com"
            | "raw.githubusercontent.com"
            | "codeload.github.com"
    )
}

/// Validates a redirect target URL during acquisition requests.
///
/// Returns `Ok(should_forward_token)`:
/// - Returns `true` if the redirect is on the exact same host/origin.
/// - Returns `false` if redirected to a different allowed host (e.g. codeload or raw CDN),
///   indicating the Authorization token MUST be stripped to prevent credential leakage.
/// - Returns `Err` if the target is unapproved, loopback, private IP, or cloud metadata.
pub fn validate_redirect_target(
    from_url: &Url,
    to_url: &Url,
    test_base_url: Option<&str>,
) -> Result<bool, AppError> {
    // Validate scheme
    if to_url.scheme() != "http" && to_url.scheme() != "https" {
        return Err(AppError::AcquisitionSecurity(format!(
            "Redirect target scheme '{}' is unsupported",
            to_url.scheme()
        )));
    }

    let to_host = to_url.host_str().unwrap_or("").to_lowercase();
    if to_host.is_empty() {
        return Err(AppError::AcquisitionSecurity(
            "Redirect target missing host".to_string(),
        ));
    }

    // Reject loopback, private, and metadata targets unconditionally unless test_base_url explicitly matches
    let is_test_match = if let Some(base) = test_base_url {
        if let Ok(base_parsed) = Url::parse(base) {
            to_host == base_parsed.host_str().unwrap_or("").to_lowercase()
                && to_url.port_or_known_default() == base_parsed.port_or_known_default()
        } else {
            false
        }
    } else {
        false
    };

    if !is_test_match {
        if to_host == "localhost"
            || to_host.ends_with(".localhost")
            || to_host.ends_with(".local")
            || to_host == "127.0.0.1"
            || to_host == "169.254.169.254"
            || to_host == "0.0.0.0"
            || to_host == "::1"
        {
            return Err(AppError::AcquisitionSecurity(format!(
                "Redirect to loopback/private host '{}' is blocked",
                to_host
            )));
        }

        if let Ok(ip) = to_host.parse::<IpAddr>() {
            if is_private_or_restricted_ip(&ip) {
                return Err(AppError::AcquisitionSecurity(format!(
                    "Redirect to restricted IP '{}' is blocked",
                    ip
                )));
            }
        }

        if !is_allowed_request_host(to_url, test_base_url) {
            return Err(AppError::AcquisitionSecurity(format!(
                "Redirect target host '{}' is not an allowed GitHub domain",
                to_host
            )));
        }
    }

    // Determine whether Authorization token can safely be forwarded
    let from_host = from_url.host_str().unwrap_or("").to_lowercase();
    let same_origin = from_host == to_host
        && from_url.port_or_known_default() == to_url.port_or_known_default();

    Ok(same_origin)
}

fn is_valid_github_id(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
}

fn is_private_or_restricted_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_documentation()
                || v4.octets()[0] == 0
        }
        IpAddr::V6(v6) => v6.is_loopback() || v6.is_multicast(),
    }
}

/// Securely constructs a GitHub contents API URL for a candidate file path and optional commit SHA.
///
/// Guarantees:
/// - Each path segment is individually percent-encoded without corrupting '/' path separators.
/// - Characters such as spaces, '#', '?', '%', and Unicode cannot alter query parameters,
///   create URL fragments, change host, or escape the '/repos/{owner}/{repo}/contents/' endpoint.
/// - Path traversal sequences ('..') are strictly rejected.
/// - The optional commit SHA is safely attached via query_pairs_mut as '?ref={sha}'.
pub fn build_github_contents_url(
    base_url: &str,
    owner: &str,
    repo: &str,
    path: &str,
    commit_sha: Option<&str>,
) -> Result<Url, AppError> {
    let mut url = Url::parse(base_url)
        .map_err(|e| AppError::InvalidRepositoryUrl(format!("Invalid base URL '{}': {}", base_url, e)))?;

    if url.scheme() != "http" && url.scheme() != "https" {
        return Err(AppError::InvalidRepositoryUrl(format!(
            "Unsupported scheme '{}'",
            url.scheme()
        )));
    }

    {
        let mut segments = url
            .path_segments_mut()
            .map_err(|_| AppError::InvalidRepositoryUrl("Base URL cannot be a base".to_string()))?;

        segments.push("repos");
        segments.push(owner);
        segments.push(repo);
        segments.push("contents");

        for segment in path.split('/') {
            if segment.is_empty() || segment == "." {
                continue;
            }
            if segment == ".." {
                return Err(AppError::AcquisitionSecurity(
                    "Path traversal sequence '..' detected in file path".to_string(),
                ));
            }
            segments.push(segment);
        }
    }

    if let Some(sha) = commit_sha {
        let trimmed = sha.trim();
        if !trimmed.is_empty() {
            url.query_pairs_mut().append_pair("ref", trimmed);
        }
    }

    Ok(url)
}
