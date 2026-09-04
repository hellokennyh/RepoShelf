use std::time::Duration;
use wiremock::matchers::{header, header_exists, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use reposhelf_lib::github::client::GITHUB_API_VERSION;
use reposhelf_lib::github::{GitHubClient, RepositorySearchRequest};
use reposhelf_lib::secrets::{InMemorySecretStore, SecretStore, SECRET_KEY_GITHUB_TOKEN};

// -----------------------------------------------------------------------------
// 1. Connection Test & Request Headers Tests
// -----------------------------------------------------------------------------

#[tokio::test]
async fn test_connection_anonymous_and_headers() {
    let mock_server = MockServer::start().await;

    let rate_limit_body = serde_json::json!({
        "resources": {
            "core": {
                "limit": 60,
                "remaining": 59,
                "reset": 1773000000,
                "used": 1
            },
            "search": {
                "limit": 10,
                "remaining": 10,
                "reset": 1773000000,
                "used": 0
            }
        }
    });

    Mock::given(method("GET"))
        .and(path("/rate_limit"))
        .and(header("Accept", "application/vnd.github+json"))
        .and(header("X-GitHub-Api-Version", GITHUB_API_VERSION))
        .and(header_exists("User-Agent"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("x-ratelimit-limit", "60")
                .insert_header("x-ratelimit-remaining", "59")
                .insert_header("x-ratelimit-used", "1")
                .insert_header("x-ratelimit-reset", "1773000000")
                .insert_header("x-ratelimit-resource", "core")
                .set_body_json(&rate_limit_body),
        )
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = GitHubClient::with_base_url(&mock_server.uri());

    // Anonymous call (no token)
    let result = client
        .test_connection(None)
        .await
        .expect("Test connection failed");

    assert!(result.connected);
    assert_eq!(result.mode, "Anonymous");
    assert_eq!(result.rate_limit.core.limit, 60);
    assert_eq!(result.rate_limit.core.remaining, 59);
    assert_eq!(result.rate_limit.search.limit, 10);
    assert_eq!(result.rate_limit.search.remaining, 10);
}

#[tokio::test]
async fn test_connection_authenticated_and_secret_boundary() {
    let mock_server = MockServer::start().await;
    let secret_store = InMemorySecretStore::new();
    let test_token = "ghp_secure_secret_token_12345";
    secret_store
        .set_secret(SECRET_KEY_GITHUB_TOKEN, test_token)
        .unwrap();

    let rate_limit_body = serde_json::json!({
        "resources": {
            "core": {
                "limit": 5000,
                "remaining": 4999,
                "reset": 1773000000,
                "used": 1
            },
            "search": {
                "limit": 30,
                "remaining": 30,
                "reset": 1773000000,
                "used": 0
            }
        }
    });

    Mock::given(method("GET"))
        .and(path("/rate_limit"))
        .and(header(
            "Authorization",
            format!("Bearer {}", test_token).as_str(),
        ))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("x-ratelimit-limit", "5000")
                .insert_header("x-ratelimit-remaining", "4999")
                .insert_header("x-ratelimit-reset", "1773000000")
                .set_body_json(&rate_limit_body),
        )
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = GitHubClient::with_base_url(&mock_server.uri());

    // Retrieve token via internal secret store
    let retrieved_token = secret_store.get_secret(SECRET_KEY_GITHUB_TOKEN).unwrap();
    let result = client
        .test_connection(retrieved_token.as_deref())
        .await
        .expect("Test connection failed");

    assert!(result.connected);
    assert_eq!(result.mode, "Authenticated");
    assert_eq!(result.rate_limit.core.limit, 5000);
    assert_eq!(result.rate_limit.core.remaining, 4999);
}

// -----------------------------------------------------------------------------
// 2. Authentication & Error Code Mappings
// -----------------------------------------------------------------------------

#[tokio::test]
async fn test_auth_failure_401_maps_to_github_auth() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/rate_limit"))
        .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
            "message": "Bad credentials"
        })))
        .mount(&mock_server)
        .await;

    let client = GitHubClient::with_base_url(&mock_server.uri());
    let err = client
        .test_connection(Some("invalid_token"))
        .await
        .unwrap_err();

    assert_eq!(err.code(), "GitHubAuth");
    // Ensure token is not leaked into error message
    assert!(!err.to_string().contains("invalid_token"));
}

