use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use reposhelf_lib::article::extractor::extract_github_candidates_from_html;
use reposhelf_lib::article::models::ExtractedRepoCandidate;
use reposhelf_lib::article::security::fetch_article_html;
use reposhelf_lib::article::validator::validate_candidates;
use reposhelf_lib::github::url::parse_github_repo_url;
use reposhelf_lib::github::GitHubClient;

// =============================================================================
// 1. SSRF Input URL & IP Protection Tests
// =============================================================================

#[tokio::test]
async fn test_ssrf_rejects_forbidden_ip_literals_and_localhost() {
    let forbidden_urls = [
        "http://127.0.0.1",
        "http://127.0.0.1:8080/article",
        "http://localhost",
        "http://localhost:3000/post",
        "http://0.0.0.0",
        "http://0.0.0.0:8000",
        "http://10.0.0.1",
        "http://10.1.2.3/blog",
        "http://172.16.0.1",
        "http://172.31.255.255/article",
        "http://192.168.1.1",
        "http://192.168.0.100:8080",
        "http://169.254.169.254/latest/meta-data",
        "http://169.254.1.1",
        "http://198.18.0.1",
        "http://198.19.0.1",
        "http://[::1]/",
        "http://[::1]:8080/article",
        "http://[fc00::1]/",
        "http://[fe80::1]/",
        "http://[::ffff:127.0.0.1]/",
        "http://[::ffff:169.254.169.254]/",
    ];

    for url in forbidden_urls {
        let res = fetch_article_html(url, None, None).await;
        assert!(
            res.is_err(),
            "Expected URL {} to be rejected by SSRF protection",
            url
        );
        let err = res.unwrap_err();
        assert_eq!(
            err.code(),
            "BlockedAddress",
            "Expected BlockedAddress for {}",
            url
        );
    }
}

#[tokio::test]
async fn test_ssrf_rejects_hostname_resolving_to_private_ip() {
    let mut mock_dns: HashMap<String, Vec<IpAddr>> = HashMap::new();
    mock_dns.insert(
        "evil-rebinding.example.com".to_string(),
        vec![IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))],
    );
    mock_dns.insert(
        "cloud-metadata.example.com".to_string(),
        vec![IpAddr::V4(Ipv4Addr::new(169, 254, 169, 254))],
    );
    mock_dns.insert(
        "internal-lan.example.com".to_string(),
        vec![IpAddr::V4(Ipv4Addr::new(192, 168, 1, 50))],
    );

    for (host, _) in &mock_dns {
        let url = format!("http://{}/article", host);
        let res = fetch_article_html(&url, Some(&mock_dns), None).await;
        assert!(res.is_err(), "Expected {} to be blocked", url);
        assert_eq!(res.unwrap_err().code(), "BlockedAddress");
    }
}

#[tokio::test]
async fn test_ssrf_rejects_unsupported_schemes() {
    let invalid_schemes = [
        "file:///etc/passwd",
        "ftp://example.com/file",
        "data:text/html,<h1>Hello</h1>",
        "javascript:alert(1)",
        "ws://example.com/socket",
        "wss://example.com/socket",
        "mailto:admin@example.com",
        "custom://app/start",
    ];

    for url in invalid_schemes {
        let res = fetch_article_html(url, None, None).await;
        assert!(res.is_err(), "Expected {} to be rejected", url);
        assert_eq!(res.unwrap_err().code(), "UnsupportedScheme");
    }
}

// =============================================================================
// 2. Redirect SSRF Protection Tests
// =============================================================================

#[tokio::test]
async fn test_redirect_to_private_ip_is_blocked() {
    let server = MockServer::start().await;

    // Public mock redirects to internal loopback
    Mock::given(method("GET"))
        .and(path("/start"))
        .respond_with(
            ResponseTemplate::new(302).insert_header("Location", "http://192.168.1.1:8080/secret"),
        )
        .mount(&server)
        .await;

    let initial_url = format!("{}/start", server.uri());
    // Allowed hosts permits initial server connection, but target redirect to 127.0.0.1:8080 has no bypass
    let allowed = vec![server.address().ip().to_string()];
    let res = fetch_article_html(&initial_url, None, Some(&allowed)).await;

    assert!(res.is_err());
    assert_eq!(res.unwrap_err().code(), "BlockedAddress");
}

