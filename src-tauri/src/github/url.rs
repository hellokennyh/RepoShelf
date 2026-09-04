use url::Url;

use crate::error::AppError;

/// Parses and normalizes a GitHub repository URL into (owner, repo, normalized_url).
///
/// Supported accepted formats:
/// - https://github.com/owner/repo
/// - https://github.com/owner/repo/
/// - http://github.com/owner/repo
/// - https://www.github.com/owner/repo
/// - With query parameters or fragments (e.g. ?tab=readme, #readme)
///
/// Rejects:
/// - https://github.com/ (no owner/repo)
/// - https://github.com/owner (no repo)
/// - Subpages like /issues, /pulls, /tree/*, /blob/*, /commit/*
/// - Non-github hosts (e.g. gitlab.com, bitbucket.org)
/// - Malformed or non-HTTP URLs
pub fn parse_github_repo_url(input: &str) -> Result<(String, String, String), AppError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(AppError::InvalidRepositoryUrl(
            "Repository URL cannot be empty".to_string(),
        ));
    }

    let parsed = Url::parse(trimmed)
        .map_err(|e| AppError::InvalidRepositoryUrl(format!("Failed to parse URL: {}", e)))?;

    // Validate scheme
    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return Err(AppError::InvalidRepositoryUrl(format!(
            "Unsupported URL scheme '{}'. Must be http or https",
            parsed.scheme()
        )));
    }

    // Validate host (case-insensitive)
    let host = parsed.host_str().unwrap_or("").to_lowercase();
    if host != "github.com" && host != "www.github.com" {
        return Err(AppError::InvalidRepositoryUrl(format!(
            "Only github.com repositories are supported, got host '{}'",
            host
        )));
    }

    // Path segments extraction (excluding empty segments)
    let segments: Vec<&str> = parsed
        .path_segments()
        .map(|s| s.filter(|seg| !seg.is_empty()).collect())
        .unwrap_or_default();

    if segments.len() < 2 {
        return Err(AppError::InvalidRepositoryUrl(
            "URL must contain both owner and repository name (e.g. https://github.com/owner/repo)"
                .to_string(),
        ));
    }

    if segments.len() > 2 {
        return Err(AppError::InvalidRepositoryUrl(
            "URL must point directly to a repository root, not subpaths like issues, pulls, or files"
                .to_string(),
        ));
    }

    let owner = segments[0];
    let mut repo = segments[1];

    // Strip optional .git suffix
    if let Some(stripped) = repo.strip_suffix(".git") {
        repo = stripped;
    }

    if owner.is_empty() || repo.is_empty() {
        return Err(AppError::InvalidRepositoryUrl(
            "Owner and repository name cannot be empty".to_string(),
        ));
    }

    // Validate owner and repo characters (alphanumeric, hyphens, underscores, periods)
    if !is_valid_github_identifier(owner) || !is_valid_github_identifier(repo) {
        return Err(AppError::InvalidRepositoryUrl(
            "Owner or repository contains invalid characters".to_string(),
        ));
    }

    let canonical_url = format!("https://github.com/{}/{}", owner, repo);
    Ok((owner.to_string(), repo.to_string(), canonical_url))
}

fn is_valid_github_identifier(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_urls() {
        let (owner, repo, canonical) =
            parse_github_repo_url("https://github.com/rust-lang/rust").unwrap();
        assert_eq!(owner, "rust-lang");
        assert_eq!(repo, "rust");
        assert_eq!(canonical, "https://github.com/rust-lang/rust");

        // Trailing slash
        let (_, _, canonical) =
            parse_github_repo_url("https://github.com/rust-lang/rust/").unwrap();
        assert_eq!(canonical, "https://github.com/rust-lang/rust");

        // HTTP scheme
        let (_, _, canonical) = parse_github_repo_url("http://github.com/tokio-rs/tokio").unwrap();
        assert_eq!(canonical, "https://github.com/tokio-rs/tokio");

        // www subdomain
        let (_, _, canonical) =
            parse_github_repo_url("https://www.github.com/tauri-apps/tauri").unwrap();
        assert_eq!(canonical, "https://github.com/tauri-apps/tauri");

        // Query params and fragments
        let (_, _, canonical) =
            parse_github_repo_url("https://github.com/facebook/react?tab=readme-ov-file#readme")
                .unwrap();
        assert_eq!(canonical, "https://github.com/facebook/react");

        // .git suffix
        let (owner, repo, canonical) =
            parse_github_repo_url("https://github.com/vuejs/core.git").unwrap();
        assert_eq!(owner, "vuejs");
        assert_eq!(repo, "core");
        assert_eq!(canonical, "https://github.com/vuejs/core");
    }

    #[test]
    fn test_rejected_urls() {
        // Root only
        assert!(parse_github_repo_url("https://github.com/").is_err());
        assert!(parse_github_repo_url("https://github.com").is_err());

        // Owner only
        assert!(parse_github_repo_url("https://github.com/rust-lang").is_err());
        assert!(parse_github_repo_url("https://github.com/rust-lang/").is_err());

        // Subpaths (issues, pulls, tree, blob, commit)
        assert!(parse_github_repo_url("https://github.com/owner/repo/issues").is_err());
        assert!(parse_github_repo_url("https://github.com/owner/repo/pulls").is_err());
        assert!(parse_github_repo_url("https://github.com/owner/repo/tree/main").is_err());
        assert!(parse_github_repo_url("https://github.com/owner/repo/blob/main/file").is_err());
        assert!(parse_github_repo_url("https://github.com/owner/repo/commit/123456").is_err());

        // Other hosts
        assert!(parse_github_repo_url("https://gitlab.com/owner/repo").is_err());
        assert!(parse_github_repo_url("https://bitbucket.org/owner/repo").is_err());
        assert!(parse_github_repo_url("https://example.com/owner/repo").is_err());

        // Empty and non-URLs
        assert!(parse_github_repo_url("").is_err());
        assert!(parse_github_repo_url("   ").is_err());
        assert!(parse_github_repo_url("not a url").is_err());
        assert!(parse_github_repo_url("ftp://github.com/owner/repo").is_err());
    }
}
