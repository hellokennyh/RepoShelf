use std::collections::HashMap;
use std::path::PathBuf;
use tempfile::tempdir;

use reposhelf_lib::init_runtime_state;
use reposhelf_lib::runtime::{
    resolve_runtime_paths_from_env, RuntimePaths, RuntimeResolutionError, ENV_CONFIG_DIR,
    ENV_DATA_DIR, ENV_SECRET_SERVICE,
};

fn get_mock_base_path(sub: &str) -> PathBuf {
    #[cfg(windows)]
    {
        PathBuf::from(format!(r"C:\reposhelf_mock\{}", sub))
    }
    #[cfg(not(windows))]
    {
        PathBuf::from(format!("/tmp/reposhelf_mock/{}", sub))
    }
}

// ---------------------------------------------------------------------------
// Section VI Required Tests
// ---------------------------------------------------------------------------

#[test]
fn no_overrides_preserve_defaults() {
    let default_data = get_mock_base_path("default_data");
    let default_config = get_mock_base_path("default_config");
    let default_secret = "com.reposhelf.desktop";

    let paths =
        resolve_runtime_paths_from_env(&default_data, &default_config, default_secret, |_| {
            Ok(None)
        })
        .expect("Resolution must succeed when no overrides are provided");

    assert_eq!(paths.data_dir, default_data);
    assert_eq!(paths.db_path, default_data.join("reposhelf.db"));
    assert_eq!(paths.config_dir, default_config);
    assert_eq!(paths.settings_path, default_config.join("settings.json"));
    assert_eq!(paths.secret_service, "com.reposhelf.desktop");

    assert!(!paths.is_data_dir_overridden(&default_data));
    assert!(!paths.is_config_dir_overridden(&default_config));
    assert!(!paths.is_secret_service_overridden(default_secret));
}

#[test]
fn data_dir_override_only() {
    let default_data = get_mock_base_path("default_data");
    let default_config = get_mock_base_path("default_config");
    let default_secret = "com.reposhelf.desktop";

    let custom_data = get_mock_base_path("custom_data");
    let custom_data_str = custom_data.to_str().unwrap().to_string();

    let paths =
        resolve_runtime_paths_from_env(&default_data, &default_config, default_secret, |key| {
            if key == ENV_DATA_DIR {
                Ok(Some(custom_data_str.clone()))
            } else {
                Ok(None)
            }
        })
        .expect("Resolution must succeed with data dir override only");

    assert_eq!(paths.data_dir, custom_data);
    assert_eq!(paths.db_path, custom_data.join("reposhelf.db"));
    assert_eq!(paths.config_dir, default_config);
    assert_eq!(paths.settings_path, default_config.join("settings.json"));
    assert_eq!(paths.secret_service, "com.reposhelf.desktop");

    assert!(paths.is_data_dir_overridden(&default_data));
    assert!(!paths.is_config_dir_overridden(&default_config));
    assert!(!paths.is_secret_service_overridden(default_secret));
}

#[test]
fn config_dir_override_only() {
    let default_data = get_mock_base_path("default_data");
    let default_config = get_mock_base_path("default_config");
    let default_secret = "com.reposhelf.desktop";

    let custom_config = get_mock_base_path("custom_config");
    let custom_config_str = custom_config.to_str().unwrap().to_string();

    let paths =
        resolve_runtime_paths_from_env(&default_data, &default_config, default_secret, |key| {
            if key == ENV_CONFIG_DIR {
                Ok(Some(custom_config_str.clone()))
            } else {
                Ok(None)
            }
        })
        .expect("Resolution must succeed with config dir override only");

    assert_eq!(paths.data_dir, default_data);
    assert_eq!(paths.db_path, default_data.join("reposhelf.db"));
    assert_eq!(paths.config_dir, custom_config);
    assert_eq!(paths.settings_path, custom_config.join("settings.json"));
    assert_eq!(paths.secret_service, "com.reposhelf.desktop");

    assert!(!paths.is_data_dir_overridden(&default_data));
    assert!(paths.is_config_dir_overridden(&default_config));
    assert!(!paths.is_secret_service_overridden(default_secret));
}

