use std::path::Path;
use std::sync::Arc;

use reposhelf_lib::db::database::init_pool;
use reposhelf_lib::error::AppError;
use reposhelf_lib::github::GitHubClient;
use reposhelf_lib::library::{
    delete_favorite, favorite_repository, list_favorites, refresh_single, update_favorite_metadata,
    FavoriteInput, LibraryQuery, LibrarySortField, SortOrder, UpdateFavoriteUserMetadataInput,
};
use reposhelf_lib::secrets::{OsSecretStore, SecretStore};
use sqlx::Row;

#[tokio::test]
async fn test_real_tauri_runtime_database_smoke_test() {
    let db_path =
        Path::new(r"C:\Users\Administrator\AppData\Roaming\com.reposhelf.desktop\reposhelf.db");
    if !db_path.exists() {
        println!(
            "Runtime database not found at {:?}, skipping live test",
            db_path
        );
        return;
    }

    let pool = init_pool(db_path)
        .await
        .expect("Failed to open real runtime database");

    // Pre-flight check: ensure clean state
    let fav_count: i64 = sqlx::query("SELECT COUNT(*) FROM favorites")
        .fetch_one(&pool)
        .await
        .unwrap()
        .get(0);
    let tag_count: i64 = sqlx::query("SELECT COUNT(*) FROM favorite_tags")
        .fetch_one(&pool)
        .await
        .unwrap()
        .get(0);
    let hist_count: i64 = sqlx::query("SELECT COUNT(*) FROM selection_history")
        .fetch_one(&pool)
        .await
        .unwrap()
        .get(0);

    assert_eq!(
        fav_count, 0,
        "Runtime favorites must be clean (0) before smoke test"
    );
    assert_eq!(
        tag_count, 0,
        "Runtime favorite_tags must be clean (0) before smoke test"
    );
    assert_eq!(
        hist_count, 0,
        "Runtime selection_history must be untouched (0)"
    );

    let secret_store: Arc<dyn SecretStore> = Arc::new(OsSecretStore::new());
    let github_client = Arc::new(GitHubClient::new());

    // Step 1: Single favorite real GitHub repository (octocat/Hello-World)
    let token = secret_store.get_secret("github_token").ok().flatten();
    let repo_meta = github_client
        .get_repository("octocat", "Hello-World", token.as_deref())
        .await
        .expect("Failed to fetch octocat/Hello-World from GitHub API");

    let input = FavoriteInput {
        github_repo_id: Some(repo_meta.github_repo_id),
        github_url: repo_meta.github_url.clone(),
        name: repo_meta.name.clone(),
        description: repo_meta.description.clone(),
        category: repo_meta.category.clone(),
        stars: repo_meta.stars as i64,
        license: repo_meta.license.clone(),
        topics: repo_meta.topics.clone(),
        pushed_at: repo_meta.pushed_at.clone(),
    };

    let fav_item = favorite_repository(&pool, input.clone())
        .await
        .expect("Failed to favorite repository in live DB");

    assert_eq!(fav_item.name, "Hello-World");
    assert_eq!(
        fav_item.github_url,
        "https://github.com/octocat/Hello-World"
    );
    let fav_id = fav_item.id;

    // Verify DB row counts
    let count_after_fav: i64 = sqlx::query("SELECT COUNT(*) FROM favorites")
        .fetch_one(&pool)
        .await
        .unwrap()
        .get(0);
    assert_eq!(count_after_fav, 1);

    // Step 2: Duplicate favorite triggers Duplicate detection
    let dup_res = favorite_repository(&pool, input).await;
    match dup_res {
        Err(AppError::FavoriteAlreadyExists { favorite_id }) => {
            assert_eq!(favorite_id, fav_id);
        }
        other => panic!("Expected FavoriteAlreadyExists error, got: {:?}", other),
    }

    // Step 3: Edit user-owned metadata (Category, Rating, Notes, User tags)
    let update_input = UpdateFavoriteUserMetadataInput {
        favorite_id: fav_id,
        category: "Productivity".to_string(),
        rating: Some(5),
        notes: "PHASE4_RUNTIME_SMOKE_VERIFIED".to_string(),
        user_tags: vec!["runtime-smoke".to_string(), "phase4-verified".to_string()],
    };

    let updated = update_favorite_metadata(&pool, update_input)
        .await
        .expect("Failed to update user metadata");

    assert_eq!(updated.category, "Productivity");
    assert_eq!(updated.rating, Some(5));
    assert_eq!(updated.notes, "PHASE4_RUNTIME_SMOKE_VERIFIED");
    assert!(updated
        .tags
        .iter()
        .any(|t| t.tag == "runtime-smoke" && t.source == "user"));
    assert!(updated
        .tags
        .iter()
        .any(|t| t.tag == "phase4-verified" && t.source == "user"));

    // Step 4: Metadata Refresh & strict protection of user-owned fields
    let refresh_res = refresh_single(&pool, &github_client, token.as_deref(), fav_id)
        .await
        .expect("Failed to refresh favorite");

    assert_eq!(refresh_res.status, "succeeded");
    let refreshed = refresh_res
        .favorite
        .expect("Refreshed favorite should be returned");

    // Critical assertion: user-owned fields must NOT be overwritten by refresh
    assert_eq!(refreshed.category, "Productivity");
    assert_eq!(refreshed.rating, Some(5));
    assert_eq!(refreshed.notes, "PHASE4_RUNTIME_SMOKE_VERIFIED");
    assert!(refreshed
        .tags
        .iter()
        .any(|t| t.tag == "runtime-smoke" && t.source == "user"));
    assert!(refreshed
        .tags
        .iter()
        .any(|t| t.tag == "phase4-verified" && t.source == "user"));

    // Step 5: Multi-field search, filter, and sorting
    let search_res = list_favorites(
        &pool,
        &LibraryQuery {
            search: Some("SMOKE_VERIFIED".to_string()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(search_res.len(), 1);

    let filter_cat_res = list_favorites(
        &pool,
        &LibraryQuery {
            category: Some("Productivity".to_string()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(filter_cat_res.len(), 1);

    let filter_tag_res = list_favorites(
        &pool,
        &LibraryQuery {
            tag: Some("runtime-smoke".to_string()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(filter_tag_res.len(), 1);

    let sort_res = list_favorites(
        &pool,
        &LibraryQuery {
            sort_field: Some(LibrarySortField::Stars),
            sort_order: Some(SortOrder::Desc),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(sort_res.len(), 1);

    // Step 6: Delete favorite with cascade
    let deleted = delete_favorite(&pool, fav_id)
        .await
        .expect("Failed to delete favorite");
    assert!(deleted);

    // Step 7: Zero residual verification (Zero residual test data)
    let final_fav_count: i64 = sqlx::query("SELECT COUNT(*) FROM favorites")
        .fetch_one(&pool)
        .await
        .unwrap()
        .get(0);
    let final_tag_count: i64 = sqlx::query("SELECT COUNT(*) FROM favorite_tags")
        .fetch_one(&pool)
        .await
        .unwrap()
        .get(0);
    let final_hist_count: i64 = sqlx::query("SELECT COUNT(*) FROM selection_history")
        .fetch_one(&pool)
        .await
        .unwrap()
        .get(0);

    assert_eq!(final_fav_count, 0, "favorites table must return to 0 rows");
    assert_eq!(
        final_tag_count, 0,
        "favorite_tags table must return to 0 rows"
    );
    assert_eq!(
        final_hist_count, 0,
        "selection_history table must remain at 0 rows"
    );

    println!("Real runtime smoke test passed with ZERO residual data!");
}