#[tokio::test]
async fn test_redirect_to_cloud_metadata_is_blocked() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/redirect-meta"))
        .respond_with(
            ResponseTemplate::new(302)
                .insert_header("Location", "http://169.254.169.254/latest/meta-data"),
        )
        .mount(&server)
        .await;

    let initial_url = format!("{}/redirect-meta", server.uri());
    let allowed = vec![server.address().ip().to_string()];
    let res = fetch_article_html(&initial_url, None, Some(&allowed)).await;

    assert!(res.is_err());
    assert_eq!(res.unwrap_err().code(), "BlockedAddress");
}

#[tokio::test]
async fn test_redirect_exceeding_max_limit_is_rejected() {
    let server = MockServer::start().await;

    // Set up a 4-hop redirect loop
    Mock::given(method("GET"))
        .and(path("/hop1"))
        .respond_with(
            ResponseTemplate::new(302).insert_header("Location", format!("{}/hop2", server.uri())),
        )
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/hop2"))
        .respond_with(
            ResponseTemplate::new(302).insert_header("Location", format!("{}/hop3", server.uri())),
        )
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/hop3"))
        .respond_with(
            ResponseTemplate::new(302).insert_header("Location", format!("{}/hop4", server.uri())),
        )
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/hop4"))
        .respond_with(
            ResponseTemplate::new(302)
                .insert_header("Location", format!("{}/destination", server.uri())),
        )
        .mount(&server)
        .await;

    let initial_url = format!("{}/hop1", server.uri());
    let allowed = vec![server.address().ip().to_string(), "127.0.0.1".to_string()];
    let res = fetch_article_html(&initial_url, None, Some(&allowed)).await;

    assert!(res.is_err());
    assert_eq!(res.unwrap_err().code(), "TooManyRedirects");
}

#[tokio::test]
async fn test_valid_redirect_under_limit_succeeds() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/step1"))
        .respond_with(
            ResponseTemplate::new(302).insert_header("Location", format!("{}/step2", server.uri())),
        )
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/step2"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Type", "text/html")
                .set_body_string("<html><body><h1>Final Destination</h1></body></html>"),
        )
        .mount(&server)
        .await;

    let initial_url = format!("{}/step1", server.uri());
    let allowed = vec![server.address().ip().to_string(), "127.0.0.1".to_string()];
    let res = fetch_article_html(&initial_url, None, Some(&allowed)).await;

    assert!(res.is_ok());
    assert!(res.unwrap().contains("Final Destination"));
}

// =============================================================================
// 3. Response Budget & Content Type Tests
// =============================================================================

#[tokio::test]
async fn test_content_length_over_2mb_rejected_early() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/huge-header"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Type", "text/html")
                .set_body_bytes(vec![b'A'; 2_500_000]),
        )
        .mount(&server)
        .await;

    let url = format!("{}/huge-header", server.uri());
    let allowed = vec![server.address().ip().to_string()];
    let res = fetch_article_html(&url, None, Some(&allowed)).await;

    assert!(res.is_err());
    assert_eq!(res.unwrap_err().code(), "ResponseTooLarge");
}

