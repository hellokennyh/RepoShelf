use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use crate::ai::provider::{AiProviderClient, GenerateTextOptions};
use crate::ai::search::strip_outer_code_fence;
use crate::error::AppError;
use crate::library::repository::validate_category;

pub const SUPPORTED_SUGGESTION_LOCALES: [&str; 3] = ["en", "zh-Hans", "zh-Hant"];

pub fn build_suggestion_system_instruction(target_locale: &str) -> String {
    let (lang_instruction, example_reason) = match target_locale {
        "zh-Hant" => (
            "Write the rating rationale (rating_reason) in Traditional Chinese (繁體中文). You may keep repository names, programming languages, API names, code identifiers, technical abbreviations, and product terms in their original form.",
            "專案定位明確，社群關注度較高，近期仍有維護。"
        ),
        "en" => (
            "Write the rating rationale (rating_reason) in English. You may keep repository names, programming languages, API names, code identifiers, technical abbreviations, and product terms in their original form.",
            "Clear project positioning, strong community interest, and active recent maintenance."
        ),
        _ => (
            "Write the rating rationale (rating_reason) in Simplified Chinese (简体中文). You may keep repository names, programming languages, API names, code identifiers, technical abbreviations, and product terms in their original form.",
            "项目定位明确，社区关注度较高，近期仍有维护。"
        ),
    };

    format!(
r#"You suggest a category, tags, an overall project reference rating, a rating reason, and confidence for a GitHub repository.

Use ONLY the repository name, description, stars, license, latest activity, and GitHub topics supplied by the application.
Judge ONLY from provided metadata. Do not claim verified code quality, verified security, or verified test quality.

Return raw JSON only with no markdown formatting, no code fences, no prose before or after JSON:
{{
  "category": "AI & Agents",
  "tags": ["tag1", "tag2"],
  "rating": 4,
  "rating_reason": "{}",
  "confidence": "high"
}}

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

Tags:
Return 0 to 5 concise tags.

Rating:
Integer from 1 to 5 representing overall reference/value rating:
1 = low value / immature / weak evidence
2 = below average
3 = useful or promising but mixed
4 = strong project worth serious attention
5 = exceptional based on available evidence

Rubric:
Practical usefulness / project value: 25%
Maintenance / recent activity: 20%
Maturity / completeness: 20%
Documentation / usability evidence: 15%
Community adoption / recognition: 10%
Innovation / differentiation: 10%

Rating reason:
1 to 3 concise sentences explaining the evidence behind the rating.
{}

Confidence:
MUST be exactly one of: low, medium, high.

Do not explain outside the JSON.
Do not return reasoning."#,
        example_reason, lang_instruction
    )
}

pub const AI_METADATA_SYSTEM_INSTRUCTION: &str = "\
You suggest a category, tags, an overall project reference rating, a rating reason, and confidence for a GitHub repository.

Use ONLY the repository name, description, stars, license, latest activity, and GitHub topics supplied by the application.
Judge ONLY from provided metadata. Do not claim verified code quality, verified security, or verified test quality.

Return raw JSON only with no markdown formatting, no code fences, no prose before or after JSON:
{
  \"category\": \"AI & Agents\",
  \"tags\": [\"tag1\", \"tag2\"],
  \"rating\": 4,
  \"rating_reason\": \"项目定位明确，社区关注度较高，近期仍有维护。\",
  \"confidence\": \"high\"
}

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

Tags:
Return 0 to 5 concise tags.

Rating:
Integer from 1 to 5 representing overall reference/value rating:
1 = low value / immature / weak evidence
2 = below average
3 = useful or promising but mixed
4 = strong project worth serious attention
5 = exceptional based on available evidence

Rubric:
Practical usefulness / project value: 25%
Maintenance / recent activity: 20%
Maturity / completeness: 20%
Documentation / usability evidence: 15%
Community adoption / recognition: 10%
Innovation / differentiation: 10%

Rating reason:
1 to 3 concise sentences explaining the evidence behind the rating.

Confidence:
MUST be exactly one of: low, medium, high.

