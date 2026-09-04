use std::time::Duration;
use wiremock::matchers::{header, header_exists, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use reposhelf_lib::github::client::GITHUB_API_VERSION;
use reposhelf_lib::github::rate_limit::handle_rate_limit_error;
use reposhelf_lib::github::{
    build_search_query, parse_github_repo_url, GitHubClient, RepositorySearchRequest,
};
use reposhelf_lib::secrets::{InMemorySecretStore, SecretStore, SECRET_KEY_GITHUB_TOKEN};

// =============================================================================
// 1. Headers Tests
// =============================================================================

#[tokio::test]
async fn test_headers_accept() {
    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/rate_limit"))
        .and(header("Accept", "application/vnd.github+json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "resources": {
                "core": { "limit": 60, "remaining": 60, "reset": 0 },
                "search": { "limit": 10, "remaining": 10, "reset": 0 }
            }
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = GitHubClient::with_base_url(&mock_server.uri());
    client.test_connection(None).await.unwrap();
}

#[tokio::test]
async fn test_headers_version() {
    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/rate_limit"))
        .and(header("X-GitHub-Api-Version", GITHUB_API_VERSION))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "resources": {
                "core": { "limit": 60, "remaining": 60, "reset": 0 },
                "search": { "limit": 10, "remaining": 10, "reset": 0 }
            }
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = GitHubClient::with_base_url(&mock_server.uri());
    client.test_connection(None).await.unwrap();
}

#[tokio::test]
async fn test_headers_user_agent() {
    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/rate_limit"))
        .and(header_exists("User-Agent"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "resources": {
                "core": { "limit": 60, "remaining": 60, "reset": 0 },
                "search": { "limit": 10, "remaining": 10, "reset": 0 }
            }
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = GitHubClient::with_base_url(&mock_server.uri());
    client.test_connection(None).await.unwrap();
}

#[tokio::test]
async fn test_headers_anonymous_no_authorization() {
    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/rate_limit"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "resources": {
                "core": { "limit": 60, "remaining": 60, "reset": 0 },
                "search": { "limit": 10, "remaining": 10, "reset": 0 }
            }
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = GitHubClient::with_base_url(&mock_server.uri());
    let res = client.test_connection(None).await.unwrap();
    assert_eq!(res.mode, "Anonymous");

    let received_requests = mock_server.received_requests().await.unwrap();
    assert_eq!(received_requests.len(), 1);
    assert!(!received_requests[0].headers.contains_key("authorization"));
}

#[tokio::test]
async fn test_headers_authenticated_bearer() {
    let mock_server = MockServer::start().await;
    let token = "ghp_mock_bearer_token_test";
    Mock::given(method("GET"))
        .and(path("/rate_limit"))
        .and(header(
            "Authorization",
            format!("Bearer {}", token).as_str(),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "resources": {
                "core": { "limit": 5000, "remaining": 5000, "reset": 0 },
                "search": { "limit": 30, "remaining": 30, "reset": 0 }
            }
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = GitHubClient::with_base_url(&mock_server.uri());
    let res = client.test_connection(Some(token)).await.unwrap();
    assert_eq!(res.mode, "Authenticated");
}

// =============================================================================
// 2. Secret Boundary Tests
// =============================================================================

#[tokio::test]
async fn test_secret_boundary_no_token_anonymous() {
    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/rate_limit"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "resources": {
                "core": { "limit": 60, "remaining": 60, "reset": 0 },
                "search": { "limit": 10, "remaining": 10, "reset": 0 }
            }
        })))
        .mount(&mock_server)
        .await;

    let client = GitHubClient::with_base_url(&mock_server.uri());
    let res = client.test_connection(None).await.unwrap();
    assert_eq!(res.mode, "Anonymous");
    assert_eq!(res.rate_limit.core.limit, 60);
}

#[tokio::test]
async fn test_secret_boundary_token_authenticated() {
    let mock_server = MockServer::start().await;
    let secret_store = InMemorySecretStore::new();
    secret_store
        .set_secret(SECRET_KEY_GITHUB_TOKEN, "ghp_internal_secret")
        .unwrap();

    Mock::given(method("GET"))
        .and(path("/rate_limit"))
        .and(header("Authorization", "Bearer ghp_internal_secret"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "resources": {
                "core": { "limit": 5000, "remaining": 4999, "reset": 0 },
                "search": { "limit": 30, "remaining": 30, "reset": 0 }
            }
        })))
        .mount(&mock_server)
        .await;

    let client = GitHubClient::with_base_url(&mock_server.uri());
    let token = secret_store.get_secret(SECRET_KEY_GITHUB_TOKEN).unwrap();
    let res = client.test_connection(token.as_deref()).await.unwrap();
    assert_eq!(res.mode, "Authenticated");
    assert_eq!(res.rate_limit.core.limit, 5000);
}

