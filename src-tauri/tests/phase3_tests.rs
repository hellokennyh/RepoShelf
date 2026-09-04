use std::sync::Arc;
use std::time::Duration;
use wiremock::matchers::{header, header_exists, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use reposhelf_lib::error::AppError;
use reposhelf_lib::github::classification::{CATEGORY_AI_AGENTS, CATEGORY_DEV_TOOLS};
use reposhelf_lib::github::GitHubClient;
use reposhelf_lib::secrets::{InMemorySecretStore, SecretStore, SECRET_KEY_GITHUB_TOKEN};
use reposhelf_lib::trending::github::GitHubTrendingProvider;
use reposhelf_lib::trending::models::{TrendingPeriod, TrendingRequest};
use reposhelf_lib::trending::parser::parse_trending_html;
use reposhelf_lib::trending::provider::TrendingProvider;

// Path to fixtures
const FIXTURE_DAILY: &str = include_str!("fixtures/trending/daily.html");
const FIXTURE_WEEKLY: &str = include_str!("fixtures/trending/weekly.html");
const FIXTURE_MONTHLY: &str = include_str!("fixtures/trending/monthly.html");
const FIXTURE_EMPTY: &str = include_str!("fixtures/trending/empty.html");
const FIXTURE_MALFORMED: &str = include_str!("fixtures/trending/malformed.html");

// =============================================================================
// 1. Parser Fixture Tests
// =============================================================================

#[test]
fn test_parser_fixture_daily() {
    let results = parse_trending_html(FIXTURE_DAILY).expect("daily fixture should parse");
    assert!(!results.is_empty(), "daily fixture should contain repos");
    assert_eq!(results.len(), 10);

    // Verify first item
    let first = &results[0];
    assert_eq!(first.trending_rank, 1);
    assert_eq!(first.github_url, "https://github.com/fmtlib/fmt");
    assert_eq!(first.owner, "fmtlib");
    assert_eq!(first.repo, "fmt");
    assert_eq!(first.name, "fmt");
    assert_eq!(
        first.description.as_deref(),
        Some("A modern formatting library")
    );
    assert!(first.stars.is_some(), "total stars should be parsed");
    assert!(
        first.period_stars.is_some(),
        "period stars today should be parsed"
    );
    assert_eq!(first.period_stars, Some(963));

    // Verify ranks follow 1..=N strictly
    for (i, repo) in results.iter().enumerate() {
        assert_eq!(repo.trending_rank, i + 1);
    }
}

#[test]
fn test_parser_fixture_weekly() {
    let results = parse_trending_html(FIXTURE_WEEKLY).expect("weekly fixture should parse");
    assert_eq!(results.len(), 10);

    let first = &results[0];
    assert_eq!(first.trending_rank, 1);
    assert_eq!(first.github_url, "https://github.com/tt-a1i/archify");
    assert_eq!(first.owner, "tt-a1i");
    assert_eq!(first.repo, "archify");
    assert_eq!(first.period_stars, Some(24227));

    for (i, repo) in results.iter().enumerate() {
        assert_eq!(repo.trending_rank, i + 1);
    }
}

#[test]
fn test_parser_fixture_monthly() {
    let results = parse_trending_html(FIXTURE_MONTHLY).expect("monthly fixture should parse");
    assert_eq!(results.len(), 10);

    let first = &results[0];
    assert_eq!(first.trending_rank, 1);
    assert_eq!(first.github_url, "https://github.com/omacom/omarchy");
    assert_eq!(first.owner, "omacom");
    assert_eq!(first.repo, "omarchy");
    assert_eq!(first.period_stars, Some(13597));

    for (i, repo) in results.iter().enumerate() {
        assert_eq!(repo.trending_rank, i + 1);
    }
}

#[test]
fn test_parser_fixture_empty() {
    let results = parse_trending_html(FIXTURE_EMPTY).expect("empty fixture should parse to empty");
    assert!(
        results.is_empty(),
        "recognized blankslate should return empty vec"
    );
}

#[test]
fn test_parser_fixture_all_malformed_fails() {
    let result = parse_trending_html(FIXTURE_MALFORMED);
    assert!(
        result.is_err(),
        "HTML with no valid repo cards should error with TrendingParse"
    );
    match result.err().unwrap() {
        AppError::TrendingParse(_) => {}
        other => panic!("expected TrendingParse, got: {:?}", other),
    }
}

#[test]
fn test_parser_unrecognized_html_fails() {
    let result = parse_trending_html("<html><body><div>Not GitHub at all</div></body></html>");
    assert!(result.is_err());
    match result.err().unwrap() {
        AppError::TrendingParse(_) => {}
        other => panic!("expected TrendingParse, got: {:?}", other),
    }
}

#[test]
fn test_parser_robustness_missing_description_and_stars_whitespace() {
    let synthetic_html = r#"
    <!DOCTYPE html>
    <html>
    <body>
    <div class="Box">
      <!-- Card 1: Missing description, stars with comma and whitespace -->
      <article class="Box-row">
        <h2><a href="/owner1/repo-alpha">owner1 / repo-alpha</a></h2>
        <div class="f6 color-fg-muted mt-2">
          <a href="/owner1/repo-alpha/stargazers">  50,420  </a>
          <span class="d-inline-block float-sm-right">  1,500 stars today </span>
        </div>
      </article>

      <!-- Card 2: Malformed without repo link (should be skipped) -->
      <article class="Box-row">
        <h2>Just a heading</h2>
      </article>

      <!-- Card 3: Normal with description -->
      <article class="Box-row">
        <h2><a href="/owner2/repo-beta">owner2 / repo-beta</a></h2>
        <p>A cool tool</p>
        <div class="f6 color-fg-muted mt-2">
          <a href="/owner2/repo-beta/stargazers">100</a>
          <span class="d-inline-block float-sm-right">10 stars today</span>
        </div>
      </article>
    </div>
    </body>
    </html>
    "#;

    let results = parse_trending_html(synthetic_html).expect("should parse synthetic HTML");
    assert_eq!(results.len(), 2, "malformed card should be safely skipped");

    assert_eq!(results[0].trending_rank, 1);
    assert_eq!(results[0].name, "repo-alpha");
    assert_eq!(results[0].description, None);
    assert_eq!(results[0].stars, Some(50420));
    assert_eq!(results[0].period_stars, Some(1500));

    assert_eq!(results[1].trending_rank, 2);
    assert_eq!(results[1].name, "repo-beta");
    assert_eq!(results[1].description.as_deref(), Some("A cool tool"));
    assert_eq!(results[1].stars, Some(100));
    assert_eq!(results[1].period_stars, Some(10));
}

// =============================================================================
// 2. Trending HTTP Mock Tests (WireMock)
// =============================================================================

#[tokio::test]
async fn test_trending_http_query_params_and_headers() {
    let mock_server = MockServer::start().await;

    // Mock daily with query param since=daily, Accept: text/html, User-Agent RepoShelf
    Mock::given(method("GET"))
        .and(path("/trending"))
        .and(query_param("since", "daily"))
        .and(header("Accept", "text/html"))
        .and(header_exists("User-Agent"))
        .respond_with(ResponseTemplate::new(200).set_body_string(FIXTURE_DAILY))
        .expect(1)
        .mount(&mock_server)
        .await;

    let secret_store: Arc<dyn SecretStore> = Arc::new(InMemorySecretStore::new());
    let github_client = Arc::new(GitHubClient::with_base_url(&mock_server.uri()));
    let provider = GitHubTrendingProvider::with_base_url(
        format!("{}/trending", mock_server.uri()),
        github_client,
        secret_store,
    );

    let html = provider.fetch_html(TrendingPeriod::Daily).await.unwrap();
    assert!(!html.is_empty());
}

#[tokio::test]
async fn test_trending_http_weekly_and_monthly_query_params() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/trending"))
        .and(query_param("since", "weekly"))
        .respond_with(ResponseTemplate::new(200).set_body_string(FIXTURE_WEEKLY))
        .expect(1)
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/trending"))
        .and(query_param("since", "monthly"))
        .respond_with(ResponseTemplate::new(200).set_body_string(FIXTURE_MONTHLY))
        .expect(1)
        .mount(&mock_server)
        .await;

    let secret_store: Arc<dyn SecretStore> = Arc::new(InMemorySecretStore::new());
    let github_client = Arc::new(GitHubClient::with_base_url(&mock_server.uri()));
    let provider = GitHubTrendingProvider::with_base_url(
        format!("{}/trending", mock_server.uri()),
        github_client,
        secret_store,
    );

    let weekly_html = provider.fetch_html(TrendingPeriod::Weekly).await.unwrap();
    assert!(!weekly_html.is_empty());

    let monthly_html = provider.fetch_html(TrendingPeriod::Monthly).await.unwrap();
    assert!(!monthly_html.is_empty());
}

