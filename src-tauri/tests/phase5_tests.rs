use std::time::Duration;
use tempfile::tempdir;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use reposhelf_lib::db::database::init_pool;
use reposhelf_lib::db::models::NewSelectionHistoryRecord;
use reposhelf_lib::db::selection_history::{count_selection_history, insert_selection};
use reposhelf_lib::error::AppError;
use reposhelf_lib::github::{build_search_query, GitHubClient, RepositorySearchRequest};
use reposhelf_lib::library::*;

async fn setup_test_db() -> (tempfile::TempDir, sqlx::SqlitePool) {
    let dir = tempdir().expect("Failed to create tempdir");
    let db_path = dir.path().join("test_phase5.db");
    let pool = init_pool(&db_path)
        .await
        .expect("Failed to initialize test pool");
    (dir, pool)
}

// =============================================================================
// 1. Phase 2 Compatibility & Request Validation Tests
// =============================================================================

#[test]
fn test_request_validation_and_serde_defaults() {
    // Serde default test: omitting exclusion flags deserializes them to false
    let json_str = r#"{ "query": "rust", "limit": 20 }"#;
    let req: RepositorySearchRequest = serde_json::from_str(json_str).unwrap();
    assert_eq!(req.query, "rust");
    assert_eq!(req.limit, 20);
    assert!(!req.exclude_selected);
    assert!(!req.exclude_favorites);

    // Existing validation: empty query rejected
    let bad_query = RepositorySearchRequest {
        query: "   ".to_string(),
        min_stars: None,
        pushed_after: None,
        limit: 20,
        exclude_selected: false,
        exclude_favorites: false,
    };
    assert!(build_search_query(&bad_query).is_err());

    // Existing validation: limit 0 rejected
    let bad_limit_0 = RepositorySearchRequest {
        query: "test".to_string(),
        min_stars: None,
        pushed_after: None,
        limit: 0,
        exclude_selected: false,
        exclude_favorites: false,
    };
    assert!(build_search_query(&bad_limit_0).is_err());

    // Existing validation: limit > 100 rejected
    let bad_limit_101 = RepositorySearchRequest {
        query: "test".to_string(),
        min_stars: None,
        pushed_after: None,
        limit: 101,
        exclude_selected: false,
        exclude_favorites: false,
    };
    assert!(build_search_query(&bad_limit_101).is_err());

    // Existing validation: negative stars rejected
    let bad_stars = RepositorySearchRequest {
        query: "test".to_string(),
        min_stars: Some(-1),
        pushed_after: None,
        limit: 20,
        exclude_selected: false,
        exclude_favorites: false,
    };
    assert!(build_search_query(&bad_stars).is_err());

    // Existing validation: invalid pushed_after format rejected
    let bad_date = RepositorySearchRequest {
        query: "test".to_string(),
        min_stars: None,
        pushed_after: Some("2026/01/01".to_string()),
        limit: 20,
        exclude_selected: false,
        exclude_favorites: false,
    };
    assert!(build_search_query(&bad_date).is_err());
}

#[tokio::test]
async fn test_no_exclusion_path_single_page_best_match_order() {
    let (_dir, pool) = setup_test_db().await;
    let mock_server = MockServer::start().await;

    let search_body = serde_json::json!({
        "total_count": 3,
        "items": [
            { "id": 101, "html_url": "https://github.com/test/alpha", "name": "alpha", "stargazers_count": 10, "private": false },
            { "id": 102, "html_url": "https://github.com/test/beta", "name": "beta", "stargazers_count": 50, "private": false },
            { "id": 103, "html_url": "https://github.com/test/gamma", "name": "gamma", "stargazers_count": 5, "private": false }
        ]
    });

    Mock::given(method("GET"))
        .and(path("/search/repositories"))
        .and(query_param("q", "test"))
        .and(query_param("per_page", "20"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&search_body))
        .expect(1) // Single page request only
        .mount(&mock_server)
        .await;

    let client = GitHubClient::with_base_url(&mock_server.uri());
    let req = RepositorySearchRequest {
        query: "test".to_string(),
        min_stars: None,
        pushed_after: None,
        limit: 20,
        exclude_selected: false,
        exclude_favorites: false,
    };

    let resp = execute_search(&pool, &client, None, &req).await.unwrap();

    // Preserves API Best Match order without re-sorting by stars
    assert_eq!(resp.items.len(), 3);
    assert_eq!(resp.items[0].github_repo_id, 101);
    assert_eq!(resp.items[1].github_repo_id, 102);
    assert_eq!(resp.items[2].github_repo_id, 103);
    assert_eq!(resp.total_count, 3);

    // Selection history recorded for final items
    let history_count = count_selection_history(&pool).await.unwrap();
    assert_eq!(history_count, 3);
}

// =============================================================================
// 2. Exclusion Filtering Tests (Selected & Favorites)
// =============================================================================