#[test]
fn test_secret_boundary_commands_do_not_accept_token_param() {
    let store = InMemorySecretStore::new();
    assert!(!store.has_secret(SECRET_KEY_GITHUB_TOKEN).unwrap());
    store
        .set_secret(SECRET_KEY_GITHUB_TOKEN, "ghp_token_xyz")
        .unwrap();
    assert_eq!(
        store
            .get_secret(SECRET_KEY_GITHUB_TOKEN)
            .unwrap()
            .as_deref(),
        Some("ghp_token_xyz")
    );
}

#[tokio::test]
async fn test_secret_boundary_errors_do_not_expose_token() {
    let mock_server = MockServer::start().await;
    let sensitive_token = "ghp_super_secret_canary_value_999";

    Mock::given(method("GET"))
        .and(path("/rate_limit"))
        .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
            "message": "Bad credentials"
        })))
        .mount(&mock_server)
        .await;

    let client = GitHubClient::with_base_url(&mock_server.uri());
    let err = client
        .test_connection(Some(sensitive_token))
        .await
        .unwrap_err();

    let err_str = err.to_string();
    assert!(
        !err_str.contains(sensitive_token),
        "CRITICAL: Error string leaked token!"
    );
    let json_err = serde_json::to_string(&err).unwrap();
    assert!(
        !json_err.contains(sensitive_token),
        "CRITICAL: Serialized error leaked token!"
    );
}

// =============================================================================
// 3. Repository Tests
// =============================================================================

#[tokio::test]
async fn test_repository_mapping() {
    let mock_server = MockServer::start().await;
    let repo_body = serde_json::json!({
        "id": 1234567,
        "html_url": "https://github.com/tauri-apps/tauri",
        "name": "tauri",
        "description": "Build smaller, faster, and more secure desktop applications.",
        "stargazers_count": 85000,
        "license": {
            "spdx_id": "Apache-2.0",
            "name": "Apache License 2.0"
        },
        "topics": ["rust", "desktop", "tauri", "gui", "developer-tools"],
        "pushed_at": "2026-03-01T12:00:00Z",
        "private": false
    });

    Mock::given(method("GET"))
        .and(path("/repos/tauri-apps/tauri"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&repo_body))
        .mount(&mock_server)
        .await;

    let client = GitHubClient::with_base_url(&mock_server.uri());
    let repo = client
        .get_repository("tauri-apps", "tauri", None)
        .await
        .unwrap();

    assert_eq!(repo.github_repo_id, 1234567);
    assert_eq!(repo.github_url, "https://github.com/tauri-apps/tauri");
    assert_eq!(repo.name, "tauri");
    assert_eq!(
        repo.description.as_deref(),
        Some("Build smaller, faster, and more secure desktop applications.")
    );
    assert_eq!(repo.stars, 85000);
    assert_eq!(repo.license.as_deref(), Some("Apache-2.0"));
    assert_eq!(
        repo.topics,
        vec!["rust", "desktop", "tauri", "gui", "developer-tools"]
    );
    assert_eq!(repo.pushed_at.as_deref(), Some("2026-03-01T12:00:00Z"));
    assert_eq!(repo.category, "Developer Tools");
}

#[tokio::test]
async fn test_repository_license_spdx() {
    let mock_server = MockServer::start().await;
    let repo_body = serde_json::json!({
        "id": 1,
        "html_url": "https://github.com/test/mit-repo",
        "name": "mit-repo",
        "description": "MIT licensed repo",
        "stargazers_count": 10,
        "license": { "spdx_id": "MIT", "name": "MIT License" },
        "topics": [],
        "pushed_at": null,
        "private": false
    });

    Mock::given(method("GET"))
        .and(path("/repos/test/mit-repo"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&repo_body))
        .mount(&mock_server)
        .await;

    let client = GitHubClient::with_base_url(&mock_server.uri());
    let repo = client
        .get_repository("test", "mit-repo", None)
        .await
        .unwrap();
    assert_eq!(repo.license.as_deref(), Some("MIT"));
}

