use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::sync::Arc;
use tauri::State;

use crate::error::AppError;
use crate::secrets::{SecretStore, SECRET_KEY_AI_API_KEY, SECRET_KEY_GITHUB_TOKEN};
use crate::settings::{Settings, SettingsStore};

pub struct AppState {
    pub db: SqlitePool,
    pub settings: Arc<SettingsStore>,
    pub secrets: Arc<dyn SecretStore>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppInfo {
    pub name: String,
    pub phase: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SecretStatus {
    pub github_token_configured: bool,
    pub ai_api_key_configured: bool,
}

#[tauri::command]
pub fn get_app_info() -> AppInfo {
    AppInfo {
        name: "RepoShelf".to_string(),
        phase: "Phase 1".to_string(),
    }
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Result<Settings, AppError> {
    state.settings.get()
}

#[tauri::command]
pub fn update_settings(
    state: State<'_, AppState>,
    settings: Settings,
) -> Result<Settings, AppError> {
    state.settings.save(&settings)
}

#[tauri::command]
pub fn get_secret_status(state: State<'_, AppState>) -> Result<SecretStatus, AppError> {
    let github_token_configured = state.secrets.has_secret(SECRET_KEY_GITHUB_TOKEN)?;
    let ai_api_key_configured = state.secrets.has_secret(SECRET_KEY_AI_API_KEY)?;

    Ok(SecretStatus {
        github_token_configured,
        ai_api_key_configured,
    })
}

#[tauri::command]
pub fn set_github_token(state: State<'_, AppState>, token: String) -> Result<(), AppError> {
    let trimmed = token.trim();
    if trimmed.is_empty() {
        return Err(AppError::InvalidInput(
            "GitHub token cannot be empty".to_string(),
        ));
    }
    state.secrets.set_secret(SECRET_KEY_GITHUB_TOKEN, trimmed)
}

#[tauri::command]
pub fn clear_github_token(state: State<'_, AppState>) -> Result<(), AppError> {
    state.secrets.delete_secret(SECRET_KEY_GITHUB_TOKEN)
}

#[tauri::command]
pub fn set_ai_api_key(state: State<'_, AppState>, api_key: String) -> Result<(), AppError> {
    let trimmed = api_key.trim();
    if trimmed.is_empty() {
        return Err(AppError::InvalidInput(
            "AI API key cannot be empty".to_string(),
        ));
    }
    state.secrets.set_secret(SECRET_KEY_AI_API_KEY, trimmed)
}

#[tauri::command]
pub fn clear_ai_api_key(state: State<'_, AppState>) -> Result<(), AppError> {
    state.secrets.delete_secret(SECRET_KEY_AI_API_KEY)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_app_info() {
        let info = get_app_info();
        assert_eq!(info.name, "RepoShelf");
        assert_eq!(info.phase, "Phase 1");
    }

    #[test]
    fn test_app_info_serialization() {
        let info = get_app_info();
        let json = serde_json::to_string(&info).expect("Failed to serialize AppInfo");
        let deserialized: AppInfo =
            serde_json::from_str(&json).expect("Failed to deserialize AppInfo");
        assert_eq!(info, deserialized);
    }
}
