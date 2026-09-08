use std::sync::Arc;
use std::time::Duration;

use tempfile::tempdir;
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use reposhelf_lib::ai::{
    check_ai_search_readiness, create_provider_client, execute_test_ai_connection,
    generate_search_queries, parse_and_validate_ai_queries, resolve_provider_endpoint,
    strip_outer_code_fence, validate_ai_search_input, validate_url_security, AiProviderClient,
    AiRepositorySearchRequest, AiRepositorySearchResponse, AiSearchReadiness,
    AI_SEARCH_SYSTEM_INSTRUCTION,
};
use reposhelf_lib::db::database::init_pool;
use reposhelf_lib::db::selection_history::count_selection_history;
use reposhelf_lib::error::AppError;
use reposhelf_lib::github::GitHubClient;
use reposhelf_lib::library::favorite_service::favorite_repository;
use reposhelf_lib::library::search_service::{execute_ai_search, execute_ai_search_with_client};
use reposhelf_lib::library::FavoriteInput;
use reposhelf_lib::secrets::{InMemorySecretStore, SecretStore, SECRET_KEY_AI_API_KEY};
use reposhelf_lib::settings::{AiProvider, SettingsStore};

// =============================================================================
// 1. URL Security & Endpoint Resolution Tests
// =============================================================================

#[test]
fn test_url_security_remote_http_rejected() {
    let u = url::Url::parse("http://api.openai.com/v1").unwrap();
    let err = validate_url_security(&u).unwrap_err();
    assert!(err
        .to_string()
        .contains("HTTP is only permitted for loopback"));
}

#[test]
fn test_url_security_loopback_http_accepted() {
    let u1 = url::Url::parse("http://127.0.0.1:11434").unwrap();
    assert!(validate_url_security(&u1).is_ok());

    let u2 = url::Url::parse("http://localhost:11434").unwrap();
    assert!(validate_url_security(&u2).is_ok());

    let u3 = url::Url::parse("http://[::1]:11434").unwrap();
    assert!(validate_url_security(&u3).is_ok());
}

#[test]
fn test_url_security_credentials_rejected() {
    let u = url::Url::parse("https://user:pass@api.openai.com/v1").unwrap();
    let err = validate_url_security(&u).unwrap_err();
    assert!(err
        .to_string()
        .contains("must not contain embedded user credentials"));
}

#[test]
fn test_url_security_query_and_fragment_rejected() {
    let u1 = url::Url::parse("https://api.openai.com/v1?token=123").unwrap();
    assert!(validate_url_security(&u1)
        .unwrap_err()
        .to_string()
        .contains("query parameters"));

    let u2 = url::Url::parse("https://api.openai.com/v1#heading").unwrap();
    assert!(validate_url_security(&u2)
        .unwrap_err()
        .to_string()
        .contains("URL fragments"));
}

#[test]
fn test_endpoint_resolution_prevents_duplicate_v1() {
    let endpoint =
        resolve_provider_endpoint(AiProvider::Anthropic, Some("http://127.0.0.1:8080/v1/"))
            .unwrap();
    assert_eq!(endpoint.as_str(), "http://127.0.0.1:8080/v1/messages");
    assert!(!endpoint.as_str().contains("/v1/v1"));
}

#[test]
fn test_endpoint_resolution_defaults() {
    assert_eq!(
        resolve_provider_endpoint(AiProvider::OpenaiCompatible, None)
            .unwrap()
            .as_str(),
        "https://api.openai.com/v1/chat/completions"
    );
    assert_eq!(
        resolve_provider_endpoint(AiProvider::Anthropic, None)
            .unwrap()
            .as_str(),
        "https://api.anthropic.com/v1/messages"
    );
    assert_eq!(
        resolve_provider_endpoint(AiProvider::Gemini, None)
            .unwrap()
            .as_str(),
        "https://generativelanguage.googleapis.com/v1beta/interactions"
    );
    assert_eq!(
        resolve_provider_endpoint(AiProvider::Ollama, None)
            .unwrap()
            .as_str(),
        "http://127.0.0.1:11434/api/chat"
    );
}

#[test]
fn test_ollama_trailing_slash_endpoint_resolution() {
    let ep1 =
        resolve_provider_endpoint(AiProvider::Ollama, Some("http://127.0.0.1:11434/")).unwrap();
    assert_eq!(ep1.as_str(), "http://127.0.0.1:11434/api/chat");

    let ep2 =
        resolve_provider_endpoint(AiProvider::Ollama, Some("http://127.0.0.1:11434/api/")).unwrap();
    assert_eq!(ep2.as_str(), "http://127.0.0.1:11434/api/chat");
}

// =============================================================================
// 2. OpenAI Compatible Tests
// =============================================================================

#[tokio::test]
async fn test_openai_success() {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(header("Authorization", "Bearer sk-test-key"))
        .and(header("Content-Type", "application/json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": "OK"
                    }
                }
            ]
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    let base_url = format!("{}/v1", mock_server.uri());
    let client = create_provider_client(
        AiProvider::OpenaiCompatible,
        Some(&base_url),
        "gpt-4o-mini",
        Some("sk-test-key"),
        Some(Duration::from_secs(5)),
    )
    .unwrap();

    let res = client.test_connection().await.unwrap();
    assert_eq!(res.provider, "OpenAI Compatible");
    assert_eq!(res.model, "gpt-4o-mini");
    assert!(res.success);
}

#[tokio::test]
async fn test_openai_generate_text_system_and_user() {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": "Generated reply"
                    }
                }
            ]
        })))
        .mount(&mock_server)
        .await;

    let base_url = format!("{}/v1", mock_server.uri());
    let client = create_provider_client(
        AiProvider::OpenaiCompatible,
        Some(&base_url),
        "gpt-4o-mini",
        Some("sk-test-key"),
        Some(Duration::from_secs(5)),
    )
    .unwrap();

    let text = client
        .generate_text("You are a helpful assistant", "Search query")
        .await
        .unwrap();
    assert_eq!(text, "Generated reply");

    let requests = mock_server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(body["model"], "gpt-4o-mini");
    assert_eq!(body["messages"][0]["role"], "system");
    assert_eq!(
        body["messages"][0]["content"],
        "You are a helpful assistant"
    );
    assert_eq!(body["messages"][1]["role"], "user");
    assert_eq!(body["messages"][1]["content"], "Search query");
}

#[tokio::test]
async fn test_openai_auth_401() {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
            "error": { "message": "Incorrect API key provided" }
        })))
        .mount(&mock_server)
        .await;

    let base_url = format!("{}/v1", mock_server.uri());
    let secret = "sk-super-secret-key-xyz";
    let client = create_provider_client(
        AiProvider::OpenaiCompatible,
        Some(&base_url),
        "gpt-4o-mini",
        Some(secret),
        Some(Duration::from_secs(5)),
    )
    .unwrap();

    let err = client.test_connection().await.unwrap_err();
    assert_eq!(err.code(), "AIAuth");
    assert!(!err.to_string().contains(secret));
}

