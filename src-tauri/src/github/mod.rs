pub mod classification;
pub mod client;
pub mod models;
pub mod rate_limit;
pub mod search;
pub mod url;

pub use classification::{classify_repository, ALL_CATEGORIES};
pub use client::{GitHubClient, GITHUB_API_VERSION, GITHUB_BASE_URL, GITHUB_REQUEST_TIMEOUT_SECS};
pub use models::{
    GitHubConnectionStatus, GitHubRateLimitStatus, GitHubRepository, RateLimitBucket,
    RepositorySearchRequest, RepositorySearchResponse,
};
pub use url::parse_github_repo_url;