Do not explain outside the JSON.
Do not return reasoning.";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AiFavoriteMetadataSuggestion {
    pub category: String,
    pub tags: Vec<String>,
    pub rating: i64,
    pub rating_reason: String,
    pub confidence: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RawAiSuggestionResponse {
    pub category: String,
    pub tags: Vec<String>,
    pub rating: serde_json::Value,
    pub rating_reason: String,
    pub confidence: String,
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Validates that the natural language in rating_reason basically matches target_locale.
/// Allows repository names, programming languages, API names, code identifiers, technical abbreviations, and product terms.
pub fn validate_rating_reason_language(reason: &str, target_locale: &str) -> Result<(), String> {
    let trimmed = reason.trim();
    if trimmed.is_empty() {
        return Err("Rating reason cannot be empty".to_string());
    }

    let cjk_count = trimmed
        .chars()
        .filter(|c| ('\u{4e00}'..='\u{9fff}').contains(c))
        .count();
    let alpha_count = trimmed.chars().filter(|c| c.is_ascii_alphabetic()).count();

    match target_locale {
        "zh-Hans" | "zh-Hant" => {
            // In Chinese, if there are multiple English words (alpha_count > 15) and 0 CJK characters,
            // it is purely English text, failing the target Chinese requirement.
            if cjk_count == 0 && alpha_count > 15 {
                return Err(format!(
                    "Rating reason was not generated in the requested locale '{}' (contains no Chinese characters)",
                    target_locale
                ));
            }
        }
        "en" => {
            // In English, if there are CJK characters and they are substantial (>= 5, or more CJK than English letters),
            // it is Chinese text failing the English requirement.
            if cjk_count >= 5 || (cjk_count > 0 && cjk_count > alpha_count) {
                return Err("Rating reason was not generated in English (contains Chinese text)".to_string());
            }
        }
        _ => {}
    }

    Ok(())
}

/// Formats repository name, description, stars, license, latest activity, and GitHub topics
/// into a strict, safe boundary.
/// Guarantees that private user metadata (notes, rating, user tags, old AI tags, URLs, IDs) is excluded.
pub fn format_suggestion_user_prompt(
    name: &str,
    description: Option<&str>,
    stars: i64,
    license: Option<&str>,
    pushed_at: Option<&str>,
    github_topics: &[String],
) -> String {
    format_suggestion_user_prompt_with_locale(name, description, stars, license, pushed_at, github_topics, None)
}

pub fn format_suggestion_user_prompt_with_locale(
    name: &str,
    description: Option<&str>,
    stars: i64,
    license: Option<&str>,
    pushed_at: Option<&str>,
    github_topics: &[String],
    target_locale: Option<&str>,
) -> String {
    let safe_name = escape_xml(name.trim());
    let safe_desc = escape_xml(description.unwrap_or("").trim());
    let safe_license = escape_xml(license.unwrap_or("Unknown").trim());
    let safe_pushed = escape_xml(pushed_at.unwrap_or("").trim());
    let mut topics_xml = String::new();
    for topic in github_topics {
        let trimmed = topic.trim();
        if !trimmed.is_empty() {
            topics_xml.push_str(&format!("  <topic>{}</topic>\n", escape_xml(trimmed)));
        }
    }

    let locale_tag = if let Some(loc) = target_locale {
        format!("<target_locale>{}</target_locale>\n", escape_xml(loc))
    } else {
        String::new()
    };

    format!(
        "<repository>\n<name>{}</name>\n<description>{}</description>\n<stars>{}</stars>\n<license>{}</license>\n<latest_activity>{}</latest_activity>\n{}<github_topics>\n{}</github_topics>\n</repository>",
        safe_name, safe_desc, stars, safe_license, safe_pushed, locale_tag, topics_xml
    )
}

/// Parses and validates the structured category, tags, rating, rating_reason, and confidence
/// suggestion from the AI provider.
pub fn parse_and_validate_suggestion(raw: &str) -> Result<AiFavoriteMetadataSuggestion, String> {
    parse_and_validate_suggestion_with_locale(raw, None)
}

/// Parses and validates with explicit locale enforcement on rating_reason.
pub fn parse_and_validate_suggestion_with_locale(
    raw: &str,
    target_locale: Option<&str>,
) -> Result<AiFavoriteMetadataSuggestion, String> {
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

    // 3. Rating validation: must be integer 1..=5
    let rating_num = match &parsed.rating {
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                if (1..=5).contains(&i) {
                    i
                } else {
                    return Err(format!(
                        "Rating must be an integer between 1 and 5, got {}",
                        i
                    ));
                }
            } else {
                return Err(
                    "Rating must be an integer between 1 and 5, decimal ratings are not allowed"
                        .to_string(),
                );
            }
        }
        _ => return Err("Rating must be an integer between 1 and 5".to_string()),
    };

    // 4. Rating reason validation: 1-3 concise sentences, non-empty
    let reason = parsed.rating_reason.trim();
    if reason.is_empty() {
        return Err("Rating reason cannot be empty".to_string());
    }
    if let Some(loc) = target_locale {
        validate_rating_reason_language(reason, loc)?;
    }

    // 5. Confidence validation: must be low, medium, or high
    let conf = parsed.confidence.trim().to_lowercase();
    if conf != "low" && conf != "medium" && conf != "high" {
        return Err(format!(
            "Confidence must be 'low', 'medium', or 'high', got '{}'",
            parsed.confidence
        ));
    }

    Ok(AiFavoriteMetadataSuggestion {
        category: valid_category.to_string(),
        tags: deduped,
        rating: rating_num,
        rating_reason: reason.to_string(),
        confidence: conf,
    })
}