#[tokio::test]
async fn test_trending_http_no_authorization_header_sent() {
    let mock_server = MockServer::start().await;

    // Must NOT have Authorization header
    Mock::given(method("GET"))
        .and(path("/trending"))
        .and(header_exists("Authorization"))
        .respond_with(ResponseTemplate::new(400).set_body_string("Authorization sent to trending!"))
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/trending"))
        .respond_with(ResponseTemplate::new(200).set_body_string(FIXTURE_DAILY))
        .expect(1)
        .mount(&mock_server)
        .await;

    let secret_store: Arc<dyn SecretStore> = Arc::new(InMemorySecretStore::new());
    // Configure token in secret store
    secret_store
        .set_secret(SECRET_KEY_GITHUB_TOKEN, "ghp_secret_token_12345")
        .unwrap();

    let github_client = Arc::new(GitHubClient::with_base_url(&mock_server.uri()));
    let provider = GitHubTrendingProvider::with_base_url(
        format!("{}/trending", mock_server.uri()),
        github_client,
        secret_store,
    );

    // HTML fetch must succeed anonymously without sending Authorization header
    let html = provider.fetch_html(TrendingPeriod::Daily).await.unwrap();
    assert!(!html.is_empty());
}

#[tokio::test]
async fn test_trending_http_status_errors() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/trending"))
        .respond_with(ResponseTemplate::new(500).set_body_string("Internal Server Error"))
        .expect(1)
        .mount(&mock_server)
        .await;

    let secret_store: Arc<dyn SecretStore> = Arc::new(InMemorySecretStore::new());
    let github_client = Arc::new(GitHubClient::with_base_url(&mock_server.uri()));
    let provider = GitHubTrendingProvider::with_base_url(
        format!("{}/trending", mock_server.uri()),
        github_client,
        secret_store,
    );

    let err = provider
        .fetch_html(TrendingPeriod::Daily)
        .await
        .unwrap_err();
    match err {
        AppError::TrendingHttp { status, .. } => assert_eq!(status, 500),
        other => panic!("expected TrendingHttp, got: {:?}", other),
    }
}