#[tokio::test]
async fn test_exclude_selected_by_repo_id() {
    let (_dir, pool) = setup_test_db().await;
    let mock_server = MockServer::start().await;

    // Seed selection_history with repo 101
    insert_selection(
        &pool,
        &NewSelectionHistoryRecord {
            github_repo_id: Some(101),
            github_url: "https://github.com/test/first".to_string(),
            selected_at: "2026-03-01T00:00:00Z".to_string(),
        },
    )
    .await
    .unwrap();

    let search_body = serde_json::json!({
        "total_count": 2,
        "items": [
            { "id": 101, "html_url": "https://github.com/test/first", "name": "first", "stargazers_count": 10, "private": false },
            { "id": 102, "html_url": "https://github.com/test/second", "name": "second", "stargazers_count": 20, "private": false }
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
        limit: 10,
        exclude_selected: true,
        exclude_favorites: false,
    };

    let resp = execute_search(&pool, &client, None, &req).await.unwrap();

    // 101 is excluded by repo ID, 102 survives
    assert_eq!(resp.items.len(), 1);
    assert_eq!(resp.items[0].github_repo_id, 102);

    // Only the newly selected item (102) was added to history (initial 1 + new 1 = 2)
    assert_eq!(count_selection_history(&pool).await.unwrap(), 2);
}

#[tokio::test]
async fn test_exclude_selected_by_canonical_url_fallback() {
    let (_dir, pool) = setup_test_db().await;
    let mock_server = MockServer::start().await;

    // Seed selection_history with NULL repo_id and canonical url
    insert_selection(
        &pool,
        &NewSelectionHistoryRecord {
            github_repo_id: None,
            github_url: "https://github.com/test/url-fallback".to_string(),
            selected_at: "2026-03-01T00:00:00Z".to_string(),
        },
    )
    .await
    .unwrap();

    let search_body = serde_json::json!({
        "total_count": 2,
        "items": [
            // Candidate has a different ID, but matches the canonical URL with trailing slash
            { "id": 999, "html_url": "https://github.com/test/url-fallback/", "name": "url-fallback", "stargazers_count": 10, "private": false },
            { "id": 102, "html_url": "https://github.com/test/survivor", "name": "survivor", "stargazers_count": 20, "private": false }
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
        limit: 10,
        exclude_selected: true,
        exclude_favorites: false,
    };

    let resp = execute_search(&pool, &client, None, &req).await.unwrap();

    assert_eq!(resp.items.len(), 1);
    assert_eq!(resp.items[0].github_repo_id, 102);
}

#[tokio::test]
async fn test_exclude_favorite_by_repo_id() {
    let (_dir, pool) = setup_test_db().await;
    let mock_server = MockServer::start().await;

    // Seed a favorite with repo_id 201
    favorite_repository(
        &pool,
        FavoriteInput {
            github_repo_id: Some(201),
            github_url: "https://github.com/test/fav1".to_string(),
            name: "fav1".to_string(),
            description: None,
            category: "Developer Tools".to_string(),
            stars: 100,
            license: None,
            topics: vec![],
            pushed_at: None,
            releases: None,
            languages: None,
        },
    )
    .await
    .unwrap();

    let search_body = serde_json::json!({
        "total_count": 2,
        "items": [
            { "id": 201, "html_url": "https://github.com/test/fav1", "name": "fav1", "stargazers_count": 100, "private": false },
            { "id": 202, "html_url": "https://github.com/test/fav2", "name": "fav2", "stargazers_count": 200, "private": false }
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
        limit: 10,
        exclude_selected: false,
        exclude_favorites: true,
    };

    let resp = execute_search(&pool, &client, None, &req).await.unwrap();

    assert_eq!(resp.items.len(), 1);
    assert_eq!(resp.items[0].github_repo_id, 202);
}

#[tokio::test]
async fn test_exclude_favorite_by_canonical_url_fallback() {
    let (_dir, pool) = setup_test_db().await;
    let mock_server = MockServer::start().await;

    // Seed a favorite with no repo_id and canonical URL
    favorite_repository(
        &pool,
        FavoriteInput {
            github_repo_id: None,
            github_url: "https://github.com/test/url-fav".to_string(),
            name: "url-fav".to_string(),
            description: None,
            category: "Other".to_string(),
            stars: 50,
            license: None,
            topics: vec![],
            pushed_at: None,
            releases: None,
            languages: None,
        },
    )
    .await
    .unwrap();

    let search_body = serde_json::json!({
        "total_count": 2,
        "items": [
            { "id": 888, "html_url": "https://github.com/test/url-fav", "name": "url-fav", "stargazers_count": 50, "private": false },
            { "id": 203, "html_url": "https://github.com/test/other", "name": "other", "stargazers_count": 10, "private": false }
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
        limit: 10,
        exclude_selected: false,
        exclude_favorites: true,
    };

    let resp = execute_search(&pool, &client, None, &req).await.unwrap();

    assert_eq!(resp.items.len(), 1);
    assert_eq!(resp.items[0].github_repo_id, 203);
}

#[tokio::test]
async fn test_both_exclusions_together() {
    let (_dir, pool) = setup_test_db().await;
    let mock_server = MockServer::start().await;

    // Seed selection_history with Item A (id: 301)
    insert_selection(
        &pool,
        &NewSelectionHistoryRecord {
            github_repo_id: Some(301),
            github_url: "https://github.com/test/item-a".to_string(),
            selected_at: "2026-03-01T00:00:00Z".to_string(),
        },
    )
    .await
    .unwrap();

    // Seed favorites with Item B (id: 302)
    favorite_repository(
        &pool,
        FavoriteInput {
            github_repo_id: Some(302),
            github_url: "https://github.com/test/item-b".to_string(),
            name: "item-b".to_string(),
            description: None,
            category: "Other".to_string(),
            stars: 10,
            license: None,
            topics: vec![],
            pushed_at: None,
            releases: None,
            languages: None,
        },
    )
    .await
    .unwrap();

    let search_body = serde_json::json!({
        "total_count": 3,
        "items": [
            { "id": 301, "html_url": "https://github.com/test/item-a", "name": "item-a", "stargazers_count": 10, "private": false },
            { "id": 302, "html_url": "https://github.com/test/item-b", "name": "item-b", "stargazers_count": 20, "private": false },
            { "id": 303, "html_url": "https://github.com/test/item-c", "name": "item-c", "stargazers_count": 30, "private": false }
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
        limit: 10,
        exclude_selected: true,
        exclude_favorites: true,
    };

    let resp = execute_search(&pool, &client, None, &req).await.unwrap();

    assert_eq!(resp.items.len(), 1);
    assert_eq!(resp.items[0].github_repo_id, 303);
}

// =============================================================================
// 3. Bounded Replenishment & Pagination Tests
// =============================================================================

#[tokio::test]
async fn test_replenishment_reaches_later_page_and_stops_when_limit_satisfied() {
    let (_dir, pool) = setup_test_db().await;
    let mock_server = MockServer::start().await;

    // Seed favorites with repos 1..=40
    for i in 1..=40 {
        favorite_repository(
            &pool,
            FavoriteInput {
                github_repo_id: Some(i),
                github_url: format!("https://github.com/test/fav{}", i),
                name: format!("fav{}", i),
                description: None,
                category: "Other".to_string(),
                stars: 10,
                license: None,
                topics: vec![],
                pushed_at: None,
                releases: None,
                languages: None,
            },
        )
        .await
        .unwrap();
    }

    // Page 1: 40 excluded favorites (1..=40) and 10 survivors (401..=410) = 50 items
    let mut p1_items: Vec<serde_json::Value> = (1..=40)
        .map(|i| {
            serde_json::json!({
                "id": i,
                "html_url": format!("https://github.com/test/fav{}", i),
                "name": format!("fav{}", i),
                "stargazers_count": 10,
                "private": false
            })
        })
        .collect();
    for i in 401..=410 {
        p1_items.push(serde_json::json!({
            "id": i,
            "html_url": format!("https://github.com/test/surv{}", i),
            "name": format!("surv{}", i),
            "stargazers_count": 20,
            "private": false
        }));
    }

    let page1_body = serde_json::json!({
        "total_count": 200,
        "items": p1_items
    });

    // Page 2: contains 40 survivors (411..=450) and 10 extra (451..=460) = 50 items
    let p2_items: Vec<serde_json::Value> = (411..=460)
        .map(|i| {
            serde_json::json!({
                "id": i,
                "html_url": format!("https://github.com/test/surv{}", i),
                "name": format!("surv{}", i),
                "stargazers_count": 30,
                "private": false
            })
        })
        .collect();

    let page2_body = serde_json::json!({
        "total_count": 200,
        "items": p2_items
    });

    Mock::given(method("GET"))
        .and(path("/search/repositories"))
        .and(query_param("page", "1"))
        .and(query_param("per_page", "50"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&page1_body))
        .expect(1)
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/search/repositories"))
        .and(query_param("page", "2"))
        .and(query_param("per_page", "50"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&page2_body))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = GitHubClient::with_base_url(&mock_server.uri());
    let req = RepositorySearchRequest {
        query: "test".to_string(),
        min_stars: None,
        pushed_after: None,
        limit: 50, // Limit is 50; candidate budget = min(max(150, 100), 300) = 150. Fixed page_size = 50.
        exclude_selected: false,
        exclude_favorites: true,
    };

    let resp = execute_search(&pool, &client, None, &req).await.unwrap();

    // Exactly 50 items returned (401..=450)
    assert_eq!(resp.items.len(), 50);
    assert_eq!(resp.items[0].github_repo_id, 401);
    assert_eq!(resp.items[49].github_repo_id, 450);
    assert_eq!(resp.total_count, 200);

    // Exactly the 50 returned items are written to selection_history
    assert_eq!(count_selection_history(&pool).await.unwrap(), 50);
}

#[tokio::test]
async fn test_candidate_budget_and_hard_ceiling_enforced() {
    let (_dir, pool) = setup_test_db().await;
    let mock_server = MockServer::start().await;

    // Limit = 100 -> candidate_budget = min(max(300, 100), 300) = 300
    // We mock page 1, 2, and 3, each returning 100 items all in favorites.
    // The loop must terminate after 300 slots without requesting page 4.
    for i in 1..=300 {
        favorite_repository(
            &pool,
            FavoriteInput {
                github_repo_id: Some(i),
                github_url: format!("https://github.com/test/repo-{}", i),
                name: format!("repo-{}", i),
                description: None,
                category: "Other".to_string(),
                stars: 10,
                license: None,
                topics: vec![],
                pushed_at: None,
                releases: None,
                languages: None,
            },
        )
        .await
        .unwrap();
    }

    for page in 1..=3 {
        let items: Vec<serde_json::Value> = ((page - 1) * 100 + 1..=page * 100)
            .map(|id| {
                serde_json::json!({
                    "id": id,
                    "html_url": format!("https://github.com/test/repo-{}", id),
                    "name": format!("repo-{}", id),
                    "stargazers_count": 10,
                    "private": false
                })
            })
            .collect();

        Mock::given(method("GET"))
            .and(path("/search/repositories"))
            .and(query_param("page", page.to_string()))
            .and(query_param("per_page", "100"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "total_count": 1000,
                "items": items
            })))
            .expect(1)
            .mount(&mock_server)
            .await;
    }

    let client = GitHubClient::with_base_url(&mock_server.uri());
    let req = RepositorySearchRequest {
        query: "test".to_string(),
        min_stars: None,
        pushed_after: None,
        limit: 100,
        exclude_selected: false,
        exclude_favorites: true,
    };

    let resp = execute_search(&pool, &client, None, &req).await.unwrap();

    // Budget exhausted, all candidates excluded
    assert_eq!(resp.items.len(), 0);
    assert_eq!(resp.total_count, 1000);
}

#[tokio::test]
async fn test_cross_page_deduplication_stable() {
    let (_dir, pool) = setup_test_db().await;
    let mock_server = MockServer::start().await;

    // Seed repos 1..=40 as favorited
    for i in 1..=40 {
        favorite_repository(
            &pool,
            FavoriteInput {
                github_repo_id: Some(i),
                github_url: format!("https://github.com/test/dedup-fav{}", i),
                name: format!("dedup-fav{}", i),
                description: None,
                category: "Other".to_string(),
                stars: 10,
                license: None,
                topics: vec![],
                pushed_at: None,
                releases: None,
                languages: None,
            },
        )
        .await
        .unwrap();
    }

    // Page 1: 40 favorited items + 10 eligible (501..=510) = 50 items
    let mut p1_items: Vec<serde_json::Value> = (1..=40)
        .map(|i| {
            serde_json::json!({
                "id": i,
                "html_url": format!("https://github.com/test/dedup-fav{}", i),
                "name": format!("dedup-fav{}", i),
                "stargazers_count": 10,
                "private": false
            })
        })
        .collect();
    for i in 501..=510 {
        p1_items.push(serde_json::json!({
            "id": i,
            "html_url": format!("https://github.com/test/repo{}", i),
            "name": format!("repo{}", i),
            "stargazers_count": 20,
            "private": false
        }));
    }

    let page1_body = serde_json::json!({
        "total_count": 200,
        "items": p1_items
    });

    // Page 2: 5 duplicates from page 1 (501..=505), and 45 new items (511..=555) = 50 items
    let mut p2_items: Vec<serde_json::Value> = (501..=505)
        .map(|i| {
            serde_json::json!({
                "id": i,
                "html_url": format!("https://github.com/test/repo{}", i),
                "name": format!("repo{}", i),
                "stargazers_count": 20,
                "private": false
            })
        })
        .collect();
    for i in 511..=555 {
        p2_items.push(serde_json::json!({
            "id": i,
            "html_url": format!("https://github.com/test/repo{}", i),
            "name": format!("repo{}", i),
            "stargazers_count": 30,
            "private": false
        }));
    }

    let page2_body = serde_json::json!({
        "total_count": 200,
        "items": p2_items
    });

    Mock::given(method("GET"))
        .and(path("/search/repositories"))
        .and(query_param("page", "1"))
        .and(query_param("per_page", "50"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&page1_body))
        .expect(1)
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/search/repositories"))
        .and(query_param("page", "2"))
        .and(query_param("per_page", "50"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&page2_body))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = GitHubClient::with_base_url(&mock_server.uri());
    let req = RepositorySearchRequest {
        query: "test".to_string(),
        min_stars: None,
        pushed_after: None,
        limit: 50, // budget = 150; page 1 per_page=50, page 2 per_page=50
        exclude_selected: false,
        exclude_favorites: true,
    };

    let resp = execute_search(&pool, &client, None, &req).await.unwrap();

    // 501..=505 must not be duplicated; items collected: 501..=510 (10) and 511..=550 (40) = 50
    assert_eq!(resp.items.len(), 50);
    assert_eq!(resp.items[0].github_repo_id, 501);
    assert_eq!(resp.items[9].github_repo_id, 510);
    assert_eq!(resp.items[10].github_repo_id, 511);
    assert_eq!(resp.items[49].github_repo_id, 550);
    assert_eq!(count_selection_history(&pool).await.unwrap(), 50);
}

#[tokio::test]
async fn test_empty_post_exclusion_success() {
    let (_dir, pool) = setup_test_db().await;
    let mock_server = MockServer::start().await;

    favorite_repository(
        &pool,
        FavoriteInput {
            github_repo_id: Some(601),
            github_url: "https://github.com/test/fav-only".to_string(),
            name: "fav-only".to_string(),
            description: None,
            category: "Other".to_string(),
            stars: 10,
            license: None,
            topics: vec![],
            pushed_at: None,
            releases: None,
            languages: None,
        },
    )
    .await
    .unwrap();

    let page1_body = serde_json::json!({
        "total_count": 1,
        "items": [
            { "id": 601, "html_url": "https://github.com/test/fav-only", "name": "fav-only", "stargazers_count": 10, "private": false }
        ]
    });

    Mock::given(method("GET"))
        .and(path("/search/repositories"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&page1_body))
        .mount(&mock_server)
        .await;

    let client = GitHubClient::with_base_url(&mock_server.uri());
    let req = RepositorySearchRequest {
        query: "test".to_string(),
        min_stars: None,
        pushed_after: None,
        limit: 20,
        exclude_selected: false,
        exclude_favorites: true,
    };

    let resp = execute_search(&pool, &client, None, &req).await.unwrap();

    assert_eq!(resp.items.len(), 0);
    assert_eq!(resp.total_count, 1);
    assert_eq!(count_selection_history(&pool).await.unwrap(), 0);
}

// =============================================================================
// 4. Selection History & Transaction Atomicity Tests
// =============================================================================

#[tokio::test]
async fn test_only_final_displayed_items_written_and_canonical_url() {
    let (_dir, pool) = setup_test_db().await;
    let mock_server = MockServer::start().await;

    // Item 701 is favorited (will be excluded)
    favorite_repository(
        &pool,
        FavoriteInput {
            github_repo_id: Some(701),
            github_url: "https://github.com/test/fav701".to_string(),
            name: "fav701".to_string(),
            description: None,
            category: "Other".to_string(),
            stars: 10,
            license: None,
            topics: vec![],
            pushed_at: None,
            releases: None,
            languages: None,
        },
    )
    .await
    .unwrap();

    // Search returns 701, 702 (non-canonical url), and 703 (surplus beyond limit of 1)
    let search_body = serde_json::json!({
        "total_count": 3,
        "items": [
            { "id": 701, "html_url": "https://github.com/test/fav701", "name": "fav701", "stargazers_count": 10, "private": false },
            { "id": 702, "html_url": "https://github.com/test/repo702.git?tab=readme#readme", "name": "repo702", "stargazers_count": 20, "private": false },
            { "id": 703, "html_url": "https://github.com/test/repo703", "name": "repo703", "stargazers_count": 30, "private": false }
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
        limit: 1, // Only 1 final item requested
        exclude_selected: false,
        exclude_favorites: true,
    };

    let resp = execute_search(&pool, &client, None, &req).await.unwrap();

    assert_eq!(resp.items.len(), 1);
    assert_eq!(resp.items[0].github_repo_id, 702);

    // Verify selection_history has exactly 1 row, and it recorded the canonical URL
    assert_eq!(count_selection_history(&pool).await.unwrap(), 1);
    let row: (i64, String) =
        sqlx::query_as("SELECT github_repo_id, github_url FROM selection_history")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(row.0, 702);
    assert_eq!(row.1, "https://github.com/test/repo702");
}

#[tokio::test]
async fn test_one_history_transaction_and_shared_timestamp() {
    let (_dir, pool) = setup_test_db().await;
    let mock_server = MockServer::start().await;

    let search_body = serde_json::json!({
        "total_count": 2,
        "items": [
            { "id": 801, "html_url": "https://github.com/test/repo801", "name": "repo801", "stargazers_count": 10, "private": false },
            { "id": 802, "html_url": "https://github.com/test/repo802", "name": "repo802", "stargazers_count": 20, "private": false }
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
        limit: 10,
        exclude_selected: false,
        exclude_favorites: false,
    };

    let _resp = execute_search(&pool, &client, None, &req).await.unwrap();

    let timestamps: Vec<(String,)> = sqlx::query_as("SELECT selected_at FROM selection_history")
        .fetch_all(&pool)
        .await
        .unwrap();

    assert_eq!(timestamps.len(), 2);
    assert_eq!(
        timestamps[0].0, timestamps[1].0,
        "Batch items must share the exact same UTC timestamp"
    );
}

#[tokio::test]
async fn test_transaction_rollback_on_insert_failure() {
    let (_dir, pool) = setup_test_db().await;
    let mock_server = MockServer::start().await;

    // Install trigger to force an error on item 999
    sqlx::query(
        "CREATE TRIGGER abort_item_999 BEFORE INSERT ON selection_history WHEN NEW.github_repo_id = 999 BEGIN SELECT RAISE(ABORT, 'forced DB abort on 999'); END;"
    )
    .execute(&pool)
    .await
    .unwrap();

    let search_body = serde_json::json!({
        "total_count": 2,
        "items": [
            { "id": 901, "html_url": "https://github.com/test/repo901", "name": "repo901", "stargazers_count": 10, "private": false },
            { "id": 999, "html_url": "https://github.com/test/repo999", "name": "repo999", "stargazers_count": 20, "private": false }
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
        limit: 10,
        exclude_selected: false,
        exclude_favorites: false,
    };

    let err = execute_search(&pool, &client, None, &req)
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::Database(_)));

    // Entire transaction rolled back: repo 901 must NOT remain in DB
    assert_eq!(count_selection_history(&pool).await.unwrap(), 0);
}

#[tokio::test]
async fn test_repeated_history_events_allowed_and_later_excluded() {
    let (_dir, pool) = setup_test_db().await;
    let mock_server = MockServer::start().await;

    let search_body = serde_json::json!({
        "total_count": 1,
        "items": [
            { "id": 1001, "html_url": "https://github.com/test/repeat", "name": "repeat", "stargazers_count": 10, "private": false }
        ]
    });

    Mock::given(method("GET"))
        .and(path("/search/repositories"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&search_body))
        .mount(&mock_server)
        .await;

    let client = GitHubClient::with_base_url(&mock_server.uri());

    // Search 1: with exclude_selected: false
    let req_off = RepositorySearchRequest {
        query: "test".to_string(),
        min_stars: None,
        pushed_after: None,
        limit: 10,
        exclude_selected: false,
        exclude_favorites: false,
    };
    let r1 = execute_search(&pool, &client, None, &req_off)
        .await
        .unwrap();
    assert_eq!(r1.items.len(), 1);
    assert_eq!(count_selection_history(&pool).await.unwrap(), 1);

    // Search 2: with exclude_selected: false -> duplicate event allowed
    let r2 = execute_search(&pool, &client, None, &req_off)
        .await
        .unwrap();
    assert_eq!(r2.items.len(), 1);
    assert_eq!(count_selection_history(&pool).await.unwrap(), 2);

    // Search 3: with exclude_selected: true -> repo 1001 is now excluded
    let req_on = RepositorySearchRequest {
        query: "test".to_string(),
        min_stars: None,
        pushed_after: None,
        limit: 10,
        exclude_selected: true,
        exclude_favorites: false,
    };
    let r3 = execute_search(&pool, &client, None, &req_on).await.unwrap();
    assert_eq!(r3.items.len(), 0);
    // History count remains 2 (nothing newly inserted)
    assert_eq!(count_selection_history(&pool).await.unwrap(), 2);
}

// =============================================================================
// 5. Error Handling & Degradation Semantics Tests
// =============================================================================

#[tokio::test]
async fn test_first_page_failure_propagates_error_and_writes_no_history() {
    let (_dir, pool) = setup_test_db().await;
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/search/repositories"))
        .respond_with(
            ResponseTemplate::new(429)
                .insert_header("x-ratelimit-remaining", "0")
                .insert_header("x-ratelimit-reset", "1773000000"),
        )
        .mount(&mock_server)
        .await;

    let client = GitHubClient::with_base_url(&mock_server.uri());
    let req = RepositorySearchRequest {
        query: "test".to_string(),
        min_stars: None,
        pushed_after: None,
        limit: 10,
        exclude_selected: true,
        exclude_favorites: false,
    };

    let err = execute_search(&pool, &client, None, &req)
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::GitHubRateLimit { .. }));
    assert_eq!(count_selection_history(&pool).await.unwrap(), 0);
}

#[tokio::test]
async fn test_later_page_rate_limit_preserves_non_empty_partial_results() {
    let (_dir, pool) = setup_test_db().await;
    let mock_server = MockServer::start().await;

    // Seed 40 favorites so page 1 has 40 excluded items + 10 eligible (1101..=1110) = 50 items
    for i in 1..=40 {
        favorite_repository(
            &pool,
            FavoriteInput {
                github_repo_id: Some(i),
                github_url: format!("https://github.com/test/deg-fav{}", i),
                name: format!("deg-fav{}", i),
                description: None,
                category: "Other".to_string(),
                stars: 10,
                license: None,
                topics: vec![],
                pushed_at: None,
                releases: None,
                languages: None,
            },
        )
        .await
        .unwrap();
    }

    let mut p1_items: Vec<serde_json::Value> = (1..=40)
        .map(|i| {
            serde_json::json!({
                "id": i,
                "html_url": format!("https://github.com/test/deg-fav{}", i),
                "name": format!("deg-fav{}", i),
                "stargazers_count": 10,
                "private": false
            })
        })
        .collect();
    for i in 1101..=1110 {
        p1_items.push(serde_json::json!({
            "id": i,
            "html_url": format!("https://github.com/test/repo{}", i),
            "name": format!("repo{}", i),
            "stargazers_count": 10,
            "private": false
        }));
    }

    let page1_body = serde_json::json!({
        "total_count": 200,
        "items": p1_items
    });

    Mock::given(method("GET"))
        .and(path("/search/repositories"))
        .and(query_param("page", "1"))
        .and(query_param("per_page", "50"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&page1_body))
        .expect(1)
        .mount(&mock_server)
        .await;

    // Page 2: hits 429 RateLimit
    Mock::given(method("GET"))
        .and(path("/search/repositories"))
        .and(query_param("page", "2"))
        .and(query_param("per_page", "50"))
        .respond_with(
            ResponseTemplate::new(429)
                .insert_header("x-ratelimit-remaining", "0")
                .insert_header("x-ratelimit-reset", "1773000000"),
        )
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = GitHubClient::with_base_url(&mock_server.uri());
    let req = RepositorySearchRequest {
        query: "test".to_string(),
        min_stars: None,
        pushed_after: None,
        limit: 50, // candidate_budget = 150, page_size = 50
        exclude_selected: false,
        exclude_favorites: true,
    };

    // Degraded success returns the 10 collected items
    let resp = execute_search(&pool, &client, None, &req).await.unwrap();
    assert_eq!(resp.items.len(), 10);
    assert_eq!(resp.items[0].github_repo_id, 1101);
    assert_eq!(resp.items[9].github_repo_id, 1110);

    // Atomically recorded into selection_history
    assert_eq!(count_selection_history(&pool).await.unwrap(), 10);
}

#[tokio::test]
async fn test_later_page_failure_with_zero_eligible_items_propagates_error() {
    let (_dir, pool) = setup_test_db().await;
    let mock_server = MockServer::start().await;

    // Seed 50 favorites so all 50 items on page 1 are excluded
    for i in 1..=50 {
        favorite_repository(
            &pool,
            FavoriteInput {
                github_repo_id: Some(i),
                github_url: format!("https://github.com/test/zero-fav{}", i),
                name: format!("zero-fav{}", i),
                description: None,
                category: "Other".to_string(),
                stars: 10,
                license: None,
                topics: vec![],
                pushed_at: None,
                releases: None,
                languages: None,
            },
        )
        .await
        .unwrap();
    }

    let p1_items: Vec<serde_json::Value> = (1..=50)
        .map(|i| {
            serde_json::json!({
                "id": i,
                "html_url": format!("https://github.com/test/zero-fav{}", i),
                "name": format!("zero-fav{}", i),
                "stargazers_count": 10,
                "private": false
            })
        })
        .collect();

    let page1_body = serde_json::json!({
        "total_count": 200,
        "items": p1_items
    });

    Mock::given(method("GET"))
        .and(path("/search/repositories"))
        .and(query_param("page", "1"))
        .and(query_param("per_page", "50"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&page1_body))
        .expect(1)
        .mount(&mock_server)
        .await;

    // Page 2: hits 429
    Mock::given(method("GET"))
        .and(path("/search/repositories"))
        .and(query_param("page", "2"))
        .and(query_param("per_page", "50"))
        .respond_with(
            ResponseTemplate::new(429)
                .insert_header("x-ratelimit-remaining", "0")
                .insert_header("x-ratelimit-reset", "1773000000"),
        )
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = GitHubClient::with_base_url(&mock_server.uri());
    let req = RepositorySearchRequest {
        query: "test".to_string(),
        min_stars: None,
        pushed_after: None,
        limit: 50, // candidate_budget = 150, page_size = 50
        exclude_selected: false,
        exclude_favorites: true,
    };

    let err = execute_search(&pool, &client, None, &req)
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::GitHubRateLimit { .. }));
    assert_eq!(count_selection_history(&pool).await.unwrap(), 0);
}

// =============================================================================
// 6. Remediation-01 Tests: Total Count Exhaustion, Fixed Page Size, Raw Page Len,
//    Transient Errors (Timeout & Network), and DB Exclusion Failures
// =============================================================================

#[tokio::test]
async fn test_replenishment_stops_when_total_count_exhausted() {
    let (_dir, pool) = setup_test_db().await;
    let mock_server = MockServer::start().await;

    // Seed 40 favorites so page 1 has 40 excluded items + 10 eligible (1401..=1410) = 50 items
    for i in 1..=40 {
        favorite_repository(
            &pool,
            FavoriteInput {
                github_repo_id: Some(i),
                github_url: format!("https://github.com/test/tc-fav{}", i),
                name: format!("tc-fav{}", i),
                description: None,
                category: "Other".to_string(),
                stars: 10,
                license: None,
                topics: vec![],
                pushed_at: None,
                releases: None,
                languages: None,
            },
        )
        .await
        .unwrap();
    }

    let mut p1_items: Vec<serde_json::Value> = (1..=40)
        .map(|i| {
            serde_json::json!({
                "id": i,
                "html_url": format!("https://github.com/test/tc-fav{}", i),
                "name": format!("tc-fav{}", i),
                "stargazers_count": 10,
                "private": false
            })
        })
        .collect();
    for i in 1401..=1410 {
        p1_items.push(serde_json::json!({
            "id": i,
            "html_url": format!("https://github.com/test/repo{}", i),
            "name": format!("repo{}", i),
            "stargazers_count": 10,
            "private": false
        }));
    }

    // total_count is 50, exactly equal to requested_through (1 * 50 = 50)
    let page1_body = serde_json::json!({
        "total_count": 50,
        "items": p1_items
    });

    Mock::given(method("GET"))
        .and(path("/search/repositories"))
        .and(query_param("page", "1"))
        .and(query_param("per_page", "50"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&page1_body))
        .expect(1)
        .mount(&mock_server)
        .await;

    // Page 2 must NOT be requested because requested_through (50) >= total_count (50)
    Mock::given(method("GET"))
        .and(path("/search/repositories"))
        .and(query_param("page", "2"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock_server)
        .await;

    let client = GitHubClient::with_base_url(&mock_server.uri());
    let req = RepositorySearchRequest {
        query: "test".to_string(),
        min_stars: None,
        pushed_after: None,
        limit: 50,
        exclude_selected: false,
        exclude_favorites: true,
    };

    let resp = execute_search(&pool, &client, None, &req).await.unwrap();
    // Only the 10 eligible items from page 1 are returned
    assert_eq!(resp.items.len(), 10);
    assert_eq!(resp.items[0].github_repo_id, 1401);
    assert_eq!(resp.items[9].github_repo_id, 1410);
    assert_eq!(resp.total_count, 50);
    assert_eq!(count_selection_history(&pool).await.unwrap(), 10);
}

#[tokio::test]
async fn test_fixed_page_size_across_pagination() {
    let (_dir, pool) = setup_test_db().await;
    let mock_server = MockServer::start().await;

    // Limit = 50 -> candidate_budget = 150 -> page_size = 50, max_pages = 3.
    // Each page returns 50 items with 45 favorites and 5 eligible survivors.
    // Total count = 500.
    for i in 1..=135 {
        favorite_repository(
            &pool,
            FavoriteInput {
                github_repo_id: Some(i),
                github_url: format!("https://github.com/test/fps-fav{}", i),
                name: format!("fps-fav{}", i),
                description: None,
                category: "Other".to_string(),
                stars: 10,
                license: None,
                topics: vec![],
                pushed_at: None,
                releases: None,
                languages: None,
            },
        )
        .await
        .unwrap();
    }

    for page in 1..=3u32 {
        let fav_offset = (page - 1) * 45;
        let mut items: Vec<serde_json::Value> = (fav_offset + 1..=fav_offset + 45)
            .map(|id| {
                serde_json::json!({
                    "id": id,
                    "html_url": format!("https://github.com/test/fps-fav{}", id),
                    "name": format!("fps-fav{}", id),
                    "stargazers_count": 10,
                    "private": false
                })
            })
            .collect();
        for s in 1..=5 {
            let surv_id = 1500 + (page - 1) * 5 + s;
            items.push(serde_json::json!({
                "id": surv_id,
                "html_url": format!("https://github.com/test/fps-surv{}", surv_id),
                "name": format!("fps-surv{}", surv_id),
                "stargazers_count": 20,
                "private": false
            }));
        }

        Mock::given(method("GET"))
            .and(path("/search/repositories"))
            .and(query_param("page", page.to_string()))
            .and(query_param("per_page", "50")) // Exact fixed page size per_page=50 on all pages
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "total_count": 500,
                "items": items
            })))
            .expect(1)
            .mount(&mock_server)
            .await;
    }

    let client = GitHubClient::with_base_url(&mock_server.uri());
    let req = RepositorySearchRequest {
        query: "test".to_string(),
        min_stars: None,
        pushed_after: None,
        limit: 50,
        exclude_selected: false,
        exclude_favorites: true,
    };

    let resp = execute_search(&pool, &client, None, &req).await.unwrap();
    // 3 pages * 5 survivors = 15 items
    assert_eq!(resp.items.len(), 15);
    assert_eq!(resp.total_count, 500);
    assert_eq!(count_selection_history(&pool).await.unwrap(), 15);
}

#[tokio::test]
async fn test_raw_page_length_not_broken_by_private_repo_filtering() {
    let (_dir, pool) = setup_test_db().await;
    let mock_server = MockServer::start().await;

    // Seed 40 favorites so page 1 has 40 excluded items + 9 public survivors + 1 private = 50 raw items
    for i in 1..=40 {
        favorite_repository(
            &pool,
            FavoriteInput {
                github_repo_id: Some(i),
                github_url: format!("https://github.com/test/priv-fav{}", i),
                name: format!("priv-fav{}", i),
                description: None,
                category: "Other".to_string(),
                stars: 10,
                license: None,
                topics: vec![],
                pushed_at: None,
                releases: None,
                languages: None,
            },
        )
        .await
        .unwrap();
    }

    let mut p1_items: Vec<serde_json::Value> = (1..=40)
        .map(|i| {
            serde_json::json!({
                "id": i,
                "html_url": format!("https://github.com/test/priv-fav{}", i),
                "name": format!("priv-fav{}", i),
                "stargazers_count": 10,
                "private": false
            })
        })
        .collect();
    for i in 1601..=1609 {
        p1_items.push(serde_json::json!({
            "id": i,
            "html_url": format!("https://github.com/test/repo{}", i),
            "name": format!("repo{}", i),
            "stargazers_count": 10,
            "private": false
        }));
    }
    // Add 1 private repo to page 1
    p1_items.push(serde_json::json!({
        "id": 9999,
        "html_url": "https://github.com/test/private-repo",
        "name": "private-repo",
        "stargazers_count": 10,
        "private": true
    }));

    let page1_body = serde_json::json!({
        "total_count": 200,
        "items": p1_items // Exactly 50 raw items, but only 49 public items after client-side private filter
    });

    // Page 2: returns 50 public survivors (1610..=1659)
    let p2_items: Vec<serde_json::Value> = (1610..=1659)
        .map(|i| {
            serde_json::json!({
                "id": i,
                "html_url": format!("https://github.com/test/repo{}", i),
                "name": format!("repo{}", i),
                "stargazers_count": 20,
                "private": false
            })
        })
        .collect();

    let page2_body = serde_json::json!({
        "total_count": 200,
        "items": p2_items
    });

    Mock::given(method("GET"))
        .and(path("/search/repositories"))
        .and(query_param("page", "1"))
        .and(query_param("per_page", "50"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&page1_body))
        .expect(1)
        .mount(&mock_server)
        .await;

    // Because raw_item_count is 50 (== page_size), pagination must NOT break early and MUST fetch page 2
    Mock::given(method("GET"))
        .and(path("/search/repositories"))
        .and(query_param("page", "2"))
        .and(query_param("per_page", "50"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&page2_body))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = GitHubClient::with_base_url(&mock_server.uri());
    let req = RepositorySearchRequest {
        query: "test".to_string(),
        min_stars: None,
        pushed_after: None,
        limit: 50,
        exclude_selected: false,
        exclude_favorites: true,
    };

    let resp = execute_search(&pool, &client, None, &req).await.unwrap();

    // Exactly 50 public items returned (9 from page 1 + 41 from page 2)
    assert_eq!(resp.items.len(), 50);
    assert_eq!(resp.items[0].github_repo_id, 1601);
    assert_eq!(resp.items[8].github_repo_id, 1609);
    assert_eq!(resp.items[9].github_repo_id, 1610);
    assert_eq!(resp.items[49].github_repo_id, 1650);
    // Ensure private repo is nowhere in final results
    assert!(!resp.items.iter().any(|r| r.github_repo_id == 9999));
    assert_eq!(count_selection_history(&pool).await.unwrap(), 50);
}

#[tokio::test]
async fn test_first_page_failure_timeout_propagates_error() {
    let (_dir, pool) = setup_test_db().await;
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/search/repositories"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(Duration::from_millis(200))
                .set_body_json(serde_json::json!({
                    "total_count": 1,
                    "items": []
                })),
        )
        .mount(&mock_server)
        .await;

    let client =
        GitHubClient::with_base_url_and_timeout(&mock_server.uri(), Duration::from_millis(30));
    let req = RepositorySearchRequest {
        query: "test".to_string(),
        min_stars: None,
        pushed_after: None,
        limit: 10,
        exclude_selected: true,
        exclude_favorites: false,
    };

    let err = execute_search(&pool, &client, None, &req)
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::GitHubTimeout(_)));
    assert_eq!(count_selection_history(&pool).await.unwrap(), 0);
}

#[tokio::test]
async fn test_first_page_failure_network_propagates_error() {
    let (_dir, pool) = setup_test_db().await;
    let client = GitHubClient::with_base_url("http://127.0.0.1:59999");
    let req = RepositorySearchRequest {
        query: "test".to_string(),
        min_stars: None,
        pushed_after: None,
        limit: 10,
        exclude_selected: true,
        exclude_favorites: false,
    };

    let err = execute_search(&pool, &client, None, &req)
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::GitHubNetwork(_)));
    assert_eq!(count_selection_history(&pool).await.unwrap(), 0);
}

