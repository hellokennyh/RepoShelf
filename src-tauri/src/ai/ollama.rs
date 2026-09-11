use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE, USER_AGENT};
use reqwest::Client;
use serde::Deserialize;
use serde_json::json;
use url::Url;

use crate::ai::provider::{map_http_error, map_reqwest_error, AiProviderClient};
use crate::ai::url::resolve_provider_endpoint;
use crate::error::AppError;
use crate::settings::AiProvider;

pub struct OllamaClient {
    client: Client,
    endpoint: Url,
    base_url: Option<String>,
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
            base_url: base_url.map(|s| s.to_string()),
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
#[allow(dead_code)]
struct OllamaChatResponse {
    message: Option<OllamaMessage>,
    done: Option<bool>,
    done_reason: Option<String>,
}

#[derive(Deserialize)]
struct OllamaMessage {
    content: Option<String>,
    thinking: Option<String>,
}

impl AiProviderClient for OllamaClient {
    fn provider_name(&self) -> &'static str {
        "Ollama"
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

            let mut body = json!({
                "model": self.model,
                "messages": messages,
                "stream": false,
            });

            if let Some(think) = options.think {
                body["think"] = json!(think);
            }

            let num_predict = options.max_tokens.unwrap_or(512);
            let mut opts = json!({
                "num_predict": num_predict
            });
            if let Some(temp) = options.temperature {
                opts["temperature"] = json!(temp);
            }
            body["options"] = opts;

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
                .as_ref()
                .and_then(|m| m.content.as_deref())
                .map(|s| s.trim().to_string())
                .unwrap_or_default();

            if content.is_empty() {
                let thinking_len = parsed
                    .message
                    .as_ref()
                    .and_then(|m| m.thinking.as_deref())
                    .map(|t| t.trim().len())
                    .unwrap_or(0);

                if thinking_len > 0 {
                    return Err(AppError::AIInvalidResponse(format!(
                        "Ollama: Response contained only thinking/reasoning content ({} chars) and no message content (done_reason: {})",
                        thinking_len,
                        parsed.done_reason.as_deref().unwrap_or("unknown")
                    )));
                }

                return Err(AppError::AIInvalidResponse(
                    "Ollama: Response contained no message content".to_string(),
                ));
            }

            Ok(content)
        })
    }

    fn test_connection<'a>(
        &'a self,
    ) -> Pin<Box<dyn Future<Output = Result<crate::ai::models::AiConnectionResult, AppError>> + Send + 'a>> {
        Box::pin(async move {
            let res = self.generate_text("", "Reply with exactly: OK").await;
            match res {
                Ok(text) => {
                    if text.trim().is_empty() {
                        return Err(AppError::AIInvalidResponse(format!(
                            "{}: Response contained no text content",
                            self.provider_name()
                        )));
                    }
                    Ok(crate::ai::models::AiConnectionResult {
                        provider: self.provider_name().to_string(),
                        model: self.model().to_string(),
                        success: true,
                        message: "Connection successful".to_string(),
                    })
                }
                Err(err) => {
                    // If error is AIModel (HTTP 404), query /api/tags to provide precise diagnostics
                    if err.code() == "AIModel" {
                        if let Ok(tags_url) = crate::ai::url::resolve_ollama_tags_endpoint(self.base_url.as_deref()) {
                            if let Ok(resp) = self.client.get(tags_url).send().await {
                                if resp.status().is_success() {
                                    if let Ok(text) = resp.text().await {
                                        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&text) {
                                            if let Some(models) = parsed.get("models").and_then(|m| m.as_array()) {
                                                let model_names: Vec<String> = models.iter().filter_map(|m| {
                                                    m.get("model").or_else(|| m.get("name"))
                                                        .and_then(|v| v.as_str())
                                                        .map(|s| s.to_string())
                                                }).collect();

                                                let target = &self.model;
                                                let target_with_latest = format!("{}:latest", target);
                                                let found = model_names.iter().any(|m| m == target || m == &target_with_latest);

                                                if !found {
                                                    let available = if model_names.is_empty() {
                                                        "无已安装模型".to_string()
                                                    } else {
                                                        model_names.join(", ")
                                                    };
                                                    return Err(AppError::AIModel(format!(
                                                        "Ollama 服务连接正常，但未检测到模型：{} (已安装: {})",
                                                        target, available
                                                    )));
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Err(err)
                }
            }
        })
    }
}
