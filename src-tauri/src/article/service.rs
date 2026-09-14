use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;

use crate::article::extractor::extract_github_candidates_from_html;
use crate::article::models::ArticleImportResult;
use crate::article::security::fetch_article_html;
use crate::article::validator::validate_candidates;
use crate::error::AppError;
use crate::github::GitHubClient;

pub async fn import_from_article(
    github: &Arc<GitHubClient>,
    token: Option<&str>,
    url: &str,
    mock_dns: Option<&HashMap<String, Vec<IpAddr>>>,
    allowed_hosts: Option<&[String]>,
) -> Result<ArticleImportResult, AppError> {
    let html = fetch_article_html(url, mock_dns, allowed_hosts).await?;
    let (candidates, truncated) = extract_github_candidates_from_html(&html, Some(url))?;
    let total_extracted = candidates.len();

    let outcome = validate_candidates(github, token, &candidates).await;

    let warning = if outcome.rate_limited {
        Some("GitHub API 达到速率限制，部分仓库已跳过验证".to_string())
    } else if truncated {
        Some("已仅处理前 50 个唯一 GitHub 仓库链接".to_string())
    } else {
        None
    };

    Ok(ArticleImportResult {
        source_url: url.to_string(),
        repositories: outcome.repositories,
        extracted_unique_count: total_extracted,
        valid_count: outcome.valid_count,
        invalid_count: outcome.invalid_count,
        skipped_count: outcome.skipped_count,
        truncated,
        warning,
    })
}