#[tokio::test]
async fn test_openai_model_404() {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
            "error": { "message": "Model not found" }
        })))
        .mount(&mock_server)
        .await;

    let base_url = format!("{}/v1", mock_server.uri());
    let client = create_provider_client(
        AiProvider::OpenaiCompatible,
        Some(&base_url),
        "nonexistent-model",
        Some("sk-key"),
        Some(Duration::from_secs(5)),
    )
    .unwrap();

    let err = client.test_connection().await.unwrap_err();
    assert_eq!(err.code(), "AIModel");
}

#[tokio::test]
async fn test_openai_rate_limit_429() {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(429).set_body_string("Rate limit reached"))
        .mount(&mock_server)
        .await;

    let base_url = format!("{}/v1", mock_server.uri());
    let client = create_provider_client(
        AiProvider::OpenaiCompatible,
        Some(&base_url),
        "gpt-4o-mini",
        Some("sk-key"),
        Some(Duration::from_secs(5)),
    )
    .unwrap();

    let err = client.test_connection().await.unwrap_err();
    assert_eq!(err.code(), "AIConnection");
}

#[tokio::test]
async fn test_openai_server_error_500() {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(500).set_body_string("Internal server error"))
        .mount(&mock_server)
        .await;

    let base_url = format!("{}/v1", mock_server.uri());
    let client = create_provider_client(
        AiProvider::OpenaiCompatible,
        Some(&base_url),
        "gpt-4o-mini",
        Some("sk-key"),
        Some(Duration::from_secs(5)),
    )
    .unwrap();

    let err = client.test_connection().await.unwrap_err();
    assert_eq!(err.code(), "AIConnection");
}

#[tokio::test]
async fn test_openai_model_error_400() {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
            "error": "The model `gpt-unknown` does not exist"
        })))
        .mount(&mock_server)
        .await;

    let base_url = format!("{}/v1", mock_server.uri());
    let client = create_provider_client(
        AiProvider::OpenaiCompatible,
        Some(&base_url),
        "gpt-unknown",
        Some("sk-key"),
        Some(Duration::from_secs(5)),
    )
    .unwrap();

    let err = client.test_connection().await.unwrap_err();
    assert_eq!(err.code(), "AIModel");
}

#[tokio::test]
async fn test_openai_malformed_response_200() {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_string("invalid-non-json"))
        .mount(&mock_server)
        .await;

    let base_url = format!("{}/v1", mock_server.uri());
    let client = create_provider_client(
        AiProvider::OpenaiCompatible,
        Some(&base_url),
        "gpt-4o-mini",
        Some("sk-key"),
        Some(Duration::from_secs(5)),
    )
    .unwrap();

    let err = client.test_connection().await.unwrap_err();
    assert_eq!(err.code(), "AIInvalidResponse");
}

#[tokio::test]
async fn test_openai_empty_content_200() {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "choices": [{ "message": { "content": "   " } }]
        })))
        .mount(&mock_server)
        .await;

    let base_url = format!("{}/v1", mock_server.uri());
    let client = create_provider_client(
        AiProvider::OpenaiCompatible,
        Some(&base_url),
        "gpt-4o-mini",
        Some("sk-key"),
        Some(Duration::from_secs(5)),
    )
    .unwrap();

    let err = client.test_connection().await.unwrap_err();
    assert_eq!(err.code(), "AIInvalidResponse");
}

#[tokio::test]
async fn test_openai_timeout() {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(Duration::from_millis(200))
                .set_body_json(serde_json::json!({
                    "choices": [{ "message": { "content": "OK" } }]
                })),
        )
        .mount(&mock_server)
        .await;

    let base_url = format!("{}/v1", mock_server.uri());
    let client = create_provider_client(
        AiProvider::OpenaiCompatible,
        Some(&base_url),
        "gpt-4o-mini",
        Some("sk-key"),
        Some(Duration::from_millis(50)),
    )
    .unwrap();

    let err = client.test_connection().await.unwrap_err();
    assert_eq!(err.code(), "AIConnection");
    assert!(err.to_string().contains("timed out"));
}

// =============================================================================
// 3. Anthropic Tests
// =============================================================================

#[tokio::test]
async fn test_anthropic_success() {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .and(header("x-api-key", "ant-secret-key"))
        .and(header("anthropic-version", "2023-06-01"))
        .and(header("Content-Type", "application/json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "msg_123",
            "type": "message",
            "role": "assistant",
            "content": [
                {
                    "type": "text",
                    "text": "OK"
                }
            ]
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = create_provider_client(
        AiProvider::Anthropic,
        Some(&mock_server.uri()),
        "claude-3-5-sonnet",
        Some("ant-secret-key"),
        Some(Duration::from_secs(5)),
    )
    .unwrap();

    let res = client.test_connection().await.unwrap();
    assert_eq!(res.provider, "Anthropic");
    assert_eq!(res.model, "claude-3-5-sonnet");
    assert!(res.success);
}

#[tokio::test]
async fn test_anthropic_generate_text_system_and_user() {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "content": [
                {
                    "type": "text",
                    "text": "Claude reply"
                }
            ]
        })))
        .mount(&mock_server)
        .await;

    let client = create_provider_client(
        AiProvider::Anthropic,
        Some(&mock_server.uri()),
        "claude-3-5-sonnet",
        Some("ant-key"),
        Some(Duration::from_secs(5)),
    )
    .unwrap();

    let text = client
        .generate_text("System instructions", "User query")
        .await
        .unwrap();
    assert_eq!(text, "Claude reply");

    let requests = mock_server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(body["model"], "claude-3-5-sonnet");
    assert_eq!(body["max_tokens"], 512);
    assert_eq!(body["system"], "System instructions");
    assert_eq!(body["messages"][0]["role"], "user");
    assert_eq!(body["messages"][0]["content"], "User query");
}

#[tokio::test]
async fn test_anthropic_auth_401() {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
            "type": "error",
            "error": { "type": "authentication_error", "message": "invalid x-api-key" }
        })))
        .mount(&mock_server)
        .await;

    let secret = "ant-invalid-secret-key";
    let client = create_provider_client(
        AiProvider::Anthropic,
        Some(&mock_server.uri()),
        "claude-3-5-sonnet",
        Some(secret),
        Some(Duration::from_secs(5)),
    )
    .unwrap();

    let err = client.test_connection().await.unwrap_err();
    assert_eq!(err.code(), "AIAuth");
    assert!(!err.to_string().contains(secret));
}

#[tokio::test]
async fn test_anthropic_model_404() {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
            "type": "error",
            "error": { "type": "not_found_error", "message": "model not found" }
        })))
        .mount(&mock_server)
        .await;

    let client = create_provider_client(
        AiProvider::Anthropic,
        Some(&mock_server.uri()),
        "claude-unknown",
        Some("ant-key"),
        Some(Duration::from_secs(5)),
    )
    .unwrap();

    let err = client.test_connection().await.unwrap_err();
    assert_eq!(err.code(), "AIModel");
}

