use std::sync::{Arc, Mutex};
use tempfile::tempdir;

use reposhelf_lib::ai::{
    build_suggestion_system_instruction, format_suggestion_user_prompt_with_locale,
    generate_favorite_suggestion, validate_rating_reason_language, AiProviderClient,
    GenerateTextOptions,
};
use reposhelf_lib::db::database::init_pool;
use reposhelf_lib::error::AppError;
use reposhelf_lib::github::classification::ALL_CATEGORIES;
use reposhelf_lib::library::favorite_service::{
    suggest_favorite_ai_metadata, suggest_favorite_ai_metadata_with_client,
};
use reposhelf_lib::library::repository::validate_category;
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
    let db_path = dir.path().join("test_s1.db");
    let pool = init_pool(&db_path)
        .await
        .expect("Failed to init test DB pool");
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

// =========================================================================
// R01 Prompt Instruction & Language Consistency Tests
// =========================================================================

#[test]
fn test_s1_prompt_instruction_locale_zh_hans() {
    let sys = build_suggestion_system_instruction("zh-Hans");
    assert!(sys.contains("Simplified Chinese (简体中文)"));
    assert!(sys.contains("项目定位明确，社区关注度较高，近期仍有维护。"));
    assert!(!sys.contains("Clear project positioning"));
    assert!(sys.contains("You may keep repository names, programming languages, API names"));
}

#[test]
fn test_s1_prompt_instruction_locale_zh_hant() {
    let sys = build_suggestion_system_instruction("zh-Hant");
    assert!(sys.contains("Traditional Chinese (繁體中文)"));
    assert!(sys.contains("專案定位明確，社群關注度較高，近期仍有維護。"));
    assert!(!sys.contains("Clear project positioning"));
    assert!(sys.contains("You may keep repository names, programming languages, API names"));
}

#[test]
fn test_s1_prompt_instruction_locale_en() {
    let sys = build_suggestion_system_instruction("en");
    assert!(sys.contains("Write the rating rationale (rating_reason) in English."));
    assert!(sys.contains("Clear project positioning, strong community interest, and active recent maintenance."));
    assert!(!sys.contains("项目定位明确"));
}

#[test]
fn test_s1_prompt_user_content_includes_target_locale() {
    let user_content = format_suggestion_user_prompt_with_locale(
        "my-repo",
        Some("A great tool"),
        1200,
        Some("MIT"),
        None,
        &["rust".to_string()],
        Some("zh-Hans"),
    );
    assert!(user_content.contains("<target_locale>zh-Hans</target_locale>"));
}

#[test]
fn test_s1_validate_rating_reason_language_permissive_for_technical_terms() {
    // English technical terms in Simplified Chinese rationale must pass
    let zh_hans_reason = "该项目是基于 Rust 和 React 构建的现代 CLI 工具，API 设计完备，GitHub 社区活跃。";
    assert!(validate_rating_reason_language(zh_hans_reason, "zh-Hans").is_ok());

    // English technical terms in Traditional Chinese rationale must pass
    let zh_hant_reason = "該專案是基於 Rust 和 React 建構的現代 CLI 工具，API 設計完備，社群活躍。";
    assert!(validate_rating_reason_language(zh_hant_reason, "zh-Hant").is_ok());

    // Natural English with code words must pass for English
    let en_reason = "Well-architected framework using Tokio and Actix-web with robust REST API documentation.";
    assert!(validate_rating_reason_language(en_reason, "en").is_ok());

    // Empty reason rejected
    assert!(validate_rating_reason_language("", "en").is_err());
    assert!(validate_rating_reason_language("   ", "zh-Hans").is_err());

    // Purely English sentence when zh-Hans was requested rejected
    let bad_zh = "This project has great documentation and high community engagement with frequent commits.";
    assert!(validate_rating_reason_language(bad_zh, "zh-Hans").is_err());

    // Purely Chinese sentence when English was requested rejected
    let bad_en = "项目架构清晰，文档完备，社区关注度极高。";
    assert!(validate_rating_reason_language(bad_en, "en").is_err());
}

