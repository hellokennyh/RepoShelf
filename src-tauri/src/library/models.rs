use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FavoriteInput {
    pub github_repo_id: Option<i64>,
    pub github_url: String,
    pub name: String,
    pub description: Option<String>,
    pub category: String,
    pub stars: i64,
    pub license: Option<String>,
    #[serde(default)]
    pub topics: Vec<String>,
    pub pushed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FavoriteTagItem {
    pub id: i64,
    pub favorite_id: i64,
    pub tag: String,
    pub source: String, // "github" | "ai" | "user"
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FavoriteItem {
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
    pub tags: Vec<FavoriteTagItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FavoriteStatus {
    pub is_favorited: bool,
    pub favorite_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BatchFavoriteItemResult {
    pub github_url: String,
    pub status: String, // "succeeded" | "already_exists" | "failed"
    pub favorite_id: Option<i64>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BatchFavoriteResult {
    pub succeeded: usize,
    pub already_exists: usize,
    pub failed: usize,
    pub items: Vec<BatchFavoriteItemResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BatchFavoriteIdentitiesResult {
    pub matched_repo_ids: Vec<i64>,
    pub matched_urls: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpdateFavoriteUserMetadataInput {
    pub favorite_id: i64,
    pub category: String,
    pub rating: Option<i64>,
    pub rating_reason: Option<String>,
    pub rating_confidence: Option<String>,
    pub notes: String,
    #[serde(default)]
    pub user_tags: Vec<String>,
    #[serde(default)]
    pub ai_tags: Option<Vec<String>>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LibrarySortField {
    FavoritedAt,
    Stars,
    Rating,
    PushedAt,
    Name,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SortOrder {
    Asc,
    Desc,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct LibraryQuery {
    pub search: Option<String>,
    pub category: Option<String>,
    pub tag: Option<String>,
    pub rating: Option<String>, // "all" | "unrated" | "1" | "2" | "3" | "4" | "5"
    pub sort_field: Option<LibrarySortField>,
    pub sort_order: Option<SortOrder>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RefreshResult {
    pub favorite_id: i64,
    pub status: String, // "succeeded" | "not_found" | "rate_limited" | "conflict" | "failed"
    pub message: Option<String>,
    pub favorite: Option<FavoriteItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BatchRefreshResult {
    pub total: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub rate_limited: usize,
    pub not_found: usize,
    pub results: Vec<RefreshResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BatchRefreshProgress {
    pub completed: u32,
    pub total: u32,
}
