use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tempfile::tempdir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use reposhelf_lib::ai::{
    generate_favorite_suggestion, parse_and_validate_suggestion, AiProviderClient,
    GenerateTextOptions, OllamaClient,
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
    recorded_calls: Mutex<Vec<(String, String, GenerateTextOptions)>>,
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
        let (sys, user, _) = self
            .recorded_calls
            .lock()
            .unwrap()
            .last()
            .cloned()
            .expect("No prompt recorded");
        (sys, user)
    }

    fn last_options(&self) -> GenerateTextOptions {
        let (_, _, opt) = self
            .recorded_calls
            .lock()
            .unwrap()
            .last()
            .cloned()
            .expect("No prompt recorded");
        opt
    }
}

impl AiProviderClient for MockAiClient {
    fn provider_name(&self) -> &'static str {
        "MockAiClient"
    }

    fn model(&self) -> &str {
        "mock-model"
    }

    fn generate_text_with_options<'a>(
        &'a self,
        system_instruction: &'a str,
        user_input: &'a str,
        options: GenerateTextOptions,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String, AppError>> + Send + 'a>>
    {
        self.recorded_calls.lock().unwrap().push((
            system_instruction.to_string(),
            user_input.to_string(),
            options,
        ));

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
            releases: None,
            languages: None,
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
            releases: None,
            languages: None,
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
            rating_reason: None,
            rating_confidence: None,
            notes: "PRIVATE_NOTES_DO_NOT_SEND".to_string(),
            user_tags: vec!["PRIVATE_USER_TAG_DO_NOT_SEND".to_string()],
            ai_tags: None,
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
        r#"{"category":"Developer Tools","tags":["rust","cli"],"rating":5,"rating_reason":"High quality Rust CLI tool","confidence":"high"}"#.to_string(),
    )]);

    let suggestion = suggest_favorite_ai_metadata_with_client(&pool, &mock_client, fav.id)
        .await
        .unwrap();

    assert_eq!(suggestion.category, "Developer Tools");
    assert_eq!(suggestion.tags, vec!["rust", "cli"]);
    assert_eq!(suggestion.rating, 5);
    assert_eq!(suggestion.rating_reason, "High quality Rust CLI tool");
    assert_eq!(suggestion.confidence, "high");
    assert_eq!(mock_client.call_count(), 1);

    let options = mock_client.last_options();
    assert_eq!(options.think, Some(false));
    assert_eq!(options.max_tokens, Some(512));
    assert_eq!(options.temperature, Some(0.2));

    let (system_instruction, user_content) = mock_client.last_prompt();

    // P6C-04: provider receives name
    assert!(user_content.contains("<name>allowed-name</name>"));

    // P6C-05: provider receives description
    assert!(user_content.contains("<description>allowed-description-text</description>"));

    // P6C-06: provider receives only source='github' topics
    assert!(user_content.contains("<topic>allowed-github-topic</topic>"));

    // Metadata passed for rating context
    assert!(user_content.contains("<stars>500</stars>"));
    assert!(user_content.contains("<license>MIT</license>"));

    // P6C-07: provider request excludes notes
    assert!(!user_content.contains("PRIVATE_NOTES_DO_NOT_SEND"));
    assert!(!system_instruction.contains("PRIVATE_NOTES_DO_NOT_SEND"));

    // P6C-08: provider request excludes user's private rating
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
    let raw = r#"{"category":"AI & Agents","tags":[],"rating":3,"rating_reason":"Decent agent framework","confidence":"medium"}"#;
    let res = parse_and_validate_suggestion(raw).unwrap();
    assert_eq!(res.category, "AI & Agents");
    assert!(res.tags.is_empty());
    assert_eq!(res.rating, 3);
    assert_eq!(res.rating_reason, "Decent agent framework");
    assert_eq!(res.confidence, "medium");
}

#[test]
fn test_p6c_12_valid_category_and_5_tags_accepted() {
    let raw = r#"{"category":"Developer Tools","tags":["t1","t2","t3","t4","t5"],"rating":5,"rating_reason":"High utility developer tool","confidence":"high"}"#;
    let res = parse_and_validate_suggestion(raw).unwrap();
    assert_eq!(res.category, "Developer Tools");
    assert_eq!(res.tags.len(), 5);
    assert_eq!(res.rating, 5);
    assert_eq!(res.confidence, "high");
}

