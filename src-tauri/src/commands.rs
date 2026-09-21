use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::sync::Arc;
use tauri::ipc::Channel;
use tauri::State;

use crate::error::AppError;
use crate::github::{
    parse_github_repo_url, GitHubClient, GitHubConnectionStatus, GitHubRepository,
    RepositorySearchRequest, RepositorySearchResponse,
};
use crate::secrets::{SecretStore, SECRET_KEY_AI_API_KEY, SECRET_KEY_GITHUB_TOKEN};
use crate::settings::{Settings, SettingsStore, UiLocale};
use crate::trending::{TrendingPeriod, TrendingProvider, TrendingResponse};

pub struct AppState {
    pub db: SqlitePool,
    pub settings: Arc<SettingsStore>,
    pub secrets: Arc<dyn SecretStore>,
    pub github: Arc<GitHubClient>,
    pub trending: Arc<dyn TrendingProvider>,
    pub translation_limiter: Arc<crate::ai::TranslationLimiter>,
    pub star_history_service: Arc<crate::star_history::StarHistoryService>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppInfo {
    pub name: String,
    pub phase: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocaleInitInfo {
    pub locale: String,
    pub detected_source_tag: Option<String>,
    pub warning: Option<String>,
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
pub fn update_ui_language(
    state: State<'_, AppState>,
    locale: String,
) -> Result<Settings, AppError> {
    let ui_locale = UiLocale::from_str_strict(&locale)
        .ok_or_else(|| AppError::InvalidInput(format!("Invalid ui_language: {}", locale)))?;
    state.settings.update_ui_language(ui_locale)
}

#[tauri::command]
pub fn get_locale_initialization_info(
    state: State<'_, AppState>,
) -> Result<LocaleInitInfo, AppError> {
    let current_settings = state.settings.get()?;
    let locale_str = current_settings
        .ui_language
        .map(|l| l.as_str().to_string())
        .unwrap_or_else(|| "en".to_string());
    Ok(LocaleInitInfo {
        locale: locale_str,
        detected_source_tag: state.settings.get_detected_source_tag(),
        warning: state.settings.get_initial_warning(),
    })
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
pub async fn test_ai_connection(
    state: State<'_, AppState>,
) -> Result<crate::ai::AiConnectionResult, AppError> {
    crate::ai::execute_test_ai_connection(&state.settings, &state.secrets).await
}

#[tauri::command]
pub async fn discover_ollama_endpoints(
    state: State<'_, AppState>,
) -> Result<Vec<crate::ai::OllamaEndpointDiscovery>, AppError> {
    let settings = state.settings.get()?;
    let configured_base = settings.ai.base_url.as_deref();
    crate::ai::discover_ollama_endpoints(configured_base).await
}

#[tauri::command]
pub async fn list_ollama_models(
    base_url: Option<String>,
) -> Result<Vec<crate::ai::OllamaModelInfo>, AppError> {
    crate::ai::list_ollama_models(base_url.as_deref()).await
}

#[tauri::command]
pub fn get_ai_search_readiness(
    state: State<'_, AppState>,
) -> Result<crate::ai::AiSearchReadiness, AppError> {
    let settings = state.settings.get()?;
    Ok(crate::ai::check_ai_search_readiness(
        &settings,
        &*state.secrets,
    ))
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
    let mut repo_model = state
        .github
        .get_repository(&owner, &repo, token.as_deref())
        .await?;

    let releases = state
        .github
        .get_repository_releases_count(&owner, &repo, token.as_deref())
        .await
        .ok();
    let languages = state
        .github
        .get_repository_languages(&owner, &repo, token.as_deref())
        .await
        .ok();

    repo_model.releases = releases;
    repo_model.languages = languages;

    Ok(repo_model)
}

#[tauri::command]
pub async fn import_repositories_from_article(
    state: State<'_, AppState>,
    url: String,
) -> Result<crate::article::ArticleImportResult, AppError> {
    let trimmed = url.trim();
    if let Ok(parsed) = url::Url::parse(trimmed) {
        if let Some(host) = parsed.host_str() {
            let host_lower = host.to_lowercase();
            if host_lower == "github.com" || host_lower == "www.github.com" {
                return Err(AppError::ArticleInvalidUrl(
                    "Direct GitHub URLs must be imported directly".to_string(),
                ));
            }
        }
    }
    let token = state.secrets.get_secret(SECRET_KEY_GITHUB_TOKEN)?;
    crate::article::import_from_article(&state.github, token.as_deref(), trimmed, None, None).await
}

#[tauri::command]
pub async fn github_search_repositories(
    state: State<'_, AppState>,
    request: RepositorySearchRequest,
) -> Result<RepositorySearchResponse, AppError> {
    let token = state.secrets.get_secret(SECRET_KEY_GITHUB_TOKEN)?;
    crate::library::search_service::execute_search(
        &state.db,
        &state.github,
        token.as_deref(),
        &request,
    )
    .await
}

#[tauri::command]
pub async fn github_ai_search_repositories(
    state: State<'_, AppState>,
    request: crate::ai::AiRepositorySearchRequest,
) -> Result<crate::ai::AiRepositorySearchResponse, AppError> {
    let token = state.secrets.get_secret(SECRET_KEY_GITHUB_TOKEN)?;
    crate::library::search_service::execute_ai_search(
        &state.db,
        &state.settings,
        &*state.secrets,
        &state.github,
        token.as_deref(),
        &request,
    )
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
pub async fn check_favorite_identities_batch(
    state: State<'_, AppState>,
    repo_ids: Vec<i64>,
    urls: Vec<String>,
) -> Result<crate::library::BatchFavoriteIdentitiesResult, AppError> {
    crate::library::favorite_service::check_favorite_identities_batch(&state.db, &repo_ids, &urls)
        .await
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
pub async fn delete_favorites_batch(
    state: State<'_, AppState>,
    ids: Vec<i64>,
) -> Result<crate::library::BatchDeleteResult, AppError> {
    crate::library::favorite_service::delete_favorites_batch(&state.db, ids).await
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
    on_progress: Channel<crate::library::BatchRefreshProgress>,
) -> Result<crate::library::BatchRefreshResult, AppError> {
    let token = state.secrets.get_secret(SECRET_KEY_GITHUB_TOKEN)?;
    let progress_cb = Some(
        Arc::new(move |progress: crate::library::BatchRefreshProgress| {
            let _ = on_progress.send(progress);
        }) as Arc<dyn Fn(crate::library::BatchRefreshProgress) + Send + Sync>,
    );
    crate::library::refresh::refresh_batch_with_progress(
        &state.db,
        Arc::clone(&state.github),
        token,
        ids,
        progress_cb,
    )
    .await
}

#[tauri::command]
pub async fn suggest_favorite_ai_metadata(
    state: State<'_, AppState>,
    favorite_id: i64,
) -> Result<crate::ai::AiFavoriteMetadataSuggestion, AppError> {
    crate::library::favorite_service::suggest_favorite_ai_metadata(
        &state.db,
        &state.settings,
        &*state.secrets,
        favorite_id,
    )
    .await
}

#[tauri::command]
pub async fn apply_favorite_ai_metadata(
    state: State<'_, AppState>,
    favorite_id: i64,
    category: String,
    tags: Vec<String>,
) -> Result<crate::library::FavoriteItem, AppError> {
    crate::library::favorite_service::apply_favorite_ai_metadata(
        &state.db,
        favorite_id,
        category,
        tags,
    )
    .await
}

pub async fn translate_repository_description_impl(
    settings_store: &SettingsStore,
    secrets: &dyn SecretStore,
    translation_limiter: &crate::ai::TranslationLimiter,
    text: &str,
    target_locale: &str,
) -> Result<crate::ai::DescriptionTranslationResult, AppError> {
    let settings = settings_store.get()?;
    let readiness = crate::ai::check_ai_search_readiness(&settings, secrets);
    if !readiness.ready {
        return Err(AppError::SettingsValidation(
            readiness
                .reason
                .unwrap_or_else(|| "AI is not configured or disabled".to_string()),
        ));
    }

    let provider = match settings.ai.provider {
        Some(p) => p,
        None => {
            return Err(AppError::SettingsValidation(
                "No AI provider configured".to_string(),
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
        Some(m) => m.to_string(),
        None => return Err(AppError::AIModel("No AI model configured".to_string())),
    };

    let _permit = translation_limiter.acquire(&provider).await?;

    // Dispatch-time recheck: verify AI is still enabled and configuration is valid after waiting in queue
    let settings_dispatch = settings_store.get()?;
    let readiness_dispatch = crate::ai::check_ai_search_readiness(&settings_dispatch, secrets);
    if !readiness_dispatch.ready {
        return Err(AppError::SettingsValidation(
            readiness_dispatch
                .reason
                .unwrap_or_else(|| "AI is not configured or disabled".to_string()),
        ));
    }
    if settings_dispatch.ai.provider != Some(provider) {
        return Err(AppError::SettingsValidation(
            "AI provider configuration changed while waiting in queue".to_string(),
        ));
    }

    let base_url = settings_dispatch.ai.base_url.as_deref();
    let api_key = if provider == crate::settings::AiProvider::Ollama {
        None
    } else {
        secrets.get_secret(SECRET_KEY_AI_API_KEY)?
    };

    let provider_name = match provider {
        crate::settings::AiProvider::Ollama => "ollama",
        crate::settings::AiProvider::OpenaiCompatible => "openai_compatible",
        crate::settings::AiProvider::Anthropic => "anthropic",
        crate::settings::AiProvider::Gemini => "gemini",
    };

    let client =
        crate::ai::create_provider_client(provider, base_url, &model, api_key.as_deref(), None)?;

    crate::ai::translate_description(&*client, text, target_locale, provider_name, &model).await
}

#[tauri::command]
pub async fn translate_repository_description(
    state: State<'_, AppState>,
    text: String,
    target_locale: String,
) -> Result<crate::ai::DescriptionTranslationResult, AppError> {
    translate_repository_description_impl(
        &state.settings,
        &*state.secrets,
        &state.translation_limiter,
        &text,
        &target_locale,
    )
    .await
}

#[tauri::command]
pub async fn get_star_history_preview(
    state: State<'_, AppState>,
    owner: String,
    repo: String,
) -> Result<crate::star_history::StarHistoryPreviewResult, AppError> {
    let token = state
        .secrets
        .get_secret(SECRET_KEY_GITHUB_TOKEN)
        .ok()
        .flatten();
    state
        .star_history_service
        .get_preview(&owner, &repo, token.as_deref())
        .await
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
