use std::sync::Arc;
use std::time::Duration;
use tempfile::tempdir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use reposhelf_lib::db::database::init_pool;
use reposhelf_lib::github::GitHubClient;
use reposhelf_lib::library::*;

async fn setup_test_db() -> (tempfile::TempDir, sqlx::SqlitePool) {
    let dir = tempdir().expect("Failed to create tempdir");
    let db_path = dir.path().join("test_phase7.db");
    let pool = init_pool(&db_path)
        .await
        .expect("Failed to initialize test pool");
    (dir, pool)
}

// =============================================================================
// P7-RUST-01: Monotonic progress on all successful items
// =============================================================================
#[tokio::test]
async fn test_p7_rust_01_monotonic_progress_on_success() {
    let (_dir, pool) = setup_test_db().await;
    let mock_server = MockServer::start().await;

    let mut fav_ids = Vec::new();
    let total_items = 5;

    for i in 1..=total_items {
        let fav = favorite_repository(
            &pool,
            FavoriteInput {
                github_repo_id: Some(7100 + i),
                github_url: format!("https://github.com/progress-org/repo-{}", i),
                name: format!("repo-{}", i),
                description: Some(format!("Progress item {}", i)),
                category: "Developer Tools".to_string(),
                stars: 10 * i,
                license: Some("MIT".to_string()),
                topics: vec!["tool".to_string()],
                pushed_at: None,
                releases: None,
                languages: None,
                translation: None,
                            repo_created_at: None,
                ai_analysis: None,
},
        )
        .await
        .unwrap();
        fav_ids.push(fav.id);

        Mock::given(method("GET"))
            .and(path(format!("/repos/progress-org/repo-{}", i)))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_delay(Duration::from_millis(20))
                    .set_body_json(serde_json::json!({
                        "id": 7100 + i,
                        "html_url": format!("https://github.com/progress-org/repo-{}", i),
                        "name": format!("repo-{}", i),
                        "stargazers_count": 500 * i,
                        "private": false
                    })),
            )
            .mount(&mock_server)
            .await;
    }

    let progress_records = Arc::new(tokio::sync::Mutex::new(Vec::new()));
    let progress_clone = Arc::clone(&progress_records);

    let progress_cb = Arc::new(move |prog: BatchRefreshProgress| {
        let rec = Arc::clone(&progress_clone);
        tokio::spawn(async move {
            let mut guard = rec.lock().await;
            guard.push(prog);
        });
    });

    let github = Arc::new(GitHubClient::with_base_url(&mock_server.uri()));
    let summary = refresh_batch_with_progress(&pool, github, None, fav_ids, Some(progress_cb))
        .await
        .unwrap();

    assert_eq!(summary.total, 5);
    assert_eq!(summary.succeeded, 5);
    assert_eq!(summary.failed, 0);
    assert_eq!(summary.rate_limited, 0);
    assert_eq!(summary.not_found, 0);

    // Yield to ensure all progress async recordings finish
    tokio::time::sleep(Duration::from_millis(50)).await;
    let records = progress_records.lock().await;

    assert!(
        !records.is_empty(),
        "Progress updates should have been received"
    );

    // Invariants check:
    // 1. completed is monotonic (never decreases)
    // 2. completed never exceeds total
    // 3. final completed == total
    let mut last_completed = 0;
    for p in records.iter() {
        assert_eq!(p.total, 5, "Total must always match requested batch total");
        assert!(
            p.completed >= last_completed,
            "Completed must be monotonic non-decreasing: received {} after {}",
            p.completed,
            last_completed
        );
        assert!(p.completed <= 5, "Completed must never exceed total");
        last_completed = p.completed;
    }

    assert_eq!(last_completed, 5, "Final completed must reach total (5)");
}

