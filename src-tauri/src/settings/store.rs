use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use std::sync::RwLock;

use super::models::Settings;
use crate::error::AppError;

pub struct SettingsStore {
    config_path: PathBuf,
    current: RwLock<Settings>,
}

impl SettingsStore {
    pub fn new(config_path: PathBuf) -> Result<Self, AppError> {
        let store = Self {
            config_path,
            current: RwLock::new(Settings::default()),
        };

        // Initialize state from disk if exists, otherwise default
        let loaded = store.load_from_disk_or_default()?;
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

    pub fn save(&self, new_settings: &Settings) -> Result<Settings, AppError> {
        new_settings
            .validate()
            .map_err(AppError::SettingsValidation)?;

        self.atomic_write_to_disk(new_settings)?;

        let mut guard = self
            .current
            .write()
            .map_err(|e| AppError::SettingsWrite(format!("RwLock poisoned during save: {}", e)))?;
        *guard = new_settings.clone();

        Ok(new_settings.clone())
    }

    fn load_from_disk_or_default(&self) -> Result<Settings, AppError> {
        if !self.config_path.exists() {
            return Ok(Settings::default());
        }

        let content = fs::read_to_string(&self.config_path).map_err(|e| {
            AppError::SettingsRead(format!(
                "Failed to read settings file at {}: {}",
                self.config_path.display(),
                e
            ))
        })?;

        let settings: Settings = serde_json::from_str(&content).map_err(|e| {
            AppError::SettingsRead(format!("Corrupted or invalid JSON in settings file: {}", e))
        })?;

        settings.validate().map_err(AppError::SettingsValidation)?;

        Ok(settings)
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