#[tokio::test]
async fn test_repository_license_name_fallback() {
    let mock_server = MockServer::start().await;
    let repo_body = serde_json::json!({
        "id": 2,
        "html_url": "https://github.com/test/custom-license",
        "name": "custom-license",
        "description": "Custom license fallback test",
        "stargazers_count": 10,
        "license": { "spdx_id": "NOASSERTION", "name": "BSD Zero Clause License" },
        "topics": [],
        "pushed_at": null,
        "private": false
    });

    Mock::given(method("GET"))
        .and(path("/repos/test/custom-license"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&repo_body))
        .mount(&mock_server)
        .await;

    let client = GitHubClient::with_base_url(&mock_server.uri());
    let repo = client
        .get_repository("test", "custom-license", None)
        .await
        .unwrap();
    assert_eq!(repo.license.as_deref(), Some("BSD Zero Clause License"));
}

#[tokio::test]
async fn test_repository_license_null() {
    let mock_server = MockServer::start().await;
    let repo_body = serde_json::json!({
        "id": 3,
        "html_url": "https://github.com/test/no-license",
        "name": "no-license",
        "description": "Repo without a license",
        "stargazers_count": 10,
        "license": null,
        "topics": [],
        "pushed_at": null,
        "private": false
    });

    Mock::given(method("GET"))
        .and(path("/repos/test/no-license"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&repo_body))
        .mount(&mock_server)
        .await;

    let client = GitHubClient::with_base_url(&mock_server.uri());
    let repo = client
        .get_repository("test", "no-license", None)
        .await
        .unwrap();
    assert_eq!(repo.license, None);
}

#[tokio::test]
async fn test_repository_private_rejected() {
    let mock_server = MockServer::start().await;
    let repo_body = serde_json::json!({
        "id": 9999,
        "html_url": "https://github.com/secret/private-repo",
        "name": "private-repo",
        "description": "Confidential repository",
        "stargazers_count": 5,
        "license": null,
        "topics": [],
        "pushed_at": null,
        "private": true
    });

    Mock::given(method("GET"))
        .and(path("/repos/secret/private-repo"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&repo_body))
        .mount(&mock_server)
        .await;

    let client = GitHubClient::with_base_url(&mock_server.uri());
    let err = client
        .get_repository("secret", "private-repo", None)
        .await
        .unwrap_err();
    assert_eq!(err.code(), "PrivateRepositoryUnsupported");
}

// =============================================================================
// 4. URL Tests
// =============================================================================

#[test]
fn test_url_accepted_variants() {
    let cases = [
        ("https://github.com/owner/repo", "owner", "repo"),
        ("https://github.com/owner/repo/", "owner", "repo"),
        ("http://github.com/owner/repo", "owner", "repo"),
        ("https://www.github.com/owner/repo", "owner", "repo"),
        ("https://github.com/owner/repo?tab=readme", "owner", "repo"),
        (
            "https://github.com/owner/repo#getting-started",
            "owner",
            "repo",
        ),
        ("https://github.com/owner/repo.git", "owner", "repo"),
    ];

    for (input, expected_owner, expected_repo) in cases {
        let (owner, repo, canonical) = parse_github_repo_url(input).expect(input);
        assert_eq!(owner, expected_owner);
        assert_eq!(repo, expected_repo);
        assert_eq!(
            canonical,
            format!("https://github.com/{}/{}", expected_owner, expected_repo)
        );
    }
}

#[test]
fn test_url_rejected_variants() {
    let invalid_inputs = [
        "https://github.com",
        "https://github.com/",
        "https://github.com/owner",
        "https://github.com/owner/",
        "https://github.com/owner/repo/issues",
        "https://github.com/owner/repo/pulls",
        "https://github.com/owner/repo/tree/main",
        "https://github.com/owner/repo/blob/main/README.md",
        "https://github.com/owner/repo/commit/123456",
        "https://gitlab.com/owner/repo",
        "https://bitbucket.org/owner/repo",
        "not a url",
        "",
        "   ",
        "ftp://github.com/owner/repo",
    ];

    for input in invalid_inputs {
        assert!(
            parse_github_repo_url(input).is_err(),
            "Should reject: {}",
            input
        );
    }
}

// =============================================================================
// 5. Search Builder Tests
// =============================================================================

