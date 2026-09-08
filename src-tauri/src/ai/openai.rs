use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE, USER_AGENT};
use reqwest::Client;
use serde::Deserialize;
use serde_json::json;
use url::Url;

use crate::ai::provider::{map_http_error, map_reqwest_error, AiProviderClient};
use crate::ai::url::resolve_provider_endpoint;
use crate::error::AppError;
use crate::settings::AiProvider;

pub struct OpenAiClient {
    client: Client,
    endpoint: Url,
    model: String,
    api_key: String,
}

impl OpenAiClient {
    pub fn new(
        base_url: Option<&str>,
        model: &str,
        api_key: &str,
        timeout: Duration,
    ) -> Result<Self, AppError> {
        let endpoint = resolve_provider_endpoint(AiProvider::OpenaiCompatible, base_url)?;
        let client = Client::builder()
            .timeout(timeout)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| AppError::AIConnection(format!("Failed to build HTTP client: {}", e)))?;

        Ok(Self {
            client,
            endpoint,
            model: model.to_string(),
            api_key: api_key.to_string(),
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

        let auth_val = format!("Bearer {}", self.api_key.trim());
        let mut auth_header = HeaderValue::from_str(&auth_val)
            .map_err(|_| AppError::AIAuth("Invalid characters in AI API key".to_string()))?;
        auth_header.set_sensitive(true);
        headers.insert(AUTHORIZATION, auth_header);

        Ok(headers)
    }
}

#[derive(Deserialize)]
struct OpenAiChatCompletionResponse {
    #[serde(default)]
    choices: Vec<OpenAiChoice>,
}

#[derive(Deserialize)]
struct OpenAiChoice {
    message: Option<OpenAiMessage>,
}

#[derive(Deserialize)]
struct OpenAiMessage {
    content: Option<String>,
}

impl AiProviderClient for OpenAiClient {
    fn provider_name(&self) -> &'static str {
        "OpenAI Compatible"
    }

    fn model(&self) -> &str {
        &self.model
    }

    fn generate_text<'a>(
        &'a self,
        system_instruction: &'a str,
        user_input: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<String, AppError>> + Send + 'a>> {
        Box::pin(async move {
            let headers = self.prepare_headers()?;

            let mut messages = Vec::new();
            let trimmed_system = system_instruction.trim();
            if !trimmed_system.is_empty() {
                messages.push(json!({
                    "role": "system",
                    "content": trimmed_system
                }));
            }
            messages.push(json!({
                "role": "user",
                "content": user_input
            }));

            let body = json!({
                "model": self.model,
                "messages": messages
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

            let parsed: OpenAiChatCompletionResponse =
                serde_json::from_str(&text_body).map_err(|_| {
                    AppError::AIInvalidResponse(
                        "OpenAI Compatible: Malformed JSON response body".to_string(),
                    )
                })?;

            let content = parsed
                .choices
                .into_iter()
                .next()
                .and_then(|c| c.message)
                .and_then(|m| m.content)
                .map(|s| s.trim().to_string())
                .unwrap_or_default();

            if content.is_empty() {
                return Err(AppError::AIInvalidResponse(
                    "OpenAI Compatible: Response contained no text content".to_string(),
                ));
            }

            Ok(content)
        })
    }
}