#[tokio::test]
async fn test_trending_limit_truncation_and_short_source() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/trending"))
        .respond_with(ResponseTemplate::new(200).set_body_string(FIXTURE_DAILY))
        .mount(&mock_server)
        .await;

    let secret_store: Arc<dyn SecretStore> = Arc::new(InMemorySecretStore::new());
    let github_client = Arc::new(GitHubClient::with_base_url(&mock_server.uri()));
    let provider = GitHubTrendingProvider::with_base_url(
        format!("{}/trending", mock_server.uri()),
        github_client,
        secret_store,
    );

    // Request limit 3 (fixture has 10)
    let resp = provider
        .get_trending(TrendingPeriod::Daily, 3)
        .await
        .unwrap();
    assert_eq!(resp.requested_limit, 3);
    assert_eq!(resp.returned_count, 3);
    assert_eq!(resp.items.len(), 3);
    assert!(!resp.is_short_source, "10 available >= 3 requested");

    // Request limit 50 (fixture has 10)
    let resp50 = provider
        .get_trending(TrendingPeriod::Daily, 50)
        .await
        .unwrap();
    assert_eq!(resp50.requested_limit, 50);
    assert_eq!(resp50.returned_count, 10);
    assert_eq!(resp50.items.len(), 10);
    assert!(resp50.is_short_source, "10 available < 50 requested");
}

