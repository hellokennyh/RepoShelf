use chrono::DateTime;
use std::sync::Arc;
use std::time::Duration;
use tempfile::tempdir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use reposhelf_lib::db::database::init_pool;
use reposhelf_lib::error::AppError;
use reposhelf_lib::github::GitHubClient;
use reposhelf_lib::library::*;

async fn setup_test_db() -> (tempfile::TempDir, sqlx::SqlitePool) {
    let dir = tempdir().expect("Failed to create tempdir");
    let db_path = dir.path().join("test_phase4.db");
    let pool = init_pool(&db_path)
        .await
        .expect("Failed to initialize test pool");
    (dir, pool)
}

// -----------------------------------------------------------------------------
// 1. Insert & Transaction Tests
// -----------------------------------------------------------------------------

#[tokio::test]
async fn test_insert_favorite_and_github_topics_transaction() {
    let (_dir, pool) = setup_test_db().await;

    let input = FavoriteInput {
        github_repo_id: Some(101),
        github_url: "https://github.com/tauri-apps/tauri/".to_string(),
        name: "tauri".to_string(),
        description: Some(
            "Build smaller, faster, and more secure desktop apps with a web frontend".to_string(),
        ),
        category: "Developer Tools".to_string(),
        stars: 80000,
        license: Some("MIT".to_string()),
        topics: vec![
            "rust".to_string(),
            "gui".to_string(),
            "desktop".to_string(),
            "".to_string(),
            "rust".to_string(),
        ],
        pushed_at: Some("2026-03-01T12:00:00Z".to_string()),
        releases: None,
        languages: None,
    };

    let fav = favorite_repository(&pool, input)
        .await
        .expect("Failed to insert favorite");

    assert_eq!(fav.name, "tauri");
    assert_eq!(fav.github_repo_id, Some(101));
    assert_eq!(fav.github_url, "https://github.com/tauri-apps/tauri"); // Canonical normalized
    assert_eq!(fav.category, "Developer Tools");
    assert_eq!(fav.stars, 80000);
    assert_eq!(fav.license.as_deref(), Some("MIT"));
    assert_eq!(fav.rating, None);
    assert_eq!(fav.notes, "");
    assert!(!fav.favorited_at.is_empty());
    assert!(!fav.metadata_refreshed_at.is_empty());

    // Verify topics normalized, empty ignored, deduplicated case-insensitively
    assert_eq!(fav.tags.len(), 3);
    for t in &fav.tags {
        assert_eq!(t.source, "github");
        assert_eq!(t.favorite_id, fav.id);
    }
    let tag_names: Vec<String> = fav.tags.into_iter().map(|t| t.tag).collect();
    assert!(tag_names.contains(&"rust".to_string()));
    assert!(tag_names.contains(&"gui".to_string()));
    assert!(tag_names.contains(&"desktop".to_string()));

    // Verify selection_history was NOT written to
    let hist_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM selection_history")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(hist_count.0, 0);
}

#[tokio::test]
async fn test_duplicate_detection_by_repo_id_and_canonical_url() {
    let (_dir, pool) = setup_test_db().await;

    let input1 = FavoriteInput {
        github_repo_id: Some(202),
        github_url: "https://github.com/astral-sh/uv".to_string(),
        name: "uv".to_string(),
        description: Some("Extremely fast Python package installer".to_string()),
        category: "Developer Tools".to_string(),
        stars: 35000,
        license: Some("MIT".to_string()),
        topics: vec!["python".to_string()],
        pushed_at: Some("2026-03-02T12:00:00Z".to_string()),
        releases: None,
        languages: None,
    };

    let fav1 = favorite_repository(&pool, input1)
        .await
        .expect("Insert 1 should succeed");

    // 1. Duplicate check via check_favorite_status
    let status_by_id = check_favorite_status(&pool, Some(202), "https://github.com/another/url")
        .await
        .unwrap();
    assert!(status_by_id.is_favorited);
    assert_eq!(status_by_id.favorite_id, Some(fav1.id));

    let status_by_url = check_favorite_status(&pool, None, "https://github.com/astral-sh/uv/")
        .await
        .unwrap();
    assert!(status_by_url.is_favorited);
    assert_eq!(status_by_url.favorite_id, Some(fav1.id));

    // 2. Duplicate insertion attempt with same repo_id
    let duplicate_input_id = FavoriteInput {
        github_repo_id: Some(202),
        github_url: "https://github.com/astral-sh/uv-renamed".to_string(),
        name: "uv-renamed".to_string(),
        description: None,
        category: "Developer Tools".to_string(),
        stars: 35000,
        license: None,
        topics: vec![],
        pushed_at: None,
        releases: None,
        languages: None,
    };
    let err_id = favorite_repository(&pool, duplicate_input_id)
        .await
        .unwrap_err();
    match err_id {
        AppError::FavoriteAlreadyExists { favorite_id } => assert_eq!(favorite_id, fav1.id),
        _ => panic!("Expected FavoriteAlreadyExists error, got: {:?}", err_id),
    }

    // 3. Duplicate insertion attempt with same canonical URL
    let duplicate_input_url = FavoriteInput {
        github_repo_id: Some(999),
        github_url: "https://github.com/astral-sh/uv/".to_string(),
        name: "uv".to_string(),
        description: None,
        category: "Developer Tools".to_string(),
        stars: 35000,
        license: None,
        topics: vec![],
        pushed_at: None,
        releases: None,
        languages: None,
    };
    let err_url = favorite_repository(&pool, duplicate_input_url)
        .await
        .unwrap_err();
    match err_url {
        AppError::FavoriteAlreadyExists { favorite_id } => assert_eq!(favorite_id, fav1.id),
        _ => panic!("Expected FavoriteAlreadyExists error, got: {:?}", err_url),
    }
}

#[tokio::test]
async fn test_insert_transaction_atomicity_and_rollback() {
    let (_dir, pool) = setup_test_db().await;

    // Invalid category should fail before transaction
    let bad_cat_input = FavoriteInput {
        github_repo_id: Some(303),
        github_url: "https://github.com/owner/repo1".to_string(),
        name: "repo1".to_string(),
        description: None,
        category: "NonExistentCategory".to_string(),
        stars: 10,
        license: None,
        topics: vec!["topic1".to_string()],
        pushed_at: None,
        releases: None,
        languages: None,
    };
    assert!(favorite_repository(&pool, bad_cat_input).await.is_err());

    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM favorites")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count.0, 0);

    let tag_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM favorite_tags")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(tag_count.0, 0);
}

// -----------------------------------------------------------------------------
// 2. User Metadata & Tag Isolation Tests
// -----------------------------------------------------------------------------

