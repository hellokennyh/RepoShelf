use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use crate::ai::provider::AiProviderClient;
use crate::ai::url::resolve_provider_endpoint;
use crate::error::AppError;
use crate::github::GitHubRepository;
use crate::secrets::{SecretStore, SECRET_KEY_AI_API_KEY};
use crate::settings::{AiProvider, Settings};

pub const AI_SEARCH_SYSTEM_INSTRUCTION: &str = "\
You generate GitHub repository search terms only.
Return JSON matching exactly this shape:
{\"queries\":[\"query 1\",\"query 2\"]}
Generate 1 to 3 concise GitHub repository search queries.
Do not return repository recommendations.
Do not claim repositories exist.
Do not analyze repositories.
Do not include explanations.
Do not include markdown except when unavoidable.
Do not include these GitHub qualifiers:
stars:
pushed:
sort:
order:
The application will add filters itself.
No tools.
No web access.
No GitHub API access by AI.";

pub const FORBIDDEN_QUALIFIERS: [&str; 4] = ["stars:", "pushed:", "sort:", "order:"];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiRepositorySearchRequest {
    pub input: String,
    #[serde(default)]
    pub min_stars: Option<i64>,
    #[serde(default)]
    pub pushed_after: Option<String>,
    pub limit: u32,
    #[serde(default)]
    pub exclude_selected: bool,
    #[serde(default)]
    pub exclude_favorites: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AiRepositorySearchResponse {
    pub items: Vec<GitHubRepository>,
    pub generated_queries: Vec<String>,
    pub partial: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AiSearchReadiness {
    pub ready: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawAiQueriesResponse {
    pub queries: Vec<String>,
}

/// Evaluates whether AI Natural-Language Search is ready given the current runtime configuration.
pub fn check_ai_search_readiness(
    settings: &Settings,
    secret_store: &dyn SecretStore,
) -> AiSearchReadiness {
    if !settings.ai.enabled {
        return AiSearchReadiness {
            ready: false,
            reason: Some(
                "AI 功能未启用，请在设置中开启 AI (AI is disabled in Settings)".to_string(),
            ),
        };
    }

    let provider = match settings.ai.provider {
        Some(p) => p,
        None => {
            return AiSearchReadiness {
                ready: false,
                reason: Some("未选择 AI Provider (No AI provider configured)".to_string()),
            }
        }
    };

    let model = match settings
        .ai
        .model
        .as_deref()
        .map(str::trim)
        .filter(|m| !m.is_empty())
    {
        Some(m) => m,
        None => {
            return AiSearchReadiness {
                ready: false,
                reason: Some("未配置 AI Model 名称 (No AI model configured)".to_string()),
            }
        }
    };

    if model.is_empty() {
        return AiSearchReadiness {
            ready: false,
            reason: Some("未配置 AI Model 名称 (No AI model configured)".to_string()),
        };
    }

    if let Err(e) = resolve_provider_endpoint(provider, settings.ai.base_url.as_deref()) {
        return AiSearchReadiness {
            ready: false,
            reason: Some(format!("AI Base URL 无效 (Invalid Base URL): {}", e)),
        };
    }

    if provider != AiProvider::Ollama {
        let has_key = match secret_store.get_secret(SECRET_KEY_AI_API_KEY) {
            Ok(Some(k)) => !k.trim().is_empty(),
            _ => false,
        };
        if !has_key {
            return AiSearchReadiness {
                ready: false,
                reason: Some("未配置 AI API Key (AI API Key not configured)".to_string()),
            };
        }
    }

    AiSearchReadiness {
        ready: true,
        reason: None,
    }
}

/// Validates input parameters for an AI repository search request.
pub fn validate_ai_search_input(req: &AiRepositorySearchRequest) -> Result<String, AppError> {
    let trimmed = req.input.trim();
    if trimmed.is_empty() {
        return Err(AppError::InvalidSearchInput(
            "Natural language search input cannot be empty".to_string(),
        ));
    }

    if trimmed.chars().count() > 2000 {
        return Err(AppError::InvalidSearchInput(format!(
            "Natural language search input exceeds 2000 characters limit (length: {})",
            trimmed.chars().count()
        )));
    }

    if req.limit < 1 || req.limit > 100 {
        return Err(AppError::InvalidSearchInput(format!(
            "Search limit must be between 1 and 100, got {}",
            req.limit
        )));
    }

    if let Some(stars) = req.min_stars {
        if stars < 0 {
            return Err(AppError::InvalidSearchInput(
                "Minimum stars cannot be negative".to_string(),
            ));
        }
    }

    if let Some(ref date_str) = req.pushed_after {
        let trimmed_date = date_str.trim();
        if !trimmed_date.is_empty() && NaiveDate::parse_from_str(trimmed_date, "%Y-%m-%d").is_err()
        {
            return Err(AppError::InvalidSearchInput(format!(
                "Invalid pushed_after date format '{}'. Expected YYYY-MM-DD",
                trimmed_date
            )));
        }
    }

    Ok(trimmed.to_string())
}

/// Strips outer markdown code block fence if the response is wrapped in one.
pub fn strip_outer_code_fence(s: &str) -> &str {
    let trimmed = s.trim();
    if !trimmed.starts_with("```") {
        return trimmed;
    }

    // Find the end of the opening line (e.g. ```json\n)
    let Some(first_line_end) = trimmed.find('\n') else {
        return trimmed;
    };

    let after_first_line = &trimmed[first_line_end + 1..];
    let trimmed_end = after_first_line.trim_end();

    if let Some(stripped) = trimmed_end.strip_suffix("```") {
        // Only strip if there are no other unclosed triple backticks inside
        stripped.trim()
    } else {
        trimmed
    }
}

/// Parses and validates the structured queries response from the AI provider.
pub fn parse_and_validate_ai_queries(raw: &str) -> Result<Vec<String>, String> {
    let cleaned = strip_outer_code_fence(raw);
    let parsed: RawAiQueriesResponse = serde_json::from_str(cleaned)
        .map_err(|e| format!("Failed to parse JSON response: {}", e))?;

    if parsed.queries.is_empty() {
        return Err("Queries list is empty".to_string());
    }

    let mut deduped = Vec::new();
    let mut seen_lower = HashSet::new();

    for raw_q in parsed.queries {
        let q = raw_q.trim();
        if q.is_empty() {
            return Err("Generated query cannot be empty".to_string());
        }

        if q.chars().count() > 200 {
            return Err(format!(
                "Generated query exceeds 200 characters limit ({} chars)",
                q.chars().count()
            ));
        }

        if q.contains('\n') || q.contains('\r') {
            return Err("Generated query must be a single line".to_string());
        }

        let lower = q.to_lowercase();
        for forbidden in &FORBIDDEN_QUALIFIERS {
            if lower.contains(forbidden) {
                return Err(format!(
                    "Generated query contains forbidden qualifier '{}': '{}'",
                    forbidden, q
                ));
            }
        }

        if seen_lower.insert(lower) {
            deduped.push(q.to_string());
        }
    }

    if deduped.is_empty() {
        return Err("At least 1 query must remain after deduplication".to_string());
    }

    if deduped.len() > 3 {
        return Err(format!(
            "At most 3 queries allowed after deduplication, got {}",
            deduped.len()
        ));
    }

    Ok(deduped)
}

/// Formats user request into an isolated prompt boundary with zero metadata leakage.
pub fn format_user_prompt(input: &str) -> String {
    format!("<user_request>\n{}\n</user_request>", input.trim())
}

/// Generates 1..=3 GitHub search queries using the configured AI provider, with up to one repair call.
pub async fn generate_search_queries(
    client: &dyn AiProviderClient,
    valid_input: &str,
) -> Result<Vec<String>, AppError> {
    let user_content = format_user_prompt(valid_input);
    let initial_response = client
        .generate_text(AI_SEARCH_SYSTEM_INSTRUCTION, &user_content)
        .await?;

    match parse_and_validate_ai_queries(&initial_response) {
        Ok(queries) => Ok(queries),
        Err(err_msg) => {
            eprintln!(
                "Initial AI query generation failed validation: {}. Attempting one repair call...",
                err_msg
            );

            let repair_user_input = format!(
                "Your previous response was invalid: {}.\nReturn ONLY a JSON object matching {{\"queries\": [\"query 1\", \"query 2\"]}} with 1 to 3 valid GitHub search queries for the user request:\n<user_request>\n{}\n</user_request>",
                err_msg,
                valid_input
            );

            let repair_response = client
                .generate_text(AI_SEARCH_SYSTEM_INSTRUCTION, &repair_user_input)
                .await?;

            parse_and_validate_ai_queries(&repair_response).map_err(|e| {
                AppError::AIInvalidResponse(format!(
                    "AI generated invalid search queries after repair attempt: {}",
                    e
                ))
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_outer_code_fence() {
        let raw1 = "```json\n{\"queries\":[\"a\",\"b\"]}\n```";
        assert_eq!(strip_outer_code_fence(raw1), "{\"queries\":[\"a\",\"b\"]}");

        let raw2 = "```\n{\"queries\":[\"a\"]}\n```";
        assert_eq!(strip_outer_code_fence(raw2), "{\"queries\":[\"a\"]}");

        let raw3 = "{\"queries\":[\"a\"]}";
        assert_eq!(strip_outer_code_fence(raw3), "{\"queries\":[\"a\"]}");

        let raw4 = "  ```json\r\n{\"queries\":[\"a\"]}\r\n```  ";
        assert_eq!(strip_outer_code_fence(raw4), "{\"queries\":[\"a\"]}");
    }

    #[test]
    fn test_parse_and_validate_clean_json() {
        let raw = r#"{"queries":["instagram scraper","instagram api"]}"#;
        let queries = parse_and_validate_ai_queries(raw).unwrap();
        assert_eq!(queries, vec!["instagram scraper", "instagram api"]);
    }

    #[test]
    fn test_case_insensitive_dedup_preserves_first() {
        let raw = r#"{"queries":["Instagram Scraper","instagram scraper","INSTAGRAM API"]}"#;
        let queries = parse_and_validate_ai_queries(raw).unwrap();
        assert_eq!(queries, vec!["Instagram Scraper", "INSTAGRAM API"]);
    }

    #[test]
    fn test_rejects_forbidden_qualifiers() {
        let raw1 = r#"{"queries":["instagram scraper stars:>=100"]}"#;
        assert!(parse_and_validate_ai_queries(raw1).is_err());

        let raw2 = r#"{"queries":["scraper pushed:>2026-01-01"]}"#;
        assert!(parse_and_validate_ai_queries(raw2).is_err());

        let raw3 = r#"{"queries":["scraper sort:stars"]}"#;
        assert!(parse_and_validate_ai_queries(raw3).is_err());

        let raw4 = r#"{"queries":["scraper order:desc"]}"#;
        assert!(parse_and_validate_ai_queries(raw4).is_err());
    }

    #[test]
    fn test_rejects_empty_or_more_than_three_queries() {
        let raw_empty = r#"{"queries":[]}"#;
        assert!(parse_and_validate_ai_queries(raw_empty).is_err());

        let raw_four = r#"{"queries":["q1","q2","q3","q4"]}"#;
        assert!(parse_and_validate_ai_queries(raw_four).is_err());
    }

    #[test]
    fn test_rejects_multiline_or_too_long() {
        let raw_multiline = "{\"queries\":[\"line1\\nline2\"]}";
        assert!(parse_and_validate_ai_queries(raw_multiline).is_err());

        let long_q = "a".repeat(201);
        let raw_long = format!("{{\"queries\":[\"{}\"]}}", long_q);
        assert!(parse_and_validate_ai_queries(&raw_long).is_err());
    }
}
