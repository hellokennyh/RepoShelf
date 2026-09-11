use serde::{Deserialize, Serialize};

/// Result returned by AI provider test connection command.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AiConnectionResult {
    pub provider: String,
    pub model: String,
    pub success: bool,
    pub message: String,
}

/// Metadata for an installed Ollama model.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OllamaModelInfo {
    pub name: String,
    pub model: String,
    pub size: Option<u64>,
    pub modified_at: Option<String>,
}

/// Information about a detected local Ollama endpoint.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OllamaEndpointDiscovery {
    pub base_url: String,
    pub models: Vec<OllamaModelInfo>,
}