#[test]
fn test_p6c_13_invalid_category_rejected() {
    let raw = r#"{"category":"NonExistentCategory","tags":["tag1"],"rating":3,"rating_reason":"Reason","confidence":"low"}"#;
    let err = parse_and_validate_suggestion(raw).unwrap_err();
    assert!(err.contains("Invalid category"));
}

#[test]
fn test_p6c_14_more_than_5_tags_rejected() {
    let raw = r#"{"category":"Automation","tags":["1","2","3","4","5","6"],"rating":3,"rating_reason":"Reason","confidence":"low"}"#;
    let err = parse_and_validate_suggestion(raw).unwrap_err();
    assert!(err.contains("At most 5 tags allowed"));
}

#[test]
fn test_p6c_15_blank_tag_rejected() {
    let raw = r#"{"category":"Automation","tags":["scraping","   ","bot"],"rating":3,"rating_reason":"Reason","confidence":"low"}"#;
    let err = parse_and_validate_suggestion(raw).unwrap_err();
    assert!(err.contains("cannot be empty"));
}

#[test]
fn test_p6c_16_tag_over_40_chars_rejected() {
    let long_tag = "a".repeat(41);
    let raw = format!(
        r#"{{"category":"Automation","tags":["valid","{}"],"rating":3,"rating_reason":"Reason","confidence":"low"}}"#,
        long_tag
    );
    let err = parse_and_validate_suggestion(&raw).unwrap_err();
    assert!(err.contains("exceeds 40 characters limit"));
}

#[test]
fn test_p6c_17_multiline_tag_rejected() {
    let raw = r#"{"category":"Automation","tags":["valid\ntag"],"rating":3,"rating_reason":"Reason","confidence":"low"}"#;
    let err = parse_and_validate_suggestion(raw).unwrap_err();
    assert!(err.contains("single-line"));
}

#[test]
fn test_p6c_18_case_insensitive_duplicate_tags_deduplicated() {
    let raw = r#"{"category":"Productivity","tags":["Notes","notes","NOTES","Wiki","wiki"],"rating":4,"rating_reason":"Helpful wiki","confidence":"medium"}"#;
    let res = parse_and_validate_suggestion(raw).unwrap();
    assert_eq!(res.tags, vec!["Notes", "Wiki"]);
}

#[test]
fn test_p6c_19_outer_json_fence_accepted() {
    let raw = "```json\n{\"category\":\"Automation\",\"tags\":[\"scraping\"],\"rating\":4,\"rating_reason\":\"Scraper tool\",\"confidence\":\"high\"}\n```";
    let res = parse_and_validate_suggestion(raw).unwrap();
    assert_eq!(res.category, "Automation");
    assert_eq!(res.tags, vec!["scraping"]);
    assert_eq!(res.rating, 4);
    assert_eq!(res.rating_reason, "Scraper tool");
    assert_eq!(res.confidence, "high");
}

#[test]
fn test_p6c_20_arbitrary_prose_and_embedded_json_rejected() {
    let raw = "Here is your suggestion:\n{\"category\":\"Automation\",\"tags\":[\"scraping\"],\"rating\":4,\"rating_reason\":\"Scraper tool\",\"confidence\":\"high\"}\nHope this helps!";
    let err = parse_and_validate_suggestion(raw).unwrap_err();
    assert!(err.contains("Failed to parse JSON"));
}

#[test]
fn test_p6c_rating_boundaries_accepted() {
    let raw_1 = r#"{"category":"Developer Tools","tags":["cli"],"rating":1,"rating_reason":"Early stage prototype","confidence":"low"}"#;
    let res_1 = parse_and_validate_suggestion(raw_1).unwrap();
    assert_eq!(res_1.rating, 1);

    let raw_5 = r#"{"category":"Developer Tools","tags":["cli"],"rating":5,"rating_reason":"Industry standard tool","confidence":"high"}"#;
    let res_5 = parse_and_validate_suggestion(raw_5).unwrap();
    assert_eq!(res_5.rating, 5);
}

#[test]
fn test_p6c_rating_out_of_range_rejected() {
    let raw_0 = r#"{"category":"Developer Tools","tags":["cli"],"rating":0,"rating_reason":"Too low","confidence":"low"}"#;
    let err_0 = parse_and_validate_suggestion(raw_0).unwrap_err();
    assert!(err_0.contains("Rating must be an integer between 1 and 5"));

    let raw_6 = r#"{"category":"Developer Tools","tags":["cli"],"rating":6,"rating_reason":"Too high","confidence":"high"}"#;
    let err_6 = parse_and_validate_suggestion(raw_6).unwrap_err();
    assert!(err_6.contains("Rating must be an integer between 1 and 5"));
}