#[tokio::test]
async fn test_later_page_timeout_preserves_non_empty_partial_results() {
    let (_dir, pool) = setup_test_db().await;
    let mock_server = MockServer::start().await;

    // Seed 40 favorites so page 1 has 40 excluded items + 10 eligible (1201..=1210) = 50 items
    for i in 1..=40 {
        favorite_repository(
            &pool,
            FavoriteInput {
                github_repo_id: Some(i),
                github_url: format!("https://github.com/test/deg-to-fav{}", i),
                name: format!("deg-to-fav{}", i),
                description: None,
                category: "Other".to_string(),
                stars: 10,
                license: None,
                topics: vec![],
                pushed_at: None,
                releases: None,
                languages: None,
            },
        )
        .await
        .unwrap();
    }

    let mut p1_items: Vec<serde_json::Value> = (1..=40)
        .map(|i| {
            serde_json::json!({
                "id": i,
                "html_url": format!("https://github.com/test/deg-to-fav{}", i),
                "name": format!("deg-to-fav{}", i),
                "stargazers_count": 10,
                "private": false
            })
        })
        .collect();
    for i in 1201..=1210 {
        p1_items.push(serde_json::json!({
            "id": i,
            "html_url": format!("https://github.com/test/repo{}", i),
            "name": format!("repo{}", i),
            "stargazers_count": 10,
            "private": false
        }));
    }

    let page1_body = serde_json::json!({
        "total_count": 200,
        "items": p1_items
    });

    Mock::given(method("GET"))
        .and(path("/search/repositories"))
        .and(query_param("page", "1"))
        .and(query_param("per_page", "50"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&page1_body))
        .expect(1)
        .mount(&mock_server)
        .await;

    // Page 2 fails with timeout via hook
    let client = GitHubClient::with_search_page_hook(&mock_server.uri(), |page| {
        if page == 2 {
            Some(AppError::GitHubTimeout("Request timed out".into()))
        } else {
            None
        }
    });

    let req = RepositorySearchRequest {
        query: "test".to_string(),
        min_stars: None,
        pushed_after: None,
        limit: 50,
        exclude_selected: false,
        exclude_favorites: true,
    };

    let resp = execute_search(&pool, &client, None, &req).await.unwrap();
    assert_eq!(resp.items.len(), 10);
    assert_eq!(resp.items[0].github_repo_id, 1201);
    assert_eq!(resp.items[9].github_repo_id, 1210);
    assert_eq!(count_selection_history(&pool).await.unwrap(), 10);
}