#[tokio::test]
async fn test_anthropic_rate_limit_529_or_429() {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(429).set_body_string("Overloaded"))
        .mount(&mock_server)
        .await;

    let client = create_provider_client(
        AiProvider::Anthropic,
        Some(&mock_server.uri()),
        "claude-3-5-sonnet",
        Some("ant-key"),
        Some(Duration::from_secs(5)),
    )
    .unwrap();

    let err = client.test_connection().await.unwrap_err();
    assert_eq!(err.code(), "AIConnection");
}

#[tokio::test]
async fn test_anthropic_timeout() {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(Duration::from_millis(200))
                .set_body_json(serde_json::json!({
                    "content": [{ "type": "text", "text": "OK" }]
                })),
        )
        .mount(&mock_server)
        .await;

    let client = create_provider_client(
        AiProvider::Anthropic,
        Some(&mock_server.uri()),
        "claude-3-5-sonnet",
        Some("ant-key"),
        Some(Duration::from_millis(50)),
    )
    .unwrap();

    let err = client.test_connection().await.unwrap_err();
    assert_eq!(err.code(), "AIConnection");
    assert!(err.to_string().contains("timed out"));
}

#[tokio::test]
async fn test_anthropic_malformed_response_200() {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_string("invalid-non-json"))
        .mount(&mock_server)
        .await;

    let client = create_provider_client(
        AiProvider::Anthropic,
        Some(&mock_server.uri()),
        "claude-3-5-sonnet",
        Some("ant-key"),
        Some(Duration::from_secs(5)),
    )
    .unwrap();

    let err = client.test_connection().await.unwrap_err();
    assert_eq!(err.code(), "AIInvalidResponse");
}

#[tokio::test]
async fn test_anthropic_empty_content_200() {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "content": [{ "type": "text", "text": "   " }]
        })))
        .mount(&mock_server)
        .await;

    let client = create_provider_client(
        AiProvider::Anthropic,
        Some(&mock_server.uri()),
        "claude-3-5-sonnet",
        Some("ant-key"),
        Some(Duration::from_secs(5)),
    )
    .unwrap();

    let err = client.test_connection().await.unwrap_err();
    assert_eq!(err.code(), "AIInvalidResponse");
}

// =============================================================================
// 4. Gemini Tests
// =============================================================================

#[tokio::test]
async fn test_gemini_success() {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/interactions"))
        .and(header("x-goog-api-key", "gemini-secret-key"))
        .and(header("Content-Type", "application/json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "steps": [
                {
                    "type": "model_output",
                    "content": {
                        "type": "text",
                        "text": "OK"
                    }
                }
            ]
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = create_provider_client(
        AiProvider::Gemini,
        Some(&mock_server.uri()),
        "gemini-2.5-flash",
        Some("gemini-secret-key"),
        Some(Duration::from_secs(5)),
    )
    .unwrap();

    let res = client.test_connection().await.unwrap();
    assert_eq!(res.provider, "Google Gemini");
    assert_eq!(res.model, "gemini-2.5-flash");
    assert!(res.success);
}

#[tokio::test]
async fn test_gemini_generate_text_system_and_user() {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/interactions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "steps": [
                {
                    "type": "model_output",
                    "content": [{ "type": "text", "text": "Gemini reply" }]
                }
            ]
        })))
        .mount(&mock_server)
        .await;

    let client = create_provider_client(
        AiProvider::Gemini,
        Some(&mock_server.uri()),
        "gemini-2.5-flash",
        Some("gemini-key"),
        Some(Duration::from_secs(5)),
    )
    .unwrap();

    let text = client
        .generate_text("System instructions", "User prompt")
        .await
        .unwrap();
    assert_eq!(text, "Gemini reply");

    let requests = mock_server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(body["model"], "gemini-2.5-flash");
    assert_eq!(body["input"], "User prompt");
    assert_eq!(body["system_instruction"], "System instructions");
    assert_eq!(body["store"], false);
    assert_eq!(body["generation_config"]["max_output_tokens"], 512);
}

#[tokio::test]
async fn test_gemini_store_false_and_headers() {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/interactions"))
        .and(header("x-goog-api-key", "gemini-test-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "steps": [
                {
                    "type": "model_output",
                    "content": [{ "type": "text", "text": "OK" }]
                }
            ]
        })))
        .mount(&mock_server)
        .await;

    let client = create_provider_client(
        AiProvider::Gemini,
        Some(&mock_server.uri()),
        "gemini-2.5-flash",
        Some("gemini-test-key"),
        Some(Duration::from_secs(5)),
    )
    .unwrap();

    let res = client.test_connection().await.unwrap();
    assert!(res.success);

    let requests = mock_server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(body["store"], false);
    assert_eq!(body["model"], "gemini-2.5-flash");
    assert_eq!(body["generation_config"]["max_output_tokens"], 512);
}

#[tokio::test]
async fn test_gemini_auth_401() {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/interactions"))
        .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
            "error": { "code": 401, "message": "API key not valid" }
        })))
        .mount(&mock_server)
        .await;

    let secret = "gemini-bad-key-123";
    let client = create_provider_client(
        AiProvider::Gemini,
        Some(&mock_server.uri()),
        "gemini-2.5-flash",
        Some(secret),
        Some(Duration::from_secs(5)),
    )
    .unwrap();

    let err = client.test_connection().await.unwrap_err();
    assert_eq!(err.code(), "AIAuth");
    assert!(!err.to_string().contains(secret));
}

#[tokio::test]
async fn test_gemini_model_404() {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/interactions"))
        .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
            "error": { "code": 404, "message": "Model not found" }
        })))
        .mount(&mock_server)
        .await;

    let client = create_provider_client(
        AiProvider::Gemini,
        Some(&mock_server.uri()),
        "gemini-unknown",
        Some("gemini-key"),
        Some(Duration::from_secs(5)),
    )
    .unwrap();

    let err = client.test_connection().await.unwrap_err();
    assert_eq!(err.code(), "AIModel");
}

#[tokio::test]
async fn test_gemini_rate_limit_429() {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/interactions"))
        .respond_with(ResponseTemplate::new(429).set_body_string("Quota exceeded"))
        .mount(&mock_server)
        .await;

    let client = create_provider_client(
        AiProvider::Gemini,
        Some(&mock_server.uri()),
        "gemini-2.5-flash",
        Some("gemini-key"),
        Some(Duration::from_secs(5)),
    )
    .unwrap();

    let err = client.test_connection().await.unwrap_err();
    assert_eq!(err.code(), "AIConnection");
}

#[tokio::test]
async fn test_gemini_server_error_500() {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/interactions"))
        .respond_with(ResponseTemplate::new(500).set_body_string("Internal error"))
        .mount(&mock_server)
        .await;

    let client = create_provider_client(
        AiProvider::Gemini,
        Some(&mock_server.uri()),
        "gemini-2.5-flash",
        Some("gemini-key"),
        Some(Duration::from_secs(5)),
    )
    .unwrap();

    let err = client.test_connection().await.unwrap_err();
    assert_eq!(err.code(), "AIConnection");
}

