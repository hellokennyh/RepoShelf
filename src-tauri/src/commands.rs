use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::sync::Arc;
use tauri::State;

use crate::error::AppError;
use crate::github::{
    parse_github_repo_url, GitHubClient, GitHubConnectionStatus, GitHubRepository,
    RepositorySearchRequest, RepositorySearchResponse,
};
use crate::secrets::{SecretStore, SECRET_KEY_AI_API_KEY, SECRET_KEY_GITHUB_TOKEN};
use crate::settings::{Settings, SettingsStore};
use crate::trending::{TrendingPeriod, TrendingProvider, TrendingResponse};

pub struct AppState {
    pub db: SqlitePool,
    pub settings: Arc<SettingsStore>,
    pub secrets: Arc<dyn SecretStore>,
    pub github: Arc<GitHubClient>,
    pub trending: Arc<dyn TrendingProvider>,
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
        phase: "Phase 2".to_string(),
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

#[tauri::command]
pub async fn github_test_connection(
    state: State<'_, AppState>,
) -> Result<GitHubConnectionStatus, AppError> {
    let token = state.secrets.get_secret(SECRET_KEY_GITHUB_TOKEN)?;
    state.github.test_connection(token.as_deref()).await
}

#[tauri::command]
pub async fn github_get_repository(
    state: State<'_, AppState>,
    url: String,
) -> Result<GitHubRepository, AppError> {
    let (owner, repo, _) = parse_github_repo_url(&url)?;
    let token = state.secrets.get_secret(SECRET_KEY_GITHUB_TOKEN)?;
    state
        .github
        .get_repository(&owner, &repo, token.as_deref())
        .await
}

#[tauri::command]
pub async fn github_search_repositories(
    state: State<'_, AppState>,
    request: RepositorySearchRequest,
) -> Result<RepositorySearchResponse, AppError> {
    let token = state.secrets.get_secret(SECRET_KEY_GITHUB_TOKEN)?;
    state
        .github
        .search_repositories(&request, token.as_deref())
        .await
}

#[tauri::command]
pub async fn get_trending(
    state: State<'_, AppState>,
    period: String,
    limit: usize,
) -> Result<TrendingResponse, AppError> {
    let parsed_period: TrendingPeriod = period.parse()?;
    state.trending.get_trending(parsed_period, limit).await
}

#[tauri::command]
pub async fn open_github_url(app: tauri::AppHandle, url: String) -> Result<(), AppError> {
    let trimmed = url.trim();
    if !trimmed.starts_with("https://github.com/") && !trimmed.starts_with("http://github.com/") {
        return Err(AppError::InvalidRepositoryUrl(
            "Only GitHub URLs are allowed to be opened".to_string(),
        ));
    }

    let parsed = url::Url::parse(trimmed)
        .map_err(|e| AppError::InvalidRepositoryUrl(format!("Invalid URL: {}", e)))?;

    let host = parsed.host_str().unwrap_or("").to_lowercase();
    if host != "github.com" && host != "www.github.com" {
        return Err(AppError::InvalidRepositoryUrl(
            "Only github.com URLs are permitted".to_string(),
        ));
    }

    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_url(parsed.as_str(), None::<&str>)
        .map_err(|e| AppError::Unknown(format!("Failed to open URL in browser: {}", e)))
}

#[tauri::command]
pub async fn check_favorite_status(
    state: State<'_, AppState>,
    repo_id: Option<i64>,
    url: String,
) -> Result<crate::library::FavoriteStatus, AppError> {
    crate::library::favorite_service::check_favorite_status(&state.db, repo_id, &url).await
}

#[tauri::command]
pub async fn favorite_repository(
    state: State<'_, AppState>,
    input: crate::library::FavoriteInput,
) -> Result<crate::library::FavoriteItem, AppError> {
    crate::library::favorite_service::favorite_repository(&state.db, input).await
}

#[tauri::command]
pub async fn favorite_repositories_batch(
    state: State<'_, AppState>,
    inputs: Vec<crate::library::FavoriteInput>,
) -> Result<crate::library::BatchFavoriteResult, AppError> {
    crate::library::favorite_service::favorite_repositories_batch(&state.db, inputs).await
}

#[tauri::command]
pub async fn get_favorite(
    state: State<'_, AppState>,
    id: i64,
) -> Result<crate::library::FavoriteItem, AppError> {
    crate::library::favorite_service::get_favorite(&state.db, id).await
}

#[tauri::command]
pub async fn list_favorites(
    state: State<'_, AppState>,
    query: crate::library::LibraryQuery,
) -> Result<Vec<crate::library::FavoriteItem>, AppError> {
    crate::library::favorite_service::get_favorites(&state.db, query).await
}

#[tauri::command]
pub async fn list_distinct_tags(state: State<'_, AppState>) -> Result<Vec<String>, AppError> {
    crate::library::favorite_service::get_distinct_tags(&state.db).await
}

#[tauri::command]
pub async fn update_favorite_user_metadata(
    state: State<'_, AppState>,
    input: crate::library::UpdateFavoriteUserMetadataInput,
) -> Result<crate::library::FavoriteItem, AppError> {
    crate::library::favorite_service::update_favorite_metadata(&state.db, input).await
}

#[tauri::command]
pub async fn delete_favorite(state: State<'_, AppState>, id: i64) -> Result<bool, AppError> {
    crate::library::favorite_service::delete_favorite(&state.db, id).await
}

#[tauri::command]
pub async fn refresh_favorite(
    state: State<'_, AppState>,
    id: i64,
) -> Result<crate::library::RefreshResult, AppError> {
    let token = state.secrets.get_secret(SECRET_KEY_GITHUB_TOKEN)?;
    crate::library::refresh::refresh_single(&state.db, &state.github, token.as_deref(), id).await
}

#[tauri::command]
pub async fn refresh_favorites_batch(
    state: State<'_, AppState>,
    ids: Vec<i64>,
) -> Result<crate::library::BatchRefreshResult, AppError> {
    let token = state.secrets.get_secret(SECRET_KEY_GITHUB_TOKEN)?;
    crate::library::refresh::refresh_batch(&state.db, Arc::clone(&state.github), token, ids).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_app_info() {
        let info = get_app_info();
        assert_eq!(info.name, "RepoShelf");
        assert_eq!(info.phase, "Phase 2");
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
