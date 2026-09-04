use serde::{Deserialize, Serialize};

/// Transient domain model for a GitHub repository.
/// NOT persisted to SQLite in Phase 2.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GitHubRepository {
    pub github_repo_id: i64,
    pub github_url: String,
    pub name: String,
    pub description: Option<String>,
    pub stars: u64,
    pub license: Option<String>,
    pub topics: Vec<String>,
    pub pushed_at: Option<String>,
    pub category: String,
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
    pub min_stars: Option<u32>,
    pub pushed_after: Option<String>,
    pub limit: u32,
}

/// Response returned by basic repository search.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RepositorySearchResponse {
    pub items: Vec<GitHubRepository>,
    pub total_count: u64,
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
            category,
        }
    }
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
