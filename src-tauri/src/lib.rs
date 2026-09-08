pub mod ai;
pub mod commands;
pub mod db;
pub mod error;
pub mod github;
pub mod library;
pub mod secrets;
pub mod settings;
pub mod trending;

use std::sync::Arc;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let app_handle = app.handle();

            // 1. Resolve app data directory for SQLite database
            let app_data_dir = app_handle
                .path()
                .app_data_dir()
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;

            // 2. Resolve app config directory for settings.json
            let app_config_dir = app_handle
                .path()
                .app_config_dir()
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;

            std::fs::create_dir_all(&app_data_dir)?;
            std::fs::create_dir_all(&app_config_dir)?;

            let db_path = app_data_dir.join("reposhelf.db");
            let config_path = app_config_dir.join("settings.json");

            // 3. Open SQLite with foreign keys and run migrations
            let pool =
                tauri::async_runtime::block_on(async { db::database::init_pool(&db_path).await })
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;

            // 4. Initialize SettingsStore with fallback to default
            let settings_store = Arc::new(
                settings::SettingsStore::new(config_path)
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?,
            );

            // 5. Initialize OS SecretStore targeting Windows Credential Manager
            let secret_store: Arc<dyn secrets::SecretStore> =
                Arc::new(secrets::OsSecretStore::new());

            // 6. Initialize GitHub REST client
            let github_client = Arc::new(github::GitHubClient::new());

            // 7. Initialize GitHub Trending provider
            let trending_provider: Arc<dyn trending::TrendingProvider> =
                Arc::new(trending::GitHubTrendingProvider::new(
                    Arc::clone(&github_client),
                    Arc::clone(&secret_store),
                ));

            let app_state = commands::AppState {
                db: pool,
                settings: settings_store,
                secrets: secret_store,
                github: github_client,
                trending: trending_provider,
            };

            app.manage(app_state);

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_app_info,
            commands::get_settings,
            commands::update_settings,
            commands::get_secret_status,
            commands::set_github_token,
            commands::clear_github_token,
            commands::set_ai_api_key,
            commands::clear_ai_api_key,
            commands::test_ai_connection,
            commands::get_ai_search_readiness,
            commands::github_test_connection,
            commands::github_get_repository,
            commands::github_search_repositories,
            commands::github_ai_search_repositories,
            commands::get_trending,
            commands::open_github_url,
            commands::check_favorite_status,
            commands::favorite_repository,
            commands::favorite_repositories_batch,
            commands::get_favorite,
            commands::list_favorites,
            commands::list_distinct_tags,
            commands::update_favorite_user_metadata,
            commands::delete_favorite,
            commands::refresh_favorite,
            commands::refresh_favorites_batch,
            commands::suggest_favorite_ai_metadata,
            commands::apply_favorite_ai_metadata,
        ])
        .run(tauri::generate_context!())
        .expect("error while running RepoShelf application");
}
