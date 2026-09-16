use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum StarHistoryPreviewResult {
    Ready { svg_data: String, embed_url: String },
    None,
    Error { message: String },
}

impl StarHistoryPreviewResult {
    pub fn ready(svg_data: String, embed_url: String) -> Self {
        Self::Ready {
            svg_data,
            embed_url,
        }
    }

    pub fn none() -> Self {
        Self::None
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self::Error {
            message: message.into(),
        }
    }
}
