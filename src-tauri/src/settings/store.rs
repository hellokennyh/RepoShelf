use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use std::sync::RwLock;

use super::locale::{
    detect_and_resolve_windows_locale, SystemWindowsUiLanguageDetector, UiLocale,
    WindowsUiLanguageDetector,
};
use super::models::Settings;
use crate::error::AppError;

#[derive(Debug)]
pub struct SettingsStore {
    config_path: PathBuf,
    current: RwLock<Settings>,
    initial_detection_warning: RwLock<Option<String>>,
    detected_source_tag: RwLock<Option<String>>,
}

impl SettingsStore {
    pub fn new(config_path: PathBuf) -> Result<Self, AppError> {
        let detector = SystemWindowsUiLanguageDetector;
        Self::new_with_detector(config_path, &detector)
    }

    pub fn new_with_detector<D: WindowsUiLanguageDetector>(
        config_path: PathBuf,
        detector: &D,
    ) -> Result<Self, AppError> {
        let store = Self {
            config_path,
            current: RwLock::new(Settings::default()),
            initial_detection_warning: RwLock::new(None),
            detected_source_tag: RwLock::new(None),
        };

        let loaded = store.load_and_initialize(detector)?;
        *store.current.write().map_err(|e| {
            AppError::SettingsRead(format!("RwLock poisoned during initialization: {}", e))
        })? = loaded;

        Ok(store)
    }

    pub fn get(&self) -> Result<Settings, AppError> {
        self.current
            .read()
            .map(|guard| guard.clone())
            .map_err(|e| AppError::SettingsRead(format!("Failed to read current settings: {}", e)))
    }

    pub fn get_initial_warning(&self) -> Option<String> {
        self.initial_detection_warning
            .read()
            .ok()
            .and_then(|guard| guard.clone())
    }

    pub fn get_detected_source_tag(&self) -> Option<String> {
        self.detected_source_tag
            .read()
            .ok()
            .and_then(|guard| guard.clone())
    }

    pub fn save(&self, new_settings: &Settings) -> Result<Settings, AppError> {
        new_settings
            .validate()
            .map_err(AppError::SettingsValidation)?;

        let mut to_save = new_settings.clone();
        if to_save.ui_language.is_none() {
            if let Ok(current) = self.get() {
                to_save.ui_language = current.ui_language;
            }
        }

        self.atomic_write_to_disk(&to_save)?;

        let mut guard = self
            .current
            .write()
            .map_err(|e| AppError::SettingsWrite(format!("RwLock poisoned during save: {}", e)))?;
        *guard = to_save.clone();

        Ok(to_save)
    }

    /// Single-field update for UI language: preserves all committed settings.
    pub fn update_ui_language(&self, locale: UiLocale) -> Result<Settings, AppError> {
        let mut current_settings = self.get()?;
        current_settings.ui_language = Some(locale);
        self.save(&current_settings)
    }

