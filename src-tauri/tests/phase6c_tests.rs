use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use tempfile::tempdir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use reposhelf_lib::ai::{
    generate_favorite_suggestion, parse_and_validate_suggestion, AiProviderClient,
};
use reposhelf_lib::db::database::init_pool;
use reposhelf_lib::db::selection_history::count_selection_history;
use reposhelf_lib::error::AppError;
use reposhelf_lib::github::GitHubClient;
use reposhelf_lib::library::favorite_service::{
    apply_favorite_ai_metadata, favorite_repository, get_favorite, suggest_favorite_ai_metadata,
    suggest_favorite_ai_metadata_with_client, update_favorite_metadata,
};
use reposhelf_lib::library::refresh::refresh_single;
use reposhelf_lib::library::{FavoriteInput, UpdateFavoriteUserMetadataInput};
use reposhelf_lib::secrets::{InMemorySecretStore, SecretStore, SECRET_KEY_AI_API_KEY};
use reposhelf_lib::settings::{AiProvider, SettingsStore};

struct MockAiClient {
    responses: Mutex<Vec<Result<String, AppError>>>,
    recorded_calls: Mutex<Vec<(String, String)>>,
}

impl MockAiClient {
    fn new(responses: Vec<Result<String, AppError>>) -> Self {
        Self {
            responses: Mutex::new(responses),
            recorded_calls: Mutex::new(Vec::new()),
        }
    }

    fn call_count(&self) -> usize {
        self.recorded_calls.lock().unwrap().len()
    }

    fn last_prompt(&self) -> (String, String) {
        self.recorded_calls
            .lock()
            .unwrap()
            .last()
            .cloned()
            .expect("No prompt recorded")
    }
}

impl AiProviderClient for MockAiClient {
    fn provider_name(&self) -> &'static str {
        "MockAiClient"
    }

    fn model(&self) -> &str {
        "mock-model"
    }

    fn generate_text<'a>(
        &'a self,
        system_instruction: &'a str,
        user_input: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String, AppError>> + Send + 'a>>
    {
        self.recorded_calls
            .lock()
            .unwrap()
            .push((system_instruction.to_string(), user_input.to_string()));

        let resp = self.responses.lock().unwrap().remove(0);

        Box::pin(async move { resp })
    }
}

async fn setup_test_db() -> (tempfile::TempDir, sqlx::SqlitePool) {
    let dir = tempdir().expect("Failed to create tempdir");
    let db_path = dir.path().join("test_p6c.db");
    let pool = init_pool(&db_path)
        .await
        .expect("Failed to initialize test DB pool");
    (dir, pool)
}

fn create_test_settings_and_secrets() -> (
    tempfile::TempDir,
    Arc<SettingsStore>,
    Arc<InMemorySecretStore>,
) {
    let dir = tempdir().expect("Failed to create tempdir");
    let config_path = dir.path().join("settings.json");
    let settings_store =
        Arc::new(SettingsStore::new(config_path).expect("Failed to init settings store"));
    let secret_store = Arc::new(InMemorySecretStore::new());

    // Configure ready settings
    let mut settings = settings_store.get().unwrap();
    settings.ai.enabled = true;
    settings.ai.provider = Some(AiProvider::OpenaiCompatible);
    settings.ai.model = Some("gpt-4o-mini".to_string());
    settings_store.save(&settings).unwrap();
    secret_store
        .set_secret(SECRET_KEY_AI_API_KEY, "test-ai-key")
        .unwrap();

    (dir, settings_store, secret_store)
}

// =============================================================================
// Phase 6-C Acceptance Criteria Tests
// =============================================================================

