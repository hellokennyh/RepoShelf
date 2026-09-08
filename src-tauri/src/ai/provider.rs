use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use reqwest::StatusCode;

use crate::ai::anthropic::AnthropicClient;
use crate::ai::gemini::GeminiClient;
use crate::ai::models::AiConnectionResult;
use crate::ai::ollama::OllamaClient;
use crate::ai::openai::OpenAiClient;
use crate::error::AppError;
use crate::settings::AiProvider;

pub const DEFAULT_TIMEOUT_SECS: u64 = 30;
pub const OLLAMA_TIMEOUT_SECS: u64 = 60;

pub trait AiProviderClient: Send + Sync {
    fn provider_name(&self) -> &'static str;
    fn model(&self) -> &str;
    fn test_connection<'a>(
        &'a self,
    ) -> Pin<Box<dyn Future<Output = Result<AiConnectionResult, AppError>> + Send + 'a>>;
}

/// Helper to map provider HTTP errors into sanitized AppError variants.
/// Never includes raw response bodies, headers, or keys in the error message.
pub fn map_http_error(provider: &str, status: StatusCode, body_snippet: Option<&str>) -> AppError {
    let code = status.as_u16();

    if code == 401 || code == 403 {
        return AppError::AIAuth(format!(
            "{}: Authentication failed (HTTP {})",
            provider, code
        ));
    }

    if code == 404 {
        return AppError::AIModel(format!(
            "{}: Model or endpoint not found (HTTP 404)",
            provider
        ));
    }

    if code == 408 || code == 429 || status.is_server_error() {
        return AppError::AIConnection(format!(
            "{}: Server or rate-limit error (HTTP {})",
            provider, code
        ));
    }

    if code == 400 {
        if let Some(body) = body_snippet {
            if body.to_lowercase().contains("model") {
                return AppError::AIModel(format!(
                    "{}: Model configuration error (HTTP 400)",
                    provider
                ));
            }
        }
        return AppError::AIInvalidResponse(format!("{}: Bad request (HTTP 400)", provider));
    }

    AppError::AIInvalidResponse(format!(
        "{}: Unexpected response status (HTTP {})",
        provider, code
    ))
}

/// Maps reqwest client execution errors (timeout, connect failure) into AIConnection.
pub fn map_reqwest_error(provider: &str, err: reqwest::Error) -> AppError {
    if err.is_timeout() {
        AppError::AIConnection(format!("{}: Request timed out", provider))
    } else {
        AppError::AIConnection(format!("{}: Connection failed: {}", provider, err))
    }
}

/// Dynamically constructs an AI provider client from settings and credentials.
pub fn create_provider_client(
    provider: AiProvider,
    base_url: Option<&str>,
    model: &str,
    api_key: Option<&str>,
    timeout_override: Option<Duration>,
) -> Result<Box<dyn AiProviderClient>, AppError> {
    let trimmed_model = model.trim();
    if trimmed_model.is_empty() {
        return Err(AppError::AIModel(
            "Configured model name cannot be blank".to_string(),
        ));
    }

    match provider {
        AiProvider::OpenaiCompatible => {
            let key = match api_key.map(str::trim).filter(|k| !k.is_empty()) {
                Some(k) => k,
                None => {
                    return Err(AppError::AIAuth(
                        "AI API key is required for OpenAI Compatible provider".to_string(),
                    ))
                }
            };
            let timeout = timeout_override.unwrap_or(Duration::from_secs(DEFAULT_TIMEOUT_SECS));
            let client = OpenAiClient::new(base_url, trimmed_model, key, timeout)?;
            Ok(Box::new(client))
        }
        AiProvider::Anthropic => {
            let key = match api_key.map(str::trim).filter(|k| !k.is_empty()) {
                Some(k) => k,
                None => {
                    return Err(AppError::AIAuth(
                        "AI API key is required for Anthropic provider".to_string(),
                    ))
                }
            };
            let timeout = timeout_override.unwrap_or(Duration::from_secs(DEFAULT_TIMEOUT_SECS));
            let client = AnthropicClient::new(base_url, trimmed_model, key, timeout)?;
            Ok(Box::new(client))
        }
        AiProvider::Gemini => {
            let key = match api_key.map(str::trim).filter(|k| !k.is_empty()) {
                Some(k) => k,
                None => {
                    return Err(AppError::AIAuth(
                        "AI API key is required for Google Gemini provider".to_string(),
                    ))
                }
            };
            let timeout = timeout_override.unwrap_or(Duration::from_secs(DEFAULT_TIMEOUT_SECS));
            let client = GeminiClient::new(base_url, trimmed_model, key, timeout)?;
            Ok(Box::new(client))
        }
        AiProvider::Ollama => {
            let timeout = timeout_override.unwrap_or(Duration::from_secs(OLLAMA_TIMEOUT_SECS));
            let client = OllamaClient::new(base_url, trimmed_model, timeout)?;
            Ok(Box::new(client))
        }
    }
}