#[tokio::test]
async fn test_later_page_network_preserves_non_empty_partial_results() {
    let (_dir, pool) = setup_test_db().await;
    let mock_server = MockServer::start().await;

    for i in 1..=40 {
        favorite_repository(
            &pool,
            FavoriteInput {
                github_repo_id: Some(i),
                github_url: format!("https://github.com/test/deg-net-fav{}", i),
                name: format!("deg-net-fav{}", i),
                description: None,
                category: "Other".to_string(),
                stars: 10,
                license: None,
                topics: vec![],
                pushed_at: None,
                releases: None,
                languages: None,
            },
        )
        .await
        .unwrap();
    }

    let mut p1_items: Vec<serde_json::Value> = (1..=40)
        .map(|i| {
            serde_json::json!({
                "id": i,
                "html_url": format!("https://github.com/test/deg-net-fav{}", i),
                "name": format!("deg-net-fav{}", i),
                "stargazers_count": 10,
                "private": false
            })
        })
        .collect();
    for i in 1301..=1310 {
        p1_items.push(serde_json::json!({
            "id": i,
            "html_url": format!("https://github.com/test/repo{}", i),
            "name": format!("repo{}", i),
            "stargazers_count": 10,
            "private": false
        }));
    }

    let page1_body = serde_json::json!({
        "total_count": 200,
        "items": p1_items
    });

    Mock::given(method("GET"))
        .and(path("/search/repositories"))
        .and(query_param("page", "1"))
        .and(query_param("per_page", "50"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&page1_body))
        .expect(1)
        .mount(&mock_server)
        .await;

    // Page 2 fails with network error via hook
    let client = GitHubClient::with_search_page_hook(&mock_server.uri(), |page| {
        if page == 2 {
            Some(AppError::GitHubNetwork("Connection reset by peer".into()))
        } else {
            None
        }
    });

    let req = RepositorySearchRequest {
        query: "test".to_string(),
        min_stars: None,
        pushed_after: None,
        limit: 50,
        exclude_selected: false,
        exclude_favorites: true,
    };

    let resp = execute_search(&pool, &client, None, &req).await.unwrap();
    assert_eq!(resp.items.len(), 10);
    assert_eq!(resp.items[0].github_repo_id, 1301);
    assert_eq!(resp.items[9].github_repo_id, 1310);
    assert_eq!(count_selection_history(&pool).await.unwrap(), 10);
}

