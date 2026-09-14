use std::collections::HashMap;
use tempfile::tempdir;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use reposhelf_lib::db::database::init_pool;
use reposhelf_lib::github::enrichment::enrich_repositories;
use reposhelf_lib::github::models::GitHubRepository;
use reposhelf_lib::github::GitHubClient;
use reposhelf_lib::library::favorite_repository;
use reposhelf_lib::library::models::{
    FavoriteInput, MetadataPatch, UpdateFavoriteUserMetadataInput,
};
use reposhelf_lib::library::repository::{
    find_favorite_by_id, update_github_owned_metadata_tx, update_user_metadata_tx,
};

async fn setup_test_db() -> (tempfile::TempDir, sqlx::SqlitePool) {
    let dir = tempdir().expect("Failed to create tempdir");
    let db_path = dir.path().join("test_phase_s2.db");
    let pool = init_pool(&db_path)
        .await
        .expect("Failed to initialize test pool");
    (dir, pool)
}

// -----------------------------------------------------------------------------
// 1. Migration 004 & Schema Verification
// -----------------------------------------------------------------------------

#[tokio::test]
async fn test_migration_004_schema_and_defaults() {
    let (_dir, pool) = setup_test_db().await;

    // Verify migration 004 is applied
    let migrations: Vec<(i64,)> =
        sqlx::query_as("SELECT version FROM _sqlx_migrations ORDER BY version ASC")
            .fetch_all(&pool)
            .await
            .unwrap();

    assert_eq!(migrations.len(), 4);
    assert_eq!(migrations[3].0, 4);

    // Verify columns exist on favorites table by querying pragma
    let columns: Vec<(i64, String, String, i64, Option<String>, i64)> =
        sqlx::query_as("PRAGMA table_info(favorites)")
            .fetch_all(&pool)
            .await
            .unwrap();

    let col_names: Vec<String> = columns.into_iter().map(|c| c.1).collect();
    assert!(col_names.contains(&"releases".to_string()));
    assert!(col_names.contains(&"languages".to_string()));
}

// -----------------------------------------------------------------------------
// 2. Favorite CRUD Persistence for Releases and Languages
// -----------------------------------------------------------------------------

#[tokio::test]
async fn test_favorite_insert_and_read_releases_and_languages() {
    let (_dir, pool) = setup_test_db().await;

    let mut langs = HashMap::new();
    langs.insert("Rust".to_string(), 75.5);
    langs.insert("TypeScript".to_string(), 24.5);

    let input = FavoriteInput {
        github_repo_id: Some(2001),
        github_url: "https://github.com/test-org/repo-s2".to_string(),
        name: "repo-s2".to_string(),
        description: Some("Testing S2 persistence".to_string()),
        category: "Developer Tools".to_string(),
        stars: 120,
        license: Some("MIT".to_string()),
        topics: vec!["rust".to_string()],
        pushed_at: Some("2026-03-01T00:00:00Z".to_string()),
        releases: Some(42),
        languages: Some(langs.clone()),
    };

    let fav = favorite_repository(&pool, input)
        .await
        .expect("Insert favorite should succeed");

    assert_eq!(fav.releases, Some(42));
    assert_eq!(fav.languages, Some(langs));

    // Lookup by ID
    let fetched = find_favorite_by_id(&pool, fav.id)
        .await
        .expect("Find favorite should succeed")
        .expect("Favorite should be present");

    assert_eq!(fetched.releases, Some(42));
    let fetched_langs = fetched.languages.expect("Languages should be present");
    assert_eq!(fetched_langs.get("Rust"), Some(&75.5));
    assert_eq!(fetched_langs.get("TypeScript"), Some(&24.5));
}

#[tokio::test]
async fn test_favorite_empty_languages_map_persistence() {
    let (_dir, pool) = setup_test_db().await;

    let empty_langs = HashMap::new();
    let input = FavoriteInput {
        github_repo_id: Some(2002),
        github_url: "https://github.com/test-org/empty-langs".to_string(),
        name: "empty-langs".to_string(),
        description: None,
        category: "Other".to_string(),
        stars: 0,
        license: None,
        topics: vec![],
        pushed_at: None,
        releases: Some(0),
        languages: Some(empty_langs),
    };

    let fav = favorite_repository(&pool, input)
        .await
        .expect("Insert favorite should succeed");

    assert_eq!(fav.releases, Some(0));
    assert_eq!(fav.languages, Some(HashMap::new()));

    // Verify directly in database that languages column is stored as "{}"
    let row: (Option<String>,) = sqlx::query_as("SELECT languages FROM favorites WHERE id = ?")
        .bind(fav.id)
        .fetch_one(&pool)
        .await
        .unwrap();

    assert_eq!(row.0.as_deref(), Some("{}"));
}

