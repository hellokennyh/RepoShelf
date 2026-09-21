use tempfile::tempdir;

use reposhelf_lib::db::database::init_pool;
use reposhelf_lib::error::AppError;
use reposhelf_lib::library::models::{FavoriteInput, UpdateFavoriteUserMetadataInput};
use reposhelf_lib::library::repository::{
    delete_favorites_batch_tx, find_favorite_by_id, insert_favorite_tx, update_user_metadata_tx,
};

async fn setup_test_db() -> (tempfile::TempDir, sqlx::SqlitePool) {
    let dir = tempdir().expect("Failed to create tempdir");
    let db_path = dir.path().join("test_phase_s5_batch_delete.db");
    let pool = init_pool(&db_path)
        .await
        .expect("Failed to initialize test pool");
    (dir, pool)
}

fn sample_favorite(repo_id: i64, name: &str, url: &str) -> FavoriteInput {
    FavoriteInput {
        github_repo_id: Some(repo_id),
        github_url: url.to_string(),
        name: name.to_string(),
        description: Some(format!("Description for {}", name)),
        category: "Developer Tools".to_string(),
        stars: 42,
        license: Some("MIT".to_string()),
        topics: vec!["rust".to_string(), "tool".to_string()],
        pushed_at: Some("2026-09-20T00:00:00Z".to_string()),
        releases: Some(1),
        languages: None,
        translation: None,
    }
}

#[tokio::test]
async fn test_batch_delete_success_single_atomic_transaction() {
    let (_dir, pool) = setup_test_db().await;

    // Insert 3 favorites
    let fav1 = insert_favorite_tx(
        &pool,
        &sample_favorite(1, "repo1", "https://github.com/owner/repo1"),
    )
    .await
    .unwrap();
    let fav2 = insert_favorite_tx(
        &pool,
        &sample_favorite(2, "repo2", "https://github.com/owner/repo2"),
    )
    .await
    .unwrap();
    let fav3 = insert_favorite_tx(
        &pool,
        &sample_favorite(3, "repo3", "https://github.com/owner/repo3"),
    )
    .await
    .unwrap();

    // Insert selection_history entries
    sqlx::query("INSERT INTO selection_history (github_repo_id, github_url, selected_at) VALUES (1, 'https://github.com/owner/repo1', '2026-09-20T01:00:00Z')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO selection_history (github_repo_id, github_url, selected_at) VALUES (2, 'https://github.com/owner/repo2', '2026-09-20T01:00:00Z')")
        .execute(&pool)
        .await
        .unwrap();

    // Batch delete fav1 and fav2
    let result = delete_favorites_batch_tx(&pool, &[fav1.id, fav2.id])
        .await
        .unwrap();

    assert_eq!(result.requested_count, 2);
    assert_eq!(result.deleted_count, 2);
    assert_eq!(result.not_found_count, 0);
    assert_eq!(result.deleted_ids, vec![fav1.id, fav2.id]);

    // fav1 and fav2 no longer exist
    assert!(find_favorite_by_id(&pool, fav1.id).await.unwrap().is_none());
    assert!(find_favorite_by_id(&pool, fav2.id).await.unwrap().is_none());

    // Tags for fav1 and fav2 are gone
    let remaining_tags: Vec<(i64,)> =
        sqlx::query_as("SELECT id FROM favorite_tags WHERE favorite_id IN (?, ?)")
            .bind(fav1.id)
            .bind(fav2.id)
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(remaining_tags.len(), 0);

    // fav3 remains intact with tags
    let fetched_fav3 = find_favorite_by_id(&pool, fav3.id)
        .await
        .unwrap()
        .expect("fav3 should exist");
    assert_eq!(fetched_fav3.name, "repo3");
    assert_eq!(fetched_fav3.tags.len(), 2);

    // selection_history is strictly preserved
    let history_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM selection_history")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(history_count.0, 2);
}