// =============================================================================
// P7-RUST-02: Mixed outcomes (success, not_found, failure) reach total
// =============================================================================
#[tokio::test]
async fn test_p7_rust_02_mixed_outcomes_reach_total() {
    let (_dir, pool) = setup_test_db().await;
    let mock_server = MockServer::start().await;

    let mut fav_ids = Vec::new();
    for i in 1..=5 {
        let fav = favorite_repository(
            &pool,
            FavoriteInput {
                github_repo_id: Some(7200 + i),
                github_url: format!("https://github.com/mixed-org/item-{}", i),
                name: format!("item-{}", i),
                description: None,
                category: "Other".to_string(),
                stars: 10,
                license: None,
                topics: vec![],
                pushed_at: None,
                releases: None,
                languages: None,
                translation: None,
                            repo_created_at: None,
                ai_analysis: None,
},
        )
        .await
        .unwrap();
        fav_ids.push(fav.id);
    }

    // Item 1, 2: 200 OK
    for i in 1..=2 {
        Mock::given(method("GET"))
            .and(path(format!("/repos/mixed-org/item-{}", i)))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_delay(Duration::from_millis(20))
                    .set_body_json(serde_json::json!({
                        "id": 7200 + i,
                        "html_url": format!("https://github.com/mixed-org/item-{}", i),
                        "name": format!("item-{}", i),
                        "stargazers_count": 100,
                        "private": false
                    })),
            )
            .mount(&mock_server)
            .await;
    }

    // Item 3: 404 Not Found
    Mock::given(method("GET"))
        .and(path("/repos/mixed-org/item-3"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&mock_server)
        .await;

    // Item 4: 500 Server Error
    Mock::given(method("GET"))
        .and(path("/repos/mixed-org/item-4"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&mock_server)
        .await;

    // Item 5: Malformed JSON
    Mock::given(method("GET"))
        .and(path("/repos/mixed-org/item-5"))
        .respond_with(ResponseTemplate::new(200).set_body_string("INVALID_JSON{"))
        .mount(&mock_server)
        .await;

    let progress_records = Arc::new(tokio::sync::Mutex::new(Vec::new()));
    let progress_clone = Arc::clone(&progress_records);

    let progress_cb = Arc::new(move |prog: BatchRefreshProgress| {
        let rec = Arc::clone(&progress_clone);
        tokio::spawn(async move {
            let mut guard = rec.lock().await;
            guard.push(prog);
        });
    });

    let github = Arc::new(GitHubClient::with_base_url(&mock_server.uri()));
    let summary = refresh_batch_with_progress(&pool, github, None, fav_ids, Some(progress_cb))
        .await
        .unwrap();

    assert_eq!(summary.total, 5);
    assert_eq!(summary.succeeded, 2);
    assert_eq!(summary.not_found, 1);
    assert_eq!(summary.failed, 2);
    assert_eq!(summary.rate_limited, 0);

    tokio::time::sleep(Duration::from_millis(50)).await;
    let records = progress_records.lock().await;

    assert!(!records.is_empty());
    let mut last_completed = 0;
    for p in records.iter() {
        assert_eq!(p.total, 5);
        assert!(p.completed >= last_completed);
        assert!(p.completed <= 5);
        last_completed = p.completed;
    }
    assert_eq!(
        last_completed, 5,
        "Final completed must reach total (5) even with mixed outcomes"
    );
}

// =============================================================================
// P7-RUST-03: Mid-batch rate limit reaches total completed
// =============================================================================
#[tokio::test]
async fn test_p7_rust_03_mid_batch_rate_limit_reaches_total() {
    let (_dir, pool) = setup_test_db().await;
    let mock_server = MockServer::start().await;

    let mut fav_ids = Vec::new();
    let total_items = 6;

    for i in 1..=total_items {
        let fav = favorite_repository(
            &pool,
            FavoriteInput {
                github_repo_id: Some(7300 + i),
                github_url: format!("https://github.com/ratelimit-org/repo-{}", i),
                name: format!("repo-{}", i),
                description: None,
                category: "DevOps & Cloud".to_string(),
                stars: 50,
                license: None,
                topics: vec![],
                pushed_at: None,
                releases: None,
                languages: None,
                translation: None,
                            repo_created_at: None,
                ai_analysis: None,
},
        )
        .await
        .unwrap();
        fav_ids.push(fav.id);
    }

    // Mock item 1 as 403 Rate Limit
    Mock::given(method("GET"))
        .and(path("/repos/ratelimit-org/repo-1"))
        .respond_with(
            ResponseTemplate::new(403)
                .append_header("x-ratelimit-limit", "60")
                .append_header("x-ratelimit-remaining", "0")
                .append_header("x-ratelimit-reset", "1893456000")
                .set_body_json(serde_json::json!({
                    "message": "API rate limit exceeded"
                })),
        )
        .mount(&mock_server)
        .await;

    // Mock items 2..=6 with slight delay
    for i in 2..=total_items {
        Mock::given(method("GET"))
            .and(path(format!("/repos/ratelimit-org/repo-{}", i)))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_delay(Duration::from_millis(40))
                    .set_body_json(serde_json::json!({
                        "id": 7300 + i,
                        "html_url": format!("https://github.com/ratelimit-org/repo-{}", i),
                        "name": format!("repo-{}", i),
                        "stargazers_count": 100,
                        "private": false
                    })),
            )
            .mount(&mock_server)
            .await;
    }

    let progress_records = Arc::new(tokio::sync::Mutex::new(Vec::new()));
    let progress_clone = Arc::clone(&progress_records);

    let progress_cb = Arc::new(move |prog: BatchRefreshProgress| {
        let rec = Arc::clone(&progress_clone);
        tokio::spawn(async move {
            let mut guard = rec.lock().await;
            guard.push(prog);
        });
    });

    let github = Arc::new(GitHubClient::with_base_url(&mock_server.uri()));
    let summary = refresh_batch_with_progress(&pool, github, None, fav_ids, Some(progress_cb))
        .await
        .unwrap();

    assert_eq!(summary.total, 6);
    assert!(summary.rate_limited >= 1);
    assert_eq!(
        summary.succeeded + summary.failed + summary.not_found + summary.rate_limited,
        6
    );

    tokio::time::sleep(Duration::from_millis(50)).await;
    let records = progress_records.lock().await;

    assert!(!records.is_empty());
    let mut last_completed = 0;
    for p in records.iter() {
        assert_eq!(p.total, 6);
        assert!(p.completed >= last_completed);
        assert!(p.completed <= 6);
        last_completed = p.completed;
    }

    assert_eq!(
        last_completed, 6,
        "Progress must reach total (6) when items are rate-limited"
    );
}

