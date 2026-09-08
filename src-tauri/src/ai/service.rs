use std::sync::Arc;

use crate::ai::models::AiConnectionResult;
use crate::ai::provider::create_provider_client;
use crate::error::AppError;
use crate::secrets::{SecretStore, SECRET_KEY_AI_API_KEY};
use crate::settings::{AiProvider, SettingsStore};

pub async fn execute_test_ai_connection(
    settings_store: &SettingsStore,
    secret_store: &Arc<dyn SecretStore>,
) -> Result<AiConnectionResult, AppError> {
    let settings = settings_store.get()?;

    let provider = match settings.ai.provider {
        Some(p) => p,
        None => {
            return Err(AppError::SettingsValidation(
                "未选择 AI Provider (No AI provider configured)".to_string(),
            ))
        }
    };

    let model = match settings
        .ai
        .model
        .as_deref()
        .map(str::trim)
        .filter(|m| !m.is_empty())
    {
        Some(m) => m,
        None => {
            return Err(AppError::AIModel(
                "测试连接必须提供已配置的 Model 名称 (Model name cannot be empty)".to_string(),
            ))
        }
    };

    let base_url = settings.ai.base_url.as_deref();

    let api_key = if provider == AiProvider::Ollama {
        None
    } else {
        let key_opt = secret_store.get_secret(SECRET_KEY_AI_API_KEY)?;
        match key_opt
            .map(|k| k.trim().to_string())
            .filter(|k| !k.is_empty())
        {
            Some(k) => Some(k),
            None => {
                let p_name = match provider {
                    AiProvider::OpenaiCompatible => "OpenAI Compatible",
                    AiProvider::Anthropic => "Anthropic",
                    AiProvider::Gemini => "Google Gemini",
                    AiProvider::Ollama => "Ollama",
                };
                return Err(AppError::AIAuth(format!(
                    "未配置 {} 的 API Key，请先保存 API Key 后再测试连接",
                    p_name
                )));
            }
        }
    };

    let client = create_provider_client(provider, base_url, model, api_key.as_deref(), None)?;
    client.test_connection().await
}