#[tokio::test]
async fn test_batch_delete_with_non_existent_ids() {
    let (_dir, pool) = setup_test_db().await;

    let fav1 = insert_favorite_tx(
        &pool,
        &sample_favorite(1, "repo1", "https://github.com/owner/repo1"),
    )
    .await
    .unwrap();
    let fav2 = insert_favorite_tx(
        &pool,
        &sample_favorite(2, "repo2", "https://github.com/owner/repo2"),
    )
    .await
    .unwrap();

    // Request delete on fav1 and non-existent IDs 9999 and 8888
    let result = delete_favorites_batch_tx(&pool, &[fav1.id, 9999, 8888])
        .await
        .unwrap();

    assert_eq!(result.requested_count, 3);
    assert_eq!(result.deleted_count, 1);
    assert_eq!(result.not_found_count, 2);
    assert_eq!(result.deleted_ids, vec![fav1.id]);

    // fav1 is deleted, fav2 is untouched
    assert!(find_favorite_by_id(&pool, fav1.id).await.unwrap().is_none());
    assert!(find_favorite_by_id(&pool, fav2.id).await.unwrap().is_some());
}

#[tokio::test]
async fn test_batch_delete_deduplicates_duplicate_ids() {
    let (_dir, pool) = setup_test_db().await;

    let fav1 = insert_favorite_tx(
        &pool,
        &sample_favorite(1, "repo1", "https://github.com/owner/repo1"),
    )
    .await
    .unwrap();

    // Request with duplicates
    let result = delete_favorites_batch_tx(&pool, &[fav1.id, fav1.id, fav1.id])
        .await
        .unwrap();

    assert_eq!(result.requested_count, 3);
    assert_eq!(result.deleted_count, 1);
    assert_eq!(result.not_found_count, 0);
    assert_eq!(result.deleted_ids, vec![fav1.id]);
}

#[tokio::test]
async fn test_batch_delete_empty_is_safe_noop() {
    let (_dir, pool) = setup_test_db().await;

    let result = delete_favorites_batch_tx(&pool, &[]).await.unwrap();

    assert_eq!(result.requested_count, 0);
    assert_eq!(result.deleted_count, 0);
    assert_eq!(result.not_found_count, 0);
    assert!(result.deleted_ids.is_empty());
}

#[tokio::test]
async fn test_batch_delete_rejects_invalid_ids() {
    let (_dir, pool) = setup_test_db().await;

    let err = delete_favorites_batch_tx(&pool, &[10, -5, 20])
        .await
        .unwrap_err();

    match err {
        AppError::InvalidFavoriteInput(msg) => {
            assert!(msg.contains("-5"));
        }
        other => panic!("Expected InvalidFavoriteInput, got: {:?}", other),
    }
}

#[tokio::test]
async fn test_deleted_favorite_cannot_be_updated_or_resurrected() {
    let (_dir, pool) = setup_test_db().await;

    let fav = insert_favorite_tx(
        &pool,
        &sample_favorite(1, "repo1", "https://github.com/owner/repo1"),
    )
    .await
    .unwrap();

    // Delete it
    delete_favorites_batch_tx(&pool, &[fav.id]).await.unwrap();

    // Attempting to update user metadata on deleted record fails with FavoriteNotFound
    let update_res = update_user_metadata_tx(
        &pool,
        &UpdateFavoriteUserMetadataInput {
            favorite_id: fav.id,
            category: "Developer Tools".to_string(),
            rating: Some(5),
            rating_reason: None,
            rating_confidence: None,
            notes: "Updated note".to_string(),
            user_tags: vec![],
            ai_tags: None,
            translation_patch: None,
        },
    )
    .await;

    assert!(matches!(update_res, Err(AppError::FavoriteNotFound(_))));
}

#[tokio::test]
async fn test_re_favorite_after_batch_delete_creates_new_identity() {
    let (_dir, pool) = setup_test_db().await;

    let fav1 = insert_favorite_tx(
        &pool,
        &sample_favorite(1, "repo1", "https://github.com/owner/repo1"),
    )
    .await
    .unwrap();
    let original_id = fav1.id;

    // Delete it
    delete_favorites_batch_tx(&pool, &[original_id])
        .await
        .unwrap();

    // User explicitly re-favorites same repository
    let fav2 = insert_favorite_tx(
        &pool,
        &sample_favorite(1, "repo1", "https://github.com/owner/repo1"),
    )
    .await
    .unwrap();

    assert_ne!(fav2.id, original_id);
    assert_eq!(fav2.name, "repo1");

    // Old ID is still not found
    assert!(find_favorite_by_id(&pool, original_id)
        .await
        .unwrap()
        .is_none());
}