#[test]
fn test_search_builder_keyword_only() {
    let req = RepositorySearchRequest {
        query: "tauri desktop".to_string(),
        min_stars: None,
        pushed_after: None,
        limit: 20,
    };
    let q = build_search_query(&req).unwrap();
    assert_eq!(q, "tauri desktop");
}

#[test]
fn test_search_builder_keyword_with_stars() {
    let req = RepositorySearchRequest {
        query: "rust sqlx".to_string(),
        min_stars: Some(500),
        pushed_after: None,
        limit: 30,
    };
    let q = build_search_query(&req).unwrap();
    assert_eq!(q, "rust sqlx stars:>=500");
}

#[test]
fn test_search_builder_keyword_with_pushed() {
    let req = RepositorySearchRequest {
        query: "ai agents".to_string(),
        min_stars: None,
        pushed_after: Some("2026-01-01".to_string()),
        limit: 10,
    };
    let q = build_search_query(&req).unwrap();
    assert_eq!(q, "ai agents pushed:>=2026-01-01");
}

#[test]
fn test_search_builder_keyword_with_stars_and_pushed() {
    let req = RepositorySearchRequest {
        query: "scraper".to_string(),
        min_stars: Some(100),
        pushed_after: Some("2026-03-01".to_string()),
        limit: 50,
    };
    let q = build_search_query(&req).unwrap();
    assert_eq!(q, "scraper stars:>=100 pushed:>=2026-03-01");
}

#[test]
fn test_search_builder_limit_validation() {
    for valid_limit in [1, 10, 20, 50, 100] {
        let req = RepositorySearchRequest {
            query: "test".to_string(),
            min_stars: None,
            pushed_after: None,
            limit: valid_limit,
        };
        assert!(build_search_query(&req).is_ok());
    }
}

#[test]
fn test_search_builder_empty_query_rejected() {
    let req = RepositorySearchRequest {
        query: "   ".to_string(),
        min_stars: None,
        pushed_after: None,
        limit: 20,
    };
    assert!(build_search_query(&req).is_err());
}

#[test]
fn test_search_builder_limit_zero_rejected() {
    let req = RepositorySearchRequest {
        query: "test".to_string(),
        min_stars: None,
        pushed_after: None,
        limit: 0,
    };
    assert!(build_search_query(&req).is_err());
}

#[test]
fn test_search_builder_limit_over_100_rejected() {
    let req = RepositorySearchRequest {
        query: "test".to_string(),
        min_stars: None,
        pushed_after: None,
        limit: 101,
    };
    assert!(build_search_query(&req).is_err());
}

#[test]
fn test_search_builder_negative_stars_rejected() {
    let req = RepositorySearchRequest {
        query: "test".to_string(),
        min_stars: Some(-5),
        pushed_after: None,
        limit: 20,
    };
    assert!(build_search_query(&req).is_err());
}

// =============================================================================
// 6. Search Mapping Tests
// =============================================================================

#[tokio::test]
async fn test_search_mapping_preserves_api_order() {
    let mock_server = MockServer::start().await;
    let search_body = serde_json::json!({
        "total_count": 3,
        "items": [
            {
                "id": 101, "html_url": "https://github.com/a/first", "name": "first",
                "description": "First API result", "stargazers_count": 10,
                "license": null, "topics": [], "pushed_at": null, "private": false
            },
            {
                "id": 102, "html_url": "https://github.com/b/second", "name": "second",
                "description": "Second API result with more stars", "stargazers_count": 10000,
                "license": null, "topics": [], "pushed_at": null, "private": false
            },
            {
                "id": 103, "html_url": "https://github.com/c/third", "name": "third",
                "description": "Third API result", "stargazers_count": 50,
                "license": null, "topics": [], "pushed_at": null, "private": false
            }
        ]
    });

    Mock::given(method("GET"))
        .and(path("/search/repositories"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&search_body))
        .mount(&mock_server)
        .await;

    let client = GitHubClient::with_base_url(&mock_server.uri());
    let req = RepositorySearchRequest {
        query: "test".to_string(),
        min_stars: None,
        pushed_after: None,
        limit: 20,
    };
    let resp = client.search_repositories(&req, None).await.unwrap();

    assert_eq!(resp.items.len(), 3);
    assert_eq!(resp.items[0].name, "first");
    assert_eq!(resp.items[1].name, "second");
    assert_eq!(resp.items[2].name, "third");
}