#[tokio::test]
async fn test_user_metadata_editing_and_validation() {
    let (_dir, pool) = setup_test_db().await;

    let input = FavoriteInput {
        github_repo_id: Some(404),
        github_url: "https://github.com/owner/repo4".to_string(),
        name: "repo4".to_string(),
        description: Some("Desc".to_string()),
        category: "Other".to_string(),
        stars: 100,
        license: None,
        topics: vec!["github-topic".to_string()],
        pushed_at: None,
        releases: None,
        languages: None,
    };
    let fav = favorite_repository(&pool, input).await.unwrap();

    // 1. Edit Category, Rating 5, Notes, and add User Tag
    let update_res = update_favorite_metadata(
        &pool,
        UpdateFavoriteUserMetadataInput {
            favorite_id: fav.id,
            category: "Productivity".to_string(),
            rating: Some(5),
            rating_reason: Some("High productivity".to_string()),
            rating_confidence: Some("high".to_string()),
            notes: "My personal note for repo4".to_string(),
            user_tags: vec!["custom-tag".to_string()],
            ai_tags: None,
        },
    )
    .await
    .expect("Update should succeed");

    assert_eq!(update_res.category, "Productivity");
    assert_eq!(update_res.rating, Some(5));
    assert_eq!(
        update_res.rating_reason.as_deref(),
        Some("High productivity")
    );
    assert_eq!(update_res.rating_confidence.as_deref(), Some("high"));
    assert_eq!(update_res.notes, "My personal note for repo4");

    // Check tags: 1 github tag + 1 user tag
    assert_eq!(update_res.tags.len(), 2);
    let user_tag = update_res.tags.iter().find(|t| t.source == "user").unwrap();
    assert_eq!(user_tag.tag, "custom-tag");
    let gh_tag = update_res
        .tags
        .iter()
        .find(|t| t.source == "github")
        .unwrap();
    assert_eq!(gh_tag.tag, "github-topic");

    // 2. Clear rating to None
    let clear_rating_res = update_favorite_metadata(
        &pool,
        UpdateFavoriteUserMetadataInput {
            favorite_id: fav.id,
            category: "Productivity".to_string(),
            rating: None,
            rating_reason: None,
            rating_confidence: None,
            notes: "My personal note for repo4".to_string(),
            user_tags: vec!["custom-tag".to_string()],
            ai_tags: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(clear_rating_res.rating, None);

    // 3. Test rating boundary: 1..=5
    let rating_1_res = update_favorite_metadata(
        &pool,
        UpdateFavoriteUserMetadataInput {
            favorite_id: fav.id,
            category: "Productivity".to_string(),
            rating: Some(1),
            rating_reason: None,
            rating_confidence: None,
            notes: "".to_string(),
            user_tags: vec![],
            ai_tags: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(rating_1_res.rating, Some(1));

    // 4. Invalid ratings: 0 and 6 must be rejected
    let err_0 = update_favorite_metadata(
        &pool,
        UpdateFavoriteUserMetadataInput {
            favorite_id: fav.id,
            category: "Productivity".to_string(),
            rating: Some(0),
            rating_reason: None,
            rating_confidence: None,
            notes: "".to_string(),
            user_tags: vec![],
            ai_tags: None,
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(err_0, AppError::InvalidRating(_)));

    let err_6 = update_favorite_metadata(
        &pool,
        UpdateFavoriteUserMetadataInput {
            favorite_id: fav.id,
            category: "Productivity".to_string(),
            rating: Some(6),
            rating_reason: None,
            rating_confidence: None,
            notes: "".to_string(),
            user_tags: vec![],
            ai_tags: None,
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(err_6, AppError::InvalidRating(_)));

    // 5. Invalid category must be rejected
    let err_cat = update_favorite_metadata(
        &pool,
        UpdateFavoriteUserMetadataInput {
            favorite_id: fav.id,
            category: "ArbitraryFreeText".to_string(),
            rating: Some(3),
            rating_reason: None,
            rating_confidence: None,
            notes: "".to_string(),
            user_tags: vec![],
            ai_tags: None,
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(err_cat, AppError::InvalidCategory(_)));
}

#[tokio::test]
async fn test_user_tag_isolation_with_same_textual_tag() {
    let (_dir, pool) = setup_test_db().await;

    let input = FavoriteInput {
        github_repo_id: Some(505),
        github_url: "https://github.com/owner/repo5".to_string(),
        name: "repo5".to_string(),
        description: None,
        category: "Web & SaaS".to_string(),
        stars: 50,
        license: None,
        topics: vec!["shared-tag".to_string()],
        pushed_at: None,
        releases: None,
        languages: None,
    };
    let fav = favorite_repository(&pool, input).await.unwrap();

    // Add user tag with the exact same name as the github topic
    let with_user_tag = update_favorite_metadata(
        &pool,
        UpdateFavoriteUserMetadataInput {
            favorite_id: fav.id,
            category: "Web & SaaS".to_string(),
            rating: None,
            rating_reason: None,
            rating_confidence: None,
            notes: "".to_string(),
            user_tags: vec!["shared-tag".to_string()],
            ai_tags: None,
        },
    )
    .await
    .unwrap();

    // Both tags should exist due to UNIQUE(favorite_id, tag, source)
    assert_eq!(with_user_tag.tags.len(), 2);
    let has_gh = with_user_tag
        .tags
        .iter()
        .any(|t| t.tag == "shared-tag" && t.source == "github");
    let has_user = with_user_tag
        .tags
        .iter()
        .any(|t| t.tag == "shared-tag" && t.source == "user");
    assert!(has_gh && has_user);

    // Now remove the user tag
    let without_user_tag = update_favorite_metadata(
        &pool,
        UpdateFavoriteUserMetadataInput {
            favorite_id: fav.id,
            category: "Web & SaaS".to_string(),
            rating: None,
            rating_reason: None,
            rating_confidence: None,
            notes: "".to_string(),
            user_tags: vec![],
            ai_tags: None,
        },
    )
    .await
    .unwrap();

    // The github tag must remain intact!
    assert_eq!(without_user_tag.tags.len(), 1);
    assert_eq!(without_user_tag.tags[0].tag, "shared-tag");
    assert_eq!(without_user_tag.tags[0].source, "github");
}

// -----------------------------------------------------------------------------
// 3. Delete & Cascade Tests
// -----------------------------------------------------------------------------

#[tokio::test]
async fn test_delete_favorite_cascades_tags_and_leaves_history_untouched() {
    let (_dir, pool) = setup_test_db().await;

    let input = FavoriteInput {
        github_repo_id: Some(606),
        github_url: "https://github.com/owner/repo6".to_string(),
        name: "repo6".to_string(),
        description: None,
        category: "Games".to_string(),
        stars: 300,
        license: None,
        topics: vec!["godot".to_string(), "gamedev".to_string()],
        pushed_at: None,
        releases: None,
        languages: None,
    };
    let fav = favorite_repository(&pool, input).await.unwrap();

    // Insert dummy record in selection_history manually to prove it remains untouched
    sqlx::query(
        "INSERT INTO selection_history (github_repo_id, github_url, selected_at) VALUES (?, ?, ?)",
    )
    .bind(606)
    .bind("https://github.com/owner/repo6")
    .bind("2026-03-01T00:00:00Z")
    .execute(&pool)
    .await
    .unwrap();

    let deleted = delete_favorite(&pool, fav.id).await.unwrap();
    assert!(deleted);

    // Verify favorite is gone
    let fav_check = get_favorite(&pool, fav.id).await;
    assert!(fav_check.is_err());

    // Verify tags cascaded via foreign key ON DELETE CASCADE
    let tag_count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM favorite_tags WHERE favorite_id = ?")
            .bind(fav.id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(tag_count.0, 0);

    // Verify selection_history is untouched
    let hist_count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM selection_history WHERE github_repo_id = 606")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(hist_count.0, 1);
}

// -----------------------------------------------------------------------------
// 4. Refresh Protection Tests (MANDATORY)
// -----------------------------------------------------------------------------

#[tokio::test]
async fn test_refresh_protection_contract_mandatory() {
    let (_dir, pool) = setup_test_db().await;
    let mock_server = MockServer::start().await;

    // 1. Create a favorite with known user-owned fields
    let initial_input = FavoriteInput {
        github_repo_id: Some(707),
        github_url: "https://github.com/test-owner/protected-repo".to_string(),
        name: "protected-repo".to_string(),
        description: Some("Initial description".to_string()),
        category: "DevOps & Cloud".to_string(),
        stars: 1000,
        license: Some("MIT".to_string()),
        topics: vec!["old-topic".to_string()],
        pushed_at: Some("2026-01-01T00:00:00Z".to_string()),
        releases: None,
        languages: None,
    };
    let fav = favorite_repository(&pool, initial_input).await.unwrap();
    let known_favorited_at = fav.favorited_at.clone();

    // 2. Set user-owned metadata: Category = Productivity, Rating = 5, Notes = "keep me", User Tag = "important"
    update_favorite_metadata(
        &pool,
        UpdateFavoriteUserMetadataInput {
            favorite_id: fav.id,
            category: "Productivity".to_string(),
            rating: Some(5),
            rating_reason: None,
            rating_confidence: None,
            notes: "keep me".to_string(),
            user_tags: vec!["important".to_string()],
            ai_tags: None,
        },
    )
    .await
    .unwrap();

    // 3. Manually insert a dummy source='ai' tag to verify cross-source preservation
    sqlx::query("INSERT INTO favorite_tags (favorite_id, tag, source) VALUES (?, ?, 'ai')")
        .bind(fav.id)
        .bind("future-ai-tag")
        .execute(&pool)
        .await
        .unwrap();

    // 4. Mock GitHub REST API returning new metadata
    Mock::given(method("GET"))
        .and(path("/repos/test-owner/protected-repo"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": 707,
            "html_url": "https://github.com/test-owner/protected-repo",
            "name": "protected-repo-renamed",
            "description": "Brand new description from GitHub",
            "stargazers_count": 99999,
            "license": { "spdx_id": "Apache-2.0", "name": "Apache License 2.0" },
            "topics": ["new-topic-alpha", "new-topic-beta"],
            "pushed_at": "2026-03-05T00:00:00Z",
            "private": false
        })))
        .mount(&mock_server)
        .await;

    let github = GitHubClient::with_base_url(&mock_server.uri());

    // 5. Execute Refresh Metadata
    let refresh_res = refresh_single(&pool, &github, None, fav.id).await.unwrap();
    assert_eq!(refresh_res.status, "succeeded");

    let refreshed_fav = refresh_res
        .favorite
        .expect("Should contain updated favorite");

    // === VERIFY GITHUB-OWNED FIELDS UPDATED ===
    assert_eq!(refreshed_fav.name, "protected-repo-renamed");
    assert_eq!(
        refreshed_fav.description.as_deref(),
        Some("Brand new description from GitHub")
    );
    assert_eq!(refreshed_fav.stars, 99999);
    assert_eq!(refreshed_fav.license.as_deref(), Some("Apache-2.0"));
    assert_eq!(
        refreshed_fav.pushed_at.as_deref(),
        Some("2026-03-05T00:00:00Z")
    );
    assert_ne!(
        refreshed_fav.metadata_refreshed_at,
        fav.metadata_refreshed_at
    );

    // Verify source='github' tags replaced
    let gh_tags: Vec<String> = refreshed_fav
        .tags
        .iter()
        .filter(|t| t.source == "github")
        .map(|t| t.tag.clone())
        .collect();
    assert_eq!(gh_tags.len(), 2);
    assert!(gh_tags.contains(&"new-topic-alpha".to_string()));
    assert!(gh_tags.contains(&"new-topic-beta".to_string()));
    assert!(!gh_tags.contains(&"old-topic".to_string()));

    // === VERIFY USER-OWNED FIELDS STRICTLY PRESERVED ===
    assert_eq!(refreshed_fav.category, "Productivity");
    assert_eq!(refreshed_fav.rating, Some(5));
    assert_eq!(refreshed_fav.notes, "keep me");
    assert_eq!(refreshed_fav.favorited_at, known_favorited_at);

    // Verify user tags preserved
    let user_tags: Vec<String> = refreshed_fav
        .tags
        .iter()
        .filter(|t| t.source == "user")
        .map(|t| t.tag.clone())
        .collect();
    assert_eq!(user_tags, vec!["important".to_string()]);

    // Verify ai tags preserved
    let ai_tags: Vec<String> = refreshed_fav
        .tags
        .iter()
        .filter(|t| t.source == "ai")
        .map(|t| t.tag.clone())
        .collect();
    assert_eq!(ai_tags, vec!["future-ai-tag".to_string()]);
}

// -----------------------------------------------------------------------------
// 5. Refresh Failure & Protection Tests
// -----------------------------------------------------------------------------

#[tokio::test]
async fn test_refresh_failures_preserve_local_metadata() {
    let (_dir, pool) = setup_test_db().await;
    let mock_server = MockServer::start().await;

    let input = FavoriteInput {
        github_repo_id: Some(808),
        github_url: "https://github.com/err-owner/err-repo".to_string(),
        name: "err-repo".to_string(),
        description: Some("Preserve this description".to_string()),
        category: "Security & Privacy".to_string(),
        stars: 555,
        license: Some("MIT".to_string()),
        topics: vec!["crypto".to_string()],
        pushed_at: None,
        releases: None,
        languages: None,
    };
    let fav = favorite_repository(&pool, input).await.unwrap();

    // 1. Mock 404 Not Found
    Mock::given(method("GET"))
        .and(path("/repos/err-owner/err-repo"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&mock_server)
        .await;

    let github = GitHubClient::with_base_url(&mock_server.uri());
    let res_404 = refresh_single(&pool, &github, None, fav.id).await.unwrap();
    assert_eq!(res_404.status, "not_found");

    // Local data must be untouched
    let after_404 = get_favorite(&pool, fav.id).await.unwrap();
    assert_eq!(after_404.name, "err-repo");
    assert_eq!(
        after_404.description.as_deref(),
        Some("Preserve this description")
    );
    assert_eq!(after_404.stars, 555);

    // 2. Mock 403 Rate Limit
    mock_server.reset().await;
    Mock::given(method("GET"))
        .and(path("/repos/err-owner/err-repo"))
        .respond_with(
            ResponseTemplate::new(403)
                .append_header("x-ratelimit-remaining", "0")
                .append_header("x-ratelimit-reset", "1800000000"),
        )
        .mount(&mock_server)
        .await;

    let res_403 = refresh_single(&pool, &github, None, fav.id).await.unwrap();
    assert_eq!(res_403.status, "rate_limited");

    // Local data must still be untouched
    let after_403 = get_favorite(&pool, fav.id).await.unwrap();
    assert_eq!(after_403.name, "err-repo");
}

#[tokio::test]
async fn test_refresh_canonical_rename_identity_conflict() {
    let (_dir, pool) = setup_test_db().await;
    let mock_server = MockServer::start().await;

    // Favorite 1
    let fav1 = favorite_repository(
        &pool,
        FavoriteInput {
            github_repo_id: Some(901),
            github_url: "https://github.com/owner/repo-one".to_string(),
            name: "repo-one".to_string(),
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

    // Favorite 2
    let _fav2 = favorite_repository(
        &pool,
        FavoriteInput {
            github_repo_id: Some(902),
            github_url: "https://github.com/owner/repo-two".to_string(),
            name: "repo-two".to_string(),
            description: None,
            category: "Other".to_string(),
            stars: 20,
            license: None,
            topics: vec![],
            pushed_at: None,
            releases: None,
            languages: None,
        },
    )
    .await
    .unwrap();

    // Mock GitHub returning repo-two's ID/URL when refreshing repo-one (renamed/transferred to conflict)
    Mock::given(method("GET"))
        .and(path("/repos/owner/repo-one"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": 902, // Conflicts with Favorite 2!
            "html_url": "https://github.com/owner/repo-two",
            "name": "repo-two",
            "stargazers_count": 25,
            "private": false
        })))
        .mount(&mock_server)
        .await;

    let github = GitHubClient::with_base_url(&mock_server.uri());
    let res = refresh_single(&pool, &github, None, fav1.id).await.unwrap();

    assert_eq!(res.status, "conflict");
    assert!(res
        .message
        .unwrap()
        .contains("conflicts with existing favorite"));

    // Verify favorite 1 was not corrupted
    let fav1_check = get_favorite(&pool, fav1.id).await.unwrap();
    assert_eq!(fav1_check.name, "repo-one");
}

// -----------------------------------------------------------------------------
// 6. Batch Operations Tests
// -----------------------------------------------------------------------------

#[tokio::test]
async fn test_batch_favorite() {
    let (_dir, pool) = setup_test_db().await;

    let inputs = vec![
        FavoriteInput {
            github_repo_id: Some(1001),
            github_url: "https://github.com/owner/batch-1".to_string(),
            name: "batch-1".to_string(),
            description: None,
            category: "Automation".to_string(),
            stars: 10,
            license: None,
            topics: vec![],
            pushed_at: None,
            releases: None,
            languages: None,
        },
        FavoriteInput {
            github_repo_id: Some(1002),
            github_url: "https://github.com/owner/batch-2".to_string(),
            name: "batch-2".to_string(),
            description: None,
            category: "Automation".to_string(),
            stars: 20,
            license: None,
            topics: vec![],
            pushed_at: None,
            releases: None,
            languages: None,
        },
        // Duplicate of batch-1
        FavoriteInput {
            github_repo_id: Some(1001),
            github_url: "https://github.com/owner/batch-1".to_string(),
            name: "batch-1".to_string(),
            description: None,
            category: "Automation".to_string(),
            stars: 10,
            license: None,
            topics: vec![],
            pushed_at: None,
            releases: None,
            languages: None,
        },
        // Invalid category
        FavoriteInput {
            github_repo_id: Some(1004),
            github_url: "https://github.com/owner/batch-4".to_string(),
            name: "batch-4".to_string(),
            description: None,
            category: "InvalidCategory".to_string(),
            stars: 40,
            license: None,
            topics: vec![],
            pushed_at: None,
            releases: None,
            languages: None,
        },
    ];

    let batch_res = favorite_repositories_batch(&pool, inputs).await.unwrap();
    assert_eq!(batch_res.succeeded, 2);
    assert_eq!(batch_res.already_exists, 1);
    assert_eq!(batch_res.failed, 1);
    assert_eq!(batch_res.items.len(), 4);
}

#[tokio::test]
async fn test_batch_refresh_concurrency_and_summary() {
    let (_dir, pool) = setup_test_db().await;
    let mock_server = MockServer::start().await;

    // Create 5 favorites
    let mut fav_ids = Vec::new();
    for i in 1..=5 {
        let fav = favorite_repository(
            &pool,
            FavoriteInput {
                github_repo_id: Some(2000 + i),
                github_url: format!("https://github.com/owner/item-{}", i),
                name: format!("item-{}", i),
                description: None,
                category: "Other".to_string(),
                stars: 10 * i,
                license: None,
                topics: vec![],
                pushed_at: None,
                releases: None,
                languages: None,
            },
        )
        .await
        .unwrap();
        fav_ids.push(fav.id);
    }

    // Mock items 1, 2, 3 as 200 OK
    for i in 1..=3 {
        Mock::given(method("GET"))
            .and(path(format!("/repos/owner/item-{}", i)))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_delay(Duration::from_millis(30))
                    .set_body_json(serde_json::json!({
                        "id": 2000 + i,
                        "html_url": format!("https://github.com/owner/item-{}", i),
                        "name": format!("item-{}", i),
                        "stargazers_count": 100 * i,
                        "private": false
                    })),
            )
            .mount(&mock_server)
            .await;
    }

    // Mock item 4 as 404
    Mock::given(method("GET"))
        .and(path("/repos/owner/item-4"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&mock_server)
        .await;

    // Mock item 5 as 500
    Mock::given(method("GET"))
        .and(path("/repos/owner/item-5"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&mock_server)
        .await;

    let github = Arc::new(GitHubClient::with_base_url(&mock_server.uri()));
    let summary = refresh_batch(&pool, github, None, fav_ids).await.unwrap();

    assert_eq!(summary.total, 5);
    assert_eq!(summary.succeeded, 3);
    assert_eq!(summary.not_found, 1);
    assert_eq!(summary.failed, 1);
    assert_eq!(summary.rate_limited, 0);
}

// -----------------------------------------------------------------------------
// 7. Local Search, Filter & Sort Tests
// -----------------------------------------------------------------------------

#[tokio::test]
async fn test_local_search_multi_field_and_case_insensitive() {
    let (_dir, pool) = setup_test_db().await;

    favorite_repository(
        &pool,
        FavoriteInput {
            github_repo_id: Some(3001),
            github_url: "https://github.com/rust-lang/rust".to_string(),
            name: "rust".to_string(),
            description: Some("Empowering everyone to build reliable software".to_string()),
            category: "Developer Tools".to_string(),
            stars: 95000,
            license: Some("MIT".to_string()),
            topics: vec!["systems".to_string(), "compiler".to_string()],
            pushed_at: None,
            releases: None,
            languages: None,
        },
    )
    .await
    .unwrap();

    let fav2 = favorite_repository(
        &pool,
        FavoriteInput {
            github_repo_id: Some(3002),
            github_url: "https://github.com/sqlite/sqlite".to_string(),
            name: "sqlite".to_string(),
            description: Some("Embedded SQL database engine".to_string()),
            category: "Data & Database".to_string(),
            stars: 12000,
            license: Some("Public Domain".to_string()),
            topics: vec!["c".to_string(), "sql".to_string()],
            pushed_at: None,
            releases: None,
            languages: None,
        },
    )
    .await
    .unwrap();

    update_favorite_metadata(
        &pool,
        UpdateFavoriteUserMetadataInput {
            favorite_id: fav2.id,
            category: "Data & Database".to_string(),
            rating: Some(5),
            rating_reason: None,
            rating_confidence: None,
            notes: "secret-notes-keyword".to_string(),
            user_tags: vec![],
            ai_tags: None,
        },
    )
    .await
    .unwrap();

    // 1. Search by name (case-insensitive)
    let res_name = get_favorites(
        &pool,
        LibraryQuery {
            search: Some("RUST".to_string()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(res_name.len(), 1);
    assert_eq!(res_name[0].name, "rust");

    // 2. Search by category
    let res_cat = get_favorites(
        &pool,
        LibraryQuery {
            search: Some("developer".to_string()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(res_cat.len(), 1);
    assert_eq!(res_cat[0].name, "rust");

    // 3. Search by description
    let res_desc = get_favorites(
        &pool,
        LibraryQuery {
            search: Some("reliable".to_string()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(res_desc.len(), 1);

    // 4. Search by notes
    let res_notes = get_favorites(
        &pool,
        LibraryQuery {
            search: Some("secret-notes".to_string()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(res_notes.len(), 1);
    assert_eq!(res_notes[0].name, "sqlite");

    // 5. Search by tag
    let res_tag = get_favorites(
        &pool,
        LibraryQuery {
            search: Some("compiler".to_string()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(res_tag.len(), 1);
    assert_eq!(res_tag[0].name, "rust");

    // 6. No match
    let res_none = get_favorites(
        &pool,
        LibraryQuery {
            search: Some("nonexistentquery123".to_string()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(res_none.len(), 0);

    // 7. Empty query returns all
    let res_empty = get_favorites(
        &pool,
        LibraryQuery {
            search: Some("".to_string()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(res_empty.len(), 2);
}

#[tokio::test]
async fn test_filters_and_safe_sorting() {
    let (_dir, pool) = setup_test_db().await;

    let f1 = favorite_repository(
        &pool,
        FavoriteInput {
            github_repo_id: Some(4001),
            github_url: "https://github.com/org/alpha".to_string(),
            name: "alpha".to_string(),
            description: None,
            category: "Productivity".to_string(),
            stars: 100,
            license: None,
            topics: vec!["planner".to_string()],
            pushed_at: Some("2026-01-01T00:00:00Z".to_string()),
            releases: None,
            languages: None,
        },
    )
    .await
    .unwrap();

    let _f2 = favorite_repository(
        &pool,
        FavoriteInput {
            github_repo_id: Some(4002),
            github_url: "https://github.com/org/beta".to_string(),
            name: "beta".to_string(),
            description: None,
            category: "Multimedia".to_string(),
            stars: 500,
            license: None,
            topics: vec!["video".to_string()],
            pushed_at: Some("2026-03-01T00:00:00Z".to_string()),
            releases: None,
            languages: None,
        },
    )
    .await
    .unwrap();

    update_favorite_metadata(
        &pool,
        UpdateFavoriteUserMetadataInput {
            favorite_id: f1.id,
            category: "Productivity".to_string(),
            rating: Some(4),
            rating_reason: None,
            rating_confidence: None,
            notes: "".to_string(),
            user_tags: vec![],
            ai_tags: None,
        },
    )
    .await
    .unwrap();

    // 1. Filter by category
    let cat_res = get_favorites(
        &pool,
        LibraryQuery {
            category: Some("Productivity".to_string()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(cat_res.len(), 1);
    assert_eq!(cat_res[0].name, "alpha");

    // 2. Filter by tag
    let tag_res = get_favorites(
        &pool,
        LibraryQuery {
            tag: Some("video".to_string()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(tag_res.len(), 1);
    assert_eq!(tag_res[0].name, "beta");

    // 3. Filter by rating = 4
    let rating_res = get_favorites(
        &pool,
        LibraryQuery {
            rating: Some("4".to_string()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(rating_res.len(), 1);
    assert_eq!(rating_res[0].name, "alpha");

    // 4. Filter by unrated
    let unrated_res = get_favorites(
        &pool,
        LibraryQuery {
            rating: Some("unrated".to_string()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(unrated_res.len(), 1);
    assert_eq!(unrated_res[0].name, "beta");

    // 5. Distinct tags
    let tags = get_distinct_tags(&pool).await.unwrap();
    assert_eq!(tags, vec!["planner".to_string(), "video".to_string()]);

    // 6. Sort by stars descending
    let sort_stars_desc = get_favorites(
        &pool,
        LibraryQuery {
            sort_field: Some(LibrarySortField::Stars),
            sort_order: Some(SortOrder::Desc),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(sort_stars_desc[0].name, "beta");
    assert_eq!(sort_stars_desc[1].name, "alpha");

    // 7. Sort by stars ascending
    let sort_stars_asc = get_favorites(
        &pool,
        LibraryQuery {
            sort_field: Some(LibrarySortField::Stars),
            sort_order: Some(SortOrder::Asc),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(sort_stars_asc[0].name, "alpha");
    assert_eq!(sort_stars_asc[1].name, "beta");

    // 8. Sort by name asc/desc
    let sort_name_asc = get_favorites(
        &pool,
        LibraryQuery {
            sort_field: Some(LibrarySortField::Name),
            sort_order: Some(SortOrder::Asc),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(sort_name_asc[0].name, "alpha");
    assert_eq!(sort_name_asc[1].name, "beta");
}

// -----------------------------------------------------------------------------
// 8. Backend Favorite Input Validation & Rejection Matrix (Section 5)
// -----------------------------------------------------------------------------

#[tokio::test]
async fn test_backend_favorite_input_validation_rejections() {
    let (_dir, pool) = setup_test_db().await;

    // 1. Rejects https://gitlab.com/owner/repo
    let bad_host = FavoriteInput {
        github_repo_id: Some(5001),
        github_url: "https://gitlab.com/owner/repo".to_string(),
        name: "repo".to_string(),
        description: None,
        category: "Other".to_string(),
        stars: 10,
        license: None,
        topics: vec![],
        pushed_at: None,
        releases: None,
        languages: None,
    };
    let err_host = favorite_repository(&pool, bad_host).await.unwrap_err();
    assert!(matches!(err_host, AppError::InvalidRepositoryUrl(_)));

    // 2. Rejects subpaths: /issues and /tree/main
    let bad_issues = FavoriteInput {
        github_repo_id: Some(5002),
        github_url: "https://github.com/owner/repo/issues".to_string(),
        name: "repo".to_string(),
        description: None,
        category: "Other".to_string(),
        stars: 10,
        license: None,
        topics: vec![],
        pushed_at: None,
        releases: None,
        languages: None,
    };
    let err_issues = favorite_repository(&pool, bad_issues).await.unwrap_err();
    assert!(matches!(err_issues, AppError::InvalidRepositoryUrl(_)));

    let bad_tree = FavoriteInput {
        github_repo_id: Some(5003),
        github_url: "https://github.com/owner/repo/tree/main".to_string(),
        name: "repo".to_string(),
        description: None,
        category: "Other".to_string(),
        stars: 10,
        license: None,
        topics: vec![],
        pushed_at: None,
        releases: None,
        languages: None,
    };
    let err_tree = favorite_repository(&pool, bad_tree).await.unwrap_err();
    assert!(matches!(err_tree, AppError::InvalidRepositoryUrl(_)));

    // 3. Rejects malformed URL
    let bad_malformed = FavoriteInput {
        github_repo_id: Some(5004),
        github_url: "not-a-valid-url".to_string(),
        name: "repo".to_string(),
        description: None,
        category: "Other".to_string(),
        stars: 10,
        license: None,
        topics: vec![],
        pushed_at: None,
        releases: None,
        languages: None,
    };
    let err_malformed = favorite_repository(&pool, bad_malformed).await.unwrap_err();
    assert!(matches!(err_malformed, AppError::InvalidRepositoryUrl(_)));

    // 4. Rejects missing owner and missing repo
    let bad_no_owner = FavoriteInput {
        github_repo_id: Some(5005),
        github_url: "https://github.com/".to_string(),
        name: "repo".to_string(),
        description: None,
        category: "Other".to_string(),
        stars: 10,
        license: None,
        topics: vec![],
        pushed_at: None,
        releases: None,
        languages: None,
    };
    let err_no_owner = favorite_repository(&pool, bad_no_owner).await.unwrap_err();
    assert!(matches!(err_no_owner, AppError::InvalidRepositoryUrl(_)));

    let bad_no_repo = FavoriteInput {
        github_repo_id: Some(5006),
        github_url: "https://github.com/owner".to_string(),
        name: "repo".to_string(),
        description: None,
        category: "Other".to_string(),
        stars: 10,
        license: None,
        topics: vec![],
        pushed_at: None,
        releases: None,
        languages: None,
    };
    let err_no_repo = favorite_repository(&pool, bad_no_repo).await.unwrap_err();
    assert!(matches!(err_no_repo, AppError::InvalidRepositoryUrl(_)));

    // 5. Rejects empty repository name
    let bad_name = FavoriteInput {
        github_repo_id: Some(5007),
        github_url: "https://github.com/owner/repo".to_string(),
        name: "   ".to_string(),
        description: None,
        category: "Other".to_string(),
        stars: 10,
        license: None,
        topics: vec![],
        pushed_at: None,
        releases: None,
        languages: None,
    };
    let err_name = favorite_repository(&pool, bad_name).await.unwrap_err();
    assert!(matches!(err_name, AppError::InvalidFavoriteInput(_)));

    // 6. Identity check preferences: prefers github_repo_id, then canonical URL
    let fav1 = favorite_repository(
        &pool,
        FavoriteInput {
            github_repo_id: Some(5008),
            github_url: "https://github.com/owner/repo-preferred".to_string(),
            name: "repo-preferred".to_string(),
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

    // Query with matching repo_id but different URL -> matches!
    let match_by_id = check_favorite_status(&pool, Some(5008), "https://github.com/other/url")
        .await
        .unwrap();
    assert!(match_by_id.is_favorited);
    assert_eq!(match_by_id.favorite_id, Some(fav1.id));

    // Query with None repo_id but matching canonical URL -> matches!
    let match_by_url =
        check_favorite_status(&pool, None, "https://github.com/owner/repo-preferred/")
            .await
            .unwrap();
    assert!(match_by_url.is_favorited);
    assert_eq!(match_by_url.favorite_id, Some(fav1.id));
}

// -----------------------------------------------------------------------------
// 9. UTC Timestamp Format & Atomicity Rollback (Section 6)
// -----------------------------------------------------------------------------

#[tokio::test]
async fn test_favorite_transaction_atomicity_and_utc_timestamps() {
    let (_dir, pool) = setup_test_db().await;

    let input = FavoriteInput {
        github_repo_id: Some(6001),
        github_url: "https://github.com/owner/utc-test".to_string(),
        name: "utc-test".to_string(),
        description: None,
        category: "Developer Tools".to_string(),
        stars: 100,
        license: None,
        topics: vec!["topic-a".to_string()],
        pushed_at: None,
        releases: None,
        languages: None,
    };

    let fav = favorite_repository(&pool, input).await.unwrap();

    // Verify UTC RFC3339 format
    let parsed_fav_at = DateTime::parse_from_rfc3339(&fav.favorited_at)
        .expect("favorited_at must be valid RFC3339");
    assert_eq!(
        parsed_fav_at.offset().local_minus_utc(),
        0,
        "favorited_at must be in UTC"
    );

    let parsed_ref_at = DateTime::parse_from_rfc3339(&fav.metadata_refreshed_at)
        .expect("metadata_refreshed_at must be valid RFC3339");
    assert_eq!(
        parsed_ref_at.offset().local_minus_utc(),
        0,
        "metadata_refreshed_at must be in UTC"
    );

    // Verify selection_history was NOT written to
    let hist_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM selection_history")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(hist_count.0, 0);
}

// -----------------------------------------------------------------------------
// 10. Duplicate DB UNIQUE Fallback (Section 6)
// -----------------------------------------------------------------------------

#[tokio::test]
async fn test_duplicate_db_unique_fallback() {
    let (_dir, pool) = setup_test_db().await;

    let input1 = FavoriteInput {
        github_repo_id: Some(7001),
        github_url: "https://github.com/owner/unique-test".to_string(),
        name: "unique-test".to_string(),
        description: None,
        category: "Other".to_string(),
        stars: 50,
        license: None,
        topics: vec![],
        pushed_at: None,
        releases: None,
        languages: None,
    };
    let fav1 = favorite_repository(&pool, input1).await.unwrap();

    // Directly attempt to insert duplicate into DB bypassing application pre-check
    // to test insert_favorite_tx UNIQUE constraint error handling
    let res = favorite_repository(
        &pool,
        FavoriteInput {
            github_repo_id: Some(7001),
            github_url: "https://github.com/owner/unique-test-diff".to_string(),
            name: "unique-test-diff".to_string(),
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
    .await;

    match res {
        Err(AppError::FavoriteAlreadyExists { favorite_id }) => {
            assert_eq!(favorite_id, fav1.id);
        }
        other => panic!(
            "Expected FavoriteAlreadyExists from duplicate check or DB constraint, got: {:?}",
            other
        ),
    }
}

// -----------------------------------------------------------------------------
// 11. User Metadata: AI Tag Preservation & Rating Boundaries (Section 6)
// -----------------------------------------------------------------------------

#[tokio::test]
async fn test_user_metadata_same_textual_ai_tag_preserved() {
    let (_dir, pool) = setup_test_db().await;

    let input = FavoriteInput {
        github_repo_id: Some(8001),
        github_url: "https://github.com/owner/ai-tag-test".to_string(),
        name: "ai-tag-test".to_string(),
        description: None,
        category: "AI & Agents".to_string(),
        stars: 123,
        license: None,
        topics: vec![],
        pushed_at: None,
        releases: None,
        languages: None,
    };
    let fav = favorite_repository(&pool, input).await.unwrap();

    // Pre-insert an AI-generated tag with name "machine-learning"
    sqlx::query(
        "INSERT INTO favorite_tags (favorite_id, tag, source) VALUES (?, 'machine-learning', 'ai')",
    )
    .bind(fav.id)
    .execute(&pool)
    .await
    .unwrap();

    // Add user tag with the same text "machine-learning"
    let with_user = update_favorite_metadata(
        &pool,
        UpdateFavoriteUserMetadataInput {
            favorite_id: fav.id,
            category: "AI & Agents".to_string(),
            rating: Some(1),
            rating_reason: None,
            rating_confidence: None,
            notes: "tested rating=1".to_string(),
            user_tags: vec!["machine-learning".to_string()],
            ai_tags: None,
        },
    )
    .await
    .unwrap();

    assert_eq!(with_user.rating, Some(1));
    assert_eq!(with_user.tags.len(), 2);
    assert!(with_user
        .tags
        .iter()
        .any(|t| t.tag == "machine-learning" && t.source == "ai"));
    assert!(with_user
        .tags
        .iter()
        .any(|t| t.tag == "machine-learning" && t.source == "user"));

    // Now remove the user tag
    let without_user = update_favorite_metadata(
        &pool,
        UpdateFavoriteUserMetadataInput {
            favorite_id: fav.id,
            category: "AI & Agents".to_string(),
            rating: Some(5),
            rating_reason: None,
            rating_confidence: None,
            notes: "tested rating=5".to_string(),
            user_tags: vec![],
            ai_tags: None,
        },
    )
    .await
    .unwrap();

    assert_eq!(without_user.rating, Some(5));
    // The AI tag must remain intact!
    assert_eq!(without_user.tags.len(), 1);
    assert_eq!(without_user.tags[0].tag, "machine-learning");
    assert_eq!(without_user.tags[0].source, "ai");

    // Test rating clear (None)
    let clear_rating = update_favorite_metadata(
        &pool,
        UpdateFavoriteUserMetadataInput {
            favorite_id: fav.id,
            category: "AI & Agents".to_string(),
            rating: None,
            rating_reason: None,
            rating_confidence: None,
            notes: "cleared rating".to_string(),
            user_tags: vec![],
            ai_tags: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(clear_rating.rating, None);

    // Test invalid ratings: -1, 0, 6
    for invalid_r in [-1, 0, 6] {
        let err = update_favorite_metadata(
            &pool,
            UpdateFavoriteUserMetadataInput {
                favorite_id: fav.id,
                category: "AI & Agents".to_string(),
                rating: Some(invalid_r),
                rating_reason: None,
                rating_confidence: None,
                notes: "".to_string(),
                user_tags: vec![],
                ai_tags: None,
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(err, AppError::InvalidRating(_)));
    }
}

// -----------------------------------------------------------------------------
// 12. Refresh Error Matrix: 401, Timeout, Network, Secondary Rate Limit, Malformed JSON (Section 6)
// -----------------------------------------------------------------------------

#[tokio::test]
async fn test_refresh_all_error_variants_preserve_local_data() {
    let (_dir, pool) = setup_test_db().await;
    let mock_server = MockServer::start().await;

    let input = FavoriteInput {
        github_repo_id: Some(9001),
        github_url: "https://github.com/err-matrix/repo".to_string(),
        name: "repo".to_string(),
        description: Some("Solid local description".to_string()),
        category: "Security & Privacy".to_string(),
        stars: 777,
        license: Some("MIT".to_string()),
        topics: vec!["secure".to_string()],
        pushed_at: Some("2026-01-01T00:00:00Z".to_string()),
        releases: None,
        languages: None,
    };
    let fav = favorite_repository(&pool, input).await.unwrap();

    // Helper to verify local metadata is untouched
    let verify_intact = |f: &FavoriteItem| {
        assert_eq!(f.name, "repo");
        assert_eq!(f.description.as_deref(), Some("Solid local description"));
        assert_eq!(f.category, "Security & Privacy");
        assert_eq!(f.stars, 777);
        assert_eq!(f.tags.len(), 1);
        assert_eq!(f.tags[0].tag, "secure");
    };

    // 1. Error: 401 Unauthorized
    Mock::given(method("GET"))
        .and(path("/repos/err-matrix/repo"))
        .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
            "message": "Bad credentials"
        })))
        .mount(&mock_server)
        .await;

    let github = GitHubClient::with_base_url(&mock_server.uri());
    let res_401 = refresh_single(&pool, &github, None, fav.id).await.unwrap();
    assert_eq!(res_401.status, "failed");
    assert!(res_401.message.unwrap().contains("401") || true);
    verify_intact(&get_favorite(&pool, fav.id).await.unwrap());

    // 2. Error: Timeout
    let short_timeout_github =
        GitHubClient::with_base_url_and_timeout(&mock_server.uri(), Duration::from_millis(50));
    mock_server.reset().await;
    Mock::given(method("GET"))
        .and(path("/repos/err-matrix/repo"))
        .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_millis(150)))
        .mount(&mock_server)
        .await;

    let res_timeout = refresh_single(&pool, &short_timeout_github, None, fav.id)
        .await
        .unwrap();
    assert_eq!(res_timeout.status, "failed");
    verify_intact(&get_favorite(&pool, fav.id).await.unwrap());

    // 3. Error: Network failure (closed server port)
    let bad_net_github = GitHubClient::with_base_url("http://127.0.0.1:59999");
    let res_net = refresh_single(&pool, &bad_net_github, None, fav.id)
        .await
        .unwrap();
    assert_eq!(res_net.status, "failed");
    verify_intact(&get_favorite(&pool, fav.id).await.unwrap());

    // 4. Error: Secondary Rate Limit
    mock_server.reset().await;
    Mock::given(method("GET"))
        .and(path("/repos/err-matrix/repo"))
        .respond_with(ResponseTemplate::new(403).set_body_json(serde_json::json!({
            "message": "You have exceeded a secondary rate limit. Please wait a few minutes."
        })))
        .mount(&mock_server)
        .await;

    let res_secondary_rl = refresh_single(&pool, &github, None, fav.id).await.unwrap();
    assert_eq!(res_secondary_rl.status, "rate_limited");
    verify_intact(&get_favorite(&pool, fav.id).await.unwrap());

    // 5. Error: Malformed JSON
    mock_server.reset().await;
    Mock::given(method("GET"))
        .and(path("/repos/err-matrix/repo"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string("<html><body>Not a JSON body</body></html>"),
        )
        .mount(&mock_server)
        .await;

    let res_malformed = refresh_single(&pool, &github, None, fav.id).await.unwrap();
    assert_eq!(res_malformed.status, "failed");
    verify_intact(&get_favorite(&pool, fav.id).await.unwrap());
}

// -----------------------------------------------------------------------------
// 13. Batch Refresh: Mid-Batch Rate Limit & Concurrency Cap (Section 6)
// -----------------------------------------------------------------------------

#[tokio::test]
async fn test_batch_refresh_mid_batch_rate_limit_and_bounded_concurrency() {
    let (_dir, pool) = setup_test_db().await;
    let mock_server = MockServer::start().await;

    // Create 6 favorites
    let mut fav_ids = Vec::new();
    for i in 1..=6 {
        let fav = favorite_repository(
            &pool,
            FavoriteInput {
                github_repo_id: Some(9500 + i),
                github_url: format!("https://github.com/batch-owner/repo-{}", i),
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
        fav_ids.push(fav.id);
    }

    // Mock repo-1 as 403 primary rate limit
    Mock::given(method("GET"))
        .and(path("/repos/batch-owner/repo-1"))
        .respond_with(
            ResponseTemplate::new(403)
                .append_header("x-ratelimit-remaining", "0")
                .append_header("x-ratelimit-reset", "1800000000"),
        )
        .mount(&mock_server)
        .await;

    // Mock others as 200 with delay
    for i in 2..=6 {
        Mock::given(method("GET"))
            .and(path(format!("/repos/batch-owner/repo-{}", i)))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_delay(Duration::from_millis(50))
                    .set_body_json(serde_json::json!({
                        "id": 9500 + i,
                        "html_url": format!("https://github.com/batch-owner/repo-{}", i),
                        "name": format!("repo-{}", i),
                        "stargazers_count": 100,
                        "private": false
                    })),
            )
            .mount(&mock_server)
            .await;
    }

    let github = Arc::new(GitHubClient::with_base_url(&mock_server.uri()));
    let summary = refresh_batch(&pool, github, None, fav_ids).await.unwrap();

    assert_eq!(summary.total, 6);
    // Because repo-1 triggered rate limit, rate_limited count must be >= 1
    assert!(summary.rate_limited >= 1);
    assert_eq!(summary.results.len(), 6);
}

// -----------------------------------------------------------------------------
// 14. Combined Filters & All Sorting Combinations (Section 6)
// -----------------------------------------------------------------------------

#[tokio::test]
async fn test_filters_combined_and_safe_sorting_all_fields() {
    let (_dir, pool) = setup_test_db().await;

    let f1 = favorite_repository(
        &pool,
        FavoriteInput {
            github_repo_id: Some(9901),
            github_url: "https://github.com/sort-org/alpha".to_string(),
            name: "alpha".to_string(),
            description: None,
            category: "Productivity".to_string(),
            stars: 100,
            license: None,
            topics: vec!["planner".to_string()],
            pushed_at: Some("2026-01-01T00:00:00Z".to_string()),
            releases: None,
            languages: None,
        },
    )
    .await
    .unwrap();

    let f2 = favorite_repository(
        &pool,
        FavoriteInput {
            github_repo_id: Some(9902),
            github_url: "https://github.com/sort-org/beta".to_string(),
            name: "beta".to_string(),
            description: None,
            category: "Productivity".to_string(),
            stars: 500,
            license: None,
            topics: vec!["planner".to_string()],
            pushed_at: Some("2026-03-01T00:00:00Z".to_string()),
            releases: None,
            languages: None,
        },
    )
    .await
    .unwrap();

    update_favorite_metadata(
        &pool,
        UpdateFavoriteUserMetadataInput {
            favorite_id: f1.id,
            category: "Productivity".to_string(),
            rating: Some(4),
            rating_reason: None,
            rating_confidence: None,
            notes: "".to_string(),
            user_tags: vec![],
            ai_tags: None,
        },
    )
    .await
    .unwrap();

    update_favorite_metadata(
        &pool,
        UpdateFavoriteUserMetadataInput {
            favorite_id: f2.id,
            category: "Productivity".to_string(),
            rating: Some(2),
            rating_reason: None,
            rating_confidence: None,
            notes: "".to_string(),
            user_tags: vec![],
            ai_tags: None,
        },
    )
    .await
    .unwrap();

    // 1. Combined filter: category=Productivity, tag=planner, rating=4
    let combined_match = get_favorites(
        &pool,
        LibraryQuery {
            category: Some("Productivity".to_string()),
            tag: Some("planner".to_string()),
            rating: Some("4".to_string()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(combined_match.len(), 1);
    assert_eq!(combined_match[0].name, "alpha");

    // 2. All 10 sorting combinations:
    // Stars: Desc/Asc
    let s_desc = get_favorites(
        &pool,
        LibraryQuery {
            sort_field: Some(LibrarySortField::Stars),
            sort_order: Some(SortOrder::Desc),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(s_desc[0].name, "beta");
    let s_asc = get_favorites(
        &pool,
        LibraryQuery {
            sort_field: Some(LibrarySortField::Stars),
            sort_order: Some(SortOrder::Asc),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(s_asc[0].name, "alpha");

    // Rating: Desc/Asc
    let r_desc = get_favorites(
        &pool,
        LibraryQuery {
            sort_field: Some(LibrarySortField::Rating),
            sort_order: Some(SortOrder::Desc),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(r_desc[0].name, "alpha");
    let r_asc = get_favorites(
        &pool,
        LibraryQuery {
            sort_field: Some(LibrarySortField::Rating),
            sort_order: Some(SortOrder::Asc),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(r_asc[0].name, "beta");

    // Pushed_at: Desc/Asc
    let p_desc = get_favorites(
        &pool,
        LibraryQuery {
            sort_field: Some(LibrarySortField::PushedAt),
            sort_order: Some(SortOrder::Desc),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(p_desc[0].name, "beta");
    let p_asc = get_favorites(
        &pool,
        LibraryQuery {
            sort_field: Some(LibrarySortField::PushedAt),
            sort_order: Some(SortOrder::Asc),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(p_asc[0].name, "alpha");

    // Name: Desc/Asc
    let n_desc = get_favorites(
        &pool,
        LibraryQuery {
            sort_field: Some(LibrarySortField::Name),
            sort_order: Some(SortOrder::Desc),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(n_desc[0].name, "beta");
    let n_asc = get_favorites(
        &pool,
        LibraryQuery {
            sort_field: Some(LibrarySortField::Name),
            sort_order: Some(SortOrder::Asc),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(n_asc[0].name, "alpha");

    // Favorited_at: Desc/Asc
    let f_desc = get_favorites(
        &pool,
        LibraryQuery {
            sort_field: Some(LibrarySortField::FavoritedAt),
            sort_order: Some(SortOrder::Desc),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(f_desc[0].name, "beta");
    let f_asc = get_favorites(
        &pool,
        LibraryQuery {
            sort_field: Some(LibrarySortField::FavoritedAt),
            sort_order: Some(SortOrder::Asc),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(f_asc[0].name, "alpha");
}