#[tokio::test]
async fn test_gemini_timeout() {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/interactions"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(Duration::from_millis(200))
                .set_body_json(serde_json::json!({
                    "steps": [{ "type": "model_output", "content": [{ "text": "OK" }] }]
                })),
        )
        .mount(&mock_server)
        .await;

    let client = create_provider_client(
        AiProvider::Gemini,
        Some(&mock_server.uri()),
        "gemini-2.5-flash",
        Some("gemini-key"),
        Some(Duration::from_millis(50)),
    )
    .unwrap();

    let err = client.test_connection().await.unwrap_err();
    assert_eq!(err.code(), "AIConnection");
    assert!(err.to_string().contains("timed out"));
}

#[tokio::test]
async fn test_gemini_malformed_response_200() {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/interactions"))
        .respond_with(ResponseTemplate::new(200).set_body_string("invalid-non-json"))
        .mount(&mock_server)
        .await;

    let client = create_provider_client(
        AiProvider::Gemini,
        Some(&mock_server.uri()),
        "gemini-2.5-flash",
        Some("gemini-key"),
        Some(Duration::from_secs(5)),
    )
    .unwrap();

    let err = client.test_connection().await.unwrap_err();
    assert_eq!(err.code(), "AIInvalidResponse");
}

#[tokio::test]
async fn test_gemini_empty_content_200() {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/interactions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "steps": [
                {
                    "type": "model_output",
                    "content": [{ "type": "text", "text": "   " }]
                }
            ]
        })))
        .mount(&mock_server)
        .await;

    let client = create_provider_client(
        AiProvider::Gemini,
        Some(&mock_server.uri()),
        "gemini-2.5-flash",
        Some("gemini-key"),
        Some(Duration::from_secs(5)),
    )
    .unwrap();

    let err = client.test_connection().await.unwrap_err();
    assert_eq!(err.code(), "AIInvalidResponse");
}

// =============================================================================
// 5. Ollama Tests
// =============================================================================

#[tokio::test]
async fn test_ollama_success_and_no_auth_header() {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "model": "llama3.2",
            "message": {
                "role": "assistant",
                "content": "OK"
            },
            "done": true
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = create_provider_client(
        AiProvider::Ollama,
        Some(&mock_server.uri()),
        "llama3.2",
        None, // No key required for Ollama
        Some(Duration::from_secs(5)),
    )
    .unwrap();

    let res = client.test_connection().await.unwrap();
    assert_eq!(res.provider, "Ollama");
    assert_eq!(res.model, "llama3.2");
    assert!(res.success);

    // Verify recorded request had NO Authorization or API key headers
    let requests = mock_server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    let req = &requests[0];
    assert!(req.headers.get("Authorization").is_none());
    assert!(req.headers.get("x-api-key").is_none());
    assert!(req.headers.get("x-goog-api-key").is_none());

    // Verify stream: false and num_predict: 512 in body
    let body: serde_json::Value = serde_json::from_slice(&req.body).unwrap();
    assert_eq!(body["stream"], false);
    assert_eq!(body["model"], "llama3.2");
    assert_eq!(body["options"]["num_predict"], 512);
}

#[tokio::test]
async fn test_ollama_generate_text_system_and_user() {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "message": { "content": "Ollama reply" }
        })))
        .mount(&mock_server)
        .await;

    let client = create_provider_client(
        AiProvider::Ollama,
        Some(&mock_server.uri()),
        "llama3.2",
        None,
        Some(Duration::from_secs(5)),
    )
    .unwrap();

    let text = client
        .generate_text("System instructions", "User prompt")
        .await
        .unwrap();
    assert_eq!(text, "Ollama reply");

    let requests = mock_server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(body["model"], "llama3.2");
    assert_eq!(body["stream"], false);
    assert_eq!(body["options"]["num_predict"], 512);
    assert_eq!(body["messages"][0]["role"], "system");
    assert_eq!(body["messages"][0]["content"], "System instructions");
    assert_eq!(body["messages"][1]["role"], "user");
    assert_eq!(body["messages"][1]["content"], "User prompt");
}

#[tokio::test]
async fn test_ollama_model_404() {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
            "error": "model 'nonexistent' not found"
        })))
        .mount(&mock_server)
        .await;

    let client = create_provider_client(
        AiProvider::Ollama,
        Some(&mock_server.uri()),
        "nonexistent",
        None,
        Some(Duration::from_secs(5)),
    )
    .unwrap();

    let err = client.test_connection().await.unwrap_err();
    assert_eq!(err.code(), "AIModel");
}

#[tokio::test]
async fn test_ollama_server_error_500() {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(ResponseTemplate::new(500).set_body_string("Internal error in Ollama"))
        .mount(&mock_server)
        .await;

    let client = create_provider_client(
        AiProvider::Ollama,
        Some(&mock_server.uri()),
        "llama3.2",
        None,
        Some(Duration::from_secs(5)),
    )
    .unwrap();

    let err = client.test_connection().await.unwrap_err();
    assert_eq!(err.code(), "AIConnection");
}

#[tokio::test]
async fn test_ollama_timeout() {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(Duration::from_millis(200))
                .set_body_json(serde_json::json!({
                    "message": { "content": "OK" }
                })),
        )
        .mount(&mock_server)
        .await;

    let client = create_provider_client(
        AiProvider::Ollama,
        Some(&mock_server.uri()),
        "llama3.2",
        None,
        Some(Duration::from_millis(50)),
    )
    .unwrap();

    let err = client.test_connection().await.unwrap_err();
    assert_eq!(err.code(), "AIConnection");
    assert!(err.to_string().contains("timed out"));
}

#[tokio::test]
async fn test_ollama_malformed_response_200() {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(ResponseTemplate::new(200).set_body_string("invalid-non-json"))
        .mount(&mock_server)
        .await;

    let client = create_provider_client(
        AiProvider::Ollama,
        Some(&mock_server.uri()),
        "llama3.2",
        None,
        Some(Duration::from_secs(5)),
    )
    .unwrap();

    let err = client.test_connection().await.unwrap_err();
    assert_eq!(err.code(), "AIInvalidResponse");
}

#[tokio::test]
async fn test_ollama_empty_content_200() {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "message": { "content": "   " }
        })))
        .mount(&mock_server)
        .await;

    let client = create_provider_client(
        AiProvider::Ollama,
        Some(&mock_server.uri()),
        "llama3.2",
        None,
        Some(Duration::from_secs(5)),
    )
    .unwrap();

    let err = client.test_connection().await.unwrap_err();
    assert_eq!(err.code(), "AIInvalidResponse");
}

// =============================================================================
// 6. Secret Leak Sentinel Regression Tests
// =============================================================================