#[tokio::test]
async fn test_streaming_body_over_2mb_aborts() {
    let server = MockServer::start().await;

    // Generate 2.5 MiB of HTML
    let oversized_html = format!("<html><body>{}</body></html>", "a".repeat(2500 * 1024));

    Mock::given(method("GET"))
        .and(path("/huge-stream"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Type", "text/html")
                .set_body_string(oversized_html),
        )
        .mount(&server)
        .await;

    let url = format!("{}/huge-stream", server.uri());
    let allowed = vec![server.address().ip().to_string()];
    let res = fetch_article_html(&url, None, Some(&allowed)).await;

    assert!(res.is_err());
    assert_eq!(res.unwrap_err().code(), "ResponseTooLarge");
}

#[tokio::test]
async fn test_unsupported_content_type_rejected() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/document.pdf"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Type", "application/pdf")
                .set_body_bytes(b"%PDF-1.4 dummy pdf bytes"),
        )
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/image.png"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Type", "image/png")
                .set_body_bytes(b"\x89PNG dummy png bytes"),
        )
        .mount(&server)
        .await;

    let allowed = vec![server.address().ip().to_string()];

    let res_pdf = fetch_article_html(
        &format!("{}/document.pdf", server.uri()),
        None,
        Some(&allowed),
    )
    .await;
    assert!(res_pdf.is_err());
    assert_eq!(res_pdf.unwrap_err().code(), "UnsupportedContentType");

    let res_png =
        fetch_article_html(&format!("{}/image.png", server.uri()), None, Some(&allowed)).await;
    assert!(res_png.is_err());
    assert_eq!(res_png.unwrap_err().code(), "UnsupportedContentType");
}

// =============================================================================
// 4. HTML Candidate Extraction & Normalization Tests
// =============================================================================

#[test]
fn test_html_extraction_root_and_deep_links() {
    let html = r#"
        <!DOCTYPE html>
        <html>
        <head><title>Top Rust Libraries</title></head>
        <body>
            <h1>Awesome Projects</h1>
            <p>1. Root: <a href="https://github.com/rust-lang/rust">Rust</a></p>
            <p>2. Issue: <a href="https://github.com/rust-lang/rust/issues/1234">Issue #1234</a></p>
            <p>3. Tree: <a href="https://github.com/rust-lang/rust/tree/master/src">Source</a></p>
            <p>4. Blob: <a href="https://github.com/rust-lang/rust/blob/master/README.md">README</a></p>
            <p>5. Release: <a href="https://github.com/rust-lang/rust/releases/tag/1.84.0">v1.84</a></p>
            <p>6. Pull: <a href="https://github.com/rust-lang/rust/pull/999">PR 999</a></p>
            <p>7. Tokio: <a href="https://github.com/tokio-rs/tokio.git?tab=readme#readme">Tokio with query</a></p>
            <p>8. Protocol-relative: <a href="//github.com/tauri-apps/tauri/">Tauri</a></p>
        </body>
        </html>
    "#;

    let (candidates, truncated) = extract_github_candidates_from_html(html, None).unwrap();
    assert!(!truncated);
    // Should deduplicate rust-lang/rust into 1 candidate, plus tokio and tauri = 3 total
    assert_eq!(candidates.len(), 3);
    assert_eq!(candidates[0].owner, "rust-lang");
    assert_eq!(candidates[0].repo, "rust");
    assert_eq!(
        candidates[0].canonical_url,
        "https://github.com/rust-lang/rust"
    );

    assert_eq!(candidates[1].owner, "tokio-rs");
    assert_eq!(candidates[1].repo, "tokio");
    assert_eq!(
        candidates[1].canonical_url,
        "https://github.com/tokio-rs/tokio"
    );

    assert_eq!(candidates[2].owner, "tauri-apps");
    assert_eq!(candidates[2].repo, "tauri");
    assert_eq!(
        candidates[2].canonical_url,
        "https://github.com/tauri-apps/tauri"
    );
}

#[test]
fn test_html_extraction_filters_non_repo_urls() {
    let html = r#"
        <html>
        <body>
            <a href="https://github.com/torvalds">User Profile</a>
            <a href="https://github.com/settings/tokens">Settings</a>
            <a href="https://github.com/marketplace/actions">Marketplace</a>
            <a href="https://github.com/topics/rust">Topics</a>
            <a href="https://github.com/search?q=test">Search</a>
            <a href="https://github.com/login">Login</a>
            <a href="https://github.com/orgs/my-org/teams">Orgs</a>
            <a href="https://gist.github.com/octocat/123456">Gist</a>
            <a href="https://gitlab.com/gitlab-org/gitlab">GitLab</a>
            <a href="https://github.com/valid-org/valid-repo">Valid Repo</a>
        </body>
        </html>
    "#;

    let (candidates, truncated) = extract_github_candidates_from_html(html, None).unwrap();
    assert!(!truncated);
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].owner, "valid-org");
    assert_eq!(candidates[0].repo, "valid-repo");
}

