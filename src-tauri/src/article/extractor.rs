use scraper::{Html, Selector};
use std::collections::HashSet;
use url::Url;

use crate::article::models::ExtractedRepoCandidate;
use crate::error::AppError;

const MAX_UNIQUE_CANDIDATES: usize = 50;

const RESERVED_ROOT_NAMESPACES: &[&str] = &[
    "about",
    "codespaces",
    "collections",
    "contact",
    "customer-stories",
    "discussions",
    "enterprise",
    "events",
    "explore",
    "features",
    "issues",
    "join",
    "login",
    "logout",
    "marketplace",
    "notifications",
    "organizations",
    "orgs",
    "pricing",
    "pulls",
    "readme",
    "search",
    "security",
    "settings",
    "site",
    "sponsors",
    "stars",
    "topics",
    "trending",
];

/// Extracts and normalizes the repository root from an arbitrary GitHub link string.
///
/// In the article context, deep paths (issues, pulls, tree, blob, releases, commits)
/// are safely normalized to the repository root `https://github.com/{owner}/{repo}`.
///
/// Rejects:
/// - Non-github hosts (including gist.github.com)
/// - Single-segment paths (user or organization profiles like github.com/torvalds)
/// - Reserved GitHub namespaces (settings, marketplace, topics, orgs, login, search, etc.)
/// - Empty or invalid owner/repo identifiers
pub fn extract_repository_root_from_github_link(raw_link: &str) -> Option<ExtractedRepoCandidate> {
    let trimmed = raw_link.trim();
    if trimmed.is_empty() {
        return None;
    }

    // Support protocol-relative URLs: //github.com/... -> https://github.com/...
    let normalized_link = if trimmed.starts_with("//") {
        format!("https:{}", trimmed)
    } else {
        trimmed.to_string()
    };

    let parsed = Url::parse(&normalized_link).ok()?;

    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return None;
    }

    let host = parsed.host_str()?.to_lowercase();
    if host != "github.com" && host != "www.github.com" {
        return None;
    }

    let segments: Vec<&str> = parsed.path_segments()?.filter(|s| !s.is_empty()).collect();

    // Must have at least owner and repo (2 segments). Single segment is user profile.
    if segments.len() < 2 {
        return None;
    }

    let owner = segments[0];
    let mut repo = segments[1];

    // Strip optional .git suffix
    if let Some(stripped) = repo.strip_suffix(".git") {
        repo = stripped;
    }

    if owner.is_empty() || repo.is_empty() {
        return None;
    }

    // Exclude reserved root namespaces
    let owner_lower = owner.to_lowercase();
    if RESERVED_ROOT_NAMESPACES.contains(&owner_lower.as_str()) {
        return None;
    }

    // Validate identifier characters
    if !is_valid_identifier(owner) || !is_valid_identifier(repo) {
        return None;
    }

    let canonical_url = format!("https://github.com/{}/{}", owner, repo);

    Some(ExtractedRepoCandidate {
        owner: owner.to_string(),
        repo: repo.to_string(),
        canonical_url,
    })
}

fn is_valid_identifier(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
}

