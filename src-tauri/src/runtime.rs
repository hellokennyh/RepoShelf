use std::path::{Path, PathBuf};
use thiserror::Error;

pub const ENV_DATA_DIR: &str = "REPOSHELF_DATA_DIR";
pub const ENV_CONFIG_DIR: &str = "REPOSHELF_CONFIG_DIR";
pub const ENV_SECRET_SERVICE: &str = "REPOSHELF_SECRET_SERVICE";

pub const DEFAULT_DB_FILENAME: &str = "reposhelf.db";
pub const DEFAULT_CONFIG_FILENAME: &str = "settings.json";

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RuntimeResolutionError {
    #[error("Environment override for '{0}' cannot be empty or blank")]
    EmptyOverride(String),

    #[error("Environment override for '{var_name}' must be an absolute path, but got '{path}'")]
    RelativePathOverride { var_name: String, path: String },

    #[error("Environment override for '{0}' contains invalid non-Unicode data")]
    InvalidUnicode(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimePaths {
    pub data_dir: PathBuf,
    pub db_path: PathBuf,
    pub config_dir: PathBuf,
    pub settings_path: PathBuf,
    pub secret_service: String,
}

impl RuntimePaths {
    pub fn is_data_dir_overridden(&self, default_data_dir: &Path) -> bool {
        self.data_dir != default_data_dir
    }

    pub fn is_config_dir_overridden(&self, default_config_dir: &Path) -> bool {
        self.config_dir != default_config_dir
    }

    pub fn is_secret_service_overridden(&self, default_secret_service: &str) -> bool {
        self.secret_service != default_secret_service
    }
}

pub fn resolve_runtime_paths_from_env<F>(
    default_data_dir: &Path,
    default_config_dir: &Path,
    default_secret_service: &str,
    mut get_env: F,
) -> Result<RuntimePaths, RuntimeResolutionError>
where
    F: FnMut(&str) -> Result<Option<String>, RuntimeResolutionError>,
{
    // 1. Resolve data directory & db path
    let data_dir = match get_env(ENV_DATA_DIR)? {
        Some(val) => {
            let trimmed = val.trim();
            if trimmed.is_empty() {
                return Err(RuntimeResolutionError::EmptyOverride(
                    ENV_DATA_DIR.to_string(),
                ));
            }
            let path = PathBuf::from(trimmed);
            if !path.is_absolute() {
                return Err(RuntimeResolutionError::RelativePathOverride {
                    var_name: ENV_DATA_DIR.to_string(),
                    path: val,
                });
            }
            path
        }
        None => default_data_dir.to_path_buf(),
    };
    let db_path = data_dir.join(DEFAULT_DB_FILENAME);

    // 2. Resolve config directory & settings path
    let config_dir = match get_env(ENV_CONFIG_DIR)? {
        Some(val) => {
            let trimmed = val.trim();
            if trimmed.is_empty() {
                return Err(RuntimeResolutionError::EmptyOverride(
                    ENV_CONFIG_DIR.to_string(),
                ));
            }
            let path = PathBuf::from(trimmed);
            if !path.is_absolute() {
                return Err(RuntimeResolutionError::RelativePathOverride {
                    var_name: ENV_CONFIG_DIR.to_string(),
                    path: val,
                });
            }
            path
        }
        None => default_config_dir.to_path_buf(),
    };
    let settings_path = config_dir.join(DEFAULT_CONFIG_FILENAME);

    // 3. Resolve secret service namespace
    let secret_service = match get_env(ENV_SECRET_SERVICE)? {
        Some(val) => {
            let trimmed = val.trim();
            if trimmed.is_empty() {
                return Err(RuntimeResolutionError::EmptyOverride(
                    ENV_SECRET_SERVICE.to_string(),
                ));
            }
            trimmed.to_string()
        }
        None => default_secret_service.to_string(),
    };

    Ok(RuntimePaths {
        data_dir,
        db_path,
        config_dir,
        settings_path,
        secret_service,
    })
}

pub fn resolve_runtime_paths(
    default_data_dir: &Path,
    default_config_dir: &Path,
) -> Result<RuntimePaths, RuntimeResolutionError> {
    resolve_runtime_paths_from_env(
        default_data_dir,
        default_config_dir,
        crate::secrets::APP_SECRET_SERVICE,
        |key| match std::env::var(key) {
            Ok(v) => Ok(Some(v)),
            Err(std::env::VarError::NotPresent) => Ok(None),
            Err(std::env::VarError::NotUnicode(_)) => {
                Err(RuntimeResolutionError::InvalidUnicode(key.to_string()))
            }
        },
    )
}

#[cfg(test)]
pub fn resolve_runtime_paths_from_map(
    default_data_dir: &Path,
    default_config_dir: &Path,
    default_secret_service: &str,
    env_map: &std::collections::HashMap<&str, &str>,
) -> Result<RuntimePaths, RuntimeResolutionError> {
    resolve_runtime_paths_from_env(
        default_data_dir,
        default_config_dir,
        default_secret_service,
        |key| Ok(env_map.get(key).map(|s| s.to_string())),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn get_test_abs_path(sub: &str) -> PathBuf {
        #[cfg(windows)]
        {
            PathBuf::from(format!(r"C:\reposhelf_mock\{}", sub))
        }
        #[cfg(not(windows))]
        {
            PathBuf::from(format!("/tmp/reposhelf_mock/{}", sub))
        }
    }

    #[test]
    fn no_overrides_preserve_defaults() {
        let default_data = get_test_abs_path("data");
        let default_config = get_test_abs_path("config");
        let default_secret = "com.reposhelf.desktop";
        let env_map = HashMap::new();

        let paths = resolve_runtime_paths_from_map(
            &default_data,
            &default_config,
            default_secret,
            &env_map,
        )
        .expect("Resolution must succeed with defaults");

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
        let default_data = get_test_abs_path("data");
        let default_config = get_test_abs_path("config");
        let default_secret = "com.reposhelf.desktop";

        let custom_data = get_test_abs_path("isolated_data");
        let custom_data_str = custom_data.to_str().unwrap();

        let mut env_map = HashMap::new();
        env_map.insert(ENV_DATA_DIR, custom_data_str);

        let paths = resolve_runtime_paths_from_map(
            &default_data,
            &default_config,
            default_secret,
            &env_map,
        )
        .expect("Resolution must succeed with custom data dir");

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
        let default_data = get_test_abs_path("data");
        let default_config = get_test_abs_path("config");
        let default_secret = "com.reposhelf.desktop";

        let custom_config = get_test_abs_path("isolated_config");
        let custom_config_str = custom_config.to_str().unwrap();

        let mut env_map = HashMap::new();
        env_map.insert(ENV_CONFIG_DIR, custom_config_str);

        let paths = resolve_runtime_paths_from_map(
            &default_data,
            &default_config,
            default_secret,
            &env_map,
        )
        .expect("Resolution must succeed with custom config dir");

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
        let default_data = get_test_abs_path("data");
        let default_config = get_test_abs_path("config");
        let default_secret = "com.reposhelf.desktop";

        let mut env_map = HashMap::new();
        env_map.insert(ENV_SECRET_SERVICE, "com.reposhelf.desktop.dev-v1.1");

        let paths = resolve_runtime_paths_from_map(
            &default_data,
            &default_config,
            default_secret,
            &env_map,
        )
        .expect("Resolution must succeed with custom secret service");

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
        let default_data = get_test_abs_path("data");
        let default_config = get_test_abs_path("config");
        let default_secret = "com.reposhelf.desktop";

        let custom_data = get_test_abs_path("all_data");
        let custom_config = get_test_abs_path("all_config");
        let custom_data_str = custom_data.to_str().unwrap();
        let custom_config_str = custom_config.to_str().unwrap();

        let mut env_map = HashMap::new();
        env_map.insert(ENV_DATA_DIR, custom_data_str);
        env_map.insert(ENV_CONFIG_DIR, custom_config_str);
        env_map.insert(ENV_SECRET_SERVICE, "com.reposhelf.desktop.dev-v1.1");

        let paths = resolve_runtime_paths_from_map(
            &default_data,
            &default_config,
            default_secret,
            &env_map,
        )
        .expect("Resolution must succeed with all overrides");

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
        let default_data = get_test_abs_path("data");
        let default_config = get_test_abs_path("config");
        let default_secret = "com.reposhelf.desktop";

        for blank in &["", "   ", "\t", "  \n  "] {
            // Test empty data dir
            let mut env_map = HashMap::new();
            env_map.insert(ENV_DATA_DIR, *blank);
            let res = resolve_runtime_paths_from_map(
                &default_data,
                &default_config,
                default_secret,
                &env_map,
            );
            assert_eq!(
                res,
                Err(RuntimeResolutionError::EmptyOverride(
                    ENV_DATA_DIR.to_string()
                )),
                "Blank override for REPOSHELF_DATA_DIR must be rejected"
            );

            // Test empty config dir
            let mut env_map = HashMap::new();
            env_map.insert(ENV_CONFIG_DIR, *blank);
            let res = resolve_runtime_paths_from_map(
                &default_data,
                &default_config,
                default_secret,
                &env_map,
            );
            assert_eq!(
                res,
                Err(RuntimeResolutionError::EmptyOverride(
                    ENV_CONFIG_DIR.to_string()
                )),
                "Blank override for REPOSHELF_CONFIG_DIR must be rejected"
            );

            // Test empty secret service
            let mut env_map = HashMap::new();
            env_map.insert(ENV_SECRET_SERVICE, *blank);
            let res = resolve_runtime_paths_from_map(
                &default_data,
                &default_config,
                default_secret,
                &env_map,
            );
            assert_eq!(
                res,
                Err(RuntimeResolutionError::EmptyOverride(
                    ENV_SECRET_SERVICE.to_string()
                )),
                "Blank override for REPOSHELF_SECRET_SERVICE must be rejected"
            );
        }
    }

    #[test]
    fn relative_path_override_is_rejected() {
        let default_data = get_test_abs_path("data");
        let default_config = get_test_abs_path("config");
        let default_secret = "com.reposhelf.desktop";

        let relative_candidates = &["relative_dir", "some/sub/dir", r".\sub", r"..\parent"];

        for rel in relative_candidates {
            // Test relative data dir
            let mut env_map = HashMap::new();
            env_map.insert(ENV_DATA_DIR, *rel);
            let res = resolve_runtime_paths_from_map(
                &default_data,
                &default_config,
                default_secret,
                &env_map,
            );
            assert!(
                matches!(res, Err(RuntimeResolutionError::RelativePathOverride { ref var_name, ref path }) if var_name == ENV_DATA_DIR && path == *rel),
                "Relative path '{}' for REPOSHELF_DATA_DIR must be rejected",
                rel
            );

            // Test relative config dir
            let mut env_map = HashMap::new();
            env_map.insert(ENV_CONFIG_DIR, *rel);
            let res = resolve_runtime_paths_from_map(
                &default_data,
                &default_config,
                default_secret,
                &env_map,
            );
            assert!(
                matches!(res, Err(RuntimeResolutionError::RelativePathOverride { ref var_name, ref path }) if var_name == ENV_CONFIG_DIR && path == *rel),
                "Relative path '{}' for REPOSHELF_CONFIG_DIR must be rejected",
                rel
            );
        }
    }

    #[test]
    fn override_does_not_fall_back_to_production_on_error() {
        let default_data = get_test_abs_path("data");
        let default_config = get_test_abs_path("config");
        let default_secret = "com.reposhelf.desktop";

        // Provide an invalid relative path for data_dir
        let mut env_map = HashMap::new();
        env_map.insert(ENV_DATA_DIR, "not_an_absolute_path");

        let res = resolve_runtime_paths_from_map(
            &default_data,
            &default_config,
            default_secret,
            &env_map,
        );

        // It MUST fail closed and return an Err, NOT return default paths
        assert!(res.is_err(), "Must return Err on invalid override");
        if let Ok(paths) = res {
            panic!(
                "CRITICAL ERROR: Resolver silently fell back to defaults: {:?}",
                paths
            );
        }

        // Provide an empty string for secret service
        let mut env_map = HashMap::new();
        env_map.insert(ENV_SECRET_SERVICE, "   ");

        let res = resolve_runtime_paths_from_map(
            &default_data,
            &default_config,
            default_secret,
            &env_map,
        );

        assert!(
            res.is_err(),
            "Must return Err on blank secret service override"
        );
        if let Ok(paths) = res {
            panic!(
                "CRITICAL ERROR: Resolver silently fell back to defaults: {:?}",
                paths
            );
        }
    }
}