#[test]
fn test_p6c_rating_string_or_float_rejected() {
    let raw_str = r#"{"category":"Developer Tools","tags":["cli"],"rating":"5","rating_reason":"String rating","confidence":"medium"}"#;
    let err_str = parse_and_validate_suggestion(raw_str).unwrap_err();
    assert!(err_str.contains("Rating must be an integer between 1 and 5"));

    let raw_float = r#"{"category":"Developer Tools","tags":["cli"],"rating":4.5,"rating_reason":"Float rating","confidence":"medium"}"#;
    let err_float = parse_and_validate_suggestion(raw_float).unwrap_err();
    assert!(err_float.contains("decimal ratings are not allowed"));
}

#[test]
fn test_p6c_rating_omitted_or_null_rejected() {
    let raw_omitted = r#"{"category":"Developer Tools","tags":["cli"],"rating_reason":"Missing rating","confidence":"medium"}"#;
    let err_omitted = parse_and_validate_suggestion(raw_omitted).unwrap_err();
    assert!(err_omitted.contains("Failed to parse JSON") || err_omitted.contains("Rating"));

    let raw_null = r#"{"category":"Developer Tools","tags":["cli"],"rating":null,"rating_reason":"Null rating","confidence":"medium"}"#;
    let err_null = parse_and_validate_suggestion(raw_null).unwrap_err();
    assert!(err_null.contains("Rating must be an integer between 1 and 5"));
}

#[test]
fn test_p6c_rating_reason_empty_or_whitespace_rejected() {
    let raw_empty = r#"{"category":"Developer Tools","tags":["cli"],"rating":4,"rating_reason":"","confidence":"medium"}"#;
    let err_empty = parse_and_validate_suggestion(raw_empty).unwrap_err();
    assert!(err_empty.contains("Rating reason cannot be empty"));

    let raw_ws = r#"{"category":"Developer Tools","tags":["cli"],"rating":4,"rating_reason":"   \n  ","confidence":"medium"}"#;
    let err_ws = parse_and_validate_suggestion(raw_ws).unwrap_err();
    assert!(err_ws.contains("Rating reason cannot be empty"));
}

#[test]
fn test_p6c_rating_reason_languages_accepted() {
    let raw_zh = r#"{"category":"Multimedia","tags":["video"],"rating":5,"rating_reason":"这是一个成熟且维护良好的短视频生成工具，拥有丰富的自动化工作流。","confidence":"high"}"#;
    let res_zh = parse_and_validate_suggestion(raw_zh).unwrap();
    assert_eq!(
        res_zh.rating_reason,
        "这是一个成熟且维护良好的短视频生成工具，拥有丰富的自动化工作流。"
    );

    let raw_en = r#"{"category":"Multimedia","tags":["video"],"rating":5,"rating_reason":"A robust, well-documented repository with active maintenance and tests.","confidence":"high"}"#;
    let res_en = parse_and_validate_suggestion(raw_en).unwrap();
    assert_eq!(
        res_en.rating_reason,
        "A robust, well-documented repository with active maintenance and tests."
    );
}

#[test]
fn test_p6c_confidence_variants_accepted() {
    for conf in &["low", "medium", "high", "LOW", "High"] {
        let raw = format!(
            r#"{{"category":"Developer Tools","tags":["cli"],"rating":4,"rating_reason":"Good tool","confidence":"{}"}}"#,
            conf
        );
        let res = parse_and_validate_suggestion(&raw).unwrap();
        assert_eq!(res.confidence, conf.to_lowercase());
    }
}

#[test]
fn test_p6c_confidence_invalid_or_omitted_rejected() {
    let raw_invalid = r#"{"category":"Developer Tools","tags":["cli"],"rating":4,"rating_reason":"Good tool","confidence":"extreme"}"#;
    let err_invalid = parse_and_validate_suggestion(raw_invalid).unwrap_err();
    assert!(err_invalid.contains("Confidence must be 'low', 'medium', or 'high'"));

    let raw_omitted =
        r#"{"category":"Developer Tools","tags":["cli"],"rating":4,"rating_reason":"Good tool"}"#;
    let err_omitted = parse_and_validate_suggestion(raw_omitted).unwrap_err();
    assert!(err_omitted.contains("Failed to parse JSON") || err_omitted.contains("confidence"));
}