/// Parses HTML content and extracts unique GitHub repository candidates.
///
/// - Scans `<a href="...">` anchor tags.
/// - Scans plain text nodes for naked GitHub links.
/// - Normalizes deep links to repository roots.
/// - Deduplicates based on case-insensitive (owner, repo) identity.
/// - Preserves first-seen document order.
/// - Limits to at most 50 unique candidates (flags `truncated = true` if more found).
/// - Returns `AppError::ArticleNoRepositories` if 0 candidates found.
pub fn extract_github_candidates_from_html(
    html_content: &str,
    base_url: Option<&str>,
) -> Result<(Vec<ExtractedRepoCandidate>, bool), AppError> {
    let document = Html::parse_document(html_content);
    let a_selector = Selector::parse("a[href]").expect("valid anchor selector");

    let base = base_url.and_then(|u| Url::parse(u).ok());

    let mut seen_identities = HashSet::new();
    let mut candidates = Vec::new();
    let mut total_found_count = 0;

    // 1. Extract from <a href="..."> elements
    for element in document.select(&a_selector) {
        if let Some(href) = element.value().attr("href") {
            let href = href.trim();
            if href.is_empty() {
                continue;
            }

            // Resolve relative URLs if base_url is available
            let resolved_str = if href.starts_with('/') && !href.starts_with("//") {
                if let Some(ref b) = base {
                    b.join(href)
                        .map(|u| u.to_string())
                        .unwrap_or_else(|_| href.to_string())
                } else {
                    href.to_string()
                }
            } else {
                href.to_string()
            };

            if let Some(candidate) = extract_repository_root_from_github_link(&resolved_str) {
                let key = (
                    candidate.owner.to_lowercase(),
                    candidate.repo.to_lowercase(),
                );
                if seen_identities.insert(key) {
                    total_found_count += 1;
                    if candidates.len() < MAX_UNIQUE_CANDIDATES {
                        candidates.push(candidate);
                    }
                }
            }
        }
    }

    // 2. Scan text for naked GitHub URLs
    for text_node in document.root_element().text() {
        for word in text_node.split_whitespace() {
            let clean_word = word.trim_matches(|c: char| {
                c == '('
                    || c == ')'
                    || c == '['
                    || c == ']'
                    || c == '<'
                    || c == '>'
                    || c == '"'
                    || c == '\''
                    || c == ','
                    || c == ';'
                    || c == '.'
            });

            if clean_word.contains("github.com/") {
                if let Some(candidate) = extract_repository_root_from_github_link(clean_word) {
                    let key = (
                        candidate.owner.to_lowercase(),
                        candidate.repo.to_lowercase(),
                    );
                    if seen_identities.insert(key) {
                        total_found_count += 1;
                        if candidates.len() < MAX_UNIQUE_CANDIDATES {
                            candidates.push(candidate);
                        }
                    }
                }
            }
        }
    }

    if candidates.is_empty() {
        return Err(AppError::ArticleNoRepositories(
            "未在文章中发现有效的 GitHub 仓库链接".to_string(),
        ));
    }

    let truncated = total_found_count > MAX_UNIQUE_CANDIDATES;
    Ok((candidates, truncated))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_root_from_various_links() {
        // Root link
        let c =
            extract_repository_root_from_github_link("https://github.com/rust-lang/rust").unwrap();
        assert_eq!(c.owner, "rust-lang");
        assert_eq!(c.repo, "rust");
        assert_eq!(c.canonical_url, "https://github.com/rust-lang/rust");

        // Trailing slash and .git
        let c =
            extract_repository_root_from_github_link("https://github.com/vuejs/core.git/").unwrap();
        assert_eq!(c.owner, "vuejs");
        assert_eq!(c.repo, "core");
        assert_eq!(c.canonical_url, "https://github.com/vuejs/core");

        // Protocol-relative
        let c = extract_repository_root_from_github_link("//github.com/tokio-rs/tokio").unwrap();
        assert_eq!(c.owner, "tokio-rs");
        assert_eq!(c.repo, "tokio");

        // Deep links
        let c = extract_repository_root_from_github_link(
            "https://github.com/facebook/react/issues/1234",
        )
        .unwrap();
        assert_eq!(c.owner, "facebook");
        assert_eq!(c.repo, "react");

        let c =
            extract_repository_root_from_github_link("https://github.com/facebook/react/pull/5678")
                .unwrap();
        assert_eq!(c.owner, "facebook");
        assert_eq!(c.repo, "react");

        let c = extract_repository_root_from_github_link(
            "https://github.com/facebook/react/tree/main/packages/react",
        )
        .unwrap();
        assert_eq!(c.owner, "facebook");
        assert_eq!(c.repo, "react");

        let c = extract_repository_root_from_github_link(
            "https://github.com/facebook/react/blob/main/README.md#getting-started",
        )
        .unwrap();
        assert_eq!(c.owner, "facebook");
        assert_eq!(c.repo, "react");

        let c = extract_repository_root_from_github_link(
            "https://github.com/facebook/react/releases/tag/v18.2.0",
        )
        .unwrap();
        assert_eq!(c.owner, "facebook");
        assert_eq!(c.repo, "react");
    }

    #[test]
    fn test_rejected_links() {
        // User profile (single segment)
        assert!(extract_repository_root_from_github_link("https://github.com/torvalds").is_none());
        assert!(extract_repository_root_from_github_link("https://github.com/torvalds/").is_none());

        // Reserved namespaces
        assert!(
            extract_repository_root_from_github_link("https://github.com/settings/profile")
                .is_none()
        );
        assert!(
            extract_repository_root_from_github_link("https://github.com/marketplace/actions")
                .is_none()
        );
        assert!(
            extract_repository_root_from_github_link("https://github.com/topics/rust").is_none()
        );
        assert!(
            extract_repository_root_from_github_link("https://github.com/search?q=reposhelf")
                .is_none()
        );
        assert!(extract_repository_root_from_github_link("https://github.com/login").is_none());
        assert!(
            extract_repository_root_from_github_link("https://github.com/orgs/myorg/teams")
                .is_none()
        );

        // Gist
        assert!(
            extract_repository_root_from_github_link("https://gist.github.com/user/12345")
                .is_none()
        );

        // Non-github
        assert!(
            extract_repository_root_from_github_link("https://gitlab.com/owner/repo").is_none()
        );
        assert!(
            extract_repository_root_from_github_link("https://example.com/owner/repo").is_none()
        );
    }

    #[test]
    fn test_deduplication_and_order() {
        let html = r#"
            <html>
                <body>
                    <p>Check out <a href="https://github.com/rust-lang/rust/issues/1">Rust Issue</a>.</p>
                    <p>Also see <a href="https://github.com/rust-lang/rust">Rust Root</a> and <a href="https://github.com/tokio-rs/tokio">Tokio</a>.</p>
                    <p>And Rust again <a href="https://github.com/rust-lang/rust/blob/master/README.md">README</a>.</p>
                </body>
            </html>
        "#;

        let (candidates, truncated) = extract_github_candidates_from_html(html, None).unwrap();
        assert!(!truncated);
        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0].owner, "rust-lang");
        assert_eq!(candidates[0].repo, "rust");
        assert_eq!(candidates[1].owner, "tokio-rs");
        assert_eq!(candidates[1].repo, "tokio");
    }

    #[test]
    fn test_zero_repositories() {
        let html = "<html><body><p>Hello world without any github link</p></body></html>";
        let res = extract_github_candidates_from_html(html, None);
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().code(), "NoGitHubRepositories");
    }
}
