use chrono::DateTime;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use crate::error::AppError;

pub const VALID_ANALYSIS_LOCALES: &[&str] = &["en", "zh-Hans", "zh-Hant"];
pub const VALID_SECTION_STATUSES: &[&str] = &["supported", "partial", "insufficient"];
pub const VALID_CLAIM_BASES: &[&str] = &["documented", "code_observed", "inferred", "uncertain"];
pub const VALID_EVIDENCE_KINDS: &[&str] = &["metadata", "tree", "file", "release"];
pub const VALID_GAP_KINDS: &[&str] = &["missing", "failed", "skipped", "truncated", "unverified"];
pub const VALID_COVERAGE_STATUSES: &[&str] = &["bounded", "partial"];

pub const MAX_EVIDENCE_COUNT: usize = 64;
pub const MAX_RANGES_PER_EVIDENCE: usize = 8;
pub const MAX_GAPS_COUNT: usize = 40;
pub const MAX_SECTION_ITEMS: usize = 6;
pub const MAX_SECTION_MISSING_INFO: usize = 4;
pub const MAX_ITEM_TEXT_CHARS: usize = 360;
pub const MAX_MISSING_INFO_CHARS: usize = 200;
pub const MAX_TOTAL_CONTENT_CHARS: usize = 10000;
pub const MAX_ENVELOPE_JSON_BYTES: usize = 128 * 1024; // 128 KiB

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum AnalysisLocale {
    #[serde(rename = "en")]
    En,
    #[serde(rename = "zh-Hans")]
    ZhHans,
    #[serde(rename = "zh-Hant")]
    ZhHant,
}

