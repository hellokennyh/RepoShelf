use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE, USER_AGENT};
use reqwest::Client;
use serde::Deserialize;
use serde_json::json;
use url::Url;

use crate::ai::models::AiConnectionResult;
use crate::ai::provider::{map_http_error, map_reqwest_error, AiProviderClient};
use crate::ai::url::resolve_provider_endpoint;
use crate::error::AppError;
use crate::settings::AiProvider;

pub struct OllamaClient {
    client: Client,
    endpoint: Url,
    model: String,
}

impl OllamaClient {
    pub fn new(base_url: Option<&str>, model: &str, timeout: Duration) -> Result<Self, AppError> {
        let endpoint = resolve_provider_endpoint(AiProvider::Ollama, base_url)?;
        let client = Client::builder()
            .timeout(timeout)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| AppError::AIConnection(format!("Failed to build HTTP client: {}", e)))?;

        Ok(Self {
            client,
            endpoint,
            model: model.to_string(),
        })
    }

    fn prepare_headers(&self) -> Result<HeaderMap, AppError> {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(
            USER_AGENT,
            HeaderValue::from_str(&format!("RepoShelf/{}", env!("CARGO_PKG_VERSION")))
                .map_err(|e| AppError::InvalidInput(e.to_string()))?,
        );

        // Ollama requires NO Authorization or API key headers
        Ok(headers)
    }
}

#[derive(Deserialize)]
struct OllamaChatResponse {
    message: Option<OllamaMessage>,
}

#[derive(Deserialize)]
struct OllamaMessage {
    content: Option<String>,
}

impl AiProviderClient for OllamaClient {
    fn provider_name(&self) -> &'static str {
        "Ollama"
    }

    fn model(&self) -> &str {
        &self.model
    }

    fn test_connection<'a>(
        &'a self,
    ) -> Pin<Box<dyn Future<Output = Result<AiConnectionResult, AppError>> + Send + 'a>> {
        Box::pin(async move {
            let headers = self.prepare_headers()?;
            let body = json!({
                "model": self.model,
                "messages": [
                    {
                        "role": "user",
                        "content": "Reply with exactly: OK"
                    }
                ],
                "stream": false
            });

            let response = self
                .client
                .post(self.endpoint.clone())
                .headers(headers)
                .json(&body)
                .send()
                .await
                .map_err(|e| map_reqwest_error(self.provider_name(), e))?;

            let status = response.status();
            if !status.is_success() {
                let body_text = response.text().await.unwrap_or_default();
                return Err(map_http_error(
                    self.provider_name(),
                    status,
                    Some(&body_text),
                ));
            }

            let text_body = response
                .text()
                .await
                .map_err(|e| map_reqwest_error(self.provider_name(), e))?;

            let parsed: OllamaChatResponse = serde_json::from_str(&text_body).map_err(|_| {
                AppError::AIInvalidResponse("Ollama: Malformed JSON response body".to_string())
            })?;

            let content = parsed
                .message
                .and_then(|m| m.content)
                .map(|s| s.trim().to_string())
                .unwrap_or_default();

            if content.is_empty() {
                return Err(AppError::AIInvalidResponse(
                    "Ollama: Response contained no message content".to_string(),
                ));
            }

            Ok(AiConnectionResult {
                provider: self.provider_name().to_string(),
                model: self.model.clone(),
                success: true,
                message: "Connection successful".to_string(),
            })
        })
    }
}