// =============================================================================
// 3. REST Metadata Enrichment Tests
// =============================================================================

#[tokio::test]
async fn test_rest_enrichment_success_and_rank_preservation() {
    let mock_server = MockServer::start().await;

    let two_cards_html = r#"
    <!DOCTYPE html>
    <html><body><div class="Box">
      <article class="Box-row">
        <h2><a href="/owner1/repo-one">owner1 / repo-one</a></h2>
        <p>HTML description one</p>
        <span class="d-inline-block float-sm-right">100 stars today</span>
      </article>
      <article class="Box-row">
        <h2><a href="/owner2/repo-two">owner2 / repo-two</a></h2>
        <p>HTML description two</p>
        <span class="d-inline-block float-sm-right">200 stars today</span>
      </article>
    </div></body></html>
    "#;

    Mock::given(method("GET"))
        .and(path("/trending"))
        .respond_with(ResponseTemplate::new(200).set_body_string(two_cards_html))
        .mount(&mock_server)
        .await;

    // Mock REST enrichment for repo-one (AI & Agents topics)
    Mock::given(method("GET"))
        .and(path("/repos/owner1/repo-one"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": 111,
            "html_url": "https://github.com/owner1/repo-one",
            "name": "repo-one",
            "description": "Canonical description one",
            "stargazers_count": 5000,
            "license": { "spdx_id": "MIT", "name": "MIT License" },
            "topics": ["llm", "agent", "ai"],
            "pushed_at": "2026-09-01T12:00:00Z"
        })))
        .mount(&mock_server)
        .await;

    // Mock REST enrichment for repo-two (Developer Tools)
    Mock::given(method("GET"))
        .and(path("/repos/owner2/repo-two"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": 222,
            "html_url": "https://github.com/owner2/repo-two",
            "name": "repo-two",
            "description": "Canonical description two",
            "stargazers_count": 8000,
            "license": { "spdx_id": "Apache-2.0", "name": "Apache License 2.0" },
            "topics": ["cli", "debugger", "profiler"],
            "pushed_at": "2026-09-02T15:00:00Z"
        })))
        .mount(&mock_server)
        .await;

    let secret_store: Arc<dyn SecretStore> = Arc::new(InMemorySecretStore::new());
    let github_client = Arc::new(GitHubClient::with_base_url(&mock_server.uri()));
    let provider = GitHubTrendingProvider::with_base_url(
        format!("{}/trending", mock_server.uri()),
        github_client,
        secret_store,
    );

    let resp = provider
        .get_trending(TrendingPeriod::Daily, 10)
        .await
        .unwrap();
    assert_eq!(resp.items.len(), 2);

    let item1 = &resp.items[0];
    assert_eq!(item1.trending_rank, 1);
    assert_eq!(item1.github_repo_id, Some(111));
    assert_eq!(
        item1.description.as_deref(),
        Some("Canonical description one")
    );
    assert_eq!(item1.stars, Some(5000));
    assert_eq!(item1.license.as_deref(), Some("MIT"));
    assert_eq!(item1.topics, vec!["llm", "agent", "ai"]);
    assert_eq!(item1.category, CATEGORY_AI_AGENTS);
    assert_eq!(item1.period_stars, Some(100));

    let item2 = &resp.items[1];
    assert_eq!(item2.trending_rank, 2);
    assert_eq!(item2.github_repo_id, Some(222));
    assert_eq!(
        item2.description.as_deref(),
        Some("Canonical description two")
    );
    assert_eq!(item2.stars, Some(8000));
    assert_eq!(item2.license.as_deref(), Some("Apache-2.0"));
    assert_eq!(item2.category, CATEGORY_DEV_TOOLS);
    assert_eq!(item2.period_stars, Some(200));

    // No partial warning on 100% success
    assert!(resp.partial_warning.is_none());
    assert_eq!(resp.enrichment_summary.succeeded, 2);
    assert_eq!(resp.enrichment_summary.failed, 0);
}

// =============================================================================
// 4. Partial Failure Tests
// =============================================================================