#[tokio::test]
async fn test_p6c_21_invalid_first_response_and_valid_repair_success() {
    let bad_response = Ok(r#"{"category":"InvalidCategory","tags":["a"],"rating":3,"rating_reason":"foo","confidence":"low"}"#.to_string());
    let good_response = Ok(r#"{"category":"Developer Tools","tags":["git","cli"],"rating":4,"rating_reason":"Helpful CLI tool","confidence":"medium"}"#.to_string());

    let mock_client = MockAiClient::new(vec![bad_response, good_response]);
    let suggestion = generate_favorite_suggestion(
        &mock_client,
        "git-helper",
        Some("Git CLI helper"),
        100,
        Some("MIT"),
        None,
        &["git".to_string()],
    )
    .await
    .unwrap();

    assert_eq!(suggestion.category, "Developer Tools");
    assert_eq!(suggestion.tags, vec!["git", "cli"]);
    assert_eq!(suggestion.rating, 4);
    assert_eq!(suggestion.rating_reason, "Helpful CLI tool");
    assert_eq!(suggestion.confidence, "medium");
    assert_eq!(mock_client.call_count(), 2);
}

#[tokio::test]
async fn test_p6c_22_and_23_invalid_first_and_invalid_repair_max_2_calls() {
    let bad1 = Ok("Not json".to_string());
    let bad2 = Ok("Still not json".to_string());

    let mock_client = MockAiClient::new(vec![bad1, bad2]);
    let err = generate_favorite_suggestion(&mock_client, "repo", Some("desc"), 50, None, None, &[])
        .await
        .unwrap_err();

    assert_eq!(err.code(), "AIInvalidResponse");
    assert_eq!(mock_client.call_count(), 2);
}

#[tokio::test]
async fn test_p6c_24_provider_transport_error_does_not_trigger_repair() {
    let auth_error = Err(AppError::AIAuth("Invalid API Key".to_string()));

    let mock_client = MockAiClient::new(vec![auth_error]);
    let err = generate_favorite_suggestion(&mock_client, "repo", Some("desc"), 50, None, None, &[])
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
            releases: None,
            languages: None,
        },
    )
    .await
    .unwrap();

    let initial_history_count = count_selection_history(&pool).await.unwrap();

    let mock_client = MockAiClient::new(vec![Ok(
        r#"{"category":"Multimedia","tags":["video","ffmpeg"],"rating":4,"rating_reason":"Good multimedia tool","confidence":"high"}"#.to_string(),
    )]);

    // Execute suggestion
    let suggestion = suggest_favorite_ai_metadata_with_client(&pool, &mock_client, fav.id)
        .await
        .unwrap();

    assert_eq!(suggestion.category, "Multimedia");
    assert_eq!(suggestion.rating, 4);
    assert_eq!(suggestion.rating_reason, "Good multimedia tool");
    assert_eq!(suggestion.confidence, "high");

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
            releases: None,
            languages: None,
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
            rating_reason: None,
            rating_confidence: None,
            notes: "Important user note".to_string(),
            user_tags: vec!["my-custom-tag".to_string()],
            ai_tags: None,
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
            releases: None,
            languages: None,
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
            releases: None,
            languages: None,
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
            releases: None,
            languages: None,
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
            releases: None,
            languages: None,
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
            releases: None,
            languages: None,
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
            releases: None,
            languages: None,
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
            rating_reason: None,
            rating_confidence: None,
            notes: "".to_string(),
            user_tags: vec!["rust".to_string()], // user: rust
            ai_tags: None,
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

    assert_eq!(rust_tags.len(), 2);
    let sources: HashSet<String> = rust_tags.iter().map(|t| t.source.clone()).collect();
    assert!(sources.contains("github"));
    assert!(sources.contains("user"));
    // Under the AI tag supplement invariant, AI tag duplicating GitHub topic is filtered out
    assert!(!sources.contains("ai"));
}

#[tokio::test]
async fn test_p6c_46_migrations_applied() {
    let (_db_dir, pool) = setup_test_db().await;

    // Verify migrations exist (001, 002, 003, and 004_repository_metadata)
    let migrations: Vec<(i64,)> =
        sqlx::query_as("SELECT version FROM _sqlx_migrations ORDER BY version ASC")
            .fetch_all(&pool)
            .await
            .unwrap();

    assert_eq!(migrations.len(), 4);
    assert_eq!(migrations[0].0, 1);
    assert_eq!(migrations[1].0, 2);
    assert_eq!(migrations[2].0, 3);
    assert_eq!(migrations[3].0, 4);
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
            releases: None,
            languages: None,
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
            releases: None,
            languages: None,
        },
    )
    .await
    .unwrap();

    let loaded = get_favorite(&pool, fav.id).await.unwrap();
    assert_eq!(loaded.category, "Web & SaaS");
    assert!(loaded.tags.iter().all(|t| t.source != "ai"));
}