#[test]
fn secret_service_override_only() {
    let default_data = get_mock_base_path("default_data");
    let default_config = get_mock_base_path("default_config");
    let default_secret = "com.reposhelf.desktop";

    let custom_service = "com.reposhelf.desktop.dev-v1.1".to_string();

    let paths =
        resolve_runtime_paths_from_env(&default_data, &default_config, default_secret, |key| {
            if key == ENV_SECRET_SERVICE {
                Ok(Some(custom_service.clone()))
            } else {
                Ok(None)
            }
        })
        .expect("Resolution must succeed with secret service override only");

    assert_eq!(paths.data_dir, default_data);
    assert_eq!(paths.db_path, default_data.join("reposhelf.db"));
    assert_eq!(paths.config_dir, default_config);
    assert_eq!(paths.settings_path, default_config.join("settings.json"));
    assert_eq!(paths.secret_service, "com.reposhelf.desktop.dev-v1.1");

    assert!(!paths.is_data_dir_overridden(&default_data));
    assert!(!paths.is_config_dir_overridden(&default_config));
    assert!(paths.is_secret_service_overridden(default_secret));
}

#[test]
fn all_overrides() {
    let default_data = get_mock_base_path("default_data");
    let default_config = get_mock_base_path("default_config");
    let default_secret = "com.reposhelf.desktop";

    let custom_data = get_mock_base_path("isolated_all_data");
    let custom_config = get_mock_base_path("isolated_all_config");
    let custom_service = "com.reposhelf.desktop.dev-v1.1";

    let env_map: HashMap<&str, String> = [
        (ENV_DATA_DIR, custom_data.to_str().unwrap().to_string()),
        (ENV_CONFIG_DIR, custom_config.to_str().unwrap().to_string()),
        (ENV_SECRET_SERVICE, custom_service.to_string()),
    ]
    .into_iter()
    .collect();

    let paths =
        resolve_runtime_paths_from_env(&default_data, &default_config, default_secret, |key| {
            Ok(env_map.get(key).cloned())
        })
        .expect("Resolution must succeed with all three overrides");

    assert_eq!(paths.data_dir, custom_data);
    assert_eq!(paths.db_path, custom_data.join("reposhelf.db"));
    assert_eq!(paths.config_dir, custom_config);
    assert_eq!(paths.settings_path, custom_config.join("settings.json"));
    assert_eq!(paths.secret_service, "com.reposhelf.desktop.dev-v1.1");

    assert!(paths.is_data_dir_overridden(&default_data));
    assert!(paths.is_config_dir_overridden(&default_config));
    assert!(paths.is_secret_service_overridden(default_secret));
}

#[test]
fn empty_override_is_rejected() {
    let default_data = get_mock_base_path("default_data");
    let default_config = get_mock_base_path("default_config");
    let default_secret = "com.reposhelf.desktop";

    for empty_val in &["", "   ", "\t", "\r\n"] {
        // Test data dir empty override
        let res =
            resolve_runtime_paths_from_env(&default_data, &default_config, default_secret, |key| {
                if key == ENV_DATA_DIR {
                    Ok(Some(empty_val.to_string()))
                } else {
                    Ok(None)
                }
            });
        assert_eq!(
            res,
            Err(RuntimeResolutionError::EmptyOverride(
                ENV_DATA_DIR.to_string()
            )),
            "Empty override for REPOSHELF_DATA_DIR must be rejected"
        );

        // Test config dir empty override
        let res =
            resolve_runtime_paths_from_env(&default_data, &default_config, default_secret, |key| {
                if key == ENV_CONFIG_DIR {
                    Ok(Some(empty_val.to_string()))
                } else {
                    Ok(None)
                }
            });
        assert_eq!(
            res,
            Err(RuntimeResolutionError::EmptyOverride(
                ENV_CONFIG_DIR.to_string()
            )),
            "Empty override for REPOSHELF_CONFIG_DIR must be rejected"
        );

        // Test secret service empty override
        let res =
            resolve_runtime_paths_from_env(&default_data, &default_config, default_secret, |key| {
                if key == ENV_SECRET_SERVICE {
                    Ok(Some(empty_val.to_string()))
                } else {
                    Ok(None)
                }
            });
        assert_eq!(
            res,
            Err(RuntimeResolutionError::EmptyOverride(
                ENV_SECRET_SERVICE.to_string()
            )),
            "Empty override for REPOSHELF_SECRET_SERVICE must be rejected"
        );
    }
}