#[tokio::test]
async fn test_favorite_null_releases_and_languages_persistence() {
    let (_dir, pool) = setup_test_db().await;

    let input = FavoriteInput {
        github_repo_id: Some(2003),
        github_url: "https://github.com/test-org/null-s2".to_string(),
        name: "null-s2".to_string(),
        description: None,
        category: "Other".to_string(),
        stars: 0,
        license: None,
        topics: vec![],
        pushed_at: None,
        releases: None,
        languages: None,
    };

    let fav = favorite_repository(&pool, input)
        .await
        .expect("Insert favorite should succeed");

    assert_eq!(fav.releases, None);
    assert_eq!(fav.languages, None);

    let row: (Option<i64>, Option<String>) =
        sqlx::query_as("SELECT releases, languages FROM favorites WHERE id = ?")
            .bind(fav.id)
            .fetch_one(&pool)
            .await
            .unwrap();

    assert_eq!(row.0, None);
    assert_eq!(row.1, None);
}

// -----------------------------------------------------------------------------
// 3. User Metadata Editing Preserves S2 Metadata
// -----------------------------------------------------------------------------

#[tokio::test]
async fn test_user_metadata_editing_preserves_releases_and_languages() {
    let (_dir, pool) = setup_test_db().await;

    let mut langs = HashMap::new();
    langs.insert("Go".to_string(), 100.0);

    let input = FavoriteInput {
        github_repo_id: Some(2004),
        github_url: "https://github.com/test-org/preserve-s2".to_string(),
        name: "preserve-s2".to_string(),
        description: None,
        category: "Developer Tools".to_string(),
        stars: 50,
        license: None,
        topics: vec![],
        pushed_at: None,
        releases: Some(10),
        languages: Some(langs.clone()),
    };

    let fav = favorite_repository(&pool, input)
        .await
        .expect("Insert favorite should succeed");

    // Update user rating and notes
    let update_input = UpdateFavoriteUserMetadataInput {
        favorite_id: fav.id,
        category: "DevOps & Cloud".to_string(),
        rating: Some(4),
        rating_reason: Some("Very solid Go library".to_string()),
        rating_confidence: Some("high".to_string()),
        notes: "Updated personal review".to_string(),
        user_tags: vec!["production-ready".to_string()],
        ai_tags: None,
    };

    let updated = update_user_metadata_tx(&pool, &update_input)
        .await
        .expect("User metadata update should succeed");

    // Assert that releases and languages are completely intact
    assert_eq!(updated.releases, Some(10));
    assert_eq!(updated.languages, Some(langs));
    assert_eq!(updated.rating, Some(4));
    assert_eq!(updated.notes, "Updated personal review");
}

// -----------------------------------------------------------------------------
// 4. Refresh Metadata Field-Level Keep / Replace Semantics
// -----------------------------------------------------------------------------