/// Generates a Category, Tag, Rating, Rating reason, and Confidence suggestion
/// using the configured AI provider, with task-specific think:false and bounded tokens,
/// with up to one repair call.
pub async fn generate_favorite_suggestion(
    client: &dyn AiProviderClient,
    name: &str,
    description: Option<&str>,
    stars: i64,
    license: Option<&str>,
    pushed_at: Option<&str>,
    github_topics: &[String],
    target_locale: &str,
) -> Result<AiFavoriteMetadataSuggestion, AppError> {
    let trimmed_locale = target_locale.trim();
    if !SUPPORTED_SUGGESTION_LOCALES.contains(&trimmed_locale) {
        return Err(AppError::SettingsValidation(format!(
            "Unsupported target_locale '{}'. Supported locales are: en, zh-Hans, zh-Hant",
            target_locale
        )));
    }

    let options = GenerateTextOptions {
        max_tokens: Some(512),
        temperature: Some(0.2),
        think: Some(false),
        ..Default::default()
    };

    let system_instruction = build_suggestion_system_instruction(trimmed_locale);
    let user_content = format_suggestion_user_prompt_with_locale(
        name,
        description,
        stars,
        license,
        pushed_at,
        github_topics,
        Some(trimmed_locale),
    );

    let initial_response = client
        .generate_text_with_options(
            &system_instruction,
            &user_content,
            options.clone(),
        )
        .await?;

    match parse_and_validate_suggestion_with_locale(&initial_response, Some(trimmed_locale)) {
        Ok(suggestion) => Ok(suggestion),
        Err(err_msg) => {
            eprintln!(
                "Initial AI suggestion failed validation: {}. Attempting one repair call...",
                err_msg
            );

            let repair_user_input = format!(
                "Your previous response was invalid: {}.\nReturn ONLY corrected raw JSON matching the required schema with category, tags, rating (1-5 integer), rating_reason (must be written in {}), and confidence (low/medium/high).\nNo explanation.\n{}",
                err_msg, trimmed_locale, user_content
            );

            let repair_response = client
                .generate_text_with_options(
                    &system_instruction,
                    &repair_user_input,
                    options,
                )
                .await?;

            parse_and_validate_suggestion_with_locale(&repair_response, Some(trimmed_locale))
                .map_err(|e| {
                    AppError::AIInvalidResponse(format!(
                        "AI metadata suggestion response invalid after repair: {}",
                        e
                    ))
                })
        }
    }
}