#[test]
fn test_html_extraction_truncation_at_50() {
    let mut html = String::from("<html><body>");
    for i in 1..=60 {
        html.push_str(&format!(
            "<p><a href=\"https://github.com/owner-{}/repo-{}\">Repo {}</a></p>",
            i, i, i
        ));
    }
    html.push_str("</body></html>");

    let (candidates, truncated) = extract_github_candidates_from_html(&html, None).unwrap();
    assert!(
        truncated,
        "Expected truncated to be true for >50 candidates"
    );
    assert_eq!(candidates.len(), 50, "Expected exactly 50 candidates");
    assert_eq!(candidates[0].owner, "owner-1");
    assert_eq!(candidates[49].owner, "owner-50");
}

#[test]
fn test_html_extraction_zero_repos_returns_error() {
    let html = "<html><body><h1>No links here</h1><p>Just ordinary text</p></body></html>";
    let res = extract_github_candidates_from_html(html, None);
    assert!(res.is_err());
    assert_eq!(res.unwrap_err().code(), "NoGitHubRepositories");
}

// =============================================================================
// 5. Candidate Validation & Partial Failure / Rate Limit Tests
// =============================================================================

#[tokio::test]
async fn test_candidate_validation_partial_404_success() {
    let server = MockServer::start().await;

    // 2 valid repos
    Mock::given(method("GET"))
        .and(path("/repos/rust-lang/rust"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": 1,
            "name": "rust",
            "full_name": "rust-lang/rust",
            "description": "A language empowering everyone to build reliable and efficient software.",
            "html_url": "https://github.com/rust-lang/rust",
            "stargazers_count": 95000,
            "language": "Rust",
            "license": { "spdx_id": "MIT" },
            "topics": ["rust"],
            "pushed_at": "2026-03-01T12:00:00Z"
        })))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/repos/tokio-rs/tokio"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": 2,
            "name": "tokio",
            "full_name": "tokio-rs/tokio",
            "description": "An asynchronous runtime for the Rust programming language.",
            "html_url": "https://github.com/tokio-rs/tokio",
            "stargazers_count": 28000,
            "language": "Rust",
            "license": { "spdx_id": "MIT" },
            "topics": ["async", "tokio"],
            "pushed_at": "2026-03-01T12:00:00Z"
        })))
        .mount(&server)
        .await;

    // 1 not found repo (404)
    Mock::given(method("GET"))
        .and(path("/repos/nonexistent/missing"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    // Releases & Languages endpoints
    Mock::given(method("GET"))
        .and(path("/repos/rust-lang/rust/releases"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/repos/rust-lang/rust/languages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"Rust": 1000})))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/repos/tokio-rs/tokio/releases"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/repos/tokio-rs/tokio/languages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"Rust": 500})))
        .mount(&server)
        .await;

    let github = Arc::new(GitHubClient::with_base_url(&server.uri()));

    let candidates = vec![
        ExtractedRepoCandidate {
            owner: "rust-lang".to_string(),
            repo: "rust".to_string(),
            canonical_url: "https://github.com/rust-lang/rust".to_string(),
        },
        ExtractedRepoCandidate {
            owner: "nonexistent".to_string(),
            repo: "missing".to_string(),
            canonical_url: "https://github.com/nonexistent/missing".to_string(),
        },
        ExtractedRepoCandidate {
            owner: "tokio-rs".to_string(),
            repo: "tokio".to_string(),
            canonical_url: "https://github.com/tokio-rs/tokio".to_string(),
        },
    ];

    let outcome = validate_candidates(&github, None, &candidates).await;

    assert_eq!(outcome.valid_count, 2);
    assert_eq!(outcome.invalid_count, 1);
    assert_eq!(outcome.skipped_count, 0);
    assert!(!outcome.rate_limited);
    assert_eq!(outcome.repositories.len(), 2);
    // Order should be preserved
    assert_eq!(outcome.repositories[0].name, "rust");
    assert_eq!(outcome.repositories[1].name, "tokio");
}

