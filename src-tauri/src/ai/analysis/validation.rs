use crate::ai::search::strip_outer_code_fence;
use crate::db::analysis::{
    AnalysisContent, ClaimBasis, EvidenceRecord,
    MAX_ENVELOPE_JSON_BYTES, MAX_ITEM_TEXT_CHARS, MAX_MISSING_INFO_CHARS,
    MAX_SECTION_ITEMS, MAX_SECTION_MISSING_INFO, MAX_TOTAL_CONTENT_CHARS,
};
use crate::error::AppError;
use super::schema::AnalysisModelOutput;


/// Result of validating a model response.
pub enum ValidationResult {
    /// Validation succeeded and returned the parsed, fully validated content.
    Success(AnalysisContent),
    /// Validation failed with a repairable structural or formatting error.
    Repairable(String),
    /// Validation failed with a non-repairable error that must abort the task immediately.
    NonRepairable(AppError),
}

/// Validates raw model output text against the strict RepoShelf Technical Spec §24 rules.
pub fn validate_model_output(
    raw_response: &str,
    sent_records: &[EvidenceRecord],
) -> ValidationResult {
    let trimmed = raw_response.trim();

    // 1. Empty content check
    if trimmed.is_empty() {
        return ValidationResult::NonRepairable(AppError::AIInvalidResponse(
            "AI returned empty response (EMPTY_CONTENT)".to_string(),
        ));
    }

    // 2. Reject obvious over-size (> 128 KiB)
    if trimmed.len() > MAX_ENVELOPE_JSON_BYTES {
        return ValidationResult::NonRepairable(AppError::AnalysisBudgetExceeded(format!(
            "Model response size ({} bytes) exceeds 128 KiB limit",
            trimmed.len()
        )));
    }

    // Strip code fences if present
    let cleaned = strip_outer_code_fence(trimmed);
    let cleaned_trimmed = cleaned.trim();

    if cleaned_trimmed.is_empty() {
        return ValidationResult::NonRepairable(AppError::AIInvalidResponse(
            "AI returned empty content inside code fence".to_string(),
        ));
    }

    // 3. Strict JSON parse
    let parsed_output: AnalysisModelOutput = match serde_json::from_str(cleaned_trimmed) {
        Ok(out) => out,
        Err(e) => {
            let err_msg = e.to_string();
            // Check if truncation caused EOF syntax error
            if err_msg.contains("EOF while parsing")
                || err_msg.contains("unexpected end of JSON")
                || err_msg.contains("trailing comma")
            {
                return ValidationResult::NonRepairable(AppError::AIInvalidResponse(format!(
                    "Model output was truncated or incomplete JSON: {}",
                    err_msg
                )));
            }
            return ValidationResult::Repairable(format!("Failed to parse JSON: {}", err_msg));
        }
    };

    let content = parsed_output.content;

    // 4. Validate evidence IDs and kinds
    let sent_id_map: std::collections::HashMap<String, &str> = sent_records
        .iter()
        .map(|r| (r.id.clone(), r.kind.as_str()))
        .collect();

    let mut total_chars: usize = 0;
    let mut has_informative_evidence_backed_claim = false;
    let mut all_insufficient = true;

    for (name, section) in content.sections_in_order() {
        if section.status != crate::db::analysis::SectionStatus::Insufficient {
            all_insufficient = false;
        }

        // Section item limits
        if section.items.len() > MAX_SECTION_ITEMS {
            return ValidationResult::Repairable(format!(
                "Section '{}' contains {} items, exceeding maximum allowed {}",
                name,
                section.items.len(),
                MAX_SECTION_ITEMS
            ));
        }

        // Missing information limits
        if section.missing_information.len() > MAX_SECTION_MISSING_INFO {
            return ValidationResult::Repairable(format!(
                "Section '{}' contains {} missing_information items, exceeding maximum allowed {}",
                name,
                section.missing_information.len(),
                MAX_SECTION_MISSING_INFO
            ));
        }

        for item in &section.items {
            let item_len = item.text.chars().count();
            if item_len > MAX_ITEM_TEXT_CHARS {
                return ValidationResult::Repairable(format!(
                    "Item text in section '{}' ({} chars) exceeds limit of {} chars",
                    name, item_len, MAX_ITEM_TEXT_CHARS
                ));
            }
            total_chars += item_len;

            // Evidence ID check
            if item.evidence_ids.is_empty() && item.basis != ClaimBasis::Uncertain {
                // Must have evidence unless uncertain
                return ValidationResult::Repairable(format!(
                    "Item in section '{}' with basis '{:?}' must reference at least one evidence_id",
                    name, item.basis
                ));
            }

            for ev_id in &item.evidence_ids {
                match sent_id_map.get(ev_id) {
                    Some(&kind) => {
                        if item.basis == ClaimBasis::CodeObserved && kind != "file" {
                            return ValidationResult::Repairable(format!(
                                "Item in section '{}' claimed code_observed but cited non-file evidence '{}' ({})",
                                name, ev_id, kind
                            ));
                        }
                    }
                    None => {
                        return ValidationResult::Repairable(format!(
                            "Item in section '{}' referenced non-existent or unsent evidence ID '{}'",
                            name, ev_id
                        ));
                    }
                }
            }

            // Check if this item contributes to meaningful content in overview or features
            if (name == "overview" || name == "features")
                && !item.text.trim().is_empty()
                && !item.evidence_ids.is_empty()
            {
                has_informative_evidence_backed_claim = true;
            }
        }

        for missing in &section.missing_information {
            let missing_len = missing.chars().count();
            if missing_len > MAX_MISSING_INFO_CHARS {
                return ValidationResult::Repairable(format!(
                    "missing_information item in section '{}' ({} chars) exceeds limit of {} chars",
                    name, missing_len, MAX_MISSING_INFO_CHARS
                ));
            }
            total_chars += missing_len;
        }
    }

    // 5. Total text length check
    if total_chars > MAX_TOTAL_CONTENT_CHARS {
        return ValidationResult::Repairable(format!(
            "Total content length ({} chars) exceeds maximum allowed {} chars",
            total_chars, MAX_TOTAL_CONTENT_CHARS
        ));
    }

    // 6. Meaningful content check
    if all_insufficient || !has_informative_evidence_backed_claim {
        return ValidationResult::NonRepairable(AppError::AnalysisInsufficientEvidence(
            "Repository evidence is insufficient to produce an informative analysis (all sections insufficient or no evidence-backed claims)".to_string(),
        ));
    }

    ValidationResult::Success(content)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::analysis::{AnalysisItem, AnalysisSection, ClaimBasis, SectionStatus};

    fn make_test_evidence_records() -> Vec<EvidenceRecord> {
        vec![
            EvidenceRecord {
                id: "E001".to_string(),
                kind: "metadata".to_string(),
                path: None,
                commit_sha: None,
                blob_sha: None,
                observed_at: "2026-03-01T00:00:00Z".to_string(),
                included_ranges: vec![],
                included_content_hash: "hash".to_string(),
                truncated: false,
                public_fact_summary: Some("meta".to_string()),
            },
            EvidenceRecord {
                id: "E003".to_string(),
                kind: "file".to_string(),
                path: Some("src/main.rs".to_string()),
                commit_sha: Some("sha40".to_string()),
                blob_sha: None,
                observed_at: "2026-03-01T00:00:00Z".to_string(),
                included_ranges: vec![],
                included_content_hash: "hash".to_string(),
                truncated: false,
                public_fact_summary: Some("source file".to_string()),
            },
        ]
    }

    #[test]
    fn test_empty_content_rejected() {
        let records = make_test_evidence_records();
        let res = validate_model_output("", &records);
        assert!(matches!(res, ValidationResult::NonRepairable(_)));
    }

    #[test]
    fn test_invalid_evidence_id_repairable() {
        let records = make_test_evidence_records();
        let json = r#"{
            "content": {
                "overview": { "status": "supported", "items": [{"text": "Hello", "basis": "documented", "evidence_ids": ["E999"]}], "missing_information": [] },
                "features": { "status": "supported", "items": [{"text": "Feature", "basis": "documented", "evidence_ids": ["E001"]}], "missing_information": [] },
                "use_cases": { "status": "insufficient", "items": [], "missing_information": [] },
                "tech_stack": { "status": "insufficient", "items": [], "missing_information": [] },
                "setup": { "status": "insufficient", "items": [], "missing_information": [] },
                "target_users": { "status": "insufficient", "items": [], "missing_information": [] },
                "maturity": { "status": "insufficient", "items": [], "missing_information": [] },
                "limitations": { "status": "insufficient", "items": [], "missing_information": [] }
            }
        }"#;

        let res = validate_model_output(json, &records);
        match res {
            ValidationResult::Repairable(err) => {
                assert!(err.contains("E999"));
            }
            _ => panic!("Expected repairable error for invalid evidence ID"),
        }
    }

    #[test]
    fn test_code_observed_without_file_evidence_repairable() {
        let records = make_test_evidence_records();
        let json = r#"{
            "content": {
                "overview": { "status": "supported", "items": [{"text": "Hello", "basis": "code_observed", "evidence_ids": ["E001"]}], "missing_information": [] },
                "features": { "status": "supported", "items": [{"text": "Feature", "basis": "documented", "evidence_ids": ["E001"]}], "missing_information": [] },
                "use_cases": { "status": "insufficient", "items": [], "missing_information": [] },
                "tech_stack": { "status": "insufficient", "items": [], "missing_information": [] },
                "setup": { "status": "insufficient", "items": [], "missing_information": [] },
                "target_users": { "status": "insufficient", "items": [], "missing_information": [] },
                "maturity": { "status": "insufficient", "items": [], "missing_information": [] },
                "limitations": { "status": "insufficient", "items": [], "missing_information": [] }
            }
        }"#;

        let res = validate_model_output(json, &records);
        match res {
            ValidationResult::Repairable(err) => {
                assert!(err.contains("code_observed"));
                assert!(err.contains("non-file evidence"));
            }
            _ => panic!("Expected repairable error for code_observed without file evidence"),
        }
    }

    #[test]
    fn test_all_insufficient_rejected() {
        let records = make_test_evidence_records();
        let json = r#"{
            "content": {
                "overview": { "status": "insufficient", "items": [], "missing_information": [] },
                "features": { "status": "insufficient", "items": [], "missing_information": [] },
                "use_cases": { "status": "insufficient", "items": [], "missing_information": [] },
                "tech_stack": { "status": "insufficient", "items": [], "missing_information": [] },
                "setup": { "status": "insufficient", "items": [], "missing_information": [] },
                "target_users": { "status": "insufficient", "items": [], "missing_information": [] },
                "maturity": { "status": "insufficient", "items": [], "missing_information": [] },
                "limitations": { "status": "insufficient", "items": [], "missing_information": [] }
            }
        }"#;

        let res = validate_model_output(json, &records);
        assert!(matches!(res, ValidationResult::NonRepairable(AppError::AnalysisInsufficientEvidence(_))));
    }
}