#[tokio::test]
async fn test_refresh_keep_and_replace_semantics() {
    let (_dir, pool) = setup_test_db().await;

    let mut initial_langs = HashMap::new();
    initial_langs.insert("Rust".to_string(), 100.0);

    let input = FavoriteInput {
        github_repo_id: Some(2005),
        github_url: "https://github.com/test-org/refresh-patch".to_string(),
        name: "refresh-patch".to_string(),
        description: None,
        category: "System Utilities".to_string(),
        stars: 100,
        license: None,
        topics: vec!["initial".to_string()],
        pushed_at: None,
        releases: Some(5),
        languages: Some(initial_langs.clone()),
    };

    let fav = favorite_repository(&pool, input).await.unwrap();

    let gh_repo = GitHubRepository {
        github_repo_id: 2005,
        name: "refresh-patch".to_string(),
        github_url: "https://github.com/test-org/refresh-patch".to_string(),
        description: Some("New desc".to_string()),
        stars: 120,
        license: Some("MIT".to_string()),
        pushed_at: Some("2026-03-05T00:00:00Z".to_string()),
        topics: vec!["refreshed".to_string()],
        category: "System Utilities".to_string(),
        releases: None,
        languages: None,
    };

    // Case A: Releases fails (Keep), Languages succeeds (Replace)
    let mut new_langs = HashMap::new();
    new_langs.insert("Rust".to_string(), 80.0);
    new_langs.insert("C".to_string(), 20.0);

    let refreshed_a = update_github_owned_metadata_tx(
        &pool,
        fav.id,
        &gh_repo,
        "2026-03-05T12:00:00Z",
        MetadataPatch::Keep,
        MetadataPatch::Replace(new_langs.clone()),
    )
    .await
    .unwrap();

    // Releases should stay 5 (kept), languages should be updated to new_langs
    assert_eq!(refreshed_a.releases, Some(5));
    assert_eq!(refreshed_a.languages, Some(new_langs));

    // Case B: Releases succeeds (Replace), Languages fails (Keep)
    let refreshed_b = update_github_owned_metadata_tx(
        &pool,
        fav.id,
        &gh_repo,
        "2026-03-05T13:00:00Z",
        MetadataPatch::Replace(12),
        MetadataPatch::Keep,
    )
    .await
    .unwrap();

    // Releases should be 12 (replaced), languages should stay new_langs (kept)
    assert_eq!(refreshed_b.releases, Some(12));
    assert_eq!(refreshed_b.languages, Some(refreshed_a.languages.unwrap()));
}

// -----------------------------------------------------------------------------
// 5. GitHub Releases Counting Logic (WireMock)
// -----------------------------------------------------------------------------

