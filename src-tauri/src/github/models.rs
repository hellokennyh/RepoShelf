use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Transient domain model for a GitHub repository.
/// NOT persisted to SQLite in Phase 2.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GitHubRepository {
    pub github_repo_id: i64,
    pub github_url: String,
    pub name: String,
    pub description: Option<String>,
    pub stars: u64,
    pub license: Option<String>,
    pub topics: Vec<String>,
    pub pushed_at: Option<String>,
    pub created_at: Option<String>,
    pub category: String,
    #[serde(default)]
    pub releases: Option<i64>,
    #[serde(default)]
    pub languages: Option<HashMap<String, f64>>,
}

/// Single rate limit resource bucket.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct RateLimitBucket {
    pub limit: u32,
    pub remaining: u32,
    pub used: u32,
    pub reset_at: String,
    pub resource: String,
}

/// GitHub rate limit status containing core and search buckets.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct GitHubRateLimitStatus {
    pub core: RateLimitBucket,
    pub search: RateLimitBucket,
}

/// GitHub connection test status response.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GitHubConnectionStatus {
    pub connected: bool,
    pub mode: String, // "Anonymous" | "Authenticated"
    pub rate_limit: GitHubRateLimitStatus,
}

/// Request parameters for basic repository search.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RepositorySearchRequest {
    pub query: String,
    pub min_stars: Option<i64>,
    pub pushed_after: Option<String>,
    pub limit: u32,
    #[serde(default)]
    pub exclude_selected: bool,
    #[serde(default)]
    pub exclude_favorites: bool,
}

/// Response returned by basic repository search.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RepositorySearchResponse {
    pub items: Vec<GitHubRepository>,
    pub total_count: u64,
}

/// Internal ranked repository item with its 1-based position in the raw GitHub API page.
#[derive(Debug, Clone, PartialEq)]
pub struct RankedGitHubRepository {
    pub repository: GitHubRepository,
    pub raw_position: usize,
}

/// Internal result of fetching a single search page from GitHub API.
/// Contains domain mapped items, ranked items with raw position, raw total_count, and raw item count before private-repo filtering.
#[derive(Debug, Clone, PartialEq)]
pub struct SearchPageResult {
    pub items: Vec<GitHubRepository>,
    pub ranked_items: Vec<RankedGitHubRepository>,
    pub total_count: u64,
    pub raw_item_count: usize,
}

// -----------------------------------------------------------------------------
// Internal GitHub REST API DTOs
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct GitHubApiLicense {
    pub spdx_id: Option<String>,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GitHubApiRepo {
    pub id: i64,
    pub html_url: String,
    pub name: String,
    pub description: Option<String>,
    #[serde(default)]
    pub stargazers_count: u64,
    pub license: Option<GitHubApiLicense>,
    #[serde(default)]
    pub topics: Vec<String>,
    pub pushed_at: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub private: bool,
}

impl GitHubApiRepo {
    /// Extracts canonical license string: prefers valid spdx_id, falls back to name, or None.
    pub fn extract_license(&self) -> Option<String> {
        if let Some(ref lic) = self.license {
            if let Some(ref spdx) = lic.spdx_id {
                let trimmed = spdx.trim();
                if !trimmed.is_empty() && trimmed != "NOASSERTION" {
                    return Some(trimmed.to_string());
                }
            }
            if let Some(ref name) = lic.name {
                let trimmed = name.trim();
                if !trimmed.is_empty() {
                    return Some(trimmed.to_string());
                }
            }
        }
        None
    }

    /// Converts raw API repo into domain model with supplied category suggestion.
    pub fn into_domain(self, category: String) -> GitHubRepository {
        let license = self.extract_license();
        GitHubRepository {
            github_repo_id: self.id,
            github_url: self.html_url,
            name: self.name,
            description: self.description,
            stars: self.stargazers_count,
            license,
            topics: self.topics,
            pushed_at: self.pushed_at,
            created_at: self.created_at,
            category,
            releases: None,
            languages: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct GitHubApiRelease {
    pub id: i64,
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub prerelease: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GitHubApiSearchResponse {
    pub total_count: u64,
    #[serde(default)]
    pub items: Vec<GitHubApiRepo>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GitHubApiRateLimitBucket {
    pub limit: u32,
    pub remaining: u32,
    pub reset: i64,
    #[serde(default)]
    pub used: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GitHubApiRateLimitResources {
    pub core: GitHubApiRateLimitBucket,
    pub search: GitHubApiRateLimitBucket,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GitHubApiRateLimitResponse {
    pub resources: GitHubApiRateLimitResources,
}