#[tokio::test]
async fn test_s1_generate_suggestion_zh_hans_success() {
    let mock = MockAiClient::new(vec![Ok(r#"{
        "category": "Developer Tools",
        "tags": ["rust", "cli"],
        "rating": 5,
        "rating_reason": "架构清晰，基于 Rust 与 TypeScript 开发，代码质量优异且社区活跃。",
        "confidence": "high"
    }"#.to_string())]);

    let res = generate_favorite_suggestion(
        &mock,
        "super-cli",
        Some("A super CLI tool"),
        3500,
        Some("MIT"),
        None,
        &["rust".to_string()],
        "zh-Hans",
    )
    .await
    .unwrap();

    assert_eq!(res.category, "Developer Tools");
    assert_eq!(res.rating, 5);
    assert!(res.rating_reason.contains("架构清晰"));
    assert_eq!(res.confidence, "high");
    assert_eq!(mock.call_count(), 1);

    let (sys, user) = mock.last_prompt();
    assert!(sys.contains("Simplified Chinese (简体中文)"));
    assert!(user.contains("<target_locale>zh-Hans</target_locale>"));
}

#[tokio::test]
async fn test_s1_generate_suggestion_zh_hant_success() {
    let mock = MockAiClient::new(vec![Ok(r#"{
        "category": "Developer Tools",
        "tags": ["rust", "cli"],
        "rating": 4,
        "rating_reason": "架構清晰，基於 Rust 開發，社群活躍且文檔完備。",
        "confidence": "high"
    }"#.to_string())]);

    let res = generate_favorite_suggestion(
        &mock,
        "super-cli",
        Some("A super CLI tool"),
        3500,
        Some("MIT"),
        None,
        &["rust".to_string()],
        "zh-Hant",
    )
    .await
    .unwrap();

    assert_eq!(res.category, "Developer Tools");
    assert_eq!(res.rating, 4);
    assert!(res.rating_reason.contains("架構清晰"));
    assert_eq!(res.confidence, "high");
    assert_eq!(mock.call_count(), 1);

    let (sys, user) = mock.last_prompt();
    assert!(sys.contains("Traditional Chinese (繁體中文)"));
    assert!(user.contains("<target_locale>zh-Hant</target_locale>"));
}

#[tokio::test]
async fn test_s1_generate_suggestion_en_success() {
    let mock = MockAiClient::new(vec![Ok(r#"{
        "category": "Developer Tools",
        "tags": ["rust", "cli"],
        "rating": 4,
        "rating_reason": "Clean architecture, active maintenance and solid community adoption.",
        "confidence": "high"
    }"#.to_string())]);

    let res = generate_favorite_suggestion(
        &mock,
        "super-cli",
        Some("A super CLI tool"),
        3500,
        Some("MIT"),
        None,
        &["rust".to_string()],
        "en",
    )
    .await
    .unwrap();

    assert_eq!(res.category, "Developer Tools");
    assert_eq!(res.rating, 4);
    assert!(res.rating_reason.contains("Clean architecture"));
    assert_eq!(res.confidence, "high");
    assert_eq!(mock.call_count(), 1);

    let (sys, user) = mock.last_prompt();
    assert!(sys.contains("Write the rating rationale (rating_reason) in English."));
    assert!(user.contains("<target_locale>en</target_locale>"));
}

#[tokio::test]
async fn test_s1_language_mismatch_triggers_repair_and_succeeds() {
    // 1st response: English when zh-Hans requested -> fails validation
    // 2nd response: Corrected to Simplified Chinese -> passes
    let bad1 = Ok(r#"{
        "category": "Developer Tools",
        "tags": ["cli"],
        "rating": 4,
        "rating_reason": "Very active project with good documentation.",
        "confidence": "high"
    }"#.to_string());
    let good2 = Ok(r#"{
        "category": "Developer Tools",
        "tags": ["cli"],
        "rating": 4,
        "rating_reason": "文档完备且社区非常活跃的优质工具。",
        "confidence": "high"
    }"#.to_string());

    let mock = MockAiClient::new(vec![bad1, good2]);

    let res = generate_favorite_suggestion(
        &mock,
        "cool-tool",
        Some("Cool tool"),
        1000,
        Some("Apache-2.0"),
        None,
        &[],
        "zh-Hans",
    )
    .await
    .unwrap();

    assert_eq!(res.rating, 4);
    assert_eq!(res.rating_reason, "文档完备且社区非常活跃的优质工具。");
    assert_eq!(mock.call_count(), 2);
}

#[tokio::test]
async fn test_s1_language_mismatch_fails_after_repair_max_2_calls() {
    // 1st response: English
    // 2nd response: Still English
    let bad1 = Ok(r#"{
        "category": "Developer Tools",
        "tags": ["cli"],
        "rating": 4,
        "rating_reason": "Very active project with good documentation.",
        "confidence": "high"
    }"#.to_string());
    let bad2 = Ok(r#"{
        "category": "Developer Tools",
        "tags": ["cli"],
        "rating": 4,
        "rating_reason": "Still English text despite instructions.",
        "confidence": "high"
    }"#.to_string());

    let mock = MockAiClient::new(vec![bad1, bad2]);

    let err = generate_favorite_suggestion(
        &mock,
        "cool-tool",
        Some("Cool tool"),
        1000,
        Some("Apache-2.0"),
        None,
        &[],
        "zh-Hans",
    )
    .await
    .unwrap_err();

    assert_eq!(err.code(), "AIInvalidResponse");
    assert_eq!(mock.call_count(), 2);
}

#[tokio::test]
async fn test_s1_missing_or_invalid_locale_rejected_before_ai_call() {
    let (_db_dir, pool) = setup_test_db().await;
    let (_set_dir, settings_store, secret_store) = create_test_settings_and_secrets();

    // 1. None locale
    let err1 = suggest_favorite_ai_metadata(&pool, &settings_store, &*secret_store, 1, None)
        .await
        .unwrap_err();
    assert_eq!(err1.code(), "SettingsValidation");
    assert!(err1.to_string().contains("Missing or unsupported target_locale"));

    // 2. Unsupported locale "fr"
    let err2 = suggest_favorite_ai_metadata(
        &pool,
        &settings_store,
        &*secret_store,
        1,
        Some("fr".to_string()),
    )
    .await
    .unwrap_err();
    assert_eq!(err2.code(), "SettingsValidation");
    assert!(err2.to_string().contains("Missing or unsupported target_locale"));

    // 3. With client
    let mock = MockAiClient::new(vec![]);
    let err3 = suggest_favorite_ai_metadata_with_client(&pool, &mock, 1, "invalid_locale")
        .await
        .unwrap_err();
    assert_eq!(err3.code(), "SettingsValidation");
    assert_eq!(mock.call_count(), 0); // ZERO provider calls!
}

// =========================================================================
// R02 Category Validation & Completeness Tests
// =========================================================================

#[test]
fn test_s1_all_16_categories_valid() {
    assert_eq!(ALL_CATEGORIES.len(), 16);
    for &cat in &ALL_CATEGORIES {
        let valid = validate_category(cat).expect(&format!("Category '{}' should be valid", cat));
        assert_eq!(valid, cat);
        // Case-insensitive test
        let lower = cat.to_lowercase();
        assert!(validate_category(&lower).is_ok());
    }
}
