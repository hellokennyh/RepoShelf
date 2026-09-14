use serde::{Deserialize, Serialize};

use crate::github::GitHubRepository;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ArticleImportResult {
    pub source_url: String,
    pub repositories: Vec<GitHubRepository>,
    pub extracted_unique_count: usize,
    pub valid_count: usize,
    pub invalid_count: usize,
    pub skipped_count: usize,
    pub truncated: bool,
    pub warning: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ExtractedRepoCandidate {
    pub owner: String,
    pub repo: String,
    pub canonical_url: String,
}