#[test]
fn relative_path_override_is_rejected() {
    let default_data = get_mock_base_path("default_data");
    let default_config = get_mock_base_path("default_config");
    let default_secret = "com.reposhelf.desktop";

    let relative_cases = &["relative_dir", "sub/dir", r".\sub", r"..\parent"];

    for rel in relative_cases {
        let res =
            resolve_runtime_paths_from_env(&default_data, &default_config, default_secret, |key| {
                if key == ENV_DATA_DIR {
                    Ok(Some(rel.to_string()))
                } else {
                    Ok(None)
                }
            });
        assert!(
            matches!(res, Err(RuntimeResolutionError::RelativePathOverride { ref var_name, ref path }) if var_name == ENV_DATA_DIR && path == *rel),
            "Relative path '{}' for REPOSHELF_DATA_DIR must be rejected",
            rel
        );

        let res =
            resolve_runtime_paths_from_env(&default_data, &default_config, default_secret, |key| {
                if key == ENV_CONFIG_DIR {
                    Ok(Some(rel.to_string()))
                } else {
                    Ok(None)
                }
            });
        assert!(
            matches!(res, Err(RuntimeResolutionError::RelativePathOverride { ref var_name, ref path }) if var_name == ENV_CONFIG_DIR && path == *rel),
            "Relative path '{}' for REPOSHELF_CONFIG_DIR must be rejected",
            rel
        );
    }
}

#[test]
fn override_does_not_fall_back_to_production_on_error() {
    let default_data = get_mock_base_path("default_data");
    let default_config = get_mock_base_path("default_config");
    let default_secret = "com.reposhelf.desktop";

    // Invalid relative path
    let res =
        resolve_runtime_paths_from_env(&default_data, &default_config, default_secret, |key| {
            if key == ENV_DATA_DIR {
                Ok(Some("invalid_relative_dir".to_string()))
            } else {
                Ok(None)
            }
        });
    assert!(res.is_err(), "Must return Err on relative path override");
    if let Ok(paths) = res {
        panic!("CRITICAL: Resolver fell back to defaults: {:?}", paths);
    }

    // Blank string
    let res =
        resolve_runtime_paths_from_env(&default_data, &default_config, default_secret, |key| {
            if key == ENV_SECRET_SERVICE {
                Ok(Some("   ".to_string()))
            } else {
                Ok(None)
            }
        });
    assert!(
        res.is_err(),
        "Must return Err on blank secret service override"
    );
    if let Ok(paths) = res {
        panic!("CRITICAL: Resolver fell back to defaults: {:?}", paths);
    }
}

// ---------------------------------------------------------------------------
// Integration Harness Test: Initializing Runtime with Isolated Paths
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_isolated_runtime_initialization_harness() {
    // 1. Create temporary directories for isolated data and config
    let temp_data_dir = tempdir().expect("Failed to create temp data dir");
    let temp_config_dir = tempdir().expect("Failed to create temp config dir");

    let isolated_data_path = temp_data_dir.path().to_path_buf();
    let isolated_config_path = temp_config_dir.path().to_path_buf();
    let isolated_secret_service = "com.reposhelf.desktop.dev-v1.1".to_string();

    let runtime_paths = RuntimePaths {
        data_dir: isolated_data_path.clone(),
        db_path: isolated_data_path.join("reposhelf.db"),
        config_dir: isolated_config_path.clone(),
        settings_path: isolated_config_path.join("settings.json"),
        secret_service: isolated_secret_service.clone(),
    };

    // 2. Execute runtime initialization using the isolated paths
    let app_state = init_runtime_state(&runtime_paths)
        .await
        .expect("Runtime initialization with isolated paths must succeed");

    // 3. Verify isolated database was created and migrations ran
    let expected_db_file = isolated_data_path.join("reposhelf.db");
    assert!(
        expected_db_file.exists(),
        "reposhelf.db must exist in isolated data dir"
    );

    // Verify foreign keys are enabled on the isolated database pool
    let row: (i64,) = sqlx::query_as("PRAGMA foreign_keys")
        .fetch_one(&app_state.db)
        .await
        .expect("Querying foreign keys on isolated pool must succeed");
    assert_eq!(
        row.0, 1,
        "Foreign keys must be enabled on isolated database"
    );

    // 4. Verify isolated settings store operates in isolated config directory
    let settings = app_state
        .settings
        .get()
        .expect("Reading settings from isolated store must succeed");
    assert_eq!(
        settings.schema_version, 1,
        "Isolated settings default schema version must be 1"
    );

    // 5. Verify SecretStore service namespace is strictly isolated
    assert_eq!(
        app_state.secrets.service(),
        "com.reposhelf.desktop.dev-v1.1",
        "SecretStore must use the isolated namespace"
    );
}