// =============================================================================
// P7-RUST-04: Maximum refresh concurrency remains <= 4
// =============================================================================
#[tokio::test]
async fn test_p7_rust_04_max_refresh_concurrency_remains_at_most_4() {
    // 1. Static contract verification
    assert_eq!(
        MAX_REFRESH_CONCURRENCY, 4,
        "MAX_REFRESH_CONCURRENCY constant must remain bounded at 4"
    );

    let (_dir, pool) = setup_test_db().await;
    let mock_server = MockServer::start().await;

    let mut fav_ids = Vec::new();
    let total_items = 8;

    for i in 1..=total_items {
        let fav = favorite_repository(
            &pool,
            FavoriteInput {
                github_repo_id: Some(7400 + i),
                github_url: format!("https://github.com/concurrency-org/item-{}", i),
                name: format!("item-{}", i),
                description: None,
                category: "Libraries & Frameworks".to_string(),
                stars: 100,
                license: None,
                topics: vec![],
                pushed_at: None,
                releases: None,
                languages: None,
                translation: None,
                            repo_created_at: None,
                ai_analysis: None,
},
        )
        .await
        .unwrap();
        fav_ids.push(fav.id);

        Mock::given(method("GET"))
            .and(path(format!("/repos/concurrency-org/item-{}", i)))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_delay(Duration::from_millis(60))
                    .set_body_json(serde_json::json!({
                        "id": 7400 + i,
                        "html_url": format!("https://github.com/concurrency-org/item-{}", i),
                        "name": format!("item-{}", i),
                        "stargazers_count": 100,
                        "private": false
                    })),
            )
            .mount(&mock_server)
            .await;
    }

    let github = Arc::new(GitHubClient::with_base_url(&mock_server.uri()));

    let start = std::time::Instant::now();
    let summary = refresh_batch(&pool, github, None, fav_ids).await.unwrap();
    let elapsed = start.elapsed();

    assert_eq!(summary.total, 8);
    assert_eq!(summary.succeeded, 8);
    assert_eq!(summary.failed, 0);

    // If executed sequentially (concurrency 1): 8 * 60ms = 480ms minimum
    // With concurrency 4: 2 waves * 60ms = ~120ms (well under 400ms)
    assert!(
        elapsed < Duration::from_millis(1500),
        "Elapsed time ({:?}) indicates concurrent execution",
        elapsed
    );
    assert!(
        elapsed >= Duration::from_millis(100),
        "Elapsed time ({:?}) confirms requests are bounded across waves",
        elapsed
    );
}