#[tokio::test]
async fn test_secret_leak_sentinel_never_in_errors() {
    let sentinel_key = "SUPER_SECRET_DO_NOT_LEAK";
    let mock_server = MockServer::start().await;

    // Return 400 Bad Request with body containing the sentinel
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
            "error": format!("Invalid param containing {}", sentinel_key)
        })))
        .mount(&mock_server)
        .await;

    let base_url = format!("{}/v1", mock_server.uri());
    let client = create_provider_client(
        AiProvider::OpenaiCompatible,
        Some(&base_url),
        "gpt-4o-mini",
        Some(sentinel_key),
        Some(Duration::from_secs(5)),
    )
    .unwrap();

    let err = client.test_connection().await.unwrap_err();

    // Verify sentinel is NOT in Display or Debug format of AppError
    let display_str = err.to_string();
    let debug_str = format!("{:?}", err);

    assert!(
        !display_str.contains(sentinel_key),
        "Error display must not contain secret sentinel! Found: {}",
        display_str
    );
    assert!(
        !debug_str.contains(sentinel_key),
        "Error debug must not contain secret sentinel! Found: {}",
        debug_str
    );
}

// =============================================================================
// 7. Validation & SecretStore Service Integration Tests
// =============================================================================

#[tokio::test]
async fn test_service_disabled_ai_can_execute_connection() {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "choices": [{ "message": { "content": "OK" } }]
        })))
        .mount(&mock_server)
        .await;

    let dir = tempdir().unwrap();
    let config_path = dir.path().join("settings.json");
    let settings_store = SettingsStore::new(config_path).unwrap();

    let mut settings = settings_store.get().unwrap();
    settings.ai.enabled = false; // explicitly false!
    settings.ai.provider = Some(AiProvider::OpenaiCompatible);
    settings.ai.base_url = Some(format!("{}/v1", mock_server.uri()));
    settings.ai.model = Some("gpt-4o-mini".to_string());
    settings_store.save(&settings).unwrap();

    let secret_store: Arc<dyn SecretStore> = Arc::new(InMemorySecretStore::new());
    secret_store
        .set_secret(SECRET_KEY_AI_API_KEY, "sk-test-key")
        .unwrap();

    let res = execute_test_ai_connection(&settings_store, &secret_store)
        .await
        .unwrap();
    assert_eq!(res.provider, "OpenAI Compatible");
    assert_eq!(res.model, "gpt-4o-mini");
    assert!(res.success);
}

#[tokio::test]
async fn test_service_no_provider_rejected() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("settings.json");
    let settings_store = SettingsStore::new(config_path).unwrap();

    let mut settings = settings_store.get().unwrap();
    settings.ai.provider = None;
    settings_store.save(&settings).unwrap();

    let secret_store: Arc<dyn SecretStore> = Arc::new(InMemorySecretStore::new());

    let err = execute_test_ai_connection(&settings_store, &secret_store)
        .await
        .unwrap_err();
    assert_eq!(err.code(), "SettingsValidation");
    assert!(err.to_string().contains("未选择 AI Provider"));
}

#[tokio::test]
async fn test_service_missing_model_rejected() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("settings.json");
    let settings_store = SettingsStore::new(config_path).unwrap();

    let mut settings = settings_store.get().unwrap();
    settings.ai.enabled = true;
    settings.ai.provider = Some(AiProvider::OpenaiCompatible);
    settings.ai.model = None; // Blank model
    settings_store.save(&settings).unwrap();

    let secret_store: Arc<dyn SecretStore> = Arc::new(InMemorySecretStore::new());

    let err = execute_test_ai_connection(&settings_store, &secret_store)
        .await
        .unwrap_err();
    assert_eq!(err.code(), "AIModel");
    assert!(err.to_string().contains("必须提供已配置的 Model 名称"));
}

#[tokio::test]
async fn test_service_cloud_missing_key_rejected() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("settings.json");
    let settings_store = SettingsStore::new(config_path).unwrap();

    let mut settings = settings_store.get().unwrap();
    settings.ai.enabled = true;
    settings.ai.provider = Some(AiProvider::Anthropic);
    settings.ai.model = Some("claude-3-5-sonnet".to_string());
    settings_store.save(&settings).unwrap();

    let secret_store: Arc<dyn SecretStore> = Arc::new(InMemorySecretStore::new());
    // No secret configured

    let err = execute_test_ai_connection(&settings_store, &secret_store)
        .await
        .unwrap_err();
    assert_eq!(err.code(), "AIAuth");
    assert!(err.to_string().contains("未配置 Anthropic 的 API Key"));
}

#[tokio::test]
async fn test_service_ollama_missing_key_accepted() {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "model": "llama3.2",
            "message": { "content": "OK" }
        })))
        .mount(&mock_server)
        .await;

    let dir = tempdir().unwrap();
    let config_path = dir.path().join("settings.json");
    let settings_store = SettingsStore::new(config_path).unwrap();

    let mut settings = settings_store.get().unwrap();
    settings.ai.enabled = false; // also verify with false!
    settings.ai.provider = Some(AiProvider::Ollama);
    settings.ai.base_url = Some(mock_server.uri());
    settings.ai.model = Some("llama3.2".to_string());
    settings_store.save(&settings).unwrap();

    let secret_store: Arc<dyn SecretStore> = Arc::new(InMemorySecretStore::new());
    // No secret configured in secret_store, but Ollama should succeed!

    let res = execute_test_ai_connection(&settings_store, &secret_store)
        .await
        .unwrap();
    assert_eq!(res.provider, "Ollama");
    assert_eq!(res.model, "llama3.2");
    assert!(res.success);
}

// =============================================================================
// 5. Phase 6-B: AI Natural-Language Search Tests
// =============================================================================

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

    fn generate_text<'a>(
        &'a self,
        _system_instruction: &'a str,
        _user_input: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String, AppError>> + Send + 'a>>
    {
        let resp = self.responses.lock().unwrap().remove(0);
        Box::pin(async move { resp })
    }
}

async fn setup_test_db() -> (tempfile::TempDir, sqlx::SqlitePool) {
    let dir = tempdir().expect("Failed to create tempdir");
    let db_path = dir.path().join("test_p6b.db");
    let pool = init_pool(&db_path)
        .await
        .expect("Failed to initialize test pool");
    (dir, pool)
}