#[tokio::test]
async fn test_process_env_isolated_startup_gate() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let isolated_data = temp_dir.path().join("isolated_data");
    let isolated_config = temp_dir.path().join("isolated_config");
    let isolated_secret = "com.reposhelf.desktop.dev-v1.1";

    std::env::set_var(ENV_DATA_DIR, isolated_data.to_str().unwrap());
    std::env::set_var(ENV_CONFIG_DIR, isolated_config.to_str().unwrap());
    std::env::set_var(ENV_SECRET_SERVICE, isolated_secret);

    let default_prod_data =
        PathBuf::from(r"C:\Users\Administrator\AppData\Roaming\com.reposhelf.desktop");
    let default_prod_config =
        PathBuf::from(r"C:\Users\Administrator\AppData\Roaming\com.reposhelf.desktop");

    let paths =
        reposhelf_lib::runtime::resolve_runtime_paths(&default_prod_data, &default_prod_config)
            .expect("Resolution from process environment must succeed");

    assert_eq!(paths.data_dir, isolated_data);
    assert_eq!(paths.config_dir, isolated_config);
    assert_eq!(paths.secret_service, isolated_secret);

    let app_state = init_runtime_state(&paths)
        .await
        .expect("Runtime initialization with process env overrides must succeed");

    assert!(isolated_data.join("reposhelf.db").exists());
    assert_eq!(app_state.secrets.service(), isolated_secret);

    // Clean up
    std::env::remove_var(ENV_DATA_DIR);
    std::env::remove_var(ENV_CONFIG_DIR);
    std::env::remove_var(ENV_SECRET_SERVICE);
}

#[tokio::test]
async fn test_external_env_isolated_startup() {
    let data_dir_env = std::env::var(ENV_DATA_DIR).ok();
    let config_dir_env = std::env::var(ENV_CONFIG_DIR).ok();
    let secret_service_env = std::env::var(ENV_SECRET_SERVICE).ok();

    if let (Some(data_dir), Some(config_dir)) = (data_dir_env, config_dir_env) {
        let default_prod_data =
            PathBuf::from(r"C:\Users\Administrator\AppData\Roaming\com.reposhelf.desktop");
        let default_prod_config =
            PathBuf::from(r"C:\Users\Administrator\AppData\Roaming\com.reposhelf.desktop");

        let paths =
            reposhelf_lib::runtime::resolve_runtime_paths(&default_prod_data, &default_prod_config)
                .expect("Resolution from external environment must succeed");

        assert_eq!(paths.data_dir, PathBuf::from(&data_dir));
        assert_eq!(paths.config_dir, PathBuf::from(&config_dir));

        if let Some(ref sec) = secret_service_env {
            assert_eq!(&paths.secret_service, sec);
        }

        let app_state = init_runtime_state(&paths)
            .await
            .expect("Initialization from external env must succeed");

        assert!(paths.db_path.exists());
        if let Some(sec) = secret_service_env {
            assert_eq!(app_state.secrets.service(), sec);
        }
    }
}