#[tokio::test]
async fn test_p6c_01_ai_not_ready_rejected_before_ai_call() {
    let (_db_dir, pool) = setup_test_db().await;
    let (_set_dir, settings_store, secret_store) = create_test_settings_and_secrets();

    // Disable AI in settings
    let mut settings = settings_store.get().unwrap();
    settings.ai.enabled = false;
    settings_store.save(&settings).unwrap();

    let fav = favorite_repository(
        &pool,
        FavoriteInput {
            github_repo_id: Some(101),
            github_url: "https://github.com/test-owner/test-repo".to_string(),
            name: "test-repo".to_string(),
            description: Some("Test repo".to_string()),
            category: "Other".to_string(),
            stars: 100,
            license: Some("MIT".to_string()),
            topics: vec!["rust".to_string()],
            pushed_at: None,
        },
    )
    .await
    .unwrap();

    let err = suggest_favorite_ai_metadata(&pool, &settings_store, &*secret_store, fav.id)
        .await
        .unwrap_err();

    assert_eq!(err.code(), "SettingsValidation");
    assert!(err.to_string().contains("AI 功能未启用") || err.to_string().contains("disabled"));
}

#[tokio::test]
async fn test_p6c_02_ollama_ready_without_key() {
    let (_db_dir, _pool) = setup_test_db().await;
    let (_set_dir, settings_store, secret_store) = create_test_settings_and_secrets();

    // Configure Ollama and remove any key
    let mut settings = settings_store.get().unwrap();
    settings.ai.enabled = true;
    settings.ai.provider = Some(AiProvider::Ollama);
    settings.ai.model = Some("llama3.2".to_string());
    settings_store.save(&settings).unwrap();
    secret_store.delete_secret(SECRET_KEY_AI_API_KEY).unwrap();

    // AI readiness check directly
    let readiness = reposhelf_lib::ai::check_ai_search_readiness(&settings, &*secret_store);
    assert!(readiness.ready);
    assert!(readiness.reason.is_none());
}

#[tokio::test]
async fn test_p6c_03_favorite_not_found_before_ai_call() {
    let (_db_dir, pool) = setup_test_db().await;
    let (_set_dir, settings_store, secret_store) = create_test_settings_and_secrets();

    let err = suggest_favorite_ai_metadata(&pool, &settings_store, &*secret_store, 999999)
        .await
        .unwrap_err();

    assert_eq!(err.code(), "FavoriteNotFound");
}

#[tokio::test]
async fn test_p6c_04_to_10_sentinel_privacy_and_allowed_data() {
    let (_db_dir, pool) = setup_test_db().await;

    // Create favorite with distinctive sentinel data
    let fav = favorite_repository(
        &pool,
        FavoriteInput {
            github_repo_id: Some(888),
            github_url: "https://github.com/secret-org/allowed-name".to_string(),
            name: "allowed-name".to_string(),
            description: Some("allowed-description-text".to_string()),
            category: "Other".to_string(),
            stars: 500,
            license: Some("MIT".to_string()),
            topics: vec!["allowed-github-topic".to_string()],
            pushed_at: None,
        },
    )
    .await
    .unwrap();

    // Add private user notes, rating, and user tags
    update_favorite_metadata(
        &pool,
        UpdateFavoriteUserMetadataInput {
            favorite_id: fav.id,
            category: "Other".to_string(),
            rating: Some(4),
            notes: "PRIVATE_NOTES_DO_NOT_SEND".to_string(),
            user_tags: vec!["PRIVATE_USER_TAG_DO_NOT_SEND".to_string()],
        },
    )
    .await
    .unwrap();

    // Add an old AI tag directly into database
    sqlx::query("INSERT INTO favorite_tags (favorite_id, tag, source) VALUES (?, ?, 'ai')")
        .bind(fav.id)
        .bind("PRIVATE_OLD_AI_TAG_DO_NOT_SEND")
        .execute(&pool)
        .await
        .unwrap();

    // Mock client to inspect the prompt received
    let mock_client = MockAiClient::new(vec![Ok(
        r#"{"category":"Developer Tools","tags":["rust","cli"]}"#.to_string(),
    )]);

    let suggestion = suggest_favorite_ai_metadata_with_client(&pool, &mock_client, fav.id)
        .await
        .unwrap();

    assert_eq!(suggestion.category, "Developer Tools");
    assert_eq!(mock_client.call_count(), 1);

    let (system_instruction, user_content) = mock_client.last_prompt();

    // P6C-04: provider receives name
    assert!(user_content.contains("<name>allowed-name</name>"));

    // P6C-05: provider receives description
    assert!(user_content.contains("<description>allowed-description-text</description>"));

    // P6C-06: provider receives only source='github' topics
    assert!(user_content.contains("<topic>allowed-github-topic</topic>"));

    // P6C-07: provider request excludes notes
    assert!(!user_content.contains("PRIVATE_NOTES_DO_NOT_SEND"));
    assert!(!system_instruction.contains("PRIVATE_NOTES_DO_NOT_SEND"));

    // P6C-08: provider request excludes rating
    assert!(!user_content.contains("rating"));
    assert!(!user_content.contains("<rating>"));

    // P6C-09: provider request excludes user tags
    assert!(!user_content.contains("PRIVATE_USER_TAG_DO_NOT_SEND"));

    // P6C-10: provider request excludes existing AI tags
    assert!(!user_content.contains("PRIVATE_OLD_AI_TAG_DO_NOT_SEND"));

    // Hard boundary: no URLs, no repo IDs, no DB IDs, no API keys
    assert!(!user_content.contains("https://github.com"));
    assert!(!user_content.contains("888"));
    assert!(!user_content.contains("test-ai-key"));
}

