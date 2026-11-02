pub mod ai;
pub mod article;
pub mod commands;
pub mod db;
pub mod error;
pub mod github;
pub mod library;
pub mod runtime;
pub mod secrets;
pub mod settings;
pub mod star_history;
pub mod trending;

use std::sync::Arc;
use tauri::Manager;

pub async fn init_runtime_state(
    paths: &runtime::RuntimePaths,
) -> Result<commands::AppState, Box<dyn std::error::Error>> {
    std::fs::create_dir_all(&paths.data_dir)?;
    std::fs::create_dir_all(&paths.config_dir)?;

    let pool = db::database::init_pool(&paths.db_path)
        .await
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;

    let settings_store = Arc::new(
        settings::SettingsStore::new(paths.settings_path.clone())
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?,
    );

    let secret_store: Arc<dyn secrets::SecretStore> =
        Arc::new(secrets::OsSecretStore::with_service(&paths.secret_service));

    let github_client = Arc::new(github::GitHubClient::new());

    let trending_provider: Arc<dyn trending::TrendingProvider> =
        Arc::new(trending::GitHubTrendingProvider::new(
            Arc::clone(&github_client),
            Arc::clone(&secret_store),
        ));

    Ok(commands::AppState {
        db: pool,
        settings: settings_store,
        secrets: secret_store,
        github: Arc::clone(&github_client),
        trending: trending_provider,
        translation_limiter: Arc::new(ai::TranslationLimiter::new()),
        star_history_service: Arc::new(star_history::StarHistoryService::new(Arc::clone(
            &github_client,
        ))),
        analysis_task_manager: Arc::new(ai::analysis::AnalysisTaskManager::new()),
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            let app_handle = app.handle();

            let default_data_dir = app_handle
                .path()
                .app_data_dir()
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;

            let default_config_dir = app_handle
                .path()
                .app_config_dir()
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;

            let runtime_paths =
                runtime::resolve_runtime_paths(&default_data_dir, &default_config_dir)
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;

            let app_state =
                tauri::async_runtime::block_on(async { init_runtime_state(&runtime_paths).await })?;

            app.manage(app_state);

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_app_info,
            commands::get_settings,
            commands::update_settings,
            commands::update_ui_language,
            commands::get_locale_initialization_info,
            commands::get_secret_status,
            commands::set_github_token,
            commands::clear_github_token,
            commands::set_ai_api_key,
            commands::clear_ai_api_key,
            commands::test_ai_connection,
            commands::discover_ollama_endpoints,
            commands::list_ollama_models,
            commands::get_ai_search_readiness,
            commands::github_test_connection,
            commands::github_get_repository,
            commands::import_repositories_from_article,
            commands::github_search_repositories,
            commands::github_ai_search_repositories,
            commands::get_trending,
            commands::open_github_url,
            commands::check_favorite_status,
            commands::check_favorite_identities_batch,
            commands::favorite_repository,
            commands::favorite_repositories_batch,
            commands::get_favorite,
            commands::list_favorites,
            commands::list_distinct_tags,
            commands::update_favorite_user_metadata,
            commands::delete_favorite,
            commands::delete_favorites_batch,
            commands::refresh_favorite,
            commands::refresh_favorites_batch,
            commands::suggest_favorite_ai_metadata,
            commands::apply_favorite_ai_metadata,
            commands::translate_repository_description,
            commands::get_star_history_preview,
            commands::analyze_repository,
            commands::cancel_repository_analysis,
        ])


        .run(tauri::generate_context!())
        .expect("error while running RepoShelf application");
}