#[tokio::test]
async fn test_later_page_failure_timeout_with_zero_eligible_items_propagates_error() {
    let (_dir, pool) = setup_test_db().await;
    let mock_server = MockServer::start().await;

    // Seed 50 favorites so all 50 items on page 1 are excluded
    for i in 1..=50 {
        favorite_repository(
            &pool,
            FavoriteInput {
                github_repo_id: Some(i),
                github_url: format!("https://github.com/test/zero-to-fav{}", i),
                name: format!("zero-to-fav{}", i),
                description: None,
                category: "Other".to_string(),
                stars: 10,
                license: None,
                topics: vec![],
                pushed_at: None,
                releases: None,
                languages: None,
            },
        )
        .await
        .unwrap();
    }

    let p1_items: Vec<serde_json::Value> = (1..=50)
        .map(|i| {
            serde_json::json!({
                "id": i,
                "html_url": format!("https://github.com/test/zero-to-fav{}", i),
                "name": format!("zero-to-fav{}", i),
                "stargazers_count": 10,
                "private": false
            })
        })
        .collect();

    let page1_body = serde_json::json!({
        "total_count": 200,
        "items": p1_items
    });

    Mock::given(method("GET"))
        .and(path("/search/repositories"))
        .and(query_param("page", "1"))
        .and(query_param("per_page", "50"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&page1_body))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = GitHubClient::with_search_page_hook(&mock_server.uri(), |page| {
        if page == 2 {
            Some(AppError::GitHubTimeout("Request timed out".into()))
        } else {
            None
        }
    });

    let req = RepositorySearchRequest {
        query: "test".to_string(),
        min_stars: None,
        pushed_after: None,
        limit: 50,
        exclude_selected: false,
        exclude_favorites: true,
    };

    let err = execute_search(&pool, &client, None, &req)
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::GitHubTimeout(_)));
    assert_eq!(count_selection_history(&pool).await.unwrap(), 0);
}

