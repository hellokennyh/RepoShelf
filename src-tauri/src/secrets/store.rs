use std::collections::HashMap;
use std::sync::RwLock;

use crate::error::AppError;

pub const APP_SECRET_SERVICE: &str = "com.reposhelf.desktop";
pub const SECRET_KEY_GITHUB_TOKEN: &str = "github_token";
pub const SECRET_KEY_AI_API_KEY: &str = "ai_api_key";

pub trait SecretStore: Send + Sync {
    fn set_secret(&self, key: &str, value: &str) -> Result<(), AppError>;
    fn get_secret(&self, key: &str) -> Result<Option<String>, AppError>;
    fn delete_secret(&self, key: &str) -> Result<(), AppError>;
    fn has_secret(&self, key: &str) -> Result<bool, AppError>;
}

pub struct OsSecretStore {
    service: String,
}

impl OsSecretStore {
    pub fn new() -> Self {
        Self {
            service: APP_SECRET_SERVICE.to_string(),
        }
    }

    pub fn with_service(service: &str) -> Self {
        Self {
            service: service.to_string(),
        }
    }

    pub fn service(&self) -> &str {
        &self.service
    }
}

impl Default for OsSecretStore {
    fn default() -> Self {
        Self::new()
    }
}

impl SecretStore for OsSecretStore {
    fn set_secret(&self, key: &str, value: &str) -> Result<(), AppError> {
        let entry = keyring::Entry::new(&self.service, key).map_err(|e| {
            AppError::SecureStorage(format!("Failed to initialize keyring entry: {}", e))
        })?;

        entry.set_password(value).map_err(|e| {
            AppError::SecureStorage(format!("Failed to store secret in OS keyring: {}", e))
        })?;

        Ok(())
    }

    fn get_secret(&self, key: &str) -> Result<Option<String>, AppError> {
        let entry = keyring::Entry::new(&self.service, key).map_err(|e| {
            AppError::SecureStorage(format!("Failed to initialize keyring entry: {}", e))
        })?;

        match entry.get_password() {
            Ok(pwd) => Ok(Some(pwd)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(AppError::SecureStorage(format!(
                "Failed to retrieve secret from OS keyring: {}",
                e
            ))),
        }
    }

    fn delete_secret(&self, key: &str) -> Result<(), AppError> {
        let entry = keyring::Entry::new(&self.service, key).map_err(|e| {
            AppError::SecureStorage(format!("Failed to initialize keyring entry: {}", e))
        })?;

        match entry.delete_credential() {
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(AppError::SecureStorage(format!(
                "Failed to delete secret from OS keyring: {}",
                e
            ))),
        }
    }

    fn has_secret(&self, key: &str) -> Result<bool, AppError> {
        let entry = keyring::Entry::new(&self.service, key).map_err(|e| {
            AppError::SecureStorage(format!("Failed to initialize keyring entry: {}", e))
        })?;

        match entry.get_password() {
            Ok(_) => Ok(true),
            Err(keyring::Error::NoEntry) => Ok(false),
            Err(e) => Err(AppError::SecureStorage(format!(
                "Failed to check secret status in OS keyring: {}",
                e
            ))),
        }
    }
}

#[derive(Default)]
pub struct InMemorySecretStore {
    store: RwLock<HashMap<String, String>>,
}

impl InMemorySecretStore {
    pub fn new() -> Self {
        Self {
            store: RwLock::new(HashMap::new()),
        }
    }
}

impl SecretStore for InMemorySecretStore {
    fn set_secret(&self, key: &str, value: &str) -> Result<(), AppError> {
        let mut guard = self.store.write().map_err(|e| {
            AppError::SecureStorage(format!("InMemorySecretStore lock poisoned: {}", e))
        })?;
        guard.insert(key.to_string(), value.to_string());
        Ok(())
    }

    fn get_secret(&self, key: &str) -> Result<Option<String>, AppError> {
        let guard = self.store.read().map_err(|e| {
            AppError::SecureStorage(format!("InMemorySecretStore lock poisoned: {}", e))
        })?;
        Ok(guard.get(key).cloned())
    }

    fn delete_secret(&self, key: &str) -> Result<(), AppError> {
        let mut guard = self.store.write().map_err(|e| {
            AppError::SecureStorage(format!("InMemorySecretStore lock poisoned: {}", e))
        })?;
        guard.remove(key);
        Ok(())
    }

    fn has_secret(&self, key: &str) -> Result<bool, AppError> {
        let guard = self.store.read().map_err(|e| {
            AppError::SecureStorage(format!("InMemorySecretStore lock poisoned: {}", e))
        })?;
        Ok(guard.contains_key(key))
    }
}