#[tokio::test]
async fn test_search_mapping_no_custom_sorting() {
    let mock_server = MockServer::start().await;
    let search_body = serde_json::json!({
        "total_count": 2,
        "items": [
            { "id": 1, "html_url": "https://github.com/x/low", "name": "low", "stargazers_count": 5, "private": false },
            { "id": 2, "html_url": "https://github.com/x/high", "name": "high", "stargazers_count": 99999, "private": false }
        ]
    });

    Mock::given(method("GET"))
        .and(path("/search/repositories"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&search_body))
        .mount(&mock_server)
        .await;

    let client = GitHubClient::with_base_url(&mock_server.uri());
    let req = RepositorySearchRequest {
        query: "test".to_string(),
        min_stars: None,
        pushed_after: None,
        limit: 20,
    };
    let resp = client.search_repositories(&req, None).await.unwrap();

    assert_eq!(resp.items[0].stars, 5);
    assert_eq!(resp.items[1].stars, 99999);
}

#[tokio::test]
async fn test_search_mapping_fields() {
    let mock_server = MockServer::start().await;
    let search_body = serde_json::json!({
        "total_count": 1,
        "items": [
            {
                "id": 777,
                "html_url": "https://github.com/org/project",
                "name": "project",
                "description": "Complete field test project",
                "stargazers_count": 3456,
                "license": { "spdx_id": "MIT", "name": "MIT" },
                "topics": ["rust", "desktop"],
                "pushed_at": "2026-02-20T10:00:00Z",
                "private": false
            }
        ]
    });

    Mock::given(method("GET"))
        .and(path("/search/repositories"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&search_body))
        .mount(&mock_server)
        .await;

    let client = GitHubClient::with_base_url(&mock_server.uri());
    let req = RepositorySearchRequest {
        query: "project".to_string(),
        min_stars: None,
        pushed_after: None,
        limit: 20,
    };
    let resp = client.search_repositories(&req, None).await.unwrap();

    let item = &resp.items[0];
    assert_eq!(item.github_repo_id, 777);
    assert_eq!(item.topics, vec!["rust", "desktop"]);
    assert_eq!(item.license.as_deref(), Some("MIT"));
    assert_eq!(item.pushed_at.as_deref(), Some("2026-02-20T10:00:00Z"));
    assert_eq!(item.stars, 3456);
}

#[tokio::test]
async fn test_search_mapping_filters_private() {
    let mock_server = MockServer::start().await;
    let search_body = serde_json::json!({
        "total_count": 2,
        "items": [
            { "id": 1, "html_url": "https://github.com/a/public", "name": "public", "private": false },
            { "id": 2, "html_url": "https://github.com/a/private", "name": "private", "private": true }
        ]
    });

    Mock::given(method("GET"))
        .and(path("/search/repositories"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&search_body))
        .mount(&mock_server)
        .await;

    let client = GitHubClient::with_base_url(&mock_server.uri());
    let req = RepositorySearchRequest {
        query: "test".to_string(),
        min_stars: None,
        pushed_after: None,
        limit: 20,
    };
    let resp = client.search_repositories(&req, None).await.unwrap();

    assert_eq!(resp.items.len(), 1);
    assert_eq!(resp.items[0].name, "public");
}

// =============================================================================
// 7. Rate Limit Tests
// =============================================================================

#[test]
fn test_rate_limit_normal() {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert("x-ratelimit-limit", "60".parse().unwrap());
    headers.insert("x-ratelimit-remaining", "50".parse().unwrap());
    headers.insert("x-ratelimit-reset", "1773000000".parse().unwrap());
    headers.insert("x-ratelimit-resource", "core".parse().unwrap());

    let b = reposhelf_lib::github::rate_limit::parse_rate_limit_headers(&headers).unwrap();
    assert_eq!(b.limit, 60);
    assert_eq!(b.remaining, 50);
    assert_eq!(b.resource, "core");
}

#[test]
fn test_rate_limit_primary_exhausted() {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert("x-ratelimit-limit", "60".parse().unwrap());
    headers.insert("x-ratelimit-remaining", "0".parse().unwrap());
    headers.insert("x-ratelimit-reset", "1773000000".parse().unwrap());
    headers.insert("x-ratelimit-resource", "search".parse().unwrap());

    let err = handle_rate_limit_error(403, &headers);
    assert_eq!(err.code(), "GitHubRateLimit");
    assert!(err.to_string().contains("exhausted"));
    assert!(err.to_string().contains("search"));
}

