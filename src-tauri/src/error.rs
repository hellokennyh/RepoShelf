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

    #[error("Initialization error: {0}")]
    Initialization(String),

    #[error("Secure storage error: {0}")]
    SecureStorage(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("GitHub network error: {0}")]
    GitHubNetwork(String),

    #[error("GitHub timeout: {0}")]
    GitHubTimeout(String),

    #[error("GitHub authentication failed: {0}")]
    GitHubAuth(String),

    #[error("GitHub rate limit reached: {message}")]
    GitHubRateLimit {
        reset_at: Option<i64>,
        message: String,
    },

    #[error("GitHub repository not found: {0}")]
    GitHubNotFound(String),

    #[error("Invalid GitHub response: {0}")]
    GitHubInvalidResponse(String),

    #[error("Private repository unsupported: {0}")]
    PrivateRepositoryUnsupported(String),

    #[error("Invalid repository URL: {0}")]
    InvalidRepositoryUrl(String),

    #[error("Invalid search input: {0}")]
    InvalidSearchInput(String),

    #[error("Trending network error: {0}")]
    TrendingNetwork(String),

    #[error("Trending timeout: {0}")]
    TrendingTimeout(String),

    #[error("Trending HTTP error ({status}): {message}")]
    TrendingHttp { status: u16, message: String },

    #[error("Failed to parse trending HTML: {0}")]
    TrendingParse(String),

    #[error("Invalid trending input: {0}")]
    InvalidTrendingInput(String),

    #[error("Favorite already exists: {favorite_id}")]
    FavoriteAlreadyExists { favorite_id: i64 },

    #[error("Favorite not found: {0}")]
    FavoriteNotFound(String),

    #[error("Favorite identity conflict: {0}")]
    FavoriteIdentityConflict(String),

    #[error("Invalid favorite input: {0}")]
    InvalidFavoriteInput(String),

    #[error("Invalid category: {0}")]
    InvalidCategory(String),

    #[error("Invalid rating: {0}")]
    InvalidRating(String),

    #[error("Invalid library query: {0}")]
    InvalidLibraryQuery(String),

    #[error("AI authentication failed: {0}")]
    AIAuth(String),

    #[error("AI connection error: {0}")]
    AIConnection(String),

    #[error("AI model error: {0}")]
    AIModel(String),

    #[error("Invalid AI response: {0}")]
    AIInvalidResponse(String),

    #[error("Invalid article URL: {0}")]
    ArticleInvalidUrl(String),

    #[error("Unsupported URL scheme: {0}")]
    ArticleUnsupportedScheme(String),

    #[error("Blocked address: {0}")]
    ArticleBlockedAddress(String),

    #[error("Too many redirects: {0}")]
    ArticleTooManyRedirects(String),

    #[error("Article fetch timeout: {0}")]
    ArticleTimeout(String),

    #[error("Article response too large: {0}")]
    ArticleResponseTooLarge(String),

    #[error("Unsupported content type: {0}")]
    ArticleUnsupportedContentType(String),

    #[error("Article HTTP error ({status}): {message}")]
    ArticleHttp { status: u16, message: String },

    #[error("Failed to parse article HTML: {0}")]
    ArticleParse(String),

    #[error("No GitHub repositories found in article: {0}")]
    ArticleNoRepositories(String),

    #[error("Article security challenge or verification detected: {0}")]
    ArticleChallenge(String),

    #[error("Translation source conflict: {0}")]
    TranslationSourceConflict(String),

    #[error("Invalid analysis: {0}")]
    InvalidAnalysis(String),

    #[error("Evidence acquisition budget exceeded: {0}")]
    AcquisitionBudgetExceeded(String),

    #[error("Evidence acquisition cancelled: {0}")]
    AcquisitionCancelled(String),

    #[error("Evidence acquisition security violation: {0}")]
    AcquisitionSecurity(String),

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
            AppError::Initialization(_) => "Initialization",
            AppError::SecureStorage(_) => "SecureStorage",
            AppError::InvalidInput(_) => "InvalidInput",
            AppError::GitHubNetwork(_) => "GitHubNetwork",
            AppError::GitHubTimeout(_) => "GitHubTimeout",
            AppError::GitHubAuth(_) => "GitHubAuth",
            AppError::GitHubRateLimit { .. } => "GitHubRateLimit",
            AppError::GitHubNotFound(_) => "GitHubNotFound",
            AppError::GitHubInvalidResponse(_) => "GitHubInvalidResponse",
            AppError::PrivateRepositoryUnsupported(_) => "PrivateRepositoryUnsupported",
            AppError::InvalidRepositoryUrl(_) => "InvalidRepositoryUrl",
            AppError::InvalidSearchInput(_) => "InvalidSearchInput",
            AppError::TrendingNetwork(_) => "TrendingNetwork",
            AppError::TrendingTimeout(_) => "TrendingTimeout",
            AppError::TrendingHttp { .. } => "TrendingHttp",
            AppError::TrendingParse(_) => "TrendingParse",
            AppError::InvalidTrendingInput(_) => "InvalidTrendingInput",
            AppError::FavoriteAlreadyExists { .. } => "FavoriteAlreadyExists",
            AppError::FavoriteNotFound(_) => "FavoriteNotFound",
            AppError::FavoriteIdentityConflict(_) => "FavoriteIdentityConflict",
            AppError::InvalidFavoriteInput(_) => "InvalidFavoriteInput",
            AppError::InvalidCategory(_) => "InvalidCategory",
            AppError::InvalidRating(_) => "InvalidRating",
            AppError::InvalidLibraryQuery(_) => "InvalidLibraryQuery",
            AppError::AIAuth(_) => "AIAuth",
            AppError::AIConnection(_) => "AIConnection",
            AppError::AIModel(_) => "AIModel",
            AppError::AIInvalidResponse(_) => "AIInvalidResponse",
            AppError::ArticleInvalidUrl(_) => "InvalidUrl",
            AppError::ArticleUnsupportedScheme(_) => "UnsupportedScheme",
            AppError::ArticleBlockedAddress(_) => "BlockedAddress",
            AppError::ArticleTooManyRedirects(_) => "TooManyRedirects",
            AppError::ArticleTimeout(_) => "Timeout",
            AppError::ArticleResponseTooLarge(_) => "ResponseTooLarge",
            AppError::ArticleUnsupportedContentType(_) => "UnsupportedContentType",
            AppError::ArticleHttp { .. } => "HttpError",
            AppError::ArticleParse(_) => "HtmlParseError",
            AppError::ArticleNoRepositories(_) => "NoGitHubRepositories",
            AppError::ArticleChallenge(_) => "ChallengeDetected",
            AppError::TranslationSourceConflict(_) => "TranslationSourceConflict",
            AppError::InvalidAnalysis(_) => "InvalidAnalysis",
            AppError::AcquisitionBudgetExceeded(_) => "AcquisitionBudgetExceeded",
            AppError::AcquisitionCancelled(_) => "AcquisitionCancelled",
            AppError::AcquisitionSecurity(_) => "AcquisitionSecurity",
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
        let mut state = serializer.serialize_struct("AppError", 4)?;
        state.serialize_field("code", self.code())?;
        state.serialize_field("message", &self.to_string())?;
        if let AppError::GitHubRateLimit { reset_at, .. } = self {
            state.serialize_field("reset_at", reset_at)?;
        }
        if let AppError::TrendingHttp { status, .. } = self {
            state.serialize_field("status", status)?;
        }
        if let AppError::ArticleHttp { status, .. } = self {
            state.serialize_field("status", status)?;
        }
        if let AppError::FavoriteAlreadyExists { favorite_id } = self {
            state.serialize_field("favorite_id", favorite_id)?;
        }
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

impl From<url::ParseError> for AppError {
    fn from(err: url::ParseError) -> Self {
        AppError::InvalidRepositoryUrl(err.to_string())
    }
}

impl From<crate::runtime::RuntimeResolutionError> for AppError {
    fn from(err: crate::runtime::RuntimeResolutionError) -> Self {
        AppError::Initialization(err.to_string())
    }
}
