use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::db::analysis::AnalysisContent;

/// The strict output shape expected from the AI model.
///
/// Follows RepoShelf Technical Spec §11 & §19:
/// - The model generates ONLY `AnalysisModelOutput { content: AnalysisContent }`.
/// - The model does NOT generate envelope metadata (schema_version, target_locale,
///   generated_at, generator, source, coverage, search_text).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AnalysisModelOutput {
    pub content: AnalysisContent,
}

/// Constructs a standard JSON Schema for Ollama structured outputs (`format: <json_schema>`).
///
/// Matches the 8 fixed sections and all enum constraints defined in Technical Spec §8 & Appendix B.
pub fn ollama_analysis_json_schema() -> serde_json::Value {
    let item_schema = json!({
        "type": "object",
        "required": ["text", "basis", "evidence_ids"],
        "properties": {
            "text": { "type": "string" },
            "basis": {
                "type": "string",
                "enum": ["documented", "code_observed", "inferred", "uncertain"]
            },
            "evidence_ids": {
                "type": "array",
                "items": { "type": "string" }
            }
        },
        "additionalProperties": false
    });

    let section_schema = json!({
        "type": "object",
        "required": ["status", "items", "missing_information"],
        "properties": {
            "status": {
                "type": "string",
                "enum": ["supported", "partial", "insufficient"]
            },
            "items": {
                "type": "array",
                "items": item_schema
            },
            "missing_information": {
                "type": "array",
                "items": { "type": "string" }
            }
        },
        "additionalProperties": false
    });

    json!({
        "type": "object",
        "required": ["content"],
        "properties": {
            "content": {
                "type": "object",
                "required": [
                    "overview",
                    "features",
                    "use_cases",
                    "tech_stack",
                    "setup",
                    "target_users",
                    "maturity",
                    "limitations"
                ],
                "properties": {
                    "overview": section_schema,
                    "features": section_schema,
                    "use_cases": section_schema,
                    "tech_stack": section_schema,
                    "setup": section_schema,
                    "target_users": section_schema,
                    "maturity": section_schema,
                    "limitations": section_schema
                },
                "additionalProperties": false
            }
        },
        "additionalProperties": false
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::analysis::{AnalysisItem, AnalysisSection, ClaimBasis, SectionStatus};

    #[test]
    fn test_schema_round_trip() {
        let dummy_section = AnalysisSection {
            status: SectionStatus::Supported,
            items: vec![AnalysisItem {
                text: "Example item".to_string(),
                basis: ClaimBasis::Documented,
                evidence_ids: vec!["E001".to_string()],
            }],
            missing_information: vec![],
        };

        let content = AnalysisContent {
            overview: dummy_section.clone(),
            features: dummy_section.clone(),
            use_cases: dummy_section.clone(),
            tech_stack: dummy_section.clone(),
            setup: dummy_section.clone(),
            target_users: dummy_section.clone(),
            maturity: dummy_section.clone(),
            limitations: dummy_section,
        };

        let output = AnalysisModelOutput { content };
        let serialized = serde_json::to_string(&output).unwrap();
        let parsed: AnalysisModelOutput = serde_json::from_str(&serialized).unwrap();
        assert_eq!(output, parsed);
    }

    #[test]
    fn test_ollama_json_schema_validity() {
        let schema = ollama_analysis_json_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["content"]["required"].as_array().unwrap().len() == 8);
    }
}