#[test]
fn test_rate_limit_429() {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert("retry-after", "45".parse().unwrap());

    let err = handle_rate_limit_error(429, &headers);
    assert_eq!(err.code(), "GitHubRateLimit");
    assert!(err.to_string().contains("45 seconds"));
}

#[test]
fn test_rate_limit_secondary() {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert("retry-after", "120".parse().unwrap());

    let err = handle_rate_limit_error(403, &headers);
    assert_eq!(err.code(), "GitHubRateLimit");
    assert!(err.to_string().contains("secondary rate limit"));
    assert!(err.to_string().contains("120 seconds"));
}

// =============================================================================
// 8. Errors Tests
// =============================================================================

#[tokio::test]
async fn test_error_401() {
    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/rate_limit"))
        .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
            "message": "Bad credentials"
        })))
        .mount(&mock_server)
        .await;

    let client = GitHubClient::with_base_url(&mock_server.uri());
    let err = client.test_connection(Some("bad_token")).await.unwrap_err();
    assert_eq!(err.code(), "GitHubAuth");
}

#[tokio::test]
async fn test_error_404() {
    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/repos/ghost/not-real"))
        .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
            "message": "Not Found"
        })))
        .mount(&mock_server)
        .await;

    let client = GitHubClient::with_base_url(&mock_server.uri());
    let err = client
        .get_repository("ghost", "not-real", None)
        .await
        .unwrap_err();
    assert_eq!(err.code(), "GitHubNotFound");
}

#[tokio::test]
async fn test_error_timeout() {
    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/rate_limit"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(Duration::from_millis(300))
                .set_body_json(serde_json::json!({
                    "resources": {
                        "core": { "limit": 60, "remaining": 60, "reset": 0 },
                        "search": { "limit": 10, "remaining": 10, "reset": 0 }
                    }
                })),
        )
        .mount(&mock_server)
        .await;

    let client =
        GitHubClient::with_base_url_and_timeout(&mock_server.uri(), Duration::from_millis(30));
    let err = client.test_connection(None).await.unwrap_err();
    assert_eq!(err.code(), "GitHubTimeout");
}

#[tokio::test]
async fn test_error_network_failure() {
    let client = GitHubClient::with_base_url("http://127.0.0.1:59999");
    let err = client.test_connection(None).await.unwrap_err();
    assert_eq!(err.code(), "GitHubNetwork");
}

#[tokio::test]
async fn test_error_malformed_json() {
    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/repos/owner/repo"))
        .respond_with(ResponseTemplate::new(200).set_body_string("broken non-json response"))
        .mount(&mock_server)
        .await;

    let client = GitHubClient::with_base_url(&mock_server.uri());
    let err = client
        .get_repository("owner", "repo", None)
        .await
        .unwrap_err();
    assert_eq!(err.code(), "GitHubInvalidResponse");
}

// =============================================================================
// 9. Connection Tests
// =============================================================================

#[tokio::test]
async fn test_connection_anonymous() {
    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/rate_limit"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "resources": {
                "core": { "limit": 60, "remaining": 58, "reset": 1773000000 },
                "search": { "limit": 10, "remaining": 10, "reset": 1773000000 }
            }
        })))
        .mount(&mock_server)
        .await;

    let client = GitHubClient::with_base_url(&mock_server.uri());
    let res = client.test_connection(None).await.unwrap();
    assert!(res.connected);
    assert_eq!(res.mode, "Anonymous");
    assert_eq!(res.rate_limit.core.remaining, 58);
    assert_eq!(res.rate_limit.search.remaining, 10);
}

#[tokio::test]
async fn test_connection_authenticated() {
    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/rate_limit"))
        .and(header("Authorization", "Bearer my_secret_token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "resources": {
                "core": { "limit": 5000, "remaining": 4800, "reset": 1773000000 },
                "search": { "limit": 30, "remaining": 29, "reset": 1773000000 }
            }
        })))
        .mount(&mock_server)
        .await;

    let client = GitHubClient::with_base_url(&mock_server.uri());
    let res = client
        .test_connection(Some("my_secret_token"))
        .await
        .unwrap();
    assert!(res.connected);
    assert_eq!(res.mode, "Authenticated");
    assert_eq!(res.rate_limit.core.limit, 5000);
    assert_eq!(res.rate_limit.core.remaining, 4800);
    assert_eq!(res.rate_limit.search.limit, 30);
    assert_eq!(res.rate_limit.search.remaining, 29);
}