#[tokio::test]
async fn test_candidate_validation_rate_limit_stops_further_requests() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/repos/owner-1/repo-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": 1,
            "name": "repo-1",
            "full_name": "owner-1/repo-1",
            "description": "First repo",
            "html_url": "https://github.com/owner-1/repo-1",
            "stargazers_count": 100,
            "language": "Rust",
            "license": { "spdx_id": "MIT" },
            "topics": [],
            "pushed_at": "2026-03-01T12:00:00Z"
        })))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/repos/owner-1/repo-1/releases"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/repos/owner-1/repo-1/languages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
        .mount(&server)
        .await;

    // Next repo returns 403 Rate Limit
    Mock::given(method("GET"))
        .and(path("/repos/owner-2/repo-2"))
        .respond_with(
            ResponseTemplate::new(403)
                .insert_header("x-ratelimit-remaining", "0")
                .insert_header("x-ratelimit-reset", "1773489600")
                .set_body_json(serde_json::json!({
                    "message": "API rate limit exceeded"
                })),
        )
        .mount(&server)
        .await;

    let github = Arc::new(GitHubClient::with_base_url(&server.uri()));

    let candidates = vec![
        ExtractedRepoCandidate {
            owner: "owner-1".to_string(),
            repo: "repo-1".to_string(),
            canonical_url: "https://github.com/owner-1/repo-1".to_string(),
        },
        ExtractedRepoCandidate {
            owner: "owner-2".to_string(),
            repo: "repo-2".to_string(),
            canonical_url: "https://github.com/owner-2/repo-2".to_string(),
        },
        ExtractedRepoCandidate {
            owner: "owner-3".to_string(),
            repo: "repo-3".to_string(),
            canonical_url: "https://github.com/owner-3/repo-3".to_string(),
        },
    ];

    let outcome = validate_candidates(&github, None, &candidates).await;

    assert!(outcome.rate_limited);
    assert_eq!(outcome.valid_count, 1);
    assert_eq!(outcome.repositories[0].name, "repo-1");
    // At least 1 candidate was skipped due to rate limit
    assert!(outcome.skipped_count >= 1);
}

// =============================================================================
// 6. Direct GitHub URL Regression Invariants
// =============================================================================

#[test]
fn test_direct_github_repo_url_parser_invariants_preserved() {
    // Normal root URL must succeed
    let (owner, repo, canonical) =
        parse_github_repo_url("https://github.com/rust-lang/rust").unwrap();
    assert_eq!(owner, "rust-lang");
    assert_eq!(repo, "rust");
    assert_eq!(canonical, "https://github.com/rust-lang/rust");

    // Deep URLs MUST STILL BE REJECTED for direct GitHub inputs!
    assert!(parse_github_repo_url("https://github.com/rust-lang/rust/issues/1").is_err());
    assert!(parse_github_repo_url("https://github.com/rust-lang/rust/pull/2").is_err());
    assert!(parse_github_repo_url("https://github.com/rust-lang/rust/tree/master").is_err());
    assert!(
        parse_github_repo_url("https://github.com/rust-lang/rust/blob/master/README.md").is_err()
    );
    assert!(parse_github_repo_url("https://github.com/rust-lang/rust/releases").is_err());

    // Root-only and user profiles must still be rejected
    assert!(parse_github_repo_url("https://github.com/").is_err());
    assert!(parse_github_repo_url("https://github.com/rust-lang").is_err());

    // Non-github hosts must be rejected
    assert!(parse_github_repo_url("https://gitlab.com/owner/repo").is_err());
}