#[tokio::test]
async fn test_partial_enrichment_failure_degradation() {
    let mock_server = MockServer::start().await;

    let three_cards_html = r#"
    <!DOCTYPE html>
    <html><body><div class="Box">
      <article class="Box-row">
        <h2><a href="/owner1/repo-one">owner1 / repo-one</a></h2>
        <p>HTML description one</p>
        <span class="d-inline-block float-sm-right">100 stars today</span>
      </article>
      <article class="Box-row">
        <h2><a href="/owner2/repo-fail">owner2 / repo-fail</a></h2>
        <p>HTML description fail</p>
        <span class="d-inline-block float-sm-right">200 stars today</span>
      </article>
      <article class="Box-row">
        <h2><a href="/owner3/repo-three">owner3 / repo-three</a></h2>
        <p>HTML description three</p>
        <span class="d-inline-block float-sm-right">300 stars today</span>
      </article>
    </div></body></html>
    "#;

    Mock::given(method("GET"))
        .and(path("/trending"))
        .respond_with(ResponseTemplate::new(200).set_body_string(three_cards_html))
        .mount(&mock_server)
        .await;

    // Repo 1: 200 OK
    Mock::given(method("GET"))
        .and(path("/repos/owner1/repo-one"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": 1,
            "html_url": "https://github.com/owner1/repo-one",
            "name": "repo-one",
            "description": "Enriched repo 1",
            "stargazers_count": 1000,
            "license": { "spdx_id": "MIT" },
            "topics": ["framework"],
            "pushed_at": "2026-09-01T00:00:00Z"
        })))
        .mount(&mock_server)
        .await;

    // Repo 2: 404 Not Found (failure)
    Mock::given(method("GET"))
        .and(path("/repos/owner2/repo-fail"))
        .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
            "message": "Not Found"
        })))
        .mount(&mock_server)
        .await;

    // Repo 3: 200 OK
    Mock::given(method("GET"))
        .and(path("/repos/owner3/repo-three"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": 3,
            "html_url": "https://github.com/owner3/repo-three",
            "name": "repo-three",
            "description": "Enriched repo 3",
            "stargazers_count": 3000,
            "license": { "spdx_id": "MIT" },
            "topics": ["tools"],
            "pushed_at": "2026-09-03T00:00:00Z"
        })))
        .mount(&mock_server)
        .await;

    let secret_store: Arc<dyn SecretStore> = Arc::new(InMemorySecretStore::new());
    let github_client = Arc::new(GitHubClient::with_base_url(&mock_server.uri()));
    let provider = GitHubTrendingProvider::with_base_url(
        format!("{}/trending", mock_server.uri()),
        github_client,
        secret_store,
    );

    let resp = provider
        .get_trending(TrendingPeriod::Daily, 10)
        .await
        .unwrap();

    // 1. All 3 items must exist in response
    assert_eq!(resp.items.len(), 3);

    // 2. Strict rank order 1, 2, 3 must be preserved
    assert_eq!(resp.items[0].trending_rank, 1);
    assert_eq!(resp.items[0].github_repo_id, Some(1));
    assert_eq!(resp.items[0].name, "repo-one");

    assert_eq!(resp.items[1].trending_rank, 2);
    assert_eq!(
        resp.items[1].github_repo_id, None,
        "failed REST item should have None id"
    );
    assert_eq!(resp.items[1].license, None);
    assert!(resp.items[1].topics.is_empty());
    assert_eq!(resp.items[1].name, "repo-fail");
    assert_eq!(
        resp.items[1].description.as_deref(),
        Some("HTML description fail")
    );

    assert_eq!(resp.items[2].trending_rank, 3);
    assert_eq!(resp.items[2].github_repo_id, Some(3));
    assert_eq!(resp.items[2].name, "repo-three");

    // 3. Summary shows 1 failure
    assert_eq!(resp.enrichment_summary.succeeded, 2);
    assert_eq!(resp.enrichment_summary.failed, 1);
    assert!(!resp.enrichment_summary.rate_limited);

    // 4. Partial warning is present
    assert!(resp.partial_warning.is_some());
}

// =============================================================================
// 5. Rate Limit Degradation Tests
// =============================================================================