#[tokio::test]
async fn test_not_found_404_maps_to_github_not_found() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/repos/nonexistent/repo"))
        .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
            "message": "Not Found"
        })))
        .mount(&mock_server)
        .await;

    let client = GitHubClient::with_base_url(&mock_server.uri());
    let err = client
        .get_repository("nonexistent", "repo", None)
        .await
        .unwrap_err();

    assert_eq!(err.code(), "GitHubNotFound");
}

#[tokio::test]
async fn test_rate_limit_403_exhausted_maps_to_github_rate_limit() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/repos/owner/repo"))
        .respond_with(
            ResponseTemplate::new(403)
                .insert_header("x-ratelimit-limit", "60")
                .insert_header("x-ratelimit-remaining", "0")
                .insert_header("x-ratelimit-reset", "1773000000")
                .insert_header("x-ratelimit-resource", "core")
                .set_body_json(serde_json::json!({
                    "message": "API rate limit exceeded"
                })),
        )
        .mount(&mock_server)
        .await;

    let client = GitHubClient::with_base_url(&mock_server.uri());
    let err = client
        .get_repository("owner", "repo", None)
        .await
        .unwrap_err();

    assert_eq!(err.code(), "GitHubRateLimit");
    assert!(err.to_string().contains("exhausted"));
}

#[tokio::test]
async fn test_rate_limit_429_secondary_maps_to_github_rate_limit() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/repos/owner/repo"))
        .respond_with(
            ResponseTemplate::new(429)
                .insert_header("retry-after", "120")
                .set_body_json(serde_json::json!({
                    "message": "You have exceeded a secondary rate limit"
                })),
        )
        .mount(&mock_server)
        .await;

    let client = GitHubClient::with_base_url(&mock_server.uri());
    let err = client
        .get_repository("owner", "repo", None)
        .await
        .unwrap_err();

    assert_eq!(err.code(), "GitHubRateLimit");
    assert!(err.to_string().contains("secondary rate limit"));
}

#[tokio::test]
async fn test_malformed_json_200_maps_to_invalid_response() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/repos/owner/repo"))
        .respond_with(ResponseTemplate::new(200).set_body_string("not valid json at all"))
        .mount(&mock_server)
        .await;

    let client = GitHubClient::with_base_url(&mock_server.uri());
    let err = client
        .get_repository("owner", "repo", None)
        .await
        .unwrap_err();

    assert_eq!(err.code(), "GitHubInvalidResponse");
}

// -----------------------------------------------------------------------------
// 3. Repository Metadata & Mapping Tests
// -----------------------------------------------------------------------------