#[tokio::test]
async fn test_github_releases_count_draft_and_prerelease() {
    let mock_server = MockServer::start().await;

    // Response contains: 1 normal release, 1 prerelease, 1 draft
    let body = serde_json::json!([
        { "id": 1, "draft": false, "prerelease": false },
        { "id": 2, "draft": false, "prerelease": true },
        { "id": 3, "draft": true, "prerelease": false },
    ]);

    Mock::given(method("GET"))
        .and(path("/repos/test-org/releases-test/releases"))
        .and(query_param("per_page", "100"))
        .and(query_param("page", "1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(&mock_server)
        .await;

    let client = GitHubClient::with_base_url(&mock_server.uri());
    let count = client
        .get_repository_releases_count("test-org", "releases-test", None)
        .await
        .expect("Should succeed");

    // Expected count: 2 (draft is excluded, prerelease is counted)
    assert_eq!(count, 2);
}

#[tokio::test]
async fn test_github_releases_count_multi_page_pagination() {
    let mock_server = MockServer::start().await;

    // Page 1: 100 published releases
    let page1: Vec<serde_json::Value> = (1..=100)
        .map(|i| serde_json::json!({ "id": i, "draft": false, "prerelease": false }))
        .collect();

    // Page 2: 15 published releases
    let page2: Vec<serde_json::Value> = (101..=115)
        .map(|i| serde_json::json!({ "id": i, "draft": false, "prerelease": false }))
        .collect();

    Mock::given(method("GET"))
        .and(path("/repos/test-org/pagination-test/releases"))
        .and(query_param("per_page", "100"))
        .and(query_param("page", "1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(page1))
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/repos/test-org/pagination-test/releases"))
        .and(query_param("per_page", "100"))
        .and(query_param("page", "2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(page2))
        .mount(&mock_server)
        .await;

    let client = GitHubClient::with_base_url(&mock_server.uri());
    let count = client
        .get_repository_releases_count("test-org", "pagination-test", None)
        .await
        .expect("Should succeed");

    assert_eq!(count, 115);
}

#[tokio::test]
async fn test_github_releases_count_page_failure_is_all_or_nothing() {
    let mock_server = MockServer::start().await;

    // Page 1 succeeds with 100 items
    let page1: Vec<serde_json::Value> = (1..=100)
        .map(|i| serde_json::json!({ "id": i, "draft": false, "prerelease": false }))
        .collect();

    Mock::given(method("GET"))
        .and(path("/repos/test-org/fail-test/releases"))
        .and(query_param("per_page", "100"))
        .and(query_param("page", "1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(page1))
        .mount(&mock_server)
        .await;

    // Page 2 fails with 500
    Mock::given(method("GET"))
        .and(path("/repos/test-org/fail-test/releases"))
        .and(query_param("per_page", "100"))
        .and(query_param("page", "2"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&mock_server)
        .await;

    let client = GitHubClient::with_base_url(&mock_server.uri());
    let res = client
        .get_repository_releases_count("test-org", "fail-test", None)
        .await;

    // All-or-nothing: must return Err, never partial 100
    assert!(res.is_err());
}

// -----------------------------------------------------------------------------
// 6. GitHub Languages Byte Percentage Calculation (WireMock)
// -----------------------------------------------------------------------------

#[tokio::test]
async fn test_github_languages_percentage_calculation() {
    let mock_server = MockServer::start().await;

    let body = serde_json::json!({
        "Rust": 7500,
        "Python": 2500,
    });

    Mock::given(method("GET"))
        .and(path("/repos/test-org/lang-test/languages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(&mock_server)
        .await;

    let client = GitHubClient::with_base_url(&mock_server.uri());
    let langs = client
        .get_repository_languages("test-org", "lang-test", None)
        .await
        .expect("Languages should succeed");

    assert_eq!(langs.get("Rust"), Some(&75.0));
    assert_eq!(langs.get("Python"), Some(&25.0));
}

#[tokio::test]
async fn test_github_languages_empty_response() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/repos/test-org/empty-lang/languages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
        .mount(&mock_server)
        .await;

    let client = GitHubClient::with_base_url(&mock_server.uri());
    let langs = client
        .get_repository_languages("test-org", "empty-lang", None)
        .await
        .expect("Languages should be Ok({})");

    assert!(langs.is_empty());
}

#[tokio::test]
async fn test_github_languages_zero_bytes_no_nan_or_inf() {
    let mock_server = MockServer::start().await;

    let body = serde_json::json!({
        "Text": 0,
        "Binary": 0
    });

    Mock::given(method("GET"))
        .and(path("/repos/test-org/zero-bytes/languages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(&mock_server)
        .await;

    let client = GitHubClient::with_base_url(&mock_server.uri());
    let langs = client
        .get_repository_languages("test-org", "zero-bytes", None)
        .await
        .expect("Languages should be Ok({}) on 0 total bytes");

    assert!(langs.is_empty());
}

#[tokio::test]
async fn test_github_languages_http_error_returns_none() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/repos/test-org/err-lang/languages"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&mock_server)
        .await;

    let client = GitHubClient::with_base_url(&mock_server.uri());
    let res = client
        .get_repository_languages("test-org", "err-lang", None)
        .await;

    assert!(res.is_err());
}

// -----------------------------------------------------------------------------
// 7. Enrichment Concurrency & Error Degradation
// -----------------------------------------------------------------------------

#[tokio::test]
async fn test_enrich_repositories_graceful_degradation() {
    let mock_server = MockServer::start().await;

    // Repo 1: Both releases and languages succeed
    Mock::given(method("GET"))
        .and(path("/repos/test-org/repo1/releases"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            { "id": 1, "draft": false, "prerelease": false }
        ])))
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/repos/test-org/repo1/languages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "Rust": 100 })))
        .mount(&mock_server)
        .await;

    // Repo 2: Releases fails with 500, Languages times out or fails with 403
    Mock::given(method("GET"))
        .and(path("/repos/test-org/repo2/releases"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/repos/test-org/repo2/languages"))
        .respond_with(ResponseTemplate::new(403))
        .mount(&mock_server)
        .await;

    let client = GitHubClient::with_base_url(&mock_server.uri());

    let mut repos = vec![
        GitHubRepository {
            github_repo_id: 1,
            name: "repo1".to_string(),
            github_url: "https://github.com/test-org/repo1".to_string(),
            description: None,
            stars: 10,
            license: None,
            pushed_at: None,
            topics: vec![],
            category: "Other".to_string(),
            releases: None,
            languages: None,
        },
        GitHubRepository {
            github_repo_id: 2,
            name: "repo2".to_string(),
            github_url: "https://github.com/test-org/repo2".to_string(),
            description: None,
            stars: 20,
            license: None,
            pushed_at: None,
            topics: vec![],
            category: "Other".to_string(),
            releases: None,
            languages: None,
        },
    ];

    enrich_repositories(&client, &mut repos, None).await;

    // Repo 1 enriched
    assert_eq!(repos[0].releases, Some(1));
    assert!(repos[0].languages.is_some());
    assert_eq!(
        repos[0].languages.as_ref().unwrap().get("Rust"),
        Some(&100.0)
    );

    // Repo 2 degraded: fields stay None without error
    assert_eq!(repos[1].releases, None);
    assert_eq!(repos[1].languages, None);
}
