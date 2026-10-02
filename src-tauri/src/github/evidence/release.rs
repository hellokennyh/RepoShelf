use serde::Deserialize;

use crate::error::AppError;
use crate::github::evidence::budget::{
    AcquisitionBudgetTracker, BUDGET_MAX_RECENT_RELEASES,
};
use crate::github::evidence::transport::AcquisitionTransport;

#[derive(Debug, Clone, Deserialize)]
pub struct GitHubApiReleaseItem {
    pub id: i64,
    pub tag_name: String,
    pub name: Option<String>,
    pub body: Option<String>,
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub prerelease: bool,
    pub published_at: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ReleaseEvidenceItem {
    pub evidence_id: String,
    pub release_id: i64,
    pub tag_name: String,
    pub name: Option<String>,
    pub published_at: Option<String>,
    pub bounded_body: Option<String>,
    pub prerelease: bool,
}

/// Fetches up to 5 recent public releases (excluding drafts).
/// Never downloads assets. Never modifies database.
pub async fn fetch_recent_releases(
    transport: &AcquisitionTransport,
    owner: &str,
    repo: &str,
    token: Option<&str>,
    budget: &AcquisitionBudgetTracker,
) -> Result<Vec<ReleaseEvidenceItem>, AppError> {
    let endpoint = format!("/repos/{}/{}/releases?per_page=5", owner, repo);

    let raw_releases: Result<Vec<GitHubApiReleaseItem>, AppError> = transport
        .get_bounded_json(&endpoint, token, budget)
        .await;

    let releases_list = match raw_releases {
        Ok(list) => list,
        Err(AppError::GitHubNotFound(_)) => {
            // Repositories with no releases or releases disabled may return 404
            return Ok(Vec::new());
        }
        Err(e) => return Err(e),
    };

    let mut result = Vec::new();

    for rel in releases_list {
        if rel.draft {
            continue;
        }

        if result.len() >= BUDGET_MAX_RECENT_RELEASES {
            break;
        }

        budget.record_release()?;

        // Bound release notes body to at most 2000 characters
        let bounded_body = rel.body.map(|b| {
            if b.chars().count() > 2000 {
                format!(
                    "{}... [truncated]",
                    b.chars().take(2000).collect::<String>()
                )
            } else {
                b
            }
        });

        result.push(ReleaseEvidenceItem {
            evidence_id: String::new(), // Populated during EvidencePack assembly
            release_id: rel.id,
            tag_name: rel.tag_name,
            name: rel.name,
            published_at: rel.published_at,
            bounded_body,
            prerelease: rel.prerelease,
        });
    }

    Ok(result)
}
