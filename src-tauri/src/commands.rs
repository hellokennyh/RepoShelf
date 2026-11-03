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
    pub analysis_task_manager: Arc<crate::ai::analysis::AnalysisTaskManager>,
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
    target_locale: Option<String>,
) -> Result<crate::ai::AiFavoriteMetadataSuggestion, AppError> {
    crate::library::favorite_service::suggest_favorite_ai_metadata(
        &state.db,
        &state.settings,
        &*state.secrets,
        favorite_id,
        target_locale,
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

pub async fn analyze_repository_impl(
    settings_store: &SettingsStore,
    secrets: &dyn SecretStore,
    ai_limiter: &crate::ai::TranslationLimiter,
    evidence_pack: &crate::github::evidence::EvidencePack,
    target_locale: crate::db::analysis::AnalysisLocale,
    cancel_token: Option<crate::github::evidence::AcquisitionCancelToken>,
    options: Option<crate::ai::analysis::AnalysisExecutionOptions>,
) -> Result<crate::db::analysis::AnalysisEnvelopeV1, AppError> {
    crate::ai::analysis::execute_repository_analysis(
        evidence_pack,
        target_locale,
        settings_store,
        secrets,
        ai_limiter,
        cancel_token,
        options.unwrap_or_default(),
    )
    .await
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyzeRepositoryRequest {
    pub repository: String,
    #[serde(default)]
    pub expected_repo_id: Option<i64>,
    pub target_locale: crate::db::analysis::AnalysisLocale,
    #[serde(default)]
    pub client_session_id: Option<String>,
    #[serde(default)]
    pub generation: Option<u64>,
}

#[tauri::command]
pub async fn analyze_repository(
    state: State<'_, AppState>,
    request: AnalyzeRepositoryRequest,
) -> Result<crate::db::analysis::AnalysisEnvelopeV1, AppError> {
    // 1. Validate minimal repository identity and SSRF restrictions
    let validated_id = crate::github::evidence::validate_repository_target(
        &request.repository,
        request.expected_repo_id,
    )?;

    // 2. Validate backend AI gate
    let settings = state.settings.get()?;
    let readiness = crate::ai::check_ai_search_readiness(&settings, &*state.secrets);
    if !readiness.ready {
        return Err(AppError::SettingsValidation(
            readiness
                .reason
                .unwrap_or_else(|| "AI is disabled or not configured".to_string()),
        ));
    }

    // 3. Bind client task/session instance identity and cancellation
    let task_id = request
        .client_session_id
        .clone()
        .unwrap_or_else(|| format!("{}:{}", validated_id.canonical_url, request.generation.unwrap_or(0)));
    let cancel_token = state.analysis_task_manager.register(&task_id)?;

    struct TaskGuard {
        manager: Arc<crate::ai::analysis::AnalysisTaskManager>,
        task_id: String,
    }
    impl Drop for TaskGuard {
        fn drop(&mut self) {
            self.manager.unregister(&self.task_id);
        }
    }
    let _guard = TaskGuard {
        manager: Arc::clone(&state.analysis_task_manager),
        task_id: task_id.clone(),
    };

    if cancel_token.is_cancelled() {
        return Err(AppError::AnalysisCancelled("Analysis cancelled before acquisition".to_string()));
    }

    // 4. Run S2A Evidence Acquisition (completely backend-controlled)
    let github_token = state.secrets.get_secret(SECRET_KEY_GITHUB_TOKEN).ok().flatten();
    let evidence_service =
        crate::github::evidence::RepositoryEvidenceService::new(Arc::clone(&state.github));
    let options = crate::github::evidence::AcquisitionOptions {
        expected_repo_id: request.expected_repo_id,
        force_fresh: false,
        custom_timeout: None,
    };
    let evidence_pack = evidence_service
        .acquire_evidence(
            &request.repository,
            github_token.as_deref(),
            options,
            Some(cancel_token.clone()),
        )
        .await?;

    if cancel_token.is_cancelled() {
        return Err(AppError::AnalysisCancelled("Analysis cancelled after acquisition".to_string()));
    }

    // 5. Run S2B Analysis Generation Engine (completely backend-controlled)
    let envelope = crate::ai::analysis::execute_repository_analysis(
        &evidence_pack,
        request.target_locale,
        &state.settings,
        &*state.secrets,
        &state.translation_limiter,
        Some(cancel_token.clone()),
        crate::ai::analysis::AnalysisExecutionOptions::default(),
    )
    .await?;

    if cancel_token.is_cancelled() {
        return Err(AppError::AnalysisCancelled("Analysis cancelled before return".to_string()));
    }

    Ok(envelope)
}

#[tauri::command]
pub async fn cancel_repository_analysis(
    state: State<'_, AppState>,
    task_id: String,
) -> Result<bool, AppError> {
    Ok(state.analysis_task_manager.cancel(&task_id))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum InstallationChannel {
    Nsis,
    Msi,
    Standalone,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct RegistryUninstallEntry {
    pub display_name: Option<String>,
    pub uninstall_string: Option<String>,
    pub windows_installer: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstallationChannelInfo {
    pub channel: InstallationChannel,
    pub can_auto_update: bool,
    pub details: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ActiveBackendTasksInfo {
    pub active_analysis_tasks: usize,
    pub is_translating: bool,
    pub has_active_tasks: bool,
}

pub fn parse_reg_query_output(output: &str) -> Option<RegistryUninstallEntry> {
    let mut entry = RegistryUninstallEntry::default();
    let mut found_any = false;

    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("HKEY_") {
            continue;
        }

        if trimmed.starts_with("DisplayName") {
            if let Some(pos) = trimmed.find("REG_SZ") {
                entry.display_name = Some(trimmed[pos + "REG_SZ".len()..].trim().to_string());
                found_any = true;
            } else if let Some(pos) = trimmed.find("REG_EXPAND_SZ") {
                entry.display_name = Some(trimmed[pos + "REG_EXPAND_SZ".len()..].trim().to_string());
                found_any = true;
            }
        } else if trimmed.starts_with("UninstallString") {
            if let Some(pos) = trimmed.find("REG_SZ") {
                entry.uninstall_string = Some(trimmed[pos + "REG_SZ".len()..].trim().to_string());
                found_any = true;
            } else if let Some(pos) = trimmed.find("REG_EXPAND_SZ") {
                entry.uninstall_string = Some(trimmed[pos + "REG_EXPAND_SZ".len()..].trim().to_string());
                found_any = true;
            }
        } else if trimmed.starts_with("WindowsInstaller") {
            if let Some(pos) = trimmed.find("REG_DWORD") {
                let val_str = trimmed[pos + "REG_DWORD".len()..].trim().trim_start_matches("0x");
                if let Ok(val) = u32::from_str_radix(val_str, 16) {
                    entry.windows_installer = Some(val);
                    found_any = true;
                }
            }
        }
    }

    if found_any {
        Some(entry)
    } else {
        None
    }
}

pub fn classify_installation_channel(entries: &[RegistryUninstallEntry]) -> InstallationChannel {
    let mut has_nsis = false;
    let mut has_msi = false;

    for entry in entries {
        let is_reposhelf = entry
            .display_name
            .as_deref()
            .map(|d| d.eq_ignore_ascii_case("RepoShelf"))
            .unwrap_or(false);

        if !is_reposhelf {
            continue;
        }

        if entry.windows_installer == Some(1) {
            has_msi = true;
            continue;
        }

        if let Some(uninstall) = &entry.uninstall_string {
            let lower = uninstall.to_lowercase();
            if lower.contains("msiexec") {
                has_msi = true;
            } else if lower.contains("uninstall") {
                has_nsis = true;
            }
        }
    }

    if has_nsis && has_msi {
        InstallationChannel::Unknown
    } else if has_msi {
        InstallationChannel::Msi
    } else if has_nsis {
        InstallationChannel::Nsis
    } else {
        InstallationChannel::Standalone
    }
}

#[tauri::command]
pub fn get_installation_channel() -> InstallationChannelInfo {
    #[cfg(windows)]
    {
        let mut entries = Vec::new();

        if let Ok(output) = std::process::Command::new("reg")
            .args(["query", r"HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\RepoShelf"])
            .output()
        {
            if output.status.success() {
                if let Ok(text) = String::from_utf8(output.stdout) {
                    if let Some(entry) = parse_reg_query_output(&text) {
                        entries.push(entry);
                    }
                }
            }
        }

        if let Ok(output) = std::process::Command::new("reg")
            .args(["query", r"HKLM\Software\Microsoft\Windows\CurrentVersion\Uninstall\RepoShelf"])
            .output()
        {
            if output.status.success() {
                if let Ok(text) = String::from_utf8(output.stdout) {
                    if let Some(entry) = parse_reg_query_output(&text) {
                        entries.push(entry);
                    }
                }
            }
        }

        let channel = classify_installation_channel(&entries);
        let (can_auto_update, details) = match channel {
            InstallationChannel::Nsis => (
                true,
                "Verified NSIS installation. Automatic update permitted upon user confirmation.".to_string(),
            ),
            InstallationChannel::Msi => (
                false,
                "Detected legacy MSI installation. Automatic update blocked to prevent side-by-side collision. Manual upgrade required.".to_string(),
            ),
            InstallationChannel::Standalone => (
                false,
                "Running as standalone or portable application without registry install records. Automatic update blocked. Manual download required.".to_string(),
            ),
            InstallationChannel::Unknown => (
                false,
                "Unable to reliably determine installation channel or conflicting install records found. Fail closed. Manual upgrade required.".to_string(),
            ),
        };

        InstallationChannelInfo {
            channel,
            can_auto_update,
            details,
        }
    }

    #[cfg(not(windows))]
    {
        InstallationChannelInfo {
            channel: InstallationChannel::Unknown,
            can_auto_update: false,
            details: "Non-Windows platform; automatic NSIS update not supported.".to_string(),
        }
    }
}

#[tauri::command]
pub fn get_active_backend_tasks(state: State<'_, AppState>) -> ActiveBackendTasksInfo {
    let active_analysis = state.analysis_task_manager.active_count();
    let is_translating = state.translation_limiter.is_active();
    ActiveBackendTasksInfo {
        active_analysis_tasks: active_analysis,
        is_translating,
        has_active_tasks: active_analysis > 0 || is_translating,
    }
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

    #[test]
    fn test_classify_nsis_channel() {
        let entries = vec![RegistryUninstallEntry {
            display_name: Some("RepoShelf".to_string()),
            uninstall_string: Some(r#""C:\Users\User\AppData\Local\RepoShelf\uninstall.exe""#.to_string()),
            windows_installer: None,
        }];
        assert_eq!(classify_installation_channel(&entries), InstallationChannel::Nsis);
    }

    #[test]
    fn test_classify_msi_channel_windows_installer_flag() {
        let entries = vec![RegistryUninstallEntry {
            display_name: Some("RepoShelf".to_string()),
            uninstall_string: Some(r#"MsiExec.exe /I{12345678-ABCD-1234-ABCD-1234567890AB}"#.to_string()),
            windows_installer: Some(1),
        }];
        assert_eq!(classify_installation_channel(&entries), InstallationChannel::Msi);
    }

    #[test]
    fn test_classify_msi_channel_msiexec_string() {
        let entries = vec![RegistryUninstallEntry {
            display_name: Some("RepoShelf".to_string()),
            uninstall_string: Some(r#"MsiExec.exe /X{AABBCCDD}"#.to_string()),
            windows_installer: None,
        }];
        assert_eq!(classify_installation_channel(&entries), InstallationChannel::Msi);
    }

    #[test]
    fn test_classify_conflicting_channels_returns_unknown() {
        let entries = vec![
            RegistryUninstallEntry {
                display_name: Some("RepoShelf".to_string()),
                uninstall_string: Some(r#"C:\path\uninstall.exe"#.to_string()),
                windows_installer: None,
            },
            RegistryUninstallEntry {
                display_name: Some("RepoShelf".to_string()),
                uninstall_string: Some(r#"MsiExec.exe /X{GUID}"#.to_string()),
                windows_installer: Some(1),
            },
        ];
        assert_eq!(classify_installation_channel(&entries), InstallationChannel::Unknown);
    }

    #[test]
    fn test_classify_standalone_when_empty_or_no_match() {
        assert_eq!(classify_installation_channel(&[]), InstallationChannel::Standalone);
        let other_entries = vec![RegistryUninstallEntry {
            display_name: Some("OtherApp".to_string()),
            uninstall_string: Some("uninst.exe".to_string()),
            windows_installer: None,
        }];
        assert_eq!(classify_installation_channel(&other_entries), InstallationChannel::Standalone);
    }

    #[test]
    fn test_parse_reg_query_output_nsis() {
        let sample = "\
HKEY_CURRENT_USER\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\RepoShelf\r\n\
    DisplayName    REG_SZ    RepoShelf\r\n\
    UninstallString    REG_SZ    \"D:\\App\\uninstall.exe\"\r\n\
    DisplayVersion    REG_SZ    1.1.4\r\n";
        let parsed = parse_reg_query_output(sample).expect("Should parse NSIS sample");
        assert_eq!(parsed.display_name.as_deref(), Some("RepoShelf"));
        assert_eq!(parsed.uninstall_string.as_deref(), Some("\"D:\\App\\uninstall.exe\""));
        assert_eq!(parsed.windows_installer, None);
    }

    #[test]
    fn test_parse_reg_query_output_msi() {
        let sample = "\
HKEY_LOCAL_MACHINE\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\{GUID}\r\n\
    DisplayName    REG_SZ    RepoShelf\r\n\
    UninstallString    REG_SZ    MsiExec.exe /I{GUID}\r\n\
    WindowsInstaller    REG_DWORD    0x1\r\n";
        let parsed = parse_reg_query_output(sample).expect("Should parse MSI sample");
        assert_eq!(parsed.display_name.as_deref(), Some("RepoShelf"));
        assert_eq!(parsed.windows_installer, Some(1));
    }
}