#[tokio::test]
async fn test_later_page_failure_network_with_zero_eligible_items_propagates_error() {
    let (_dir, pool) = setup_test_db().await;
    let mock_server = MockServer::start().await;

    // Seed 50 favorites so all 50 items on page 1 are excluded
    for i in 1..=50 {
        favorite_repository(
            &pool,
            FavoriteInput {
                github_repo_id: Some(i),
                github_url: format!("https://github.com/test/zero-net-fav{}", i),
                name: format!("zero-net-fav{}", i),
                description: None,
                category: "Other".to_string(),
                stars: 10,
                license: None,
                topics: vec![],
                pushed_at: None,
                releases: None,
                languages: None,
            },
        )
        .await
        .unwrap();
    }

    let p1_items: Vec<serde_json::Value> = (1..=50)
        .map(|i| {
            serde_json::json!({
                "id": i,
                "html_url": format!("https://github.com/test/zero-net-fav{}", i),
                "name": format!("zero-net-fav{}", i),
                "stargazers_count": 10,
                "private": false
            })
        })
        .collect();

    let page1_body = serde_json::json!({
        "total_count": 200,
        "items": p1_items
    });

    Mock::given(method("GET"))
        .and(path("/search/repositories"))
        .and(query_param("page", "1"))
        .and(query_param("per_page", "50"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&page1_body))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = GitHubClient::with_search_page_hook(&mock_server.uri(), |page| {
        if page == 2 {
            Some(AppError::GitHubNetwork("Network connection lost".into()))
        } else {
            None
        }
    });

    let req = RepositorySearchRequest {
        query: "test".to_string(),
        min_stars: None,
        pushed_after: None,
        limit: 50,
        exclude_selected: false,
        exclude_favorites: true,
    };

    let err = execute_search(&pool, &client, None, &req)
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::GitHubNetwork(_)));
    assert_eq!(count_selection_history(&pool).await.unwrap(), 0);
}

