pub mod classification;
pub mod client;
pub mod enrichment;
pub mod evidence;
pub mod models;
pub mod rate_limit;
pub mod search;
pub mod url;

pub use classification::{classify_repository, ALL_CATEGORIES};
pub use client::{GitHubClient, GITHUB_API_VERSION, GITHUB_BASE_URL, GITHUB_REQUEST_TIMEOUT_SECS};
pub use enrichment::enrich_repositories;
pub use evidence::{
    AcquisitionCancelToken, AcquisitionOptions, EvidencePack, RepositoryEvidenceService,
};
pub use models::{
    GitHubConnectionStatus, GitHubRateLimitStatus, GitHubRepository, RankedGitHubRepository,
    RateLimitBucket, RepositorySearchRequest, RepositorySearchResponse, SearchPageResult,
};
pub use search::build_search_query;
pub use url::parse_github_repo_url;