#[test]
fn test_p6c_11_valid_category_and_0_tags_accepted() {
    let raw = r#"{"category":"AI & Agents","tags":[]}"#;
    let res = parse_and_validate_suggestion(raw).unwrap();
    assert_eq!(res.category, "AI & Agents");
    assert!(res.tags.is_empty());
}

#[test]
fn test_p6c_12_valid_category_and_5_tags_accepted() {
    let raw = r#"{"category":"Developer Tools","tags":["t1","t2","t3","t4","t5"]}"#;
    let res = parse_and_validate_suggestion(raw).unwrap();
    assert_eq!(res.category, "Developer Tools");
    assert_eq!(res.tags.len(), 5);
}

#[test]
fn test_p6c_13_invalid_category_rejected() {
    let raw = r#"{"category":"NonExistentCategory","tags":["tag1"]}"#;
    let err = parse_and_validate_suggestion(raw).unwrap_err();
    assert!(err.contains("Invalid category"));
}

#[test]
fn test_p6c_14_more_than_5_tags_rejected() {
    let raw = r#"{"category":"Automation","tags":["1","2","3","4","5","6"]}"#;
    let err = parse_and_validate_suggestion(raw).unwrap_err();
    assert!(err.contains("At most 5 tags allowed"));
}

#[test]
fn test_p6c_15_blank_tag_rejected() {
    let raw = r#"{"category":"Automation","tags":["scraping","   ","bot"]}"#;
    let err = parse_and_validate_suggestion(raw).unwrap_err();
    assert!(err.contains("cannot be empty"));
}

#[test]
fn test_p6c_16_tag_over_40_chars_rejected() {
    let long_tag = "a".repeat(41);
    let raw = format!(
        r#"{{"category":"Automation","tags":["valid","{}"]}}"#,
        long_tag
    );
    let err = parse_and_validate_suggestion(&raw).unwrap_err();
    assert!(err.contains("exceeds 40 characters limit"));
}

#[test]
fn test_p6c_17_multiline_tag_rejected() {
    let raw = r#"{"category":"Automation","tags":["valid\ntag"]}"#;
    let err = parse_and_validate_suggestion(raw).unwrap_err();
    assert!(err.contains("single-line"));
}

#[test]
fn test_p6c_18_case_insensitive_duplicate_tags_deduplicated() {
    let raw = r#"{"category":"Productivity","tags":["Notes","notes","NOTES","Wiki","wiki"]}"#;
    let res = parse_and_validate_suggestion(raw).unwrap();
    assert_eq!(res.tags, vec!["Notes", "Wiki"]);
}

#[test]
fn test_p6c_19_outer_json_fence_accepted() {
    let raw = "```json\n{\"category\":\"Automation\",\"tags\":[\"scraping\"]}\n```";
    let res = parse_and_validate_suggestion(raw).unwrap();
    assert_eq!(res.category, "Automation");
    assert_eq!(res.tags, vec!["scraping"]);
}