#[tokio::test]
async fn test_p6c_ollama_suggestion_request_contract_think_false_and_budget() {
    let mock_server = MockServer::start().await;

    let response_body = serde_json::json!({
        "model": "qwen3.5:9b",
        "message": {
            "role": "assistant",
            "content": "{\"category\":\"Developer Tools\",\"tags\":[\"cli\",\"git\"],\"rating\":5,\"rating_reason\":\"Excellent Git utility with comprehensive commands\",\"confidence\":\"high\"}"
        },
        "done": true,
        "done_reason": "stop"
    });

    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = OllamaClient::new(
        Some(&mock_server.uri()),
        "qwen3.5:9b",
        Duration::from_secs(5),
    )
    .expect("Failed to build OllamaClient");

    let suggestion = generate_favorite_suggestion(
        &client,
        "git-wizard",
        Some("A powerful git assistant"),
        1500,
        Some("Apache-2.0"),
        Some("2026-03-10T12:00:00Z"),
        &["cli".to_string(), "git".to_string()],
    )
    .await
    .expect("Suggestion should succeed");

    assert_eq!(suggestion.category, "Developer Tools");
    assert_eq!(suggestion.tags, vec!["cli", "git"]);
    assert_eq!(suggestion.rating, 5);
    assert_eq!(
        suggestion.rating_reason,
        "Excellent Git utility with comprehensive commands"
    );
    assert_eq!(suggestion.confidence, "high");

    // Verify wire contract details
    let requests = mock_server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    let req_body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(req_body["model"], "qwen3.5:9b");
    assert_eq!(req_body["stream"], false);
    assert_eq!(req_body["think"], false);
    assert_eq!(req_body["options"]["num_predict"], 512);
    let temp = req_body["options"]["temperature"].as_f64().unwrap();
    assert!((temp - 0.2).abs() < 1e-4);
}

#[tokio::test]
async fn test_p6c_ollama_thinking_only_response_produces_diagnostic_error_and_does_not_consume_thinking(
) {
    let mock_server = MockServer::start().await;

    let response_body = serde_json::json!({
        "model": "qwen3.5:9b",
        "message": {
            "role": "assistant",
            "content": "",
            "thinking": "The user wants me to suggest categories and tags. Let me think about this repository... First let's examine the name and topics."
        },
        "done": true,
        "done_reason": "length"
    });

    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
        .expect(1) // Transport/provider-level error halts immediately without repair
        .mount(&mock_server)
        .await;

    let client = OllamaClient::new(
        Some(&mock_server.uri()),
        "qwen3.5:9b",
        Duration::from_secs(5),
    )
    .expect("Failed to build OllamaClient");

    let err = generate_favorite_suggestion(
        &client,
        "git-wizard",
        Some("A powerful git assistant"),
        1500,
        Some("Apache-2.0"),
        None,
        &["cli".to_string()],
    )
    .await
    .expect_err("Should fail with diagnostic error when Ollama returns only thinking");

    let err_msg = err.to_string();
    assert!(
        err_msg.contains("thinking/reasoning content") && err_msg.contains("done_reason: length"),
        "Error message should clearly diagnose thinking exhaustion: {}",
        err_msg
    );
}

#[tokio::test]
async fn test_p6c_ai_search_general_text_does_not_send_think_false() {
    let mock_server = MockServer::start().await;

    let response_body = serde_json::json!({
        "model": "qwen3.5:9b",
        "message": {
            "role": "assistant",
            "content": "Rust terminal tools including ripgrep and bat"
        },
        "done": true,
        "done_reason": "stop"
    });

    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = OllamaClient::new(
        Some(&mock_server.uri()),
        "qwen3.5:9b",
        Duration::from_secs(5),
    )
    .expect("Failed to build OllamaClient");

    // Calling general generate_text (as used by AI search) without think:false
    let reply = client
        .generate_text("System instruction for search", "Find cli tools")
        .await
        .expect("General generate_text should succeed");

    assert_eq!(reply, "Rust terminal tools including ripgrep and bat");

    let requests = mock_server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    let req_body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(req_body["model"], "qwen3.5:9b");
    assert_eq!(req_body["stream"], false);
    // General generate_text MUST NOT contain think: false
    assert!(
        req_body.get("think").is_none(),
        "General text generation (AI search) must not inject think:false: {:?}",
        req_body.get("think")
    );
}
