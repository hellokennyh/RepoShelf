use serde::Deserialize;

use crate::error::AppError;
use crate::github::evidence::budget::{
    AcquisitionBudgetTracker, BUDGET_MODEL_DIRECTORY_MAX_PATHS,
};
use crate::github::evidence::transport::AcquisitionTransport;

#[derive(Debug, Clone, Deserialize)]
pub struct GitHubApiTreeItem {
    pub path: String,
    pub mode: String,
    #[serde(rename = "type")]
    pub entry_type: String,
    pub sha: String,
    pub size: Option<usize>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GitHubApiTreeResponse {
    pub sha: String,
    #[serde(default)]
    pub tree: Vec<GitHubApiTreeItem>,
    #[serde(default)]
    pub truncated: bool,
}

/// Normalized entry in a Git tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitTreeEntry {
    pub path: String,
    pub mode: String,
    pub entry_type: String,
    pub sha: String,
    pub size: Option<usize>,
}

impl GitTreeEntry {
    pub fn is_blob(&self) -> bool {
        self.entry_type == "blob"
    }

    pub fn is_tree(&self) -> bool {
        self.entry_type == "tree"
    }

    /// Git submodule entry: represented as mode 160000 or type "commit"
    pub fn is_submodule(&self) -> bool {
        self.mode == "160000" || self.entry_type == "commit"
    }

    /// Symbolic link entry: represented as mode 120000
    pub fn is_symlink(&self) -> bool {
        self.mode == "120000"
    }
}

/// Result of bounded tree and directory discovery.
#[derive(Debug, Clone)]
pub struct BoundedTreeResult {
    pub entries: Vec<GitTreeEntry>,
    pub candidate_paths: Vec<String>,
    pub total_tree_entries: usize,
    pub truncated: bool,
    pub submodules: Vec<String>,
    pub symlinks: Vec<String>,
}

/// Discovers repository tree entries pinned to a specific commit SHA within hard bounded limits.
pub async fn discover_bounded_tree(
    transport: &AcquisitionTransport,
    owner: &str,
    repo: &str,
    commit_sha: &str,
    token: Option<&str>,
    budget: &AcquisitionBudgetTracker,
) -> Result<BoundedTreeResult, AppError> {
    // Record directory request against the budget limit (max 8)
    budget.record_directory_request()?;

    let endpoint = format!(
        "/repos/{}/{}/git/trees/{}?recursive=1",
        owner, repo, commit_sha
    );

    let raw_tree: GitHubApiTreeResponse = transport
        .get_bounded_json(&endpoint, token, budget)
        .await?;

    let total_count = raw_tree.tree.len();
    let mut truncated = raw_tree.truncated;

    let mut entries = Vec::with_capacity(total_count);
    let mut candidate_paths = Vec::new();
    let mut submodules = Vec::new();
    let mut symlinks = Vec::new();

    for item in raw_tree.tree {
        let entry = GitTreeEntry {
            path: item.path.clone(),
            mode: item.mode,
            entry_type: item.entry_type,
            sha: item.sha,
            size: item.size,
        };

        if entry.is_submodule() {
            submodules.push(item.path.clone());
        }

        if entry.is_symlink() {
            symlinks.push(item.path.clone());
        }

        if entry.is_blob() && candidate_paths.len() < BUDGET_MODEL_DIRECTORY_MAX_PATHS {
            candidate_paths.push(item.path);
        }

        entries.push(entry);
    }

    if total_count > BUDGET_MODEL_DIRECTORY_MAX_PATHS {
        truncated = true;
    }

    Ok(BoundedTreeResult {
        entries,
        candidate_paths,
        total_tree_entries: total_count,
        truncated,
        submodules,
        symlinks,
    })
}