#[test]
fn test_p6c_20_arbitrary_prose_and_embedded_json_rejected() {
    let raw = "Here is your suggestion:\n{\"category\":\"Automation\",\"tags\":[\"scraping\"]}\nHope this helps!";
    let err = parse_and_validate_suggestion(raw).unwrap_err();
    assert!(err.contains("Failed to parse JSON"));
}

#[tokio::test]
async fn test_p6c_21_invalid_first_response_and_valid_repair_success() {
    let bad_response = Ok(r#"{"category":"InvalidCategory","tags":["a"]}"#.to_string());
    let good_response = Ok(r#"{"category":"Developer Tools","tags":["git","cli"]}"#.to_string());

    let mock_client = MockAiClient::new(vec![bad_response, good_response]);
    let suggestion = generate_favorite_suggestion(
        &mock_client,
        "git-helper",
        Some("Git CLI helper"),
        &["git".to_string()],
    )
    .await
    .unwrap();

    assert_eq!(suggestion.category, "Developer Tools");
    assert_eq!(suggestion.tags, vec!["git", "cli"]);
    assert_eq!(mock_client.call_count(), 2);
}

#[tokio::test]
async fn test_p6c_22_and_23_invalid_first_and_invalid_repair_max_2_calls() {
    let bad1 = Ok("Not json".to_string());
    let bad2 = Ok("Still not json".to_string());

    let mock_client = MockAiClient::new(vec![bad1, bad2]);
    let err = generate_favorite_suggestion(&mock_client, "repo", Some("desc"), &[])
        .await
        .unwrap_err();

    assert_eq!(err.code(), "AIInvalidResponse");
    assert_eq!(mock_client.call_count(), 2);
}

#[tokio::test]
async fn test_p6c_24_provider_transport_error_does_not_trigger_repair() {
    let auth_error = Err(AppError::AIAuth("Invalid API Key".to_string()));

    let mock_client = MockAiClient::new(vec![auth_error]);
    let err = generate_favorite_suggestion(&mock_client, "repo", Some("desc"), &[])
        .await
        .unwrap_err();

    assert_eq!(err.code(), "AIAuth");
    assert_eq!(mock_client.call_count(), 1);
}

#[tokio::test]
async fn test_p6c_25_and_26_suggestion_writes_zero_db_changes_and_zero_history() {
    let (_db_dir, pool) = setup_test_db().await;

    let fav = favorite_repository(
        &pool,
        FavoriteInput {
            github_repo_id: Some(555),
            github_url: "https://github.com/owner/zero-mutation".to_string(),
            name: "zero-mutation".to_string(),
            description: Some("desc".to_string()),
            category: "Other".to_string(),
            stars: 10,
            license: None,
            topics: vec!["topic1".to_string()],
            pushed_at: None,
        },
    )
    .await
    .unwrap();

    let initial_history_count = count_selection_history(&pool).await.unwrap();

    let mock_client = MockAiClient::new(vec![Ok(
        r#"{"category":"Multimedia","tags":["video","ffmpeg"]}"#.to_string(),
    )]);

    // Execute suggestion
    let suggestion = suggest_favorite_ai_metadata_with_client(&pool, &mock_client, fav.id)
        .await
        .unwrap();

    assert_eq!(suggestion.category, "Multimedia");

    // Verify DB state after suggestion is completely identical
    let fav_after = get_favorite(&pool, fav.id).await.unwrap();
    assert_eq!(fav_after.category, "Other"); // untouched!
    assert_eq!(fav_after.tags.len(), 1);
    assert_eq!(fav_after.tags[0].source, "github");
    assert_eq!(fav_after.tags[0].tag, "topic1");

    let history_after = count_selection_history(&pool).await.unwrap();
    assert_eq!(history_after, initial_history_count);
}

#[tokio::test]
async fn test_p6c_27_to_37_apply_valid_suggestion_and_preservation_contract() {
    let (_db_dir, pool) = setup_test_db().await;

    let initial_fav = favorite_repository(
        &pool,
        FavoriteInput {
            github_repo_id: Some(777),
            github_url: "https://github.com/preserved/repo".to_string(),
            name: "repo".to_string(),
            description: Some("original description".to_string()),
            category: "Other".to_string(),
            stars: 1234,
            license: Some("MIT".to_string()),
            topics: vec!["gh-topic".to_string()],
            pushed_at: Some("2026-03-01T00:00:00Z".to_string()),
        },
    )
    .await
    .unwrap();

    // Add user metadata
    update_favorite_metadata(
        &pool,
        UpdateFavoriteUserMetadataInput {
            favorite_id: initial_fav.id,
            category: "Other".to_string(),
            rating: Some(5),
            notes: "Important user note".to_string(),
            user_tags: vec!["my-custom-tag".to_string()],
        },
    )
    .await
    .unwrap();

    // Add existing AI tag
    sqlx::query("INSERT INTO favorite_tags (favorite_id, tag, source) VALUES (?, ?, 'ai')")
        .bind(initial_fav.id)
        .bind("old-ai-tag")
        .execute(&pool)
        .await
        .unwrap();

    let before_apply = get_favorite(&pool, initial_fav.id).await.unwrap();
    let initial_history = count_selection_history(&pool).await.unwrap();

    // P6C-27: Apply valid suggestion updates category
    // P6C-28: Apply stores tags as source='ai'
    // P6C-29: Apply replaces previous AI tags
    let applied = apply_favorite_ai_metadata(
        &pool,
        initial_fav.id,
        "Automation".to_string(),
        vec!["crawler".to_string(), "scraper".to_string()],
    )
    .await
    .unwrap();

    assert_eq!(applied.category, "Automation");

    let ai_tags: Vec<String> = applied
        .tags
        .iter()
        .filter(|t| t.source == "ai")
        .map(|t| t.tag.clone())
        .collect();
    assert_eq!(ai_tags, vec!["crawler", "scraper"]);

    // Old AI tag replaced
    assert!(!applied.tags.iter().any(|t| t.tag == "old-ai-tag"));

    // P6C-31: Preserves source='github' tags
    let gh_tags: Vec<String> = applied
        .tags
        .iter()
        .filter(|t| t.source == "github")
        .map(|t| t.tag.clone())
        .collect();
    assert_eq!(gh_tags, vec!["gh-topic"]);

    // P6C-32: Preserves source='user' tags
    let user_tags: Vec<String> = applied
        .tags
        .iter()
        .filter(|t| t.source == "user")
        .map(|t| t.tag.clone())
        .collect();
    assert_eq!(user_tags, vec!["my-custom-tag"]);

    // P6C-33: Preserves rating
    assert_eq!(applied.rating, Some(5));

    // P6C-34: Preserves notes
    assert_eq!(applied.notes, "Important user note");

    // P6C-35: Preserves favorited_at
    assert_eq!(applied.favorited_at, before_apply.favorited_at);

    // P6C-36: Preserves GitHub-owned metadata
    assert_eq!(applied.github_repo_id, Some(777));
    assert_eq!(applied.github_url, "https://github.com/preserved/repo");
    assert_eq!(applied.name, "repo");
    assert_eq!(applied.description.as_deref(), Some("original description"));
    assert_eq!(applied.stars, 1234);
    assert_eq!(applied.license.as_deref(), Some("MIT"));
    assert_eq!(applied.pushed_at.as_deref(), Some("2026-03-01T00:00:00Z"));

    // P6C-37: Apply does not alter selection_history
    assert_eq!(
        count_selection_history(&pool).await.unwrap(),
        initial_history
    );
}

#[tokio::test]
async fn test_p6c_30_apply_empty_tags_clears_previous_ai_tags() {
    let (_db_dir, pool) = setup_test_db().await;

    let fav = favorite_repository(
        &pool,
        FavoriteInput {
            github_repo_id: Some(300),
            github_url: "https://github.com/owner/empty-ai-tags".to_string(),
            name: "empty-ai-tags".to_string(),
            description: None,
            category: "Other".to_string(),
            stars: 50,
            license: None,
            topics: vec!["topic".to_string()],
            pushed_at: None,
        },
    )
    .await
    .unwrap();

    // Apply with tags first
    apply_favorite_ai_metadata(
        &pool,
        fav.id,
        "Games".to_string(),
        vec!["gamedev".to_string()],
    )
    .await
    .unwrap();

    // Now apply with empty tags
    let applied_empty = apply_favorite_ai_metadata(&pool, fav.id, "Games".to_string(), vec![])
        .await
        .unwrap();

    let ai_tags: Vec<String> = applied_empty
        .tags
        .iter()
        .filter(|t| t.source == "ai")
        .map(|t| t.tag.clone())
        .collect();
    assert!(ai_tags.is_empty());

    let gh_tags: Vec<String> = applied_empty
        .tags
        .iter()
        .filter(|t| t.source == "github")
        .map(|t| t.tag.clone())
        .collect();
    assert_eq!(gh_tags, vec!["topic"]);
}

#[tokio::test]
async fn test_p6c_38_apply_transaction_rollback_is_atomic() {
    let (_db_dir, pool) = setup_test_db().await;

    let fav = favorite_repository(
        &pool,
        FavoriteInput {
            github_repo_id: Some(400),
            github_url: "https://github.com/owner/rollback-test".to_string(),
            name: "rollback-test".to_string(),
            description: None,
            category: "Other".to_string(),
            stars: 10,
            license: None,
            topics: vec!["gh".to_string()],
            pushed_at: None,
        },
    )
    .await
    .unwrap();

    // Attempt apply with invalid category -> rejected before DB writes
    let err = apply_favorite_ai_metadata(
        &pool,
        fav.id,
        "InvalidCategory".to_string(),
        vec!["tag1".to_string()],
    )
    .await
    .unwrap_err();

    assert_eq!(err.code(), "InvalidCategory");

    let current = get_favorite(&pool, fav.id).await.unwrap();
    assert_eq!(current.category, "Other");
    assert_eq!(current.tags.len(), 1);
}

#[tokio::test]
async fn test_p6c_39_invalid_apply_category_rejected_with_zero_mutation() {
    let (_db_dir, pool) = setup_test_db().await;
    let fav = favorite_repository(
        &pool,
        FavoriteInput {
            github_repo_id: Some(401),
            github_url: "https://github.com/owner/cat-test".to_string(),
            name: "cat-test".to_string(),
            description: None,
            category: "Productivity".to_string(),
            stars: 10,
            license: None,
            topics: vec![],
            pushed_at: None,
        },
    )
    .await
    .unwrap();

    let err = apply_favorite_ai_metadata(
        &pool,
        fav.id,
        "TotallyBogusCategory".to_string(),
        vec!["tag".to_string()],
    )
    .await
    .unwrap_err();

    assert_eq!(err.code(), "InvalidCategory");

    let fav_after = get_favorite(&pool, fav.id).await.unwrap();
    assert_eq!(fav_after.category, "Productivity");
    assert!(fav_after.tags.is_empty());
}

#[tokio::test]
async fn test_p6c_40_invalid_apply_tags_rejected_with_zero_mutation() {
    let (_db_dir, pool) = setup_test_db().await;
    let fav = favorite_repository(
        &pool,
        FavoriteInput {
            github_repo_id: Some(402),
            github_url: "https://github.com/owner/tags-test".to_string(),
            name: "tags-test".to_string(),
            description: None,
            category: "Productivity".to_string(),
            stars: 10,
            license: None,
            topics: vec![],
            pushed_at: None,
        },
    )
    .await
    .unwrap();

    // 1. More than 5 tags
    let err1 = apply_favorite_ai_metadata(
        &pool,
        fav.id,
        "Productivity".to_string(),
        vec![
            "1".into(),
            "2".into(),
            "3".into(),
            "4".into(),
            "5".into(),
            "6".into(),
        ],
    )
    .await
    .unwrap_err();
    assert_eq!(err1.code(), "InvalidFavoriteInput");

    // 2. Empty tag
    let err2 = apply_favorite_ai_metadata(
        &pool,
        fav.id,
        "Productivity".to_string(),
        vec!["   ".into()],
    )
    .await
    .unwrap_err();
    assert_eq!(err2.code(), "InvalidFavoriteInput");

    // 3. > 40 chars tag
    let err3 = apply_favorite_ai_metadata(
        &pool,
        fav.id,
        "Productivity".to_string(),
        vec!["x".repeat(45)],
    )
    .await
    .unwrap_err();
    assert_eq!(err3.code(), "InvalidFavoriteInput");

    // Verify zero mutations
    let fav_after = get_favorite(&pool, fav.id).await.unwrap();
    assert_eq!(fav_after.category, "Productivity");
    assert!(fav_after.tags.is_empty());
}

#[tokio::test]
async fn test_p6c_41_missing_favorite_during_apply_returns_favorite_not_found() {
    let (_db_dir, pool) = setup_test_db().await;
    let err = apply_favorite_ai_metadata(
        &pool,
        1234567,
        "Developer Tools".to_string(),
        vec!["test".to_string()],
    )
    .await
    .unwrap_err();

    assert_eq!(err.code(), "FavoriteNotFound");
}

#[tokio::test]
async fn test_p6c_42_to_44_applied_ai_category_and_tags_survive_github_refresh() {
    let (_db_dir, pool) = setup_test_db().await;
    let mock_server = MockServer::start().await;

    // 1. Create favorite with initial metadata
    let fav = favorite_repository(
        &pool,
        FavoriteInput {
            github_repo_id: Some(999),
            github_url: "https://github.com/refresh-owner/ai-refresh-repo".to_string(),
            name: "ai-refresh-repo".to_string(),
            description: Some("Old desc".to_string()),
            category: "Other".to_string(),
            stars: 100,
            license: Some("MIT".to_string()),
            topics: vec!["old-github-topic".to_string()],
            pushed_at: None,
        },
    )
    .await
    .unwrap();

    // 2. Apply AI metadata
    apply_favorite_ai_metadata(
        &pool,
        fav.id,
        "Security & Privacy".to_string(),
        vec!["crypto".to_string(), "vault".to_string()],
    )
    .await
    .unwrap();

    // 3. Mock GitHub API with updated stars, description, and topics
    Mock::given(method("GET"))
        .and(path("/repos/refresh-owner/ai-refresh-repo"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": 999,
            "html_url": "https://github.com/refresh-owner/ai-refresh-repo",
            "name": "ai-refresh-repo",
            "description": "New description from GitHub",
            "stargazers_count": 9999,
            "license": { "spdx_id": "Apache-2.0", "name": "Apache 2.0" },
            "topics": ["new-github-topic-1", "new-github-topic-2"],
            "pushed_at": "2026-03-08T00:00:00Z",
            "private": false
        })))
        .mount(&mock_server)
        .await;

    let github = GitHubClient::with_base_url(&mock_server.uri());

    // 4. Perform GitHub Refresh
    let refresh_res = refresh_single(&pool, &github, None, fav.id).await.unwrap();
    assert_eq!(refresh_res.status, "succeeded");

    let refreshed = refresh_res.favorite.expect("Refreshed favorite present");

    // P6C-42: Applied Category survives GitHub Refresh
    assert_eq!(refreshed.category, "Security & Privacy");

    // P6C-43: Applied AI tags survive GitHub Refresh
    let ai_tags: Vec<String> = refreshed
        .tags
        .iter()
        .filter(|t| t.source == "ai")
        .map(|t| t.tag.clone())
        .collect();
    assert_eq!(ai_tags, vec!["crypto", "vault"]);

    // P6C-44: Refresh still replaces source='github' tags normally
    let gh_tags: Vec<String> = refreshed
        .tags
        .iter()
        .filter(|t| t.source == "github")
        .map(|t| t.tag.clone())
        .collect();
    assert_eq!(gh_tags, vec!["new-github-topic-1", "new-github-topic-2"]);
    assert!(!gh_tags.contains(&"old-github-topic".to_string()));

    // GitHub-owned fields updated
    assert_eq!(refreshed.stars, 9999);
    assert_eq!(
        refreshed.description.as_deref(),
        Some("New description from GitHub")
    );
    assert_eq!(refreshed.license.as_deref(), Some("Apache-2.0"));
}

