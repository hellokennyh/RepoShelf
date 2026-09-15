use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt::Write;

use crate::error::AppError;

pub const VALID_LOCALES: &[&str] = &["en", "zh-Hans", "zh-Hant"];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DescriptionTranslation {
    pub text: String,
    pub target_locale: String,
}

impl DescriptionTranslation {
    pub fn new(text: String, target_locale: String) -> Result<Self, AppError> {
        let trimmed_text = text.trim();
        if trimmed_text.is_empty() {
            return Err(AppError::InvalidInput(
                "Translation text cannot be empty".to_string(),
            ));
        }

        if !VALID_LOCALES.contains(&target_locale.as_str()) {
            return Err(AppError::InvalidInput(format!(
                "Invalid translation target_locale '{}', expected one of: en, zh-Hans, zh-Hant",
                target_locale
            )));
        }

        Ok(Self {
            text,
            target_locale,
        })
    }

    pub fn validate(&self) -> Result<(), AppError> {
        if self.text.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "Translation text cannot be empty".to_string(),
            ));
        }
        if !VALID_LOCALES.contains(&self.target_locale.as_str()) {
            return Err(AppError::InvalidInput(format!(
                "Invalid translation target_locale '{}', expected one of: en, zh-Hans, zh-Hant",
                self.target_locale
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TranslationStatus {
    Valid,
    Stale,
    Corrupt,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TranslationPatch {
    Keep,
    Replace {
        translation: DescriptionTranslation,
        source_hash: String,
    },
    Clear,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum TranslationPatchInput {
    Keep,
    Replace {
        text: String,
        target_locale: String,
        source_text: String,
    },
    Clear,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DescriptionTranslationInput {
    pub text: String,
    pub target_locale: String,
}

/// Computes the exact SHA-256 fingerprint for a non-null description string.
/// Encodes raw Unicode string to UTF-8 without trimming, normalization, or case alterations.
/// Returns exactly 64 lowercase hexadecimal characters.
pub fn compute_source_hash(description: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(description.as_bytes());
    let hash = hasher.finalize();
    let mut hex_str = String::with_capacity(64);
    for b in hash {
        let _ = write!(hex_str, "{:02x}", b);
    }
    hex_str
}

/// Authoritative parsing of database translation columns.
/// Guarantees that malformed DB records do not crash the application,
/// and detects corrupt, stale, and valid states according to the frozen contract.
pub fn parse_db_translation(
    translations_json: Option<&str>,
    source_hash: Option<&str>,
    current_description: Option<&str>,
) -> (
    Option<DescriptionTranslation>,
    Option<String>,
    Option<TranslationStatus>,
    Option<String>,
) {
    match (translations_json, source_hash) {
        (None, None) => (None, None, None, None),
        (Some(_), None) => (
            None,
            None,
            Some(TranslationStatus::Corrupt),
            Some("Stored translation is missing its source hash pair".to_string()),
        ),
        (None, Some(h)) => (
            None,
            Some(h.to_string()),
            Some(TranslationStatus::Corrupt),
            Some("Stored source hash is missing its translation pair".to_string()),
        ),
        (Some(json_raw), Some(hash_raw)) => {
            // Guard against empty string or literal "null"
            let trimmed_json = json_raw.trim();
            if trimmed_json.is_empty() || trimmed_json == "null" {
                return (
                    None,
                    Some(hash_raw.to_string()),
                    Some(TranslationStatus::Corrupt),
                    Some("Malformed empty or null translation data in database".to_string()),
                );
            }

            let trimmed_hash = hash_raw.trim();
            if trimmed_hash.len() != 64 || !trimmed_hash.chars().all(|c| c.is_ascii_hexdigit()) {
                return (
                    None,
                    Some(hash_raw.to_string()),
                    Some(TranslationStatus::Corrupt),
                    Some(format!(
                        "Invalid source hash format in database: '{}'",
                        hash_raw
                    )),
                );
            }

            // Parse JSON and enforce that it is a single JSON Object (not array, string, etc.)
            let parsed_value: Result<serde_json::Value, _> = serde_json::from_str(trimmed_json);
            let val = match parsed_value {
                Ok(v) => v,
                Err(e) => {
                    return (
                        None,
                        Some(hash_raw.to_string()),
                        Some(TranslationStatus::Corrupt),
                        Some(format!("Invalid translation JSON: {}", e)),
                    );
                }
            };

            let obj = match val.as_object() {
                Some(o) => o,
                None => {
                    return (
                        None,
                        Some(hash_raw.to_string()),
                        Some(TranslationStatus::Corrupt),
                        Some("Translation JSON must be a single JSON object".to_string()),
                    );
                }
            };

            // Extract and validate text
            let text_val = match obj.get("text") {
                Some(serde_json::Value::String(s)) => s,
                _ => {
                    return (
                        None,
                        Some(hash_raw.to_string()),
                        Some(TranslationStatus::Corrupt),
                        Some("Translation object missing valid 'text' string field".to_string()),
                    );
                }
            };

            if text_val.trim().is_empty() {
                return (
                    None,
                    Some(hash_raw.to_string()),
                    Some(TranslationStatus::Corrupt),
                    Some("Translation 'text' cannot be empty".to_string()),
                );
            }

            // Extract and validate target_locale
            let locale_val = match obj.get("target_locale") {
                Some(serde_json::Value::String(s)) => s,
                _ => {
                    return (
                        None,
                        Some(hash_raw.to_string()),
                        Some(TranslationStatus::Corrupt),
                        Some(
                            "Translation object missing valid 'target_locale' string field"
                                .to_string(),
                        ),
                    );
                }
            };

            if !VALID_LOCALES.contains(&locale_val.as_str()) {
                return (
                    None,
                    Some(hash_raw.to_string()),
                    Some(TranslationStatus::Corrupt),
                    Some(format!(
                        "Invalid target_locale '{}' in translation",
                        locale_val
                    )),
                );
            }

            let translation = DescriptionTranslation {
                text: text_val.clone(),
                target_locale: locale_val.clone(),
            };

            // Derive validity/staleness based on current description
            let status = match current_description {
                Some(current_desc) => {
                    let current_hash = compute_source_hash(current_desc);
                    if current_hash == hash_raw {
                        TranslationStatus::Valid
                    } else {
                        TranslationStatus::Stale
                    }
                }
                None => TranslationStatus::Stale,
            };

            (
                Some(translation),
                Some(hash_raw.to_string()),
                Some(status),
                None,
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_source_hash_exact() {
        let d = "A fast and lightweight tool";
        let h = compute_source_hash(d);
        assert_eq!(h.len(), 64);
        assert!(h
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));

        // Different whitespace produces different hash
        assert_ne!(h, compute_source_hash(" A fast and lightweight tool"));
        assert_ne!(h, compute_source_hash("A fast and lightweight tool "));
        assert_ne!(h, compute_source_hash("A fast and  lightweight tool"));

        // Different line endings produce different hash
        assert_ne!(
            compute_source_hash("line1\nline2"),
            compute_source_hash("line1\r\nline2")
        );

        // Unicode non-ascii
        let zh = "一个用于管理 GitHub 仓库收藏的轻量级工具。";
        let zh_h = compute_source_hash(zh);
        assert_eq!(zh_h.len(), 64);
        assert_eq!(zh_h, compute_source_hash(zh));
    }

    #[test]
    fn test_parse_db_translation_valid_round_trip() {
        let desc = "Sample description";
        let hash = compute_source_hash(desc);
        let json = r#"{"text":"样本描述","target_locale":"zh-Hans"}"#;

        let (trans, h, status, err) = parse_db_translation(Some(json), Some(&hash), Some(desc));
        assert!(err.is_none());
        assert_eq!(status, Some(TranslationStatus::Valid));
        assert_eq!(h.as_deref(), Some(hash.as_str()));
        let t = trans.unwrap();
        assert_eq!(t.text, "样本描述");
        assert_eq!(t.target_locale, "zh-Hans");
    }

    #[test]
    fn test_parse_db_translation_stale_on_changed_desc() {
        let desc1 = "Sample description 1";
        let desc2 = "Sample description 2";
        let hash1 = compute_source_hash(desc1);
        let json = r#"{"text":"样本描述","target_locale":"zh-Hans"}"#;

        let (trans, h, status, err) = parse_db_translation(Some(json), Some(&hash1), Some(desc2));
        assert!(err.is_none());
        assert_eq!(status, Some(TranslationStatus::Stale));
        assert_eq!(h.as_deref(), Some(hash1.as_str()));
        assert!(trans.is_some());
    }

    #[test]
    fn test_parse_db_translation_stale_on_null_desc() {
        let desc1 = "Sample description 1";
        let hash1 = compute_source_hash(desc1);
        let json = r#"{"text":"样本描述","target_locale":"zh-Hans"}"#;

        let (trans, h, status, err) = parse_db_translation(Some(json), Some(&hash1), None);
        assert!(err.is_none());
        assert_eq!(status, Some(TranslationStatus::Stale));
        assert_eq!(h.as_deref(), Some(hash1.as_str()));
        assert!(trans.is_some());
    }

    #[test]
    fn test_parse_db_translation_corrupt_cases() {
        // Missing hash
        let (trans, _, status, err) = parse_db_translation(
            Some(r#"{"text":"abc","target_locale":"en"}"#),
            None,
            Some("x"),
        );
        assert_eq!(status, Some(TranslationStatus::Corrupt));
        assert!(trans.is_none());
        assert!(err.is_some());

        // Missing translation
        let (_, _, status, err) = parse_db_translation(None, Some("1234"), Some("x"));
        assert_eq!(status, Some(TranslationStatus::Corrupt));
        assert!(err.is_some());

        // Invalid locale
        let hash = compute_source_hash("x");
        let (_, _, status, err) = parse_db_translation(
            Some(r#"{"text":"abc","target_locale":"fr"}"#),
            Some(&hash),
            Some("x"),
        );
        assert_eq!(status, Some(TranslationStatus::Corrupt));
        assert!(err.is_some());

        // Array instead of object
        let (_, _, status, err) = parse_db_translation(
            Some(r#"[{"text":"abc","target_locale":"en"}]"#),
            Some(&hash),
            Some("x"),
        );
        assert_eq!(status, Some(TranslationStatus::Corrupt));
        assert!(err.is_some());

        // Literal null string
        let (_, _, status, err) = parse_db_translation(Some("null"), Some(&hash), Some("x"));
        assert_eq!(status, Some(TranslationStatus::Corrupt));
        assert!(err.is_some());

        // Empty string
        let (_, _, status, err) = parse_db_translation(Some(""), Some(&hash), Some("x"));
        assert_eq!(status, Some(TranslationStatus::Corrupt));
        assert!(err.is_some());
    }
}
