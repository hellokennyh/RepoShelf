use std::sync::Arc;
use std::time::Duration;

use tempfile::tempdir;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use reposhelf_lib::ai::{
    create_provider_client, execute_test_ai_connection, resolve_provider_endpoint,
    validate_url_security,
};
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
