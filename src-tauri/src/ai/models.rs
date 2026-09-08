use serde::{Deserialize, Serialize};

/// Result returned by AI provider test connection command.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AiConnectionResult {
    pub provider: String,
    pub model: String,
    pub success: bool,
    pub message: String,
}
