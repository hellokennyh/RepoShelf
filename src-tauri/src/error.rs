use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("Database error: {0}")]
    Database(String),

    #[error("Database migration error: {0}")]
    DatabaseMigration(String),

    #[error("Failed to read settings: {0}")]
    SettingsRead(String),

    #[error("Failed to write settings: {0}")]
    SettingsWrite(String),

    #[error("Settings validation error: {0}")]
    SettingsValidation(String),

    #[error("Secure storage error: {0}")]
    SecureStorage(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Unknown error: {0}")]
    Unknown(String),
}

impl AppError {
    pub fn code(&self) -> &'static str {
        match self {
            AppError::Database(_) => "Database",
            AppError::DatabaseMigration(_) => "DatabaseMigration",
            AppError::SettingsRead(_) => "SettingsRead",
            AppError::SettingsWrite(_) => "SettingsWrite",
            AppError::SettingsValidation(_) => "SettingsValidation",
            AppError::SecureStorage(_) => "SecureStorage",
            AppError::InvalidInput(_) => "InvalidInput",
            AppError::Unknown(_) => "Unknown",
        }
    }
}

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("AppError", 2)?;
        state.serialize_field("code", self.code())?;
        state.serialize_field("message", &self.to_string())?;
        state.end()
    }
}

impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        AppError::Database(err.to_string())
    }
}

impl From<sqlx::migrate::MigrateError> for AppError {
    fn from(err: sqlx::migrate::MigrateError) -> Self {
        AppError::DatabaseMigration(err.to_string())
    }
}
