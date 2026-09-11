use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, sqlx::FromRow)]
pub struct FavoriteRecord {
    pub id: i64,
    pub github_repo_id: Option<i64>,
    pub github_url: String,
    pub name: String,
    pub description: Option<String>,
    pub category: String,
    pub stars: i64,
    pub license: Option<String>,
    pub pushed_at: Option<String>,
    pub rating: Option<i64>,
    pub rating_reason: Option<String>,
    pub rating_confidence: Option<String>,
    pub notes: String,
    pub favorited_at: String,
    pub metadata_refreshed_at: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NewFavoriteRecord {
    pub github_repo_id: Option<i64>,
    pub github_url: String,
    pub name: String,
    pub description: Option<String>,
    pub category: Option<String>,
    pub stars: Option<i64>,
    pub license: Option<String>,
    pub pushed_at: Option<String>,
    pub rating: Option<i64>,
    pub rating_reason: Option<String>,
    pub rating_confidence: Option<String>,
    pub notes: Option<String>,
    pub favorited_at: String,
    pub metadata_refreshed_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, sqlx::FromRow)]
pub struct FavoriteTagRecord {
    pub id: i64,
    pub favorite_id: i64,
    pub tag: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewFavoriteTagRecord {
    pub favorite_id: i64,
    pub tag: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, sqlx::FromRow)]
pub struct SelectionHistoryRecord {
    pub id: i64,
    pub github_repo_id: Option<i64>,
    pub github_url: String,
    pub selected_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewSelectionHistoryRecord {
    pub github_repo_id: Option<i64>,
    pub github_url: String,
    pub selected_at: String,
}