#[test]
fn test_ai_search_readiness_all_cases() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("settings.json");
    let settings_store = SettingsStore::new(config_path).unwrap();

    let secret_store: Arc<dyn SecretStore> = Arc::new(InMemorySecretStore::new());

    // 1. AI disabled
    let mut settings = settings_store.get().unwrap();
    settings.ai.enabled = false;
    let r1 = check_ai_search_readiness(&settings, &*secret_store);
    assert!(!r1.ready);
    assert!(r1.reason.unwrap().contains("未启用"));

    // 2. AI enabled, no provider
    settings.ai.enabled = true;
    settings.ai.provider = None;
    let r2 = check_ai_search_readiness(&settings, &*secret_store);
    assert!(!r2.ready);
    assert!(r2.reason.unwrap().contains("未选择"));

    // 3. Provider set, no model
    settings.ai.provider = Some(AiProvider::OpenaiCompatible);
    settings.ai.model = None;
    let r3 = check_ai_search_readiness(&settings, &*secret_store);
    assert!(!r3.ready);
    assert!(r3.reason.unwrap().contains("未配置 AI Model"));

    // 4. Invalid base URL
    settings.ai.model = Some("gpt-4o".to_string());
    settings.ai.base_url = Some("http://api.remote-invalid.com/v1".to_string());
    let r4 = check_ai_search_readiness(&settings, &*secret_store);
    assert!(!r4.ready);
    assert!(r4.reason.unwrap().contains("Base URL 无效"));

    // 5. Cloud provider missing key
    settings.ai.base_url = None;
    let r5 = check_ai_search_readiness(&settings, &*secret_store);
    assert!(!r5.ready);
    assert!(r5.reason.unwrap().contains("未配置 AI API Key"));

    // 6. Cloud provider with key configured -> ready!
    secret_store
        .set_secret(SECRET_KEY_AI_API_KEY, "test-key")
        .unwrap();
    let r6: AiSearchReadiness = check_ai_search_readiness(&settings, &*secret_store);
    assert!(r6.ready);
    assert!(r6.reason.is_none());

    // 7. Ollama without key -> ready!
    secret_store.delete_secret(SECRET_KEY_AI_API_KEY).unwrap();
    settings.ai.provider = Some(AiProvider::Ollama);
    settings.ai.model = Some("llama3.2".to_string());
    let r7 = check_ai_search_readiness(&settings, &*secret_store);
    assert!(r7.ready);
    assert!(r7.reason.is_none());
}

#[test]
fn test_ai_search_input_validation_rules() {
    // Empty query
    let req1 = AiRepositorySearchRequest {
        input: "   ".to_string(),
        min_stars: None,
        pushed_after: None,
        limit: 20,
        exclude_selected: false,
        exclude_favorites: false,
    };
    assert!(validate_ai_search_input(&req1).is_err());

    // > 2000 chars
    let req2 = AiRepositorySearchRequest {
        input: "a".repeat(2001),
        min_stars: None,
        pushed_after: None,
        limit: 20,
        exclude_selected: false,
        exclude_favorites: false,
    };
    assert!(validate_ai_search_input(&req2).is_err());

    // Limit out of bounds
    let req3 = AiRepositorySearchRequest {
        input: "valid".to_string(),
        min_stars: None,
        pushed_after: None,
        limit: 0,
        exclude_selected: false,
        exclude_favorites: false,
    };
    assert!(validate_ai_search_input(&req3).is_err());

    let req4 = AiRepositorySearchRequest {
        input: "valid".to_string(),
        min_stars: None,
        pushed_after: None,
        limit: 101,
        exclude_selected: false,
        exclude_favorites: false,
    };
    assert!(validate_ai_search_input(&req4).is_err());

    // Negative stars
    let req5 = AiRepositorySearchRequest {
        input: "valid".to_string(),
        min_stars: Some(-5),
        pushed_after: None,
        limit: 20,
        exclude_selected: false,
        exclude_favorites: false,
    };
    assert!(validate_ai_search_input(&req5).is_err());

    // Invalid date format
    let req6 = AiRepositorySearchRequest {
        input: "valid".to_string(),
        min_stars: None,
        pushed_after: Some("2026/01/01".to_string()),
        limit: 20,
        exclude_selected: false,
        exclude_favorites: false,
    };
    assert!(validate_ai_search_input(&req6).is_err());

    // Valid request
    let req_ok = AiRepositorySearchRequest {
        input: "  instagram scraper tools  ".to_string(),
        min_stars: Some(100),
        pushed_after: Some("2026-01-01".to_string()),
        limit: 20,
        exclude_selected: true,
        exclude_favorites: true,
    };
    assert_eq!(
        validate_ai_search_input(&req_ok).unwrap(),
        "instagram scraper tools"
    );
}

