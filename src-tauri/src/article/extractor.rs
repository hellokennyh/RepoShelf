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
    // Also support raw github.com/... -> https://github.com/...
    let normalized_link = if trimmed.starts_with("//") {
        format!("https:{}", trimmed)
    } else if trimmed.starts_with("github.com/") || trimmed.starts_with("www.github.com/") {
        format!("https://{}", trimmed)
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

/// Checks if a URL points to a WeChat Official Account article (mp.weixin.qq.com).
///
/// Strict security checks:
/// - Hostname must strictly be `mp.weixin.qq.com` (ignoring case), preventing spoofing domains
///   like `mp.weixin.qq.com.attacker.com`.
/// - Path must match supported article paths: `/s/` or `/s` (including `/s?__biz=...`).
pub fn is_wechat_article_url(url_str: &str) -> bool {
    let trimmed = url_str.trim();
    if let Ok(parsed) = Url::parse(trimmed) {
        if parsed.scheme() != "http" && parsed.scheme() != "https" {
            return false;
        }
        if let Some(host) = parsed.host_str() {
            if host.eq_ignore_ascii_case("mp.weixin.qq.com") {
                let path = parsed.path();
                return path.starts_with("/s/") || path == "/s" || path.starts_with("/s?");
            }
        }
    }
    false
}

/// Detects if a WeChat article response is an anti-scraping challenge, environment anomaly, or verification shell.
pub fn detect_wechat_challenge(document: &Html) -> bool {
    // 1. Check <title> element text
    if let Ok(title_sel) = Selector::parse("title") {
        for el in document.select(&title_sel) {
            let title = el.text().collect::<String>();
            if title.contains("验证码")
                || title.contains("环境异常")
                || title.contains("访问过于频繁")
                || title.contains("安全验证")
                || title.contains("操作过于频繁")
            {
                return true;
            }
        }
    }

    // 2. Check for WeChat verification / error message containers
    let challenge_selectors = [
        "#verify_code",
        ".weui-msg__title",
        ".weui-msg",
        "#js_verify",
        ".page_verify",
    ];
    for sel_str in &challenge_selectors {
        if let Ok(sel) = Selector::parse(sel_str) {
            for el in document.select(&sel) {
                let txt = el.text().collect::<String>();
                if txt.contains("验证")
                    || txt.contains("异常")
                    || txt.contains("频繁")
                    || txt.contains("安全")
                {
                    return true;
                }
            }
        }
    }

    false
}

/// Extracts GitHub candidates from a specific HTML element subtree (or whole document).
fn extract_from_element(
    element: &scraper::ElementRef,
    base: Option<&Url>,
    seen_identities: &mut HashSet<(String, String)>,
    candidates: &mut Vec<ExtractedRepoCandidate>,
    total_found_count: &mut usize,
) {
    let a_selector = Selector::parse("a[href]").expect("valid anchor selector");

    // 1. Extract from <a href="..."> elements
    for el in element.select(&a_selector) {
        if let Some(href) = el.value().attr("href") {
            let href = href.trim();
            if href.is_empty() {
                continue;
            }

            let resolved_str = if href.starts_with('/') && !href.starts_with("//") {
                if let Some(b) = base {
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
                    *total_found_count += 1;
                    if candidates.len() < MAX_UNIQUE_CANDIDATES {
                        candidates.push(candidate);
                    }
                }
            }
        }
    }

    // 2. Scan text for naked GitHub URLs, handling embedded URLs in Chinese text and punctuation
    for text_node in element.text() {
        for url_str in find_github_urls_in_text(text_node) {
            if let Some(candidate) = extract_repository_root_from_github_link(url_str) {
                let key = (
                    candidate.owner.to_lowercase(),
                    candidate.repo.to_lowercase(),
                );
                if seen_identities.insert(key) {
                    *total_found_count += 1;
                    if candidates.len() < MAX_UNIQUE_CANDIDATES {
                        candidates.push(candidate);
                    }
                }
            }
        }
    }
}

/// Helper that locates all GitHub URLs inside arbitrary text (including Chinese text without spaces).
fn find_github_urls_in_text(text: &str) -> Vec<&str> {
    let mut results = Vec::new();
    let mut search_from = 0;

    while let Some(rel_idx) = text[search_from..].find("github.com/") {
        let match_idx = search_from + rel_idx;

        // Determine prefix start (e.g. https:// or http:// or //)
        let prefix_start = if match_idx >= 8 && &text[match_idx - 8..match_idx] == "https://" {
            match_idx - 8
        } else if match_idx >= 7 && &text[match_idx - 7..match_idx] == "http://" {
            match_idx - 7
        } else if match_idx >= 2 && &text[match_idx - 2..match_idx] == "//" {
            match_idx - 2
        } else {
            match_idx
        };

        // Determine URL end: until whitespace, tag bracket, quotes, or Chinese punctuation
        let remainder = &text[match_idx..];
        let url_end_offset = remainder
            .find(|c: char| {
                c.is_whitespace()
                    || c == '<'
                    || c == '>'
                    || c == '"'
                    || c == '\''
                    || c == '`'
                    || c == '。'
                    || c == '，'
                    || c == '；'
                    || c == '！'
                    || c == '？'
                    || c == '、'
                    || c == '：'
                    || c == '）'
                    || c == '（'
                    || c == '【'
                    || c == '】'
                    || c == '《'
                    || c == '》'
                    || c == '“'
                    || c == '”'
            })
            .unwrap_or(remainder.len());

        let mut raw_url = &text[prefix_start..match_idx + url_end_offset];
        // Trim trailing ASCII punctuation
        raw_url = raw_url.trim_end_matches(|c: char| {
            c == '.'
                || c == ','
                || c == ';'
                || c == ')'
                || c == ']'
                || c == '}'
                || c == '>'
                || c == '!'
                || c == '?'
        });

        if !raw_url.is_empty() {
            results.push(raw_url);
        }

        search_from = match_idx + 11;
    }

    results
}

/// Parses HTML content and extracts unique GitHub repository candidates.
///
/// - Scans `<a href="...">` anchor tags and naked text nodes.
/// - Prioritizes dedicated article containers (`#js_content` for WeChat, `<article>` or `<main>` for general articles).
/// - Detects WeChat anti-scraping challenge / verification shells and returns `ArticleChallenge`.
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
    let base = base_url.and_then(|u| Url::parse(u).ok());

    let mut seen_identities = HashSet::new();
    let mut candidates = Vec::new();
    let mut total_found_count = 0;

    let is_wechat = base_url.map(is_wechat_article_url).unwrap_or(false);

    if is_wechat {
        // Check for WeChat verification / security challenge page
        if detect_wechat_challenge(&document) {
            return Err(AppError::ArticleChallenge(
                "微信公众号安全验证或环境异常，无法自动提取内容".to_string(),
            ));
        }

        // Primary WeChat article container is #js_content
        let js_content_sel = Selector::parse("#js_content").expect("valid selector");
        if let Some(js_content) = document.select(&js_content_sel).next() {
            extract_from_element(
                &js_content,
                base.as_ref(),
                &mut seen_identities,
                &mut candidates,
                &mut total_found_count,
            );
        } else {
            // #js_content missing on mp.weixin.qq.com indicates challenge shell or unsupported format
            return Err(AppError::ArticleChallenge(
                "未能识别微信公众号文章正文内容，可能需要验证或页面格式不受支持".to_string(),
            ));
        }
    } else {
        // General articles: prioritize <article> or <main> container if present
        let article_sel = Selector::parse("article, main").expect("valid selector");
        let mut extracted_from_container = false;

        for container in document.select(&article_sel) {
            extract_from_element(
                &container,
                base.as_ref(),
                &mut seen_identities,
                &mut candidates,
                &mut total_found_count,
            );
            if !candidates.is_empty() {
                extracted_from_container = true;
            }
        }

        // Fallback to entire document if no candidates in <article> / <main>
        if !extracted_from_container {
            let root_elem = document.root_element();
            extract_from_element(
                &root_elem,
                base.as_ref(),
                &mut seen_identities,
                &mut candidates,
                &mut total_found_count,
            );
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

    #[test]
    fn test_wechat_url_identification() {
        // Valid WeChat article URLs
        assert!(is_wechat_article_url(
            "https://mp.weixin.qq.com/s/abcdef123456"
        ));
        assert!(is_wechat_article_url(
            "https://mp.weixin.qq.com/s?__biz=MzA4...&mid=123"
        ));
        assert!(is_wechat_article_url("http://mp.weixin.qq.com/s/short_id"));

        // Domain spoofing attempts MUST be rejected
        assert!(!is_wechat_article_url(
            "https://mp.weixin.qq.com.attacker.example.com/s/test"
        ));
        assert!(!is_wechat_article_url(
            "https://attacker.com/mp.weixin.qq.com/s/test"
        ));
        assert!(!is_wechat_article_url("https://example.com/s/test"));

        // Unsupported WeChat paths
        assert!(!is_wechat_article_url(
            "https://mp.weixin.qq.com/cgi-bin/home"
        ));
    }

    #[test]
    fn test_wechat_js_content_targeted_extraction() {
        let html = r#"
            <html>
                <head><title>WeChat Article</title></head>
                <body>
                    <div id="js_content">
                        <p>In article body: <a href="https://github.com/astral-sh/uv">Fast Python package manager</a></p>
                        <p>Also mentioned in text: https://github.com/astral-sh/ruff。</p>
                    </div>
                    <div class="footer_recommendations">
                        <a href="https://github.com/outside-org/noise">Noise outside article content</a>
                    </div>
                </body>
            </html>
        "#;

        let (candidates, truncated) =
            extract_github_candidates_from_html(html, Some("https://mp.weixin.qq.com/s/mock_id"))
                .unwrap();
        assert!(!truncated);
        assert_eq!(candidates.len(), 2);
        assert_eq!(
            candidates[0].canonical_url,
            "https://github.com/astral-sh/uv"
        );
        assert_eq!(
            candidates[1].canonical_url,
            "https://github.com/astral-sh/ruff"
        );
    }

    #[test]
    fn test_wechat_challenge_detection() {
        let html_verification = r#"
            <html>
                <head><title>环境异常 - 安全验证</title></head>
                <body>
                    <div class="weui-msg">
                        <h2 class="weui-msg__title">当前操作过于频繁，请输入验证码</h2>
                        <div id="verify_code"></div>
                    </div>
                </body>
            </html>
        "#;

        let res = extract_github_candidates_from_html(
            html_verification,
            Some("https://mp.weixin.qq.com/s/test"),
        );
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().code(), "ChallengeDetected");
    }

    #[test]
    fn test_chinese_fullwidth_punctuation_trimming() {
        let html = r#"
            <html>
                <body>
                    <p>推荐优质开源项目：https://github.com/tauri-apps/tauri。同时请参考（https://github.com/tokio-rs/tokio）！</p>
                </body>
            </html>
        "#;

        let (candidates, _) = extract_github_candidates_from_html(html, None).unwrap();
        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0].owner, "tauri-apps");
        assert_eq!(candidates[0].repo, "tauri");
        assert_eq!(candidates[1].owner, "tokio-rs");
        assert_eq!(candidates[1].repo, "tokio");
    }
}
