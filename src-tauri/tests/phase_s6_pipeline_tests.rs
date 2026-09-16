use std::sync::Arc;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use reposhelf_lib::github::GitHubClient;
use reposhelf_lib::star_history::models::StarHistoryPreviewResult;
use reposhelf_lib::star_history::service::{
    StarHistoryService, MAX_STAR_HISTORY_PREVIEW_CONCURRENCY,
};

#[tokio::test]
async fn test_readme_404_returns_none_without_error() {
    let gh_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/repos/owner/repo/readme"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&gh_server)
        .await;

    let client = Arc::new(GitHubClient::with_base_url(&gh_server.uri()));
    let service = StarHistoryService::new(client);

    let res = service.get_preview("owner", "repo", None).await;
    assert!(res.is_ok());
    assert_eq!(res.unwrap(), StarHistoryPreviewResult::none());
}

#[tokio::test]
async fn test_readme_over_1_mib_returns_none() {
    let gh_server = MockServer::start().await;

    // Return GitHub contents JSON with size declared > 1 MiB (1,048,576 bytes)
    let body = serde_json::json!({
        "name": "README.md",
        "size": 2_000_000,
        "content": "",
        "encoding": "base64"
    });

    Mock::given(method("GET"))
        .and(path("/repos/owner/repo/readme"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(&gh_server)
        .await;

    let client = Arc::new(GitHubClient::with_base_url(&gh_server.uri()));
    let service = StarHistoryService::new(client);

    let res = service.get_preview("owner", "repo", None).await;
    assert!(res.is_ok());
    assert_eq!(res.unwrap(), StarHistoryPreviewResult::none());
}

#[tokio::test]
async fn test_github_rate_limit_returns_none_gracefully() {
    let gh_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/repos/owner/repo/readme"))
        .respond_with(
            ResponseTemplate::new(403)
                .insert_header("X-RateLimit-Remaining", "0")
                .insert_header("X-RateLimit-Reset", "1893456000")
                .set_body_string("API rate limit exceeded"),
        )
        .mount(&gh_server)
        .await;

    let client = Arc::new(GitHubClient::with_base_url(&gh_server.uri()));
    let service = StarHistoryService::new(client);

    let res = service.get_preview("owner", "repo", None).await;
    assert!(res.is_ok());
    assert_eq!(res.unwrap(), StarHistoryPreviewResult::none());
}

#[tokio::test]
async fn test_star_history_404_or_500_returns_none() {
    let gh_server = MockServer::start().await;
    let sh_server = MockServer::start().await;

    let sh_host = sh_server.address().ip().to_string();

    // README contains official canonical embed URL
    let readme = "# Sample\n\n![Star History](https://api.star-history.com/svg?repos=owner/repo&type=Date)\n";
    let body = serde_json::json!({
        "name": "README.md",
        "size": readme.len(),
        "content": readme,
        "encoding": null
    });

    Mock::given(method("GET"))
        .and(path("/repos/owner/repo/readme"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(&gh_server)
        .await;

    // SH server responds with 404
    Mock::given(method("GET"))
        .and(path("/svg"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&sh_server)
        .await;

    let client = Arc::new(GitHubClient::with_base_url(&gh_server.uri()));
    let service = StarHistoryService::with_test_config(
        client,
        Some(sh_server.uri()),
        None,
        Some(vec![sh_host]),
    );

    let res = service.get_preview("owner", "repo", None).await;
    assert!(res.is_ok());
    assert_eq!(res.unwrap(), StarHistoryPreviewResult::none());
}

#[tokio::test]
async fn test_invalid_svg_from_server_returns_none() {
    let gh_server = MockServer::start().await;
    let sh_server = MockServer::start().await;

    let sh_host = sh_server.address().ip().to_string();

    let readme = "# Sample\n\n![Star History](https://api.star-history.com/svg?repos=owner/repo&type=Date)\n";
    let body = serde_json::json!({
        "name": "README.md",
        "size": readme.len(),
        "content": readme,
        "encoding": null
    });

    Mock::given(method("GET"))
        .and(path("/repos/owner/repo/readme"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(&gh_server)
        .await;

    // Malicious SVG with script
    let bad_svg = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100"><script>alert(1)</script></svg>"#;
    Mock::given(method("GET"))
        .and(path("/svg"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(bad_svg.as_bytes().to_vec())
                .insert_header("Content-Type", "image/svg+xml"),
        )
        .mount(&sh_server)
        .await;

    let client = Arc::new(GitHubClient::with_base_url(&gh_server.uri()));
    let service = StarHistoryService::with_test_config(
        client,
        Some(sh_server.uri()),
        None,
        Some(vec![sh_host]),
    );

    let res = service.get_preview("owner", "repo", None).await;
    assert!(res.is_ok());
    assert_eq!(res.unwrap(), StarHistoryPreviewResult::none());
}

#[tokio::test]
async fn test_successful_preview_pipeline_flow() {
    let gh_server = MockServer::start().await;
    let sh_server = MockServer::start().await;

    let sh_host = sh_server.address().ip().to_string();

    let readme = "# Sample\n\n![Star History](https://api.star-history.com/svg?repos=test-owner/test-repo&type=Date)\n";
    let body = serde_json::json!({
        "name": "README.md",
        "size": readme.len(),
        "content": readme,
        "encoding": null
    });

    Mock::given(method("GET"))
        .and(path("/repos/test-owner/test-repo/readme"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(&gh_server)
        .await;

    let good_svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 800 400" width="800" height="400"><text x="10" y="20">Stars</text></svg>"##;
    Mock::given(method("GET"))
        .and(path("/svg"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(good_svg.as_bytes().to_vec())
                .insert_header("Content-Type", "image/svg+xml"),
        )
        .mount(&sh_server)
        .await;

    let client = Arc::new(GitHubClient::with_base_url(&gh_server.uri()));
    let service = StarHistoryService::with_test_config(
        client,
        Some(sh_server.uri()),
        None,
        Some(vec![sh_host]),
    );

    let res = service.get_preview("test-owner", "test-repo", None).await;
    assert!(res.is_ok());
    match res.unwrap() {
        StarHistoryPreviewResult::Ready {
            svg_data,
            embed_url,
        } => {
            assert_eq!(svg_data, good_svg);
            assert_eq!(
                embed_url,
                "https://api.star-history.com/svg?repos=test-owner/test-repo&type=Date"
            );
        }
        other => panic!("Expected Ready result, got {:?}", other),
    }
}

#[tokio::test]
async fn test_request_coalescing_deduplicates_concurrent_calls() {
    let gh_server = MockServer::start().await;
    let sh_server = MockServer::start().await;

    let sh_host = sh_server.address().ip().to_string();

    let readme = "# Sample\n\n![Star History](https://api.star-history.com/svg?repos=coalesce-owner/coalesce-repo&type=Date)\n";
    let body = serde_json::json!({
        "name": "README.md",
        "size": readme.len(),
        "content": readme,
        "encoding": null
    });

    // Exactly 1 README request should be received
    Mock::given(method("GET"))
        .and(path("/repos/coalesce-owner/coalesce-repo/readme"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .expect(1)
        .mount(&gh_server)
        .await;

    let good_svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100"></svg>"##;
    // Exactly 1 SVG request should be received
    Mock::given(method("GET"))
        .and(path("/svg"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(good_svg.as_bytes().to_vec())
                .insert_header("Content-Type", "image/svg+xml"),
        )
        .expect(1)
        .mount(&sh_server)
        .await;

    let client = Arc::new(GitHubClient::with_base_url(&gh_server.uri()));
    let service = Arc::new(StarHistoryService::with_test_config(
        client,
        Some(sh_server.uri()),
        None,
        Some(vec![sh_host]),
    ));

    // Launch 5 concurrent requests for the exact same repository
    let mut handles = Vec::new();
    for _ in 0..5 {
        let s = Arc::clone(&service);
        handles.push(tokio::spawn(async move {
            s.get_preview("coalesce-owner", "coalesce-repo", None).await
        }));
    }

    for h in handles {
        let res = h.await.expect("join handle").expect("get_preview ok");
        match res {
            StarHistoryPreviewResult::Ready { svg_data, .. } => {
                assert_eq!(svg_data, good_svg);
            }
            other => panic!("Expected Ready result, got {:?}", other),
        }
    }

    // WireMock verifies that exactly 1 request was sent to each server
}

#[test]
fn test_global_concurrency_limit_constant() {
    assert_eq!(MAX_STAR_HISTORY_PREVIEW_CONCURRENCY, 2);
}