#[tokio::test]
async fn test_exclude_selected_db_read_failure_aborts_search() {
    let (_dir, pool) = setup_test_db().await;
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/search/repositories"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "total_count": 1,
            "items": [
                { "id": 101, "html_url": "https://github.com/test/item", "name": "item", "stargazers_count": 10, "private": false }
            ]
        })))
        .mount(&mock_server)
        .await;

    // Drop selection_history table to simulate a database failure during exclusion read
    sqlx::query("DROP TABLE selection_history")
        .execute(&pool)
        .await
        .unwrap();

    let client = GitHubClient::with_base_url(&mock_server.uri());
    let req = RepositorySearchRequest {
        query: "test".to_string(),
        min_stars: None,
        pushed_after: None,
        limit: 10,
        exclude_selected: true,
        exclude_favorites: false,
    };

    let err = execute_search(&pool, &client, None, &req)
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::Database(_)));
}

#[tokio::test]
async fn test_exclude_favorites_db_read_failure_aborts_search() {
    let (_dir, pool) = setup_test_db().await;
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/search/repositories"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "total_count": 1,
            "items": [
                { "id": 101, "html_url": "https://github.com/test/item", "name": "item", "stargazers_count": 10, "private": false }
            ]
        })))
        .mount(&mock_server)
        .await;

    // Drop favorites table to simulate a database failure during exclusion read
    sqlx::query("DROP TABLE favorites")
        .execute(&pool)
        .await
        .unwrap();

    let client = GitHubClient::with_base_url(&mock_server.uri());
    let req = RepositorySearchRequest {
        query: "test".to_string(),
        min_stars: None,
        pushed_after: None,
        limit: 10,
        exclude_selected: false,
        exclude_favorites: true,
    };

    let err = execute_search(&pool, &client, None, &req)
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::Database(_)));
}