#[tokio::test]
async fn test_p6c_45_same_tag_text_across_sources_remains_valid() {
    let (_db_dir, pool) = setup_test_db().await;

    let fav = favorite_repository(
        &pool,
        FavoriteInput {
            github_repo_id: Some(600),
            github_url: "https://github.com/owner/multi-source-tag".to_string(),
            name: "multi-source-tag".to_string(),
            description: None,
            category: "Other".to_string(),
            stars: 10,
            license: None,
            topics: vec!["rust".to_string()], // github: rust
            pushed_at: None,
        },
    )
    .await
    .unwrap();

    // Add user tag "rust"
    update_favorite_metadata(
        &pool,
        UpdateFavoriteUserMetadataInput {
            favorite_id: fav.id,
            category: "Other".to_string(),
            rating: None,
            notes: "".to_string(),
            user_tags: vec!["rust".to_string()], // user: rust
        },
    )
    .await
    .unwrap();

    // Apply AI tag "rust"
    let applied = apply_favorite_ai_metadata(
        &pool,
        fav.id,
        "Developer Tools".to_string(),
        vec!["rust".to_string()], // ai: rust
    )
    .await
    .unwrap();

    let rust_tags: Vec<_> = applied
        .tags
        .iter()
        .filter(|t| t.tag.eq_ignore_ascii_case("rust"))
        .collect();

    assert_eq!(rust_tags.len(), 3);
    let sources: HashSet<String> = rust_tags.iter().map(|t| t.source.clone()).collect();
    assert!(sources.contains("github"));
    assert!(sources.contains("user"));
    assert!(sources.contains("ai"));
}

