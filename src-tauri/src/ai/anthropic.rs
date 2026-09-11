use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use reqwest::header::{HeaderMap, HeaderName, HeaderValue, CONTENT_TYPE, USER_AGENT};
use reqwest::Client;
use serde::Deserialize;
use serde_json::json;
use url::Url;

use crate::ai::provider::{map_http_error, map_reqwest_error, AiProviderClient};
use crate::ai::url::resolve_provider_endpoint;
use crate::error::AppError;
use crate::settings::AiProvider;

pub const ANTHROPIC_VERSION_HEADER: &str = "anthropic-version";
pub const ANTHROPIC_VERSION_VALUE: &str = "2023-06-01";
pub const ANTHROPIC_API_KEY_HEADER: &str = "x-api-key";

pub struct AnthropicClient {
    client: Client,
    endpoint: Url,
    model: String,
    api_key: String,
}

impl AnthropicClient {
    pub fn new(
        base_url: Option<&str>,
        model: &str,
        api_key: &str,
        timeout: Duration,
    ) -> Result<Self, AppError> {
        let endpoint = resolve_provider_endpoint(AiProvider::Anthropic, base_url)?;
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
        headers.insert(
            HeaderName::from_static(ANTHROPIC_VERSION_HEADER),
            HeaderValue::from_static(ANTHROPIC_VERSION_VALUE),
        );

        let mut api_key_header = HeaderValue::from_str(self.api_key.trim())
            .map_err(|_| AppError::AIAuth("Invalid characters in Anthropic API key".to_string()))?;
        api_key_header.set_sensitive(true);
        headers.insert(
            HeaderName::from_static(ANTHROPIC_API_KEY_HEADER),
            api_key_header,
        );

        Ok(headers)
    }
}

#[derive(Deserialize)]
struct AnthropicMessagesResponse {
    #[serde(default)]
    content: Vec<AnthropicContentBlock>,
}

#[derive(Deserialize)]
struct AnthropicContentBlock {
    #[serde(rename = "type", default)]
    block_type: String,
    text: Option<String>,
}

impl AiProviderClient for AnthropicClient {
    fn provider_name(&self) -> &'static str {
        "Anthropic"
    }

    fn model(&self) -> &str {
        &self.model
    }

    fn generate_text_with_options<'a>(
        &'a self,
        system_instruction: &'a str,
        user_input: &'a str,
        options: crate::ai::provider::GenerateTextOptions,
    ) -> Pin<Box<dyn Future<Output = Result<String, AppError>> + Send + 'a>> {
        Box::pin(async move {
            let headers = self.prepare_headers()?;

            let max_tokens = options.max_tokens.unwrap_or(512);
            let mut body = json!({
                "model": self.model,
                "max_tokens": max_tokens,
                "messages": [
                    {
                        "role": "user",
                        "content": user_input
                    }
                ]
            });
            if let Some(temperature) = options.temperature {
                body["temperature"] = json!(temperature);
            }

            let trimmed_system = system_instruction.trim();
            if !trimmed_system.is_empty() {
                body["system"] = json!(trimmed_system);
            }

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

            let parsed: AnthropicMessagesResponse =
                serde_json::from_str(&text_body).map_err(|_| {
                    AppError::AIInvalidResponse(
                        "Anthropic: Malformed JSON response body".to_string(),
                    )
                })?;

            let text_parts: Vec<String> = parsed
                .content
                .into_iter()
                .filter(|b| b.block_type == "text")
                .filter_map(|b| b.text)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();

            let joined = text_parts.join(" ");
            if joined.trim().is_empty() {
                return Err(AppError::AIInvalidResponse(
                    "Anthropic: Response contained no text content".to_string(),
                ));
            }

            Ok(joined)
        })
    }
}
