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

pub const GEMINI_API_KEY_HEADER: &str = "x-goog-api-key";

pub struct GeminiClient {
    client: Client,
    endpoint: Url,
    model: String,
    api_key: String,
}

impl GeminiClient {
    pub fn new(
        base_url: Option<&str>,
        model: &str,
        api_key: &str,
        timeout: Duration,
    ) -> Result<Self, AppError> {
        let endpoint = resolve_provider_endpoint(AiProvider::Gemini, base_url)?;
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

        let mut key_header = HeaderValue::from_str(self.api_key.trim())
            .map_err(|_| AppError::AIAuth("Invalid characters in Gemini API key".to_string()))?;
        key_header.set_sensitive(true);
        headers.insert(HeaderName::from_static(GEMINI_API_KEY_HEADER), key_header);

        Ok(headers)
    }
}

#[derive(Deserialize)]
struct GeminiInteractionResponse {
    #[serde(default)]
    steps: Vec<GeminiStep>,
}

#[derive(Deserialize)]
struct GeminiStep {
    #[serde(rename = "type", default)]
    step_type: String,
    content: Option<GeminiContent>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum GeminiContent {
    Single(GeminiContentBlock),
    Multiple(Vec<GeminiContentBlock>),
}

#[derive(Deserialize)]
struct GeminiContentBlock {
    #[serde(rename = "type", default)]
    block_type: Option<String>,
    text: Option<String>,
}

impl AiProviderClient for GeminiClient {
    fn provider_name(&self) -> &'static str {
        "Google Gemini"
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

            let mut body = json!({
                "model": self.model,
                "input": user_input,
                "store": false,
                "generation_config": {
                    "max_output_tokens": 512
                }
            });

            let trimmed_system = system_instruction.trim();
            if !trimmed_system.is_empty() {
                body["system_instruction"] = json!(trimmed_system);
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

            let parsed: GeminiInteractionResponse =
                serde_json::from_str(&text_body).map_err(|_| {
                    AppError::AIInvalidResponse(
                        "Google Gemini: Malformed JSON response body".to_string(),
                    )
                })?;

            let mut extracted_texts = Vec::new();
            for step in parsed.steps {
                if step.step_type == "model_output" {
                    if let Some(content) = step.content {
                        match content {
                            GeminiContent::Single(block) => {
                                if block.block_type.as_deref() == Some("text")
                                    || block.block_type.is_none()
                                {
                                    if let Some(t) = block.text {
                                        let trimmed = t.trim().to_string();
                                        if !trimmed.is_empty() {
                                            extracted_texts.push(trimmed);
                                        }
                                    }
                                }
                            }
                            GeminiContent::Multiple(blocks) => {
                                for block in blocks {
                                    if block.block_type.as_deref() == Some("text")
                                        || block.block_type.is_none()
                                    {
                                        if let Some(t) = block.text {
                                            let trimmed = t.trim().to_string();
                                            if !trimmed.is_empty() {
                                                extracted_texts.push(trimmed);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            let joined = extracted_texts.join(" ");
            if joined.trim().is_empty() {
                return Err(AppError::AIInvalidResponse(
                    "Google Gemini: Response contained no model_output text".to_string(),
                ));
            }

            Ok(joined)
        })
    }
}