impl AnalysisLocale {
    pub fn as_str(&self) -> &'static str {
        match self {
            AnalysisLocale::En => "en",
            AnalysisLocale::ZhHans => "zh-Hans",
            AnalysisLocale::ZhHant => "zh-Hant",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SectionStatus {
    Supported,
    Partial,
    Insufficient,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ClaimBasis {
    Documented,
    CodeObserved,
    Inferred,
    Uncertain,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AnalysisItem {
    pub text: String,
    pub basis: ClaimBasis,
    pub evidence_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AnalysisSection {
    pub status: SectionStatus,
    pub items: Vec<AnalysisItem>,
    pub missing_information: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AnalysisContent {
    pub overview: AnalysisSection,
    pub features: AnalysisSection,
    pub use_cases: AnalysisSection,
    pub tech_stack: AnalysisSection,
    pub setup: AnalysisSection,
    pub target_users: AnalysisSection,
    pub maturity: AnalysisSection,
    pub limitations: AnalysisSection,
}

impl AnalysisContent {
    /// Iterates through the 8 sections in the authoritative fixed order specified by RepoShelf Technical Spec §8 & §13.
    pub fn sections_in_order(&self) -> [(&'static str, &AnalysisSection); 8] {
        [
            ("overview", &self.overview),
            ("features", &self.features),
            ("use_cases", &self.use_cases),
            ("tech_stack", &self.tech_stack),
            ("setup", &self.setup),
            ("target_users", &self.target_users),
            ("maturity", &self.maturity),
            ("limitations", &self.limitations),
        ]
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AnalysisGenerator {
    pub provider: String,
    pub model_requested: String,
    pub model_reported: Option<String>,
    pub prompt_version: String,
    pub selector_version: String,
    pub context_budget_tokens: u32,
    pub input_tokens: u32,
    pub input_tokens_is_estimate: bool,
    pub output_limit_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AnalysisSource {
    pub github_repo_id: String,
    pub canonical_url: String,
    pub default_branch: Option<String>,
    pub commit_sha: Option<String>,
    pub metadata_observed_at: String,
    pub evidence_fingerprint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EvidenceRange {
    pub start_line: usize,
    pub end_line: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EvidenceRecord {
    pub id: String,
    pub kind: String, // "metadata" | "tree" | "file" | "release"
    pub path: Option<String>,
    pub commit_sha: Option<String>,
    pub blob_sha: Option<String>,
    pub observed_at: String,
    pub included_ranges: Vec<EvidenceRange>,
    pub included_content_hash: String,
    pub truncated: bool,
    pub public_fact_summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AnalysisGap {
    pub kind: String, // "missing" | "failed" | "skipped" | "truncated" | "unverified"
    pub subject: String,
    pub reason_code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AnalysisCoverage {
    pub status: String, // "bounded" | "partial"
    pub selected_file_count: u32,
    pub sent_file_count: u32,
    pub evidence_count: u32,
    pub gaps: Vec<AnalysisGap>,
    pub executed_code: bool,
    pub verified_deployment: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AnalysisEnvelopeV1 {
    pub schema_version: u32,
    pub content: AnalysisContent,
    pub target_locale: AnalysisLocale,
    pub generated_at: String,
    pub generator: AnalysisGenerator,
    pub source: AnalysisSource,
    pub evidence: Vec<EvidenceRecord>,
    pub coverage: AnalysisCoverage,
    pub search_text: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AnalysisPatch {
    Keep,
    Replace(AnalysisEnvelopeV1),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum AnalysisPatchInput {
    Keep,
    Replace { envelope: AnalysisEnvelopeV1 },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum AnalysisReadStatus {
    None,
    Valid,
    Corrupt { reason: String },
    Unsupported { schema_version: Option<u32>, reason: String },
}

/// Deterministically derives `search_text` from `content` following the frozen Technical Spec §13 rules:
/// 1. Iterates through the 8 sections in strict fixed order:
///    overview, features, use_cases, tech_stack, setup, target_users, maturity, limitations.
/// 2. For each section, extracts all `items[].text` in order, then `missing_information` in order.
/// 3. Normalizes whitespace within each segment and joins segments with newlines.
pub fn derive_search_text(content: &AnalysisContent) -> String {
    let mut parts = Vec::new();
    for (_name, section) in content.sections_in_order() {
        for item in &section.items {
            let normalized = item.text.split_whitespace().collect::<Vec<_>>().join(" ");
            if !normalized.is_empty() {
                parts.push(normalized);
            }
        }
        for missing in &section.missing_information {
            let normalized = missing.split_whitespace().collect::<Vec<_>>().join(" ");
            if !normalized.is_empty() {
                parts.push(normalized);
            }
        }
    }
    parts.join("\n")
}

/// Validates all structural constraints of `AnalysisEnvelopeV1`, overrides caller-supplied
/// `search_text` with the deterministic re-derived search text, and serializes to JSON string.
///
/// Returns `(json_string, derived_search_text)`.
pub fn validate_and_serialize_envelope(
    envelope: &AnalysisEnvelopeV1,
) -> Result<(String, String), AppError> {
    // 1. Schema version
    if envelope.schema_version != 1 {
        return Err(AppError::InvalidAnalysis(format!(
            "Unsupported schema_version {}; only version 1 is supported",
            envelope.schema_version
        )));
    }

    // 2. Timestamps must be valid RFC3339
    if DateTime::parse_from_rfc3339(&envelope.generated_at).is_err() {
        return Err(AppError::InvalidAnalysis(format!(
            "generated_at '{}' is not a valid RFC3339 timestamp",
            envelope.generated_at
        )));
    }
    if DateTime::parse_from_rfc3339(&envelope.source.metadata_observed_at).is_err() {
        return Err(AppError::InvalidAnalysis(format!(
            "source.metadata_observed_at '{}' is not a valid RFC3339 timestamp",
            envelope.source.metadata_observed_at
        )));
    }

    // 3. Evidence bounds and uniqueness
    if envelope.evidence.len() > MAX_EVIDENCE_COUNT {
        return Err(AppError::InvalidAnalysis(format!(
            "Evidence count {} exceeds maximum allowed {}",
            envelope.evidence.len(),
            MAX_EVIDENCE_COUNT
        )));
    }

    let mut evidence_ids = HashSet::new();
    let mut file_evidence_ids = HashSet::new();
    for ev in &envelope.evidence {
        if ev.id.trim().is_empty() {
            return Err(AppError::InvalidAnalysis("Evidence ID cannot be empty".to_string()));
        }
        if !evidence_ids.insert(ev.id.clone()) {
            return Err(AppError::InvalidAnalysis(format!(
                "Duplicate evidence ID '{}' in analysis envelope",
                ev.id
            )));
        }
        if !VALID_EVIDENCE_KINDS.contains(&ev.kind.as_str()) {
            return Err(AppError::InvalidAnalysis(format!(
                "Invalid evidence kind '{}' for evidence ID '{}'",
                ev.kind, ev.id
            )));
        }
        if ev.kind == "file" {
            file_evidence_ids.insert(ev.id.clone());
        }

        if ev.included_ranges.len() > MAX_RANGES_PER_EVIDENCE {
            return Err(AppError::InvalidAnalysis(format!(
                "Evidence '{}' included_ranges count {} exceeds maximum {}",
                ev.id,
                ev.included_ranges.len(),
                MAX_RANGES_PER_EVIDENCE
            )));
        }
        for range in &ev.included_ranges {
            if range.start_line < 1 || range.start_line > range.end_line {
                return Err(AppError::InvalidAnalysis(format!(
                    "Evidence '{}' contains invalid range {}-{}",
                    ev.id, range.start_line, range.end_line
                )));
            }
        }
    }

    // 4. Coverage consistency
    if envelope.coverage.evidence_count as usize != envelope.evidence.len() {
        return Err(AppError::InvalidAnalysis(format!(
            "coverage.evidence_count ({}) does not match actual evidence count ({})",
            envelope.coverage.evidence_count,
            envelope.evidence.len()
        )));
    }
    if envelope.coverage.sent_file_count > envelope.coverage.selected_file_count {
        return Err(AppError::InvalidAnalysis(format!(
            "coverage.sent_file_count ({}) cannot exceed selected_file_count ({})",
            envelope.coverage.sent_file_count, envelope.coverage.selected_file_count
        )));
    }
    if envelope.coverage.selected_file_count > 20 {
        return Err(AppError::InvalidAnalysis(format!(
            "coverage.selected_file_count ({}) exceeds maximum limit 20",
            envelope.coverage.selected_file_count
        )));
    }
    if envelope.coverage.executed_code {
        return Err(AppError::InvalidAnalysis(
            "coverage.executed_code must be false; executing code is strictly forbidden".to_string(),
        ));
    }
    if envelope.coverage.verified_deployment {
        return Err(AppError::InvalidAnalysis(
            "coverage.verified_deployment must be false; verifying deployment is unsupported in static analysis".to_string(),
        ));
    }
    if envelope.coverage.gaps.len() > MAX_GAPS_COUNT {
        return Err(AppError::InvalidAnalysis(format!(
            "coverage.gaps count {} exceeds maximum {}",
            envelope.coverage.gaps.len(),
            MAX_GAPS_COUNT
        )));
    }
    for gap in &envelope.coverage.gaps {
        if !VALID_GAP_KINDS.contains(&gap.kind.as_str()) {
            return Err(AppError::InvalidAnalysis(format!(
                "Invalid coverage gap kind '{}'",
                gap.kind
            )));
        }
    }
    if !VALID_COVERAGE_STATUSES.contains(&envelope.coverage.status.as_str()) {
        return Err(AppError::InvalidAnalysis(format!(
            "Invalid coverage status '{}'",
            envelope.coverage.status
        )));
    }

    // 5. Section items & text bounds
    let mut total_chars = 0;
    for (sec_name, section) in envelope.content.sections_in_order() {
        if section.items.len() > MAX_SECTION_ITEMS {
            return Err(AppError::InvalidAnalysis(format!(
                "Section '{}' items count {} exceeds maximum {}",
                sec_name,
                section.items.len(),
                MAX_SECTION_ITEMS
            )));
        }
        if section.missing_information.len() > MAX_SECTION_MISSING_INFO {
            return Err(AppError::InvalidAnalysis(format!(
                "Section '{}' missing_information count {} exceeds maximum {}",
                sec_name,
                section.missing_information.len(),
                MAX_SECTION_MISSING_INFO
            )));
        }

        for item in &section.items {
            let char_count = item.text.chars().count();
            if char_count == 0 {
                return Err(AppError::InvalidAnalysis(format!(
                    "Section '{}' contains an item with empty text",
                    sec_name
                )));
            }
            if char_count > MAX_ITEM_TEXT_CHARS {
                return Err(AppError::InvalidAnalysis(format!(
                    "Section '{}' item text length ({} chars) exceeds maximum {} chars",
                    sec_name, char_count, MAX_ITEM_TEXT_CHARS
                )));
            }
            total_chars += char_count;

            // Validate item evidence references
            for ev_id in &item.evidence_ids {
                if !evidence_ids.contains(ev_id) {
                    return Err(AppError::InvalidAnalysis(format!(
                        "Section '{}' item references non-existent evidence_id '{}'",
                        sec_name, ev_id
                    )));
                }
            }

            // Basis rule: code_observed requires at least one file evidence
            if item.basis == ClaimBasis::CodeObserved {
                let has_file_evidence = item.evidence_ids.iter().any(|id| file_evidence_ids.contains(id));
                if !has_file_evidence {
                    return Err(AppError::InvalidAnalysis(format!(
                        "Section '{}' item with basis 'code_observed' must reference at least one file evidence",
                        sec_name
                    )));
                }
            }
        }

        for missing in &section.missing_information {
            let char_count = missing.chars().count();
            if char_count == 0 {
                return Err(AppError::InvalidAnalysis(format!(
                    "Section '{}' contains empty missing_information entry",
                    sec_name
                )));
            }
            if char_count > MAX_MISSING_INFO_CHARS {
                return Err(AppError::InvalidAnalysis(format!(
                    "Section '{}' missing_information length ({} chars) exceeds maximum {} chars",
                    sec_name, char_count, MAX_MISSING_INFO_CHARS
                )));
            }
            total_chars += char_count;
        }
    }

    if total_chars > MAX_TOTAL_CONTENT_CHARS {
        return Err(AppError::InvalidAnalysis(format!(
            "Total content text length ({} chars) exceeds maximum {} chars",
            total_chars, MAX_TOTAL_CONTENT_CHARS
        )));
    }

    // 6. Overwrite caller's search_text with deterministic Rust derivation
    let derived_search = derive_search_text(&envelope.content);
    let mut normalized_envelope = envelope.clone();
    normalized_envelope.search_text = derived_search.clone();

    // 7. Serialize and check byte budget
    let serialized = serde_json::to_string(&normalized_envelope)
        .map_err(|e| AppError::InvalidAnalysis(format!("Failed to serialize envelope JSON: {}", e)))?;

    if serialized.len() > MAX_ENVELOPE_JSON_BYTES {
        return Err(AppError::InvalidAnalysis(format!(
            "Serialized analysis envelope size ({} bytes) exceeds maximum allowed {} bytes",
            serialized.len(),
            MAX_ENVELOPE_JSON_BYTES
        )));
    }

    Ok((serialized, derived_search))
}

/// Authoritative parsing and read-tolerance for database `ai_analysis` column.
///
/// Guaranteed behavior:
/// - Never panics.
/// - Does not crash on SQL NULL, malformed JSON, scalar values, arrays, unknown schemas, or partial objects.
/// - Returns `(Some(envelope), AnalysisReadStatus::Valid)` for fully compliant schema v1 envelopes.
/// - Returns `(None, AnalysisReadStatus::None)` for SQL NULL / empty strings.
/// - Returns `(None, AnalysisReadStatus::Corrupt { .. })` for malformed/corrupted data.
/// - Returns `(None, AnalysisReadStatus::Unsupported { .. })` for unknown schema_version.
pub fn parse_db_analysis(
    raw_json: Option<&str>,
) -> (Option<AnalysisEnvelopeV1>, AnalysisReadStatus) {
    let raw = match raw_json {
        Some(s) => s.trim(),
        None => return (None, AnalysisReadStatus::None),
    };

    if raw.is_empty() || raw == "null" {
        return (None, AnalysisReadStatus::None);
    }

    let parsed_val: Result<serde_json::Value, _> = serde_json::from_str(raw);
    let val = match parsed_val {
        Ok(v) => v,
        Err(e) => {
            return (
                None,
                AnalysisReadStatus::Corrupt {
                    reason: format!("Malformed JSON: {}", e),
                },
            );
        }
    };

    let obj = match val.as_object() {
        Some(o) => o,
        None => {
            return (
                None,
                AnalysisReadStatus::Corrupt {
                    reason: "Root value is not a JSON object".to_string(),
                },
            );
        }
    };

    // Inspect schema_version
    match obj.get("schema_version") {
        Some(serde_json::Value::Number(n)) => {
            if let Some(v) = n.as_u64() {
                if v != 1 {
                    return (
                        None,
                        AnalysisReadStatus::Unsupported {
                            schema_version: Some(v as u32),
                            reason: format!("Unsupported schema_version {}", v),
                        },
                    );
                }
            } else {
                return (
                    None,
                    AnalysisReadStatus::Corrupt {
                        reason: "schema_version is not an integer".to_string(),
                    },
                );
            }
        }
        Some(_) => {
            return (
                None,
                AnalysisReadStatus::Corrupt {
                    reason: "schema_version is not a number".to_string(),
                },
            );
        }
        None => {
            return (
                None,
                AnalysisReadStatus::Corrupt {
                    reason: "Missing schema_version field".to_string(),
                },
            );
        }
    };

    // Deserialize into AnalysisEnvelopeV1
    match serde_json::from_value::<AnalysisEnvelopeV1>(val) {
        Ok(envelope) => (Some(envelope), AnalysisReadStatus::Valid),
        Err(e) => (
            None,
            AnalysisReadStatus::Corrupt {
                reason: format!("Failed to decode AnalysisEnvelopeV1: {}", e),
            },
        ),
    }
}