#[tokio::test]
async fn test_p6c_46_no_migration_or_schema_change() {
    let (_db_dir, pool) = setup_test_db().await;

    // Verify exactly 1 migration exists (001_initial_schema)
    let migrations: Vec<(i64,)> = sqlx::query_as("SELECT version FROM _sqlx_migrations")
        .fetch_all(&pool)
        .await
        .unwrap();

    assert_eq!(migrations.len(), 1);
    assert_eq!(migrations[0].0, 1);
}

#[tokio::test]
async fn test_p6c_47_no_add_repository_ai_suggestion() {
    let (_db_dir, pool) = setup_test_db().await;

    // Normal favorite_repository call from AddRepositoryPage does not invoke AI
    // and produces only source='github' tags.
    let fav = favorite_repository(
        &pool,
        FavoriteInput {
            github_repo_id: Some(12345),
            github_url: "https://github.com/owner/add-manual".to_string(),
            name: "add-manual".to_string(),
            description: Some("manual desc".to_string()),
            category: "Developer Tools".to_string(),
            stars: 100,
            license: Some("MIT".to_string()),
            topics: vec!["cli".to_string()],
            pushed_at: None,
        },
    )
    .await
    .unwrap();

    // Verify no AI tags created automatically
    assert!(fav.tags.iter().all(|t| t.source != "ai"));
    assert_eq!(fav.category, "Developer Tools");
}

#[tokio::test]
async fn test_p6c_48_no_search_or_trending_automatic_ai_suggestion() {
    let (_db_dir, pool) = setup_test_db().await;

    // Standard favorite creation from Search or Trending does not automatically populate AI suggestions
    let fav = favorite_repository(
        &pool,
        FavoriteInput {
            github_repo_id: Some(67890),
            github_url: "https://github.com/owner/search-trending-item".to_string(),
            name: "search-trending-item".to_string(),
            description: None,
            category: "Web & SaaS".to_string(),
            stars: 200,
            license: None,
            topics: vec!["web".to_string()],
            pushed_at: None,
        },
    )
    .await
    .unwrap();

    let loaded = get_favorite(&pool, fav.id).await.unwrap();
    assert_eq!(loaded.category, "Web & SaaS");
    assert!(loaded.tags.iter().all(|t| t.source != "ai"));
}