    fn load_and_initialize<D: WindowsUiLanguageDetector>(
        &self,
        detector: &D,
    ) -> Result<Settings, AppError> {
        if !self.config_path.exists() {
            // Fresh safe settings state: perform detection and persist normal settings
            let (resolved_locale, source_tag) = detect_and_resolve_windows_locale(detector);
            if let Ok(mut guard) = self.detected_source_tag.write() {
                *guard = source_tag;
            }

            let initial_settings = Settings {
                schema_version: 1,
                github: Default::default(),
                ai: Default::default(),
                ui_language: Some(resolved_locale),
            };

            initial_settings
                .validate()
                .map_err(AppError::SettingsValidation)?;

            // Try to persist the initial settings
            if let Err(write_err) = self.atomic_write_to_disk(&initial_settings) {
                if let Ok(mut guard) = self.initial_detection_warning.write() {
                    *guard = Some(format!(
                        "Failed to persist initial language setting: {}",
                        write_err
                    ));
                }
            }

            return Ok(initial_settings);
        }

        // Settings file exists: read it safely
        let content = fs::read_to_string(&self.config_path).map_err(|e| {
            AppError::SettingsRead(format!(
                "Failed to read settings file at {}: {}",
                self.config_path.display(),
                e
            ))
        })?;

        // Parse into raw serde_json::Value to check key existence and validity without loss
        let raw_value: serde_json::Value = serde_json::from_str(&content).map_err(|e| {
            AppError::SettingsRead(format!("Corrupted or invalid JSON in settings file: {}", e))
        })?;

        // Inspect raw_value for ui_language specifically
        let raw_obj = raw_value.as_object().ok_or_else(|| {
            AppError::SettingsRead("Settings JSON root is not an object".to_string())
        })?;

        let mut sanitized_value = raw_value.clone();
        if let Some(obj) = sanitized_value.as_object_mut() {
            obj.remove("ui_language");
        }

        let mut settings: Settings = serde_json::from_value(sanitized_value)
            .map_err(|e| AppError::SettingsRead(format!("Corrupted settings structure: {}", e)))?;

        settings.validate().map_err(AppError::SettingsValidation)?;

        let ui_lang_val = raw_obj.get("ui_language");

        match ui_lang_val {
            Some(serde_json::Value::String(s)) => {
                if let Some(locale) = UiLocale::from_str_strict(s) {
                    // Valid existing setting has absolute priority!
                    // Do not call Windows locale detection, do not rewrite settings.
                    settings.ui_language = Some(locale);
                    Ok(settings)
                } else {
                    // Key exists but is invalid: detect Windows locale, repair and persist
                    let (resolved_locale, source_tag) = detect_and_resolve_windows_locale(detector);
                    if let Ok(mut guard) = self.detected_source_tag.write() {
                        *guard = source_tag;
                    }
                    settings.ui_language = Some(resolved_locale);

                    if let Err(write_err) = self.atomic_write_to_disk(&settings) {
                        if let Ok(mut guard) = self.initial_detection_warning.write() {
                            *guard = Some(format!(
                                "Failed to persist repaired language setting: {}",
                                write_err
                            ));
                        }
                    }

                    Ok(settings)
                }
            }
            Some(serde_json::Value::Null) | None => {
                // Key missing or null: detect Windows locale and persist only the intended language setting
                let (resolved_locale, source_tag) = detect_and_resolve_windows_locale(detector);
                if let Ok(mut guard) = self.detected_source_tag.write() {
                    *guard = source_tag;
                }
                settings.ui_language = Some(resolved_locale);

                if let Err(write_err) = self.atomic_write_to_disk(&settings) {
                    if let Ok(mut guard) = self.initial_detection_warning.write() {
                        *guard = Some(format!(
                            "Failed to persist detected language setting: {}",
                            write_err
                        ));
                    }
                }

                Ok(settings)
            }
            Some(_) => {
                // Non-string value (number, bool, array, obj): treat as invalid
                let (resolved_locale, source_tag) = detect_and_resolve_windows_locale(detector);
                if let Ok(mut guard) = self.detected_source_tag.write() {
                    *guard = source_tag;
                }
                settings.ui_language = Some(resolved_locale);

                if let Err(write_err) = self.atomic_write_to_disk(&settings) {
                    if let Ok(mut guard) = self.initial_detection_warning.write() {
                        *guard = Some(format!(
                            "Failed to persist repaired language setting: {}",
                            write_err
                        ));
                    }
                }

                Ok(settings)
            }
        }
    }

    fn atomic_write_to_disk(&self, settings: &Settings) -> Result<(), AppError> {
        if let Some(parent) = self.config_path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                AppError::SettingsWrite(format!("Failed to create config directory: {}", e))
            })?;
        }

        let json = serde_json::to_string_pretty(settings)
            .map_err(|e| AppError::SettingsWrite(format!("Failed to serialize settings: {}", e)))?;

        let tmp_path = self.config_path.with_extension("json.tmp");

        // Write to temporary file
        let mut tmp_file = File::create(&tmp_path).map_err(|e| {
            AppError::SettingsWrite(format!("Failed to create temporary settings file: {}", e))
        })?;

        tmp_file.write_all(json.as_bytes()).map_err(|e| {
            AppError::SettingsWrite(format!("Failed to write to temporary settings file: {}", e))
        })?;

        tmp_file.sync_all().map_err(|e| {
            AppError::SettingsWrite(format!("Failed to sync temporary settings file: {}", e))
        })?;

        drop(tmp_file);

        // Atomic replace
        fs::rename(&tmp_path, &self.config_path).map_err(|e| {
            AppError::SettingsWrite(format!(
                "Failed to replace settings file with temporary file: {}",
                e
            ))
        })?;

        Ok(())
    }
}