#[tokio::test]
async fn test_rate_limit_enrichment_degradation() {
    let mock_server = MockServer::start().await;

    let two_cards_html = r#"
    <!DOCTYPE html>
    <html><body><div class="Box">
      <article class="Box-row">
        <h2><a href="/owner1/repo-one">owner1 / repo-one</a></h2>
        <span class="d-inline-block float-sm-right">100 stars today</span>
      </article>
      <article class="Box-row">
        <h2><a href="/owner2/repo-two">owner2 / repo-two</a></h2>
        <span class="d-inline-block float-sm-right">200 stars today</span>
      </article>
    </div></body></html>
    "#;

    Mock::given(method("GET"))
        .and(path("/trending"))
        .respond_with(ResponseTemplate::new(200).set_body_string(two_cards_html))
        .mount(&mock_server)
        .await;

    // Both repos return 403 Rate Limit
    Mock::given(method("GET"))
        .and(path("/repos/owner1/repo-one"))
        .respond_with(
            ResponseTemplate::new(403)
                .insert_header("x-ratelimit-limit", "60")
                .insert_header("x-ratelimit-remaining", "0")
                .insert_header("x-ratelimit-reset", "1725450000")
                .set_body_json(serde_json::json!({
                    "message": "API rate limit exceeded"
                })),
        )
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/repos/owner2/repo-two"))
        .respond_with(
            ResponseTemplate::new(403)
                .insert_header("x-ratelimit-limit", "60")
                .insert_header("x-ratelimit-remaining", "0")
                .insert_header("x-ratelimit-reset", "1725450000")
                .set_body_json(serde_json::json!({
                    "message": "API rate limit exceeded"
                })),
        )
        .mount(&mock_server)
        .await;

    let secret_store: Arc<dyn SecretStore> = Arc::new(InMemorySecretStore::new());
    let github_client = Arc::new(GitHubClient::with_base_url(&mock_server.uri()));
    let provider = GitHubTrendingProvider::with_base_url(
        format!("{}/trending", mock_server.uri()),
        github_client,
        secret_store,
    );

    let resp = provider
        .get_trending(TrendingPeriod::Daily, 10)
        .await
        .unwrap();

    // 1. Trending items still returned despite rate limit
    assert_eq!(resp.items.len(), 2);
    assert_eq!(resp.items[0].trending_rank, 1);
    assert_eq!(resp.items[1].trending_rank, 2);

    // 2. Summary indicates rate limited
    assert!(resp.enrichment_summary.rate_limited);
    assert_eq!(resp.enrichment_summary.rate_limit_reset, Some(1725450000));

    // 3. Partial warning explicitly mentions rate limit
    let warning = resp.partial_warning.expect("should have partial warning");
    assert!(warning.contains("限额") || warning.contains("rate limit"));
}

// =============================================================================
// 6. Concurrency Tests
// =============================================================================

