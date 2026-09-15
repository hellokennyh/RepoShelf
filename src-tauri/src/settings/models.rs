use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum TrendingPeriod {
    #[default]
    Daily,
    Weekly,
    Monthly,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AiProvider {
    OpenaiCompatible,
    Anthropic,
    Gemini,
    Ollama,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GitHubSettings {
    pub default_search_limit: u32,
    pub default_trending_period: TrendingPeriod,
}

impl Default for GitHubSettings {
    fn default() -> Self {
        Self {
            default_search_limit: 20,
            default_trending_period: TrendingPeriod::Daily,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct AiSettings {
    pub enabled: bool,
    pub provider: Option<AiProvider>,
    pub base_url: Option<String>,
    pub model: Option<String>,
}

use super::locale::UiLocale;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Settings {
    pub schema_version: u32,
    pub github: GitHubSettings,
    pub ai: AiSettings,
    #[serde(default)]
    pub ui_language: Option<UiLocale>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            schema_version: 1,
            github: GitHubSettings::default(),
            ai: AiSettings::default(),
            ui_language: None,
        }
    }
}

impl Settings {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != 1 {
            return Err(format!(
                "Unsupported schema_version: {}. Supported versions: 1",
                self.schema_version
            ));
        }

        if self.github.default_search_limit < 1 || self.github.default_search_limit > 100 {
            return Err(format!(
                "default_search_limit must be between 1 and 100, got {}",
                self.github.default_search_limit
            ));
        }

        // Clean empty base_url/model to None if blank
        // (Validation passes as optional strings)

        Ok(())
    }
}
