use tempfile::tempdir;

use reposhelf_lib::ai::{
    check_ai_search_readiness, AiProviderClient, AiRepositorySearchRequest, GenerateTextOptions,
    TranslationLimiter,
};
use reposhelf_lib::commands::translate_repository_description_impl;
use reposhelf_lib::db::database::init_pool;
use reposhelf_lib::db::selection_history::count_selection_history;
use reposhelf_lib::error::AppError;
use reposhelf_lib::github::GitHubClient;
use reposhelf_lib::library::search_service::{
    execute_ai_search, execute_ai_search_with_client_and_cancel_check,
};
use reposhelf_lib::secrets::{InMemorySecretStore, SecretStore, SECRET_KEY_AI_API_KEY};
use reposhelf_lib::settings::{AiProvider, AiSettings, GitHubSettings, Settings, SettingsStore};

struct MockAiClient {
    responses: std::sync::Mutex<Vec<Result<String, AppError>>>,
}

impl MockAiClient {
    fn new(responses: Vec<Result<String, AppError>>) -> Self {
        Self {
            responses: std::sync::Mutex::new(responses),
        }
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
        _system_instruction: &'a str,
        _user_input: &'a str,
        _options: GenerateTextOptions,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String, AppError>> + Send + 'a>>
    {
        let resp = self.responses.lock().unwrap().remove(0);
        Box::pin(async move { resp })
    }
}

async fn setup_test_db() -> (tempfile::TempDir, sqlx::SqlitePool) {
    let dir = tempdir().expect("Failed to create tempdir");
    let db_path = dir.path().join("test_phase_s2a.db");
    let pool = init_pool(&db_path)
        .await
        .expect("Failed to initialize test pool");
    (dir, pool)
}

fn create_test_settings_store(ai: AiSettings) -> (tempfile::TempDir, SettingsStore) {
    let dir = tempdir().expect("Failed to create tempdir");
    let file_path = dir.path().join("settings.json");
    let store = SettingsStore::new(file_path).expect("Failed to create settings store");
    let mut current = store.get().expect("Failed to get current settings");
    current.ai = ai;
    store.save(&current).expect("Failed to save settings");
    (dir, store)
}

#[tokio::test]
async fn test_translation_rejected_when_ai_disabled() {
    let (_dir, store) = create_test_settings_store(AiSettings {
        enabled: false,
        provider: Some(AiProvider::OpenaiCompatible),
        base_url: Some("https://api.openai.com/v1".to_string()),
        model: Some("gpt-4o-mini".to_string()),
        ..Default::default()
    });
    let secrets = InMemorySecretStore::new();
    secrets
        .set_secret(SECRET_KEY_AI_API_KEY, "test-key")
        .unwrap();
    let limiter = TranslationLimiter::new();

    let result = translate_repository_description_impl(
        &store,
        &secrets,
        &limiter,
        "Build faster desktop apps",
        "zh-Hans",
    )
    .await;

    match result {
        Err(AppError::SettingsValidation(msg)) => {
            assert!(
                msg.contains("AI is disabled in Settings"),
                "Expected error message about AI being disabled, got: {}",
                msg
            );
        }
        other => panic!("Expected SettingsValidation error, got: {:?}", other),
    }
}

#[tokio::test]
async fn test_translation_rejected_when_provider_requires_key_and_missing() {
    let (_dir, store) = create_test_settings_store(AiSettings {
        enabled: true,
        provider: Some(AiProvider::OpenaiCompatible),
        base_url: Some("https://api.openai.com/v1".to_string()),
        model: Some("gpt-4o-mini".to_string()),
        ..Default::default()
    });
    let secrets = InMemorySecretStore::new(); // No API key set
    let limiter = TranslationLimiter::new();

    let result = translate_repository_description_impl(
        &store,
        &secrets,
        &limiter,
        "Build faster desktop apps",
        "zh-Hans",
    )
    .await;

    match result {
        Err(AppError::SettingsValidation(msg)) => {
            assert!(
                msg.contains("AI API Key not configured"),
                "Expected error message about missing API key, got: {}",
                msg
            );
        }
        other => panic!("Expected SettingsValidation error, got: {:?}", other),
    }
}

#[test]
fn test_translation_readiness_acceptance_with_ollama() {
    let settings = Settings {
        schema_version: 1,
        github: GitHubSettings::default(),
        ai: AiSettings {
            enabled: true,
            provider: Some(AiProvider::Ollama),
            base_url: Some("http://localhost:11434".to_string()),
            model: Some("llama3".to_string()),
            ..Default::default()
        },
        ui_language: None,
    };
    let secrets = InMemorySecretStore::new();

    let readiness = check_ai_search_readiness(&settings, &secrets);
    assert!(readiness.ready);
    assert!(readiness.reason.is_none());
}

#[tokio::test]
async fn test_ai_search_rejected_when_disabled() {
    let (_db_dir, pool) = setup_test_db().await;
    let (_dir, store) = create_test_settings_store(AiSettings {
        enabled: false,
        provider: Some(AiProvider::OpenaiCompatible),
        base_url: Some("https://api.openai.com/v1".to_string()),
        model: Some("gpt-4o-mini".to_string()),
        ..Default::default()
    });
    let secrets = InMemorySecretStore::new();
    secrets
        .set_secret(SECRET_KEY_AI_API_KEY, "test-key")
        .unwrap();
    let github = GitHubClient::new();

    let req = AiRepositorySearchRequest {
        input: "rust desktop framework".to_string(),
        min_stars: None,
        pushed_after: None,
        limit: 10,
        exclude_selected: true,
        exclude_favorites: true,
    };

    let result = execute_ai_search(&pool, &store, &secrets, &github, None, &req).await;
    match result {
        Err(AppError::SettingsValidation(msg)) => {
            assert!(
                msg.contains("AI is disabled in Settings"),
                "Expected error about AI disabled, got: {}",
                msg
            );
        }
        other => panic!("Expected SettingsValidation error, got: {:?}", other),
    }
}

#[tokio::test]
async fn test_ai_search_cancellation_prevents_selection_history() {
    let (_db_dir, pool) = setup_test_db().await;
    let mock_client = MockAiClient::new(vec![Ok(r#"{"queries":["rust gui"]}"#.to_string())]);
    let github = GitHubClient::new();

    let req = AiRepositorySearchRequest {
        input: "rust desktop framework".to_string(),
        min_stars: None,
        pushed_after: None,
        limit: 10,
        exclude_selected: true,
        exclude_favorites: true,
    };

    // Simulate cancellation triggered during execution
    let is_cancelled = || true;

    let result = execute_ai_search_with_client_and_cancel_check(
        &pool,
        &mock_client,
        &github,
        None,
        &req,
        is_cancelled,
    )
    .await;

    match result {
        Err(AppError::SettingsValidation(msg)) => {
            assert!(
                msg.contains("cancelled"),
                "Expected cancellation message, got: {}",
                msg
            );
        }
        other => panic!(
            "Expected SettingsValidation error due to cancellation, got: {:?}",
            other
        ),
    }

    // Verify selection history was NOT inserted into DB
    let count = count_selection_history(&pool).await.unwrap();
    assert_eq!(
        count, 0,
        "Cancelled AI search must not insert into selection history"
    );
}
