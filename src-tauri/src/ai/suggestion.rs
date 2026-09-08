use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use crate::ai::provider::AiProviderClient;
use crate::ai::search::strip_outer_code_fence;
use crate::error::AppError;
use crate::library::repository::validate_category;

pub const AI_METADATA_SYSTEM_INSTRUCTION: &str = "\
You suggest a category and short tags for a GitHub repository.

Use ONLY the repository name, description, and GitHub topics supplied by the application.

Return JSON only with this exact logical shape:

{\"category\":\"Automation\",\"tags\":[\"scraping\",\"instagram\",\"automation\"]}

Category MUST be exactly one of:

AI & Agents
Developer Tools
Web & SaaS
Data & Database
DevOps & Cloud
Security & Privacy
Networking
Multimedia
Automation
Productivity
Design & UI
Mobile
Games
System Utilities
Libraries & Frameworks
Other

Return 0 to 5 tags.

Do not explain the result.

Do not analyze README, source code, Issues, or Pull Requests.

Do not claim access to data not provided.

Do not return confidence scores.

Do not return reasoning.";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AiFavoriteMetadataSuggestion {
    pub category: String,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RawAiSuggestionResponse {
    pub category: String,
    pub tags: Vec<String>,
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Formats repository name, description, and GitHub topics into a strict, safe boundary.
/// Guarantees that private user metadata (notes, rating, user tags, old AI tags, URLs, IDs) is excluded.
pub fn format_suggestion_user_prompt(
    name: &str,
    description: Option<&str>,
    github_topics: &[String],
) -> String {
    let safe_name = escape_xml(name.trim());
    let safe_desc = escape_xml(description.unwrap_or("").trim());
    let mut topics_xml = String::new();
    for topic in github_topics {
        let trimmed = topic.trim();
        if !trimmed.is_empty() {
            topics_xml.push_str(&format!("  <topic>{}</topic>\n", escape_xml(trimmed)));
        }
    }

    format!(
        "<repository>\n<name>{}</name>\n<description>{}</description>\n<github_topics>\n{}</github_topics>\n</repository>",
        safe_name, safe_desc, topics_xml
    )
}

/// Parses and validates the structured category and tags suggestion from the AI provider.
pub fn parse_and_validate_suggestion(raw: &str) -> Result<AiFavoriteMetadataSuggestion, String> {
    let cleaned = strip_outer_code_fence(raw);
    let parsed: RawAiSuggestionResponse = serde_json::from_str(cleaned)
        .map_err(|e| format!("Failed to parse JSON response: {}", e))?;

    // 1. Category validation
    let valid_category = validate_category(&parsed.category)
        .map_err(|e| format!("Invalid category '{}': {}", parsed.category, e))?;

    // 2. Tags validation
    if parsed.tags.len() > 5 {
        return Err(format!("At most 5 tags allowed, got {}", parsed.tags.len()));
    }

    let mut deduped = Vec::new();
    let mut seen_lower = HashSet::new();

    for raw_t in parsed.tags {
        let t = raw_t.trim();
        if t.is_empty() {
            return Err("Suggested tag cannot be empty".to_string());
        }

        if t.chars().count() > 40 {
            return Err(format!(
                "Suggested tag exceeds 40 characters limit ({} chars)",
                t.chars().count()
            ));
        }

        if t.contains('\n') || t.contains('\r') {
            return Err("Suggested tag must be single-line".to_string());
        }

        let lower = t.to_lowercase();
        if seen_lower.insert(lower) {
            deduped.push(t.to_string());
        }
    }

    if deduped.len() > 5 {
        return Err(format!(
            "At most 5 tags allowed after deduplication, got {}",
            deduped.len()
        ));
    }

    Ok(AiFavoriteMetadataSuggestion {
        category: valid_category.to_string(),
        tags: deduped,
    })
}

/// Generates a Category and Tag suggestion using the configured AI provider, with up to one repair call.
pub async fn generate_favorite_suggestion(
    client: &dyn AiProviderClient,
    name: &str,
    description: Option<&str>,
    github_topics: &[String],
) -> Result<AiFavoriteMetadataSuggestion, AppError> {
    let user_content = format_suggestion_user_prompt(name, description, github_topics);
    let initial_response = client
        .generate_text(AI_METADATA_SYSTEM_INSTRUCTION, &user_content)
        .await?;

    match parse_and_validate_suggestion(&initial_response) {
        Ok(suggestion) => Ok(suggestion),
        Err(err_msg) => {
            eprintln!(
                "Initial AI suggestion failed validation: {}. Attempting one repair call...",
                err_msg
            );

            let repair_user_input = format!(
                "Your previous response was invalid: {}.\nReturn only corrected JSON matching the required category/tags schema.\nNo explanation.\n{}",
                err_msg, user_content
            );

            let repair_response = client
                .generate_text(AI_METADATA_SYSTEM_INSTRUCTION, &repair_user_input)
                .await?;

            parse_and_validate_suggestion(&repair_response).map_err(|e| {
                AppError::AIInvalidResponse(format!(
                    "AI metadata suggestion response invalid after repair: {}",
                    e
                ))
            })
        }
    }
}