#[tokio::test]
async fn test_rest_enrichment_max_concurrency_bounded_to_4() {
    let mock_server = MockServer::start().await;

    // Create 8 cards to ensure potential concurrency > 4 if unbounded
    let mut eight_cards = String::from("<!DOCTYPE html><html><body><div class=\"Box\">");
    for i in 1..=8 {
        eight_cards.push_str(&format!(
            r#"<article class="Box-row">
                 <h2><a href="/owner{i}/repo-{i}">owner{i} / repo-{i}</a></h2>
                 <span class="d-inline-block float-sm-right">{i}00 stars today</span>
               </article>"#,
            i = i
        ));
    }
    eight_cards.push_str("</div></body></html>");

    Mock::given(method("GET"))
        .and(path("/trending"))
        .respond_with(ResponseTemplate::new(200).set_body_string(eight_cards))
        .mount(&mock_server)
        .await;

    // Async delay in WireMock to allow overlapping concurrent calls
    for i in 1..=8 {
        Mock::given(method("GET"))
            .and(path(format!("/repos/owner{}/repo-{}", i, i)))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_delay(Duration::from_millis(80))
                    .set_body_json(serde_json::json!({
                        "id": i,
                        "html_url": format!("https://github.com/owner{i}/repo-{i}", i = i),
                        "name": format!("repo-{}", i),
                        "description": format!("Repo {}", i),
                        "stargazers_count": 1000 * i,
                        "license": { "spdx_id": "MIT" },
                        "topics": ["rust"],
                        "pushed_at": "2026-09-01T00:00:00Z"
                    })),
            )
            .mount(&mock_server)
            .await;
    }

    let secret_store: Arc<dyn SecretStore> = Arc::new(InMemorySecretStore::new());
    let github_client = Arc::new(GitHubClient::with_base_url(&mock_server.uri()));
    let provider = GitHubTrendingProvider::with_base_url(
        format!("{}/trending", mock_server.uri()),
        github_client,
        secret_store,
    );

    let resp = provider
        .get_trending(TrendingPeriod::Daily, 8)
        .await
        .unwrap();
    assert_eq!(resp.items.len(), 8);

    let observed_max = provider.max_observed_concurrency();
    println!(
        "Observed max concurrent REST enrichment requests: {}",
        observed_max
    );

    // Concurrency must NEVER exceed 4
    assert!(
        observed_max <= 4,
        "Observed concurrency was {} which exceeds maximum limit of 4!",
        observed_max
    );
    // Concurrency should be >= 2 to prove parallel execution occurred
    assert!(
        observed_max >= 2,
        "Observed concurrency was {} which indicates no parallelism occurred",
        observed_max
    );
}

// =============================================================================
// 7. Authentication Boundary Tests
// =============================================================================

#[tokio::test]
async fn test_auth_boundary_rest_uses_token_trending_html_does_not() {
    let mock_server = MockServer::start().await;

    let one_card_html = r#"
    <!DOCTYPE html>
    <html><body><div class="Box">
      <article class="Box-row">
        <h2><a href="/owner/repo-secret">owner / repo-secret</a></h2>
        <span class="d-inline-block float-sm-right">100 stars today</span>
      </article>
    </div></body></html>
    "#;

    // Trending HTML: MUST NOT have Authorization
    Mock::given(method("GET"))
        .and(path("/trending"))
        .respond_with(ResponseTemplate::new(200).set_body_string(one_card_html))
        .expect(1)
        .mount(&mock_server)
        .await;

    // REST enrichment: MUST have Authorization: Bearer ghp_valid_token_xyz
    Mock::given(method("GET"))
        .and(path("/repos/owner/repo-secret"))
        .and(header("Authorization", "Bearer ghp_valid_token_xyz"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": 999,
            "html_url": "https://github.com/owner/repo-secret",
            "name": "repo-secret",
            "description": "Secret authenticated repo",
            "stargazers_count": 999,
            "license": { "spdx_id": "MIT" },
            "topics": ["secret"],
            "pushed_at": "2026-09-01T00:00:00Z"
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    let secret_store: Arc<dyn SecretStore> = Arc::new(InMemorySecretStore::new());
    secret_store
        .set_secret(SECRET_KEY_GITHUB_TOKEN, "ghp_valid_token_xyz")
        .unwrap();

    let github_client = Arc::new(GitHubClient::with_base_url(&mock_server.uri()));
    let provider = GitHubTrendingProvider::with_base_url(
        format!("{}/trending", mock_server.uri()),
        github_client,
        secret_store,
    );

    let resp = provider
        .get_trending(TrendingPeriod::Daily, 1)
        .await
        .unwrap();
    assert_eq!(resp.items.len(), 1);
    assert_eq!(resp.items[0].github_repo_id, Some(999));
    assert_eq!(resp.enrichment_summary.succeeded, 1);
}

#[test]
fn test_trending_request_input_validation() {
    assert!(TrendingRequest::new(TrendingPeriod::Daily, 20).is_ok());
    assert!(TrendingRequest::new(TrendingPeriod::Weekly, 1).is_ok());
    assert!(TrendingRequest::new(TrendingPeriod::Monthly, 50).is_ok());

    // 0 is invalid
    assert!(TrendingRequest::new(TrendingPeriod::Daily, 0).is_err());
    // 51 is invalid
    assert!(TrendingRequest::new(TrendingPeriod::Daily, 51).is_err());
}