#[tokio::test]
async fn test_repository_metadata_mapping_spdx_license() {
    let mock_server = MockServer::start().await;

    let repo_body = serde_json::json!({
        "id": 1234567,
        "html_url": "https://github.com/tauri-apps/tauri",
        "name": "tauri",
        "description": "Build smaller, faster, and more secure desktop applications with a web frontend.",
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
        Some("Build smaller, faster, and more secure desktop applications with a web frontend.")
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
async fn test_timeout_maps_to_github_timeout() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/rate_limit"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(Duration::from_millis(500))
                .set_body_json(serde_json::json!({
                    "resources": {
                        "core": { "limit": 60, "remaining": 60, "reset": 0 },
                        "search": { "limit": 10, "remaining": 10, "reset": 0 }
                    }
                })),
        )
        .mount(&mock_server)
        .await;

    // Use a short 50ms timeout to trigger client timeout without slow tests
    let client =
        GitHubClient::with_base_url_and_timeout(&mock_server.uri(), Duration::from_millis(50));
    let err = client.test_connection(None).await.unwrap_err();

    assert_eq!(err.code(), "GitHubTimeout");
}

#[tokio::test]
async fn test_repository_license_fallback_and_null() {
    let mock_server = MockServer::start().await;

    // 1. Fallback to name when spdx_id is NOASSERTION
    let repo_fallback = serde_json::json!({
        "id": 111,
        "html_url": "https://github.com/owner/repo-custom-license",
        "name": "repo-custom-license",
        "description": "Custom license test",
        "stargazers_count": 10,
        "license": {
            "spdx_id": "NOASSERTION",
            "name": "BSD Zero Clause License"
        },
        "topics": [],
        "pushed_at": null,
        "private": false
    });

    // 2. License is null
    let repo_null = serde_json::json!({
        "id": 222,
        "html_url": "https://github.com/owner/repo-no-license",
        "name": "repo-no-license",
        "description": null,
        "stargazers_count": 0,
        "license": null,
        "topics": [],
        "pushed_at": null,
        "private": false
    });

    Mock::given(method("GET"))
        .and(path("/repos/owner/repo-custom-license"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&repo_fallback))
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/repos/owner/repo-no-license"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&repo_null))
        .mount(&mock_server)
        .await;

    let client = GitHubClient::with_base_url(&mock_server.uri());

    let res_fallback = client
        .get_repository("owner", "repo-custom-license", None)
        .await
        .unwrap();
    assert_eq!(
        res_fallback.license.as_deref(),
        Some("BSD Zero Clause License")
    );

    let res_null = client
        .get_repository("owner", "repo-no-license", None)
        .await
        .unwrap();
    assert_eq!(res_null.license, None);
}

#[tokio::test]
async fn test_private_repository_rejected() {
    let mock_server = MockServer::start().await;

    let private_repo = serde_json::json!({
        "id": 9999,
        "html_url": "https://github.com/secret/private-repo",
        "name": "private-repo",
        "description": "Top secret company repository",
        "stargazers_count": 5,
        "license": null,
        "topics": [],
        "pushed_at": "2026-03-01T00:00:00Z",
        "private": true
    });

    Mock::given(method("GET"))
        .and(path("/repos/secret/private-repo"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&private_repo))
        .mount(&mock_server)
        .await;

    let client = GitHubClient::with_base_url(&mock_server.uri());
    let err = client
        .get_repository("secret", "private-repo", Some("token"))
        .await
        .unwrap_err();

    assert_eq!(err.code(), "PrivateRepositoryUnsupported");
}

// -----------------------------------------------------------------------------
// 4. Search API & Filtering Tests
// -----------------------------------------------------------------------------

#[tokio::test]
async fn test_search_repositories_mapping_order_and_private_filtering() {
    let mock_server = MockServer::start().await;

    let search_body = serde_json::json!({
        "total_count": 3,
        "items": [
            {
                "id": 101,
                "html_url": "https://github.com/public/repo-alpha",
                "name": "repo-alpha",
                "description": "Alpha public search result",
                "stargazers_count": 500,
                "license": { "spdx_id": "MIT", "name": "MIT" },
                "topics": ["llm", "ai"],
                "pushed_at": "2026-03-01T00:00:00Z",
                "private": false
            },
            {
                "id": 102,
                "html_url": "https://github.com/secret/repo-private",
                "name": "repo-private",
                "description": "Private search result that should be filtered out",
                "stargazers_count": 1000,
                "license": null,
                "topics": [],
                "pushed_at": "2026-03-01T00:00:00Z",
                "private": true
            },
            {
                "id": 103,
                "html_url": "https://github.com/public/repo-beta",
                "name": "repo-beta",
                "description": "Beta public search result",
                "stargazers_count": 250,
                "license": { "spdx_id": "Apache-2.0", "name": "Apache-2.0" },
                "topics": ["database", "sqlite"],
                "pushed_at": "2026-02-28T00:00:00Z",
                "private": false
            }
        ]
    });

    Mock::given(method("GET"))
        .and(path("/search/repositories"))
        .and(query_param("q", "test-query stars:>=100"))
        .and(query_param("per_page", "20"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&search_body))
        .mount(&mock_server)
        .await;

    let client = GitHubClient::with_base_url(&mock_server.uri());
    let req = RepositorySearchRequest {
        query: "test-query".to_string(),
        min_stars: Some(100),
        pushed_after: None,
        limit: 20,
    };

    let resp = client.search_repositories(&req, None).await.unwrap();

    assert_eq!(resp.total_count, 3);
    // Private repo must be filtered out
    assert_eq!(resp.items.len(), 2);

    // Verify ordering is preserved from API (Best Match) without sorting by stars
    assert_eq!(resp.items[0].github_repo_id, 101);
    assert_eq!(resp.items[0].name, "repo-alpha");
    assert_eq!(resp.items[0].category, "AI & Agents");
    assert_eq!(resp.items[0].stars, 500);

    assert_eq!(resp.items[1].github_repo_id, 103);
    assert_eq!(resp.items[1].name, "repo-beta");
    assert_eq!(resp.items[1].category, "Data & Database");
    assert_eq!(resp.items[1].stars, 250);
}