#[tokio::test]
async fn test_ai_search_repair_flow_success() {
    // First response contains forbidden qualifier "stars:>=10"
    let bad_response = Ok(r#"{"queries":["scraper stars:>=10"]}"#.to_string());
    // Repair response is valid
    let good_response = Ok(r#"{"queries":["instagram scraper","instagram api"]}"#.to_string());

    let mock_client = MockAiClient::new(vec![bad_response, good_response]);
    let queries = generate_search_queries(&mock_client, "instagram scraper")
        .await
        .unwrap();

    assert_eq!(queries, vec!["instagram scraper", "instagram api"]);
}

#[tokio::test]
async fn test_ai_search_repair_flow_double_failure_fails() {
    // Both responses are invalid JSON
    let bad1 = Ok("Not JSON at all".to_string());
    let bad2 = Ok("Still not JSON".to_string());

    let mock_client = MockAiClient::new(vec![bad1, bad2]);
    let err = generate_search_queries(&mock_client, "instagram scraper")
        .await
        .unwrap_err();

    assert_eq!(err.code(), "AIInvalidResponse");
    assert!(err.to_string().contains("repair attempt"));
}

#[tokio::test]
async fn test_multi_query_candidate_retrieval_and_rrf_ranking() {
    let (_dir, pool) = setup_test_db().await;
    let mock_server = MockServer::start().await;

    // AI queries: "query A" and "query B"
    let ai_resp = Ok(r#"{"queries":["query A","query B"]}"#.to_string());
    let mock_client = MockAiClient::new(vec![ai_resp]);

    // Query A returns Item 101 (rank 1, stars 500) and Item 102 (rank 2, stars 100)
    let q_a_body = serde_json::json!({
        "total_count": 2,
        "items": [
            { "id": 101, "html_url": "https://github.com/test/repo101", "name": "repo101", "stargazers_count": 500, "private": false },
            { "id": 102, "html_url": "https://github.com/test/repo102", "name": "repo102", "stargazers_count": 100, "private": false }
        ]
    });

    // Query B returns Item 102 (rank 1, stars 100), Item 103 (rank 2, stars 50), and Item 101 (rank 3, stars 500)
    let q_b_body = serde_json::json!({
        "total_count": 3,
        "items": [
            { "id": 102, "html_url": "https://github.com/test/repo102", "name": "repo102", "stargazers_count": 100, "private": false },
            { "id": 103, "html_url": "https://github.com/test/repo103", "name": "repo103", "stargazers_count": 50, "private": false },
            { "id": 101, "html_url": "https://github.com/test/repo101", "name": "repo101", "stargazers_count": 500, "private": false }
        ]
    });

    Mock::given(method("GET"))
        .and(path("/search/repositories"))
        .and(query_param("q", "query A"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&q_a_body))
        .expect(1)
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/search/repositories"))
        .and(query_param("q", "query B"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&q_b_body))
        .expect(1)
        .mount(&mock_server)
        .await;

    let github = GitHubClient::with_base_url(&mock_server.uri());
    let req = AiRepositorySearchRequest {
        input: "search terms".to_string(),
        min_stars: None,
        pushed_after: None,
        limit: 10,
        exclude_selected: false,
        exclude_favorites: false,
    };

    let resp = execute_ai_search_with_client(&pool, &mock_client, &github, None, &req)
        .await
        .unwrap();

    assert_eq!(resp.generated_queries, vec!["query A", "query B"]);
    assert!(!resp.partial);
    assert_eq!(resp.items.len(), 3);

    // RRF scores:
    // Item 102: 1/(60+2) + 1/(60+1) = 0.032522
    // Item 101: 1/(60+1) + 1/(60+3) = 0.032266
    // Item 103: 1/(60+2) = 0.016129
    // Therefore, Item 102 is #1, Item 101 is #2, Item 103 is #3!
    assert_eq!(resp.items[0].github_repo_id, 102);
    assert_eq!(resp.items[1].github_repo_id, 101);
    assert_eq!(resp.items[2].github_repo_id, 103);

    // Verify all 3 items recorded to selection_history
    assert_eq!(count_selection_history(&pool).await.unwrap(), 3);
}

#[tokio::test]
async fn test_single_query_preserves_github_best_match_order() {
    let (_dir, pool) = setup_test_db().await;
    let mock_server = MockServer::start().await;

    let ai_resp = Ok(r#"{"queries":["single query"]}"#.to_string());
    let mock_client = MockAiClient::new(vec![ai_resp]);

    let body = serde_json::json!({
        "total_count": 3,
        "items": [
            { "id": 501, "html_url": "https://github.com/test/repo501", "name": "repo501", "stargazers_count": 10, "private": false },
            { "id": 502, "html_url": "https://github.com/test/repo502", "name": "repo502", "stargazers_count": 1000, "private": false },
            { "id": 503, "html_url": "https://github.com/test/repo503", "name": "repo503", "stargazers_count": 50, "private": false }
        ]
    });

    Mock::given(method("GET"))
        .and(path("/search/repositories"))
        .and(query_param("q", "single query"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&body))
        .expect(1)
        .mount(&mock_server)
        .await;

    let github = GitHubClient::with_base_url(&mock_server.uri());
    let req = AiRepositorySearchRequest {
        input: "find single".to_string(),
        min_stars: None,
        pushed_after: None,
        limit: 10,
        exclude_selected: false,
        exclude_favorites: false,
    };

    let resp = execute_ai_search_with_client(&pool, &mock_client, &github, None, &req)
        .await
        .unwrap();

    // Exactly matches initial order despite 502 having more stars
    assert_eq!(resp.items.len(), 3);
    assert_eq!(resp.items[0].github_repo_id, 501);
    assert_eq!(resp.items[1].github_repo_id, 502);
    assert_eq!(resp.items[2].github_repo_id, 503);
    assert_eq!(resp.generated_queries, vec!["single query"]);
    assert!(!resp.partial);
}

#[tokio::test]
async fn test_multi_query_exclusions_and_atomic_history() {
    let (_dir, pool) = setup_test_db().await;
    let mock_server = MockServer::start().await;

    // Favorited item 601 (will be excluded)
    favorite_repository(
        &pool,
        FavoriteInput {
            github_repo_id: Some(601),
            github_url: "https://github.com/test/repo601".to_string(),
            name: "repo601".to_string(),
            description: None,
            category: "Other".to_string(),
            stars: 10,
            license: None,
            topics: vec![],
            pushed_at: None,
        },
    )
    .await
    .unwrap();

    // Pre-insert item 602 into selection_history (will be excluded)
    reposhelf_lib::db::selection_history::insert_selection(
        &pool,
        &reposhelf_lib::db::models::NewSelectionHistoryRecord {
            github_repo_id: Some(602),
            github_url: "https://github.com/test/repo602".to_string(),
            selected_at: "2026-01-01T00:00:00Z".to_string(),
        },
    )
    .await
    .unwrap();

    let ai_resp = Ok(r#"{"queries":["query X"]}"#.to_string());
    let mock_client = MockAiClient::new(vec![ai_resp]);

    let body = serde_json::json!({
        "total_count": 4,
        "items": [
            { "id": 601, "html_url": "https://github.com/test/repo601", "name": "repo601", "stargazers_count": 10, "private": false },
            { "id": 602, "html_url": "https://github.com/test/repo602", "name": "repo602", "stargazers_count": 10, "private": false },
            { "id": 603, "html_url": "https://github.com/test/repo603", "name": "repo603", "stargazers_count": 30, "private": false },
            { "id": 604, "html_url": "https://github.com/test/repo604", "name": "repo604", "stargazers_count": 40, "private": false }
        ]
    });

    Mock::given(method("GET"))
        .and(path("/search/repositories"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&body))
        .mount(&mock_server)
        .await;

    let github = GitHubClient::with_base_url(&mock_server.uri());
    let req = AiRepositorySearchRequest {
        input: "find repos".to_string(),
        min_stars: None,
        pushed_after: None,
        limit: 10,
        exclude_selected: true,
        exclude_favorites: true,
    };

    let resp = execute_ai_search_with_client(&pool, &mock_client, &github, None, &req)
        .await
        .unwrap();

    // 601 and 602 excluded; 603 and 604 survive
    assert_eq!(resp.items.len(), 2);
    assert_eq!(resp.items[0].github_repo_id, 603);
    assert_eq!(resp.items[1].github_repo_id, 604);

    // Initial count was 1 (from 602); now 1 + 2 = 3
    assert_eq!(count_selection_history(&pool).await.unwrap(), 3);
}

#[tokio::test]
async fn test_multi_query_candidate_budget_and_graceful_degradation() {
    let (_dir, pool) = setup_test_db().await;
    let mock_server = MockServer::start().await;

    let ai_resp = Ok(r#"{"queries":["query 1","query 2"]}"#.to_string());
    let mock_client = MockAiClient::new(vec![ai_resp]);

    // Query 1 succeeds and returns 2 items
    let q1_body = serde_json::json!({
        "total_count": 2,
        "items": [
            { "id": 701, "html_url": "https://github.com/test/repo701", "name": "repo701", "stargazers_count": 10, "private": false },
            { "id": 702, "html_url": "https://github.com/test/repo702", "name": "repo702", "stargazers_count": 20, "private": false }
        ]
    });

    Mock::given(method("GET"))
        .and(path("/search/repositories"))
        .and(query_param("q", "query 1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&q1_body))
        .expect(1)
        .mount(&mock_server)
        .await;

    // Query 2 hits rate limit 429
    Mock::given(method("GET"))
        .and(path("/search/repositories"))
        .and(query_param("q", "query 2"))
        .respond_with(ResponseTemplate::new(429))
        .expect(1)
        .mount(&mock_server)
        .await;

    let github = GitHubClient::with_base_url(&mock_server.uri());
    let req = AiRepositorySearchRequest {
        input: "test query".to_string(),
        min_stars: None,
        pushed_after: None,
        limit: 10,
        exclude_selected: false,
        exclude_favorites: false,
    };

    let resp = execute_ai_search_with_client(&pool, &mock_client, &github, None, &req)
        .await
        .unwrap();

    // Partial degradation succeeds with items from Query 1
    assert_eq!(resp.items.len(), 2);
    assert!(resp.partial);
    assert_eq!(count_selection_history(&pool).await.unwrap(), 2);
}

#[tokio::test]
async fn test_query_one_failure_with_zero_items_propagates_error() {
    let (_dir, pool) = setup_test_db().await;
    let mock_server = MockServer::start().await;

    let ai_resp = Ok(r#"{"queries":["query 1","query 2"]}"#.to_string());
    let mock_client = MockAiClient::new(vec![ai_resp]);

    // Query 1 hits 429 right away
    Mock::given(method("GET"))
        .and(path("/search/repositories"))
        .and(query_param("q", "query 1"))
        .respond_with(ResponseTemplate::new(429))
        .expect(1)
        .mount(&mock_server)
        .await;

    let github = GitHubClient::with_base_url(&mock_server.uri());
    let req = AiRepositorySearchRequest {
        input: "test query".to_string(),
        min_stars: None,
        pushed_after: None,
        limit: 10,
        exclude_selected: false,
        exclude_favorites: false,
    };

    let err = execute_ai_search_with_client(&pool, &mock_client, &github, None, &req)
        .await
        .unwrap_err();

    assert_eq!(err.code(), "GitHubRateLimit");
    assert_eq!(count_selection_history(&pool).await.unwrap(), 0);
}

#[test]
fn test_strip_outer_code_fence_variants() {
    assert_eq!(
        strip_outer_code_fence("```json\n{\"queries\":[\"test\"]}\n```"),
        "{\"queries\":[\"test\"]}"
    );
    assert_eq!(
        strip_outer_code_fence("```\n{\"queries\":[\"test\"]}\n```"),
        "{\"queries\":[\"test\"]}"
    );
    assert_eq!(
        strip_outer_code_fence("  {\"queries\":[\"test\"]}  "),
        "{\"queries\":[\"test\"]}"
    );
}

#[test]
fn test_parse_and_validate_ai_queries_variants() {
    let q1 = parse_and_validate_ai_queries(r#"{"queries": ["rust cli"]}"#).unwrap();
    assert_eq!(q1, vec!["rust cli"]);

    let q3 = parse_and_validate_ai_queries(r#"{"queries": ["q1", "q2", "q3"]}"#).unwrap();
    assert_eq!(q3, vec!["q1", "q2", "q3"]);

    assert!(parse_and_validate_ai_queries(r#"{"queries": []}"#).is_err());
    assert!(parse_and_validate_ai_queries(r#"{"queries": ["1", "2", "3", "4"]}"#).is_err());
    assert!(parse_and_validate_ai_queries(r#"{"queries": ["   "]}"#).is_err());
    assert!(parse_and_validate_ai_queries(r#"{"queries": ["rust stars:>100"]}"#).is_err());
    assert!(parse_and_validate_ai_queries(r#"{"queries": ["rust PUSHED:>2024-01-01"]}"#).is_err());
    assert!(parse_and_validate_ai_queries(r#"{"queries": ["rust sort:stars"]}"#).is_err());
    assert!(parse_and_validate_ai_queries(r#"{"queries": ["rust order:desc"]}"#).is_err());
}

#[test]
fn test_system_instruction_contains_strict_rules() {
    assert!(AI_SEARCH_SYSTEM_INSTRUCTION.contains("stars:"));
    assert!(AI_SEARCH_SYSTEM_INSTRUCTION.contains("pushed:"));
    assert!(AI_SEARCH_SYSTEM_INSTRUCTION.contains("sort:"));
    assert!(AI_SEARCH_SYSTEM_INSTRUCTION.contains("order:"));
    assert!(AI_SEARCH_SYSTEM_INSTRUCTION.contains("JSON"));
}

#[tokio::test]
async fn test_end_to_end_ai_search_with_settings_and_secret_store() {
    let (_dir, pool) = setup_test_db().await;
    let ai_server = MockServer::start().await;
    let github_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(header("authorization", "Bearer sk-ai-test-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "choices": [{
                "message": {
                    "content": "{\"queries\": [\"rust tui\", \"terminal ui\"]}"
                }
            }]
        })))
        .expect(1)
        .mount(&ai_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/search/repositories"))
        .and(query_param("q", "rust tui"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "total_count": 1,
            "items": [
                {
                    "id": 101,
                    "name": "ratatui",
                    "full_name": "ratatui-org/ratatui",
                    "html_url": "https://github.com/ratatui-org/ratatui",
                    "description": "Rust library to build rich terminal user interfaces",
                    "stargazers_count": 10000,
                    "language": "Rust",
                    "pushed_at": "2025-01-01T00:00:00Z"
                }
            ]
        })))
        .expect(1)
        .mount(&github_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/search/repositories"))
        .and(query_param("q", "terminal ui"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "total_count": 1,
            "items": [
                {
                    "id": 102,
                    "name": "crossterm",
                    "full_name": "crossterm-rs/crossterm",
                    "html_url": "https://github.com/crossterm-rs/crossterm",
                    "description": "Cross platform terminal library rust",
                    "stargazers_count": 5000,
                    "language": "Rust",
                    "pushed_at": "2025-01-01T00:00:00Z"
                }
            ]
        })))
        .expect(1)
        .mount(&github_server)
        .await;

    let dir = tempdir().unwrap();
    let config_path = dir.path().join("settings.json");
    let settings_store = SettingsStore::new(config_path).unwrap();

    let mut settings = settings_store.get().unwrap();
    settings.ai.enabled = true;
    settings.ai.provider = Some(AiProvider::OpenaiCompatible);
    settings.ai.base_url = Some(format!("{}/v1", ai_server.uri()));
    settings.ai.model = Some("gpt-4o".to_string());
    settings_store.save(&settings).unwrap();

    let secret_store = InMemorySecretStore::new();
    secret_store
        .set_secret(SECRET_KEY_AI_API_KEY, "sk-ai-test-key")
        .unwrap();

    let github = GitHubClient::with_base_url(&github_server.uri());
    let req = AiRepositorySearchRequest {
        input: "best rust terminal libraries".to_string(),
        min_stars: None,
        pushed_after: None,
        limit: 10,
        exclude_selected: false,
        exclude_favorites: false,
    };

    let resp: AiRepositorySearchResponse =
        execute_ai_search(&pool, &settings_store, &secret_store, &github, None, &req)
            .await
            .unwrap();

    assert_eq!(resp.generated_queries, vec!["rust tui", "terminal ui"]);
    assert_eq!(resp.items.len(), 2);
    assert_eq!(resp.items[0].name, "ratatui");
    assert_eq!(resp.items[1].name, "crossterm");
    assert!(!resp.partial);

    assert_eq!(count_selection_history(&pool).await.unwrap(), 2);
}
