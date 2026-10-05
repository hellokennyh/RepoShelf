use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use sha2::Digest;
use tempfile::tempdir;
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use reposhelf_lib::ai::analysis::*;
use reposhelf_lib::ai::translation::{translate_description, TranslationLimiter};
use reposhelf_lib::db::analysis::*;
use reposhelf_lib::error::AppError;
use reposhelf_lib::github::evidence::identity::ValidatedRepoIdentity;
use reposhelf_lib::github::evidence::release::ReleaseEvidenceItem;
use reposhelf_lib::github::evidence::service::{
    DirectorySummary, EvidencePack, RepositoryMetadataSummary, SelectedTextEvidence,
};
use reposhelf_lib::secrets::{InMemorySecretStore, SecretStore, SECRET_KEY_AI_API_KEY};
use reposhelf_lib::settings::{AiProvider, AiSettings, Settings, SettingsStore};

fn make_test_settings_store(ai_settings: AiSettings) -> (tempfile::TempDir, SettingsStore) {
    let dir = tempdir().expect("Failed to create tempdir");
    let config_path = dir.path().join("settings.json");
    let mut settings = Settings::default();
    settings.ai = ai_settings;

    let store = SettingsStore::new(config_path).expect("Failed to create SettingsStore");
    store.save(&settings).expect("Failed to save initial settings");
    (dir, store)
}

fn make_valid_8_section_json(overview_text: &str, features_text: &str) -> String {
    format!(
        r#"{{
  "content": {{
    "overview": {{ "status": "supported", "items": [{{ "text": "{}", "basis": "documented", "evidence_ids": ["E001"] }}], "missing_information": [] }},
    "features": {{ "status": "supported", "items": [{{ "text": "{}", "basis": "documented", "evidence_ids": ["E001"] }}], "missing_information": [] }},
    "use_cases": {{ "status": "insufficient", "items": [], "missing_information": ["No use cases"] }},
    "tech_stack": {{ "status": "insufficient", "items": [], "missing_information": [] }},
    "setup": {{ "status": "insufficient", "items": [], "missing_information": [] }},
    "target_users": {{ "status": "insufficient", "items": [], "missing_information": [] }},
    "maturity": {{ "status": "insufficient", "items": [], "missing_information": [] }},
    "limitations": {{ "status": "insufficient", "items": [], "missing_information": [] }}
  }}
}}"#,
        overview_text, features_text
    )
}

fn ollama_chat_response(content: &str) -> serde_json::Value {
    serde_json::json!({
        "message": {
            "content": content
        },
        "done": true
    })
}

/// Creates an oversized synthetic EvidencePack mimicking bat / large repositories:
/// - README first file >= 38 KiB (~1,000 lines)
/// - Directory sample with 100 paths
/// - 5 releases
/// - Rich metadata
fn create_oversized_synthetic_evidence_pack() -> EvidencePack {
    // Generate ~39 KiB README (1,000 lines of markdown)
    let mut readme_lines = Vec::new();
    for i in 1..=1000 {
        readme_lines.push(format!(
            "Line {}: Bat is a cat clone with wings. Highlighting syntax and git integration features for Rust code.",
            i
        ));
    }
    let readme_content = readme_lines.join("\n");
    assert!(readme_content.len() >= 38 * 1024, "README must be >= 38 KiB");

    let readme_hash = format!("{:x}", sha2::Sha256::digest(readme_content.as_bytes()));

    let mut sample_paths = Vec::new();
    for i in 1..=100 {
        sample_paths.push(format!("src/module_{}/subfile_{}.rs", i % 10, i));
    }

    let mut recent_releases = Vec::new();
    let mut evidence_records = vec![
        EvidenceRecord {
            id: "E001".to_string(),
            kind: "metadata".to_string(),
            path: None,
            commit_sha: None,
            blob_sha: None,
            observed_at: "2026-10-05T00:00:00Z".to_string(),
            included_ranges: vec![],
            included_content_hash: "meta_hash".to_string(),
            truncated: false,
            public_fact_summary: Some("Repo metadata".to_string()),
        },
        EvidenceRecord {
            id: "E002".to_string(),
            kind: "tree".to_string(),
            path: None,
            commit_sha: None,
            blob_sha: None,
            observed_at: "2026-10-05T00:00:00Z".to_string(),
            included_ranges: vec![],
            included_content_hash: "tree_hash".to_string(),
            truncated: false,
            public_fact_summary: Some("Directory paths".to_string()),
        },
        EvidenceRecord {
            id: "E003".to_string(),
            kind: "file".to_string(),
            path: Some("README.md".to_string()),
            commit_sha: Some("abcdef1234567890abcdef1234567890abcdef12".to_string()),
            blob_sha: None,
            observed_at: "2026-10-05T00:00:00Z".to_string(),
            included_ranges: vec![EvidenceRange { start_line: 1, end_line: 1000 }],
            included_content_hash: readme_hash.clone(),
            truncated: false,
            public_fact_summary: Some("README introduction".to_string()),
        },
    ];

    for i in 1..=5 {
        let rel_id = format!("E01{}", i);
        recent_releases.push(ReleaseEvidenceItem {
            evidence_id: rel_id.clone(),
            release_id: i,
            tag_name: format!("v1.{}.0", i),
            name: Some(format!("Release v1.{}.0", i)),
            published_at: Some("2026-09-01T00:00:00Z".to_string()),
            bounded_body: Some(format!("Release notes for version 1.{}.0 with multiple bug fixes and enhancements.", i)),
            prerelease: false,
        });
        evidence_records.push(EvidenceRecord {
            id: rel_id,
            kind: "release".to_string(),
            path: None,
            commit_sha: None,
            blob_sha: None,
            observed_at: "2026-10-05T00:00:00Z".to_string(),
            included_ranges: vec![],
            included_content_hash: format!("rel_hash_{}", i),
            truncated: false,
            public_fact_summary: Some(format!("Release v1.{}.0", i)),
        });
    }

    EvidencePack {
        identity: ValidatedRepoIdentity {
            owner: "sharkdp".to_string(),
            repo: "bat".to_string(),
            canonical_url: "https://github.com/sharkdp/bat".to_string(),
            expected_repo_id: Some(12345678),
        },
        metadata_summary: RepositoryMetadataSummary {
            github_repo_id: 12345678,
            owner: "sharkdp".to_string(),
            repo: "bat".to_string(),
            canonical_url: "https://github.com/sharkdp/bat".to_string(),
            default_branch: Some("master".to_string()),
            description: Some("A cat(1) clone with wings.".to_string()),
            stars: 48000,
            license: Some("MIT".to_string()),
            topics: vec!["cli".to_string(), "rust".to_string(), "syntax-highlighting".to_string()],
            created_at: Some("2018-05-01T00:00:00Z".to_string()),
            pushed_at: Some("2026-10-01T00:00:00Z".to_string()),
            archived: false,
            is_empty: false,
        },
        directory_summary: DirectorySummary {
            total_candidate_paths: 100,
            sample_paths,
            truncated: false,
        },
        recent_releases,
        selected_texts: vec![SelectedTextEvidence {
            evidence_id: "E003".to_string(),
            path: "README.md".to_string(),
            text_content: readme_content,
            content_hash: readme_hash,
            commit_sha: Some("abcdef1234567890abcdef1234567890abcdef12".to_string()),
            blob_sha: None,
            truncated: false,
            included_ranges: vec![EvidenceRange { start_line: 1, end_line: 1000 }],
            role: "readme".to_string(),
        }],
        evidence_records,
        coverage: AnalysisCoverage {
            status: "bounded".to_string(),
            selected_file_count: 1,
            sent_file_count: 1,
            evidence_count: 7,
            gaps: vec![],
            executed_code: false,
            verified_deployment: false,
        },
        source: AnalysisSource {
            github_repo_id: "12345678".to_string(),
            canonical_url: "https://github.com/sharkdp/bat".to_string(),
            default_branch: Some("master".to_string()),
            commit_sha: Some("abcdef1234567890abcdef1234567890abcdef12".to_string()),
            metadata_observed_at: "2026-10-05T00:00:00Z".to_string(),
            evidence_fingerprint: "bat_fingerprint".to_string(),
        },
        evidence_fingerprint: "bat_fingerprint".to_string(),
        selector_version: "1.1.2-v1".to_string(),
        resource_accounting: Default::default(),
        untrusted_warning: "UNTRUSTED REPOSITORY DATA",
    }
}

// ============================================================
// 18. TEST — FALLBACK OVERSIZED FIRST README (FINDING-S5-MAJ-01 Core Test)
// ============================================================
#[tokio::test]
async fn test_section_18_fallback_oversized_first_readme() {
    let mock_server = MockServer::start().await;
    let valid_json = make_valid_8_section_json("Bat overview", "Syntax highlighting");

    // Provider mock expects num_ctx = 14336 in Ollama options
    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .and(body_string_contains("\"num_ctx\":14336"))
        .respond_with(ResponseTemplate::new(200).set_body_json(ollama_chat_response(&valid_json)))
        .mount(&mock_server)
        .await;

    let (_dir, store) = make_test_settings_store(AiSettings {
        enabled: true,
        provider: Some(AiProvider::Ollama),
        base_url: Some(mock_server.uri()),
        model: Some("qwen3.5:9b".to_string()),
        repository_analysis_context_tokens: None, // Conservative fallback
    });
    let secrets = InMemorySecretStore::new();
    let limiter = TranslationLimiter::new();
    let pack = create_oversized_synthetic_evidence_pack();

    // 1. Verify pack_evidence_for_model behavior
    let budget = compute_context_budget(None);
    assert_eq!(budget.effective_context_tokens, 14336);
    assert_eq!(budget.model_input_target, 8192);

    let packed = pack_evidence_for_model(&pack, AnalysisLocale::En, budget.model_input_target).unwrap();

    // Must NOT send full README
    assert!(packed.user_prompt.len() < pack.selected_texts[0].text_content.len());
    // Final estimated input <= 8192
    assert!(packed.estimated_input_tokens <= 8192, "Estimated input ({}) must be <= 8192", packed.estimated_input_tokens);

    // Sent EvidenceRecord must be truthfully marked truncated
    let readme_rec = packed.sent_evidence_records.iter().find(|e| e.id == "E003").expect("README record must be present");
    assert!(readme_rec.truncated, "Oversized README must be marked truncated = true");
    assert!(!readme_rec.included_ranges.is_empty(), "Included ranges must not be empty");
    assert!(readme_rec.included_ranges[0].end_line < 1000, "Sent line range must be truncated from 1000 lines");

    // Content hash must match the actual sent slice, NOT the original 38KB
    assert_ne!(readme_rec.included_content_hash, pack.selected_texts[0].content_hash);

    // 2. Full pipeline execution under mock
    let env = execute_repository_analysis(
        &pack,
        AnalysisLocale::En,
        &store,
        &secrets,
        &limiter,
        None,
        AnalysisExecutionOptions::default(),
    )
    .await
    .expect("Analysis must succeed under mock with fallback 14336 context");

    assert_eq!(env.generator.context_budget_tokens, 14336);
    assert!(env.generator.input_tokens <= 8192);
    let readme_ev = env.evidence.iter().find(|e| e.id == "E003").expect("E003 must be present");
    assert!(readme_ev.truncated);
}

// ============================================================
// 19. TEST — EXPLICIT 14336
// ============================================================
#[tokio::test]
async fn test_section_19_explicit_14336() {
    let mock_server = MockServer::start().await;
    let valid_json = make_valid_8_section_json("Bat overview", "Syntax highlighting");

    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .and(body_string_contains("\"num_ctx\":14336"))
        .respond_with(ResponseTemplate::new(200).set_body_json(ollama_chat_response(&valid_json)))
        .mount(&mock_server)
        .await;

    let (_dir, store) = make_test_settings_store(AiSettings {
        enabled: true,
        provider: Some(AiProvider::Ollama),
        base_url: Some(mock_server.uri()),
        model: Some("qwen3.5:9b".to_string()),
        repository_analysis_context_tokens: Some(14336), // Explicit 14336
    });
    let secrets = InMemorySecretStore::new();
    let limiter = TranslationLimiter::new();
    let pack = create_oversized_synthetic_evidence_pack();

    let env = execute_repository_analysis(
        &pack,
        AnalysisLocale::En,
        &store,
        &secrets,
        &limiter,
        None,
        AnalysisExecutionOptions::default(),
    )
    .await
    .expect("Explicit 14336 analysis must succeed under mock");

    assert_eq!(env.generator.context_budget_tokens, 14336);
    assert!(env.generator.input_tokens <= 8192);
    let readme_ev = env.evidence.iter().find(|e| e.id == "E003").expect("E003 must be present");
    assert!(readme_ev.truncated);
}

// ============================================================
// 20. TEST — FALLBACK VS EXPLICIT EQUIVALENCE
// ============================================================
#[test]
fn test_section_20_fallback_vs_explicit_equivalence() {
    let pack = create_oversized_synthetic_evidence_pack();

    let budget_fallback = compute_context_budget(None);
    let budget_explicit = compute_context_budget(Some(14336));

    assert_eq!(budget_fallback.effective_context_tokens, budget_explicit.effective_context_tokens);
    assert_eq!(budget_fallback.model_input_target, budget_explicit.model_input_target);
    assert_eq!(budget_fallback.output_reserve_tokens, budget_explicit.output_reserve_tokens);
    assert_eq!(budget_fallback.safety_margin_tokens, budget_explicit.safety_margin_tokens);

    assert!(budget_fallback.is_conservative_fallback);
    assert!(!budget_explicit.is_conservative_fallback);

    let packed_fallback = pack_evidence_for_model(&pack, AnalysisLocale::En, budget_fallback.model_input_target).unwrap();
    let packed_explicit = pack_evidence_for_model(&pack, AnalysisLocale::En, budget_explicit.model_input_target).unwrap();

    // Packing policy, sent files, ranges, and hashes must be identical
    assert_eq!(packed_fallback.sent_file_count, packed_explicit.sent_file_count);
    assert_eq!(packed_fallback.estimated_input_tokens, packed_explicit.estimated_input_tokens);
    assert_eq!(packed_fallback.user_prompt, packed_explicit.user_prompt);

    assert_eq!(packed_fallback.sent_evidence_records.len(), packed_explicit.sent_evidence_records.len());
    for (r1, r2) in packed_fallback.sent_evidence_records.iter().zip(&packed_explicit.sent_evidence_records) {
        assert_eq!(r1.id, r2.id);
        assert_eq!(r1.included_ranges, r2.included_ranges);
        assert_eq!(r1.included_content_hash, r2.included_content_hash);
        assert_eq!(r1.truncated, r2.truncated);
    }
}

// ============================================================
// 21. TEST — FIXED OVERHEAD ACCOUNTING
// ============================================================
#[test]
fn test_section_21_fixed_overhead_accounting() {
    let mut pack = create_oversized_synthetic_evidence_pack();
    // Replace README with 2 small files
    pack.selected_texts = vec![
        SelectedTextEvidence {
            evidence_id: "E003".to_string(),
            path: "Cargo.toml".to_string(),
            text_content: "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n".to_string(),
            content_hash: "hash_toml".to_string(),
            commit_sha: Some("sha40".to_string()),
            blob_sha: None,
            truncated: false,
            included_ranges: vec![EvidenceRange { start_line: 1, end_line: 3 }],
            role: "manifest".to_string(),
        },
        SelectedTextEvidence {
            evidence_id: "E004".to_string(),
            path: "src/main.rs".to_string(),
            text_content: "fn main() {\n    println!(\"hello\");\n}\n".to_string(),
            content_hash: "hash_main".to_string(),
            commit_sha: Some("sha40".to_string()),
            blob_sha: None,
            truncated: false,
            included_ranges: vec![EvidenceRange { start_line: 1, end_line: 3 }],
            role: "core_source".to_string(),
        },
    ];

    let packed = pack_evidence_for_model(&pack, AnalysisLocale::En, 8192).unwrap();
    // Prove that total input tokens is strictly bounded by 8192
    assert!(packed.estimated_input_tokens <= 8192);
    // Both small files fit cleanly
    assert_eq!(packed.sent_file_count, 2);
}

// ============================================================
// 22. TEST — FIXED OVERHEAD ALONE TOO LARGE
// ============================================================
#[test]
fn test_section_22_fixed_overhead_alone_too_large() {
    let pack = create_oversized_synthetic_evidence_pack();

    // Artificially pass a tiny budget smaller than the fixed system + schema overhead (~1374 tokens)
    let tiny_budget = 1000;
    let res = pack_evidence_for_model(&pack, AnalysisLocale::En, tiny_budget);

    assert!(res.is_err());
    assert!(matches!(res.unwrap_err(), AppError::AnalysisBudgetExceeded(_)));
}

// ============================================================
// 23. TEST — FINAL HARD GATE (Defense-In-Depth)
// ============================================================
#[tokio::test]
async fn test_section_23_final_hard_gate_blocks_dispatch() {
    let mock_server = MockServer::start().await;

    // Mount mock expecting 0 calls
    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(ResponseTemplate::new(200).set_body_json(ollama_chat_response("test")))
        .expect(0) // Provider MUST receive 0 calls!
        .mount(&mock_server)
        .await;

    let (_dir, store) = make_test_settings_store(AiSettings {
        enabled: true,
        provider: Some(AiProvider::Ollama),
        base_url: Some(mock_server.uri()),
        model: Some("qwen3.5:9b".to_string()),
        repository_analysis_context_tokens: None,
    });
    let secrets = InMemorySecretStore::new();
    let limiter = TranslationLimiter::new();
    let pack = create_oversized_synthetic_evidence_pack();

    // Construct a synthetic PackedModelInput that maliciously exceeds model_input_target (8192)
    let giant_user_prompt = format!("<selected_files>\n{}\n</selected_files>", "line of code\n".repeat(4000));
    let illegal_packed = PackedModelInput {
        user_prompt: giant_user_prompt,
        sent_evidence_records: pack.evidence_records.clone(),
        sent_file_count: 1,
        estimated_input_tokens: 15000,
    };

    // Also verify unit gate directly
    let budget = compute_context_budget(None);
    let gate_res = verify_pre_dispatch_budget_gate(&illegal_packed.user_prompt, &budget);
    assert!(gate_res.is_err());
    assert!(matches!(gate_res.unwrap_err(), AppError::AnalysisBudgetExceeded(_)));

    // Run engine with packed override
    let res = execute_repository_analysis_with_packed_override(
        &pack,
        AnalysisLocale::En,
        &store,
        &secrets,
        &limiter,
        None,
        AnalysisExecutionOptions::default(),
        Some(illegal_packed),
    )
    .await;

    assert!(res.is_err(), "Engine must reject over-budget input at the hard gate");
    assert!(matches!(res.unwrap_err(), AppError::AnalysisBudgetExceeded(_)));
}

// ============================================================
// 24. TEST — 32768 CONTEXT
// ============================================================
#[tokio::test]
async fn test_section_24_context_32768() {
    let mock_server = MockServer::start().await;
    let valid_json = make_valid_8_section_json("Overview 32k", "Features 32k");

    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .and(body_string_contains("\"num_ctx\":32768"))
        .respond_with(ResponseTemplate::new(200).set_body_json(ollama_chat_response(&valid_json)))
        .mount(&mock_server)
        .await;

    let (_dir, store) = make_test_settings_store(AiSettings {
        enabled: true,
        provider: Some(AiProvider::Ollama),
        base_url: Some(mock_server.uri()),
        model: Some("qwen3.5:9b".to_string()),
        repository_analysis_context_tokens: Some(32768),
    });
    let secrets = InMemorySecretStore::new();
    let limiter = TranslationLimiter::new();
    let pack = create_oversized_synthetic_evidence_pack();

    let budget = compute_context_budget(Some(32768));
    assert_eq!(budget.effective_context_tokens, 32768);
    assert_eq!(budget.model_input_target, 20000);
    assert_eq!(budget.output_reserve_tokens, 6144);
    assert!(budget.safety_margin_tokens >= 2048);

    let packed = pack_evidence_for_model(&pack, AnalysisLocale::En, budget.model_input_target).unwrap();
    assert!(packed.estimated_input_tokens <= 20000);

    let env = execute_repository_analysis(
        &pack,
        AnalysisLocale::En,
        &store,
        &secrets,
        &limiter,
        None,
        AnalysisExecutionOptions::default(),
    )
    .await
    .unwrap();

    assert_eq!(env.generator.context_budget_tokens, 32768);
    assert!(env.generator.input_tokens <= 20000);
}

// ============================================================
// 25. TEST — 51200 CONTEXT
// ============================================================
#[tokio::test]
async fn test_section_25_context_51200() {
    let mock_server = MockServer::start().await;
    let valid_json = make_valid_8_section_json("Overview 51k", "Features 51k");

    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .and(body_string_contains("\"num_ctx\":51200"))
        .respond_with(ResponseTemplate::new(200).set_body_json(ollama_chat_response(&valid_json)))
        .mount(&mock_server)
        .await;

    let (_dir, store) = make_test_settings_store(AiSettings {
        enabled: true,
        provider: Some(AiProvider::Ollama),
        base_url: Some(mock_server.uri()),
        model: Some("qwen3.5:9b".to_string()),
        repository_analysis_context_tokens: Some(51200),
    });
    let secrets = InMemorySecretStore::new();
    let limiter = TranslationLimiter::new();
    let pack = create_oversized_synthetic_evidence_pack();

    let budget = compute_context_budget(Some(51200));
    assert_eq!(budget.effective_context_tokens, 51200);
    assert_eq!(budget.model_input_target, 30000, "Input target must cap strictly at 30,000");
    assert_eq!(budget.output_reserve_tokens, 8192);
    assert!(budget.safety_margin_tokens >= 2048);
    assert!(budget.model_input_target + budget.output_reserve_tokens + budget.safety_margin_tokens <= 51200);

    let env = execute_repository_analysis(
        &pack,
        AnalysisLocale::En,
        &store,
        &secrets,
        &limiter,
        None,
        AnalysisExecutionOptions::default(),
    )
    .await
    .unwrap();

    assert_eq!(env.generator.context_budget_tokens, 51200);
    assert!(env.generator.input_tokens <= 30000);
}

// ============================================================
// 26. TEST — REPAIR REQUEST BUDGET ENFORCEMENT
// ============================================================
#[tokio::test]
async fn test_section_26_repair_request_budget_enforcement() {
    let mock_server = MockServer::start().await;
    let call_count = Arc::new(AtomicUsize::new(0));

    // First call: returns repairable invalid output with a HUGE body (40 KiB)
    let huge_invalid_json = format!(
        "{{\"content\": {{\"overview\": {{\"status\": \"supported\", \"items\": [{{\"text\": \"{}\", \"basis\": \"unknown_basis\", \"evidence_ids\": [\"E001\"]}}], \"missing_information\": []}}}}}}",
        "A".repeat(40 * 1024)
    );

    let valid_repaired_json = make_valid_8_section_json("Repaired overview", "Repaired features");

    let count_clone = call_count.clone();
    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(move |_req: &wiremock::Request| {
            let n = count_clone.fetch_add(1, Ordering::SeqCst);
            if n == 0 {
                ResponseTemplate::new(200).set_body_json(ollama_chat_response(&huge_invalid_json))
            } else {
                ResponseTemplate::new(200).set_body_json(ollama_chat_response(&valid_repaired_json))
            }
        })
        .mount(&mock_server)
        .await;

    let (_dir, store) = make_test_settings_store(AiSettings {
        enabled: true,
        provider: Some(AiProvider::Ollama),
        base_url: Some(mock_server.uri()),
        model: Some("qwen3.5:9b".to_string()),
        repository_analysis_context_tokens: None, // 14336 context -> 8192 target
    });
    let secrets = InMemorySecretStore::new();
    let limiter = TranslationLimiter::new();
    let pack = create_oversized_synthetic_evidence_pack();

    let env = execute_repository_analysis(
        &pack,
        AnalysisLocale::En,
        &store,
        &secrets,
        &limiter,
        None,
        AnalysisExecutionOptions::default(),
    )
    .await
    .expect("Repair request must succeed when bounded");

    // Must make exactly 2 requests (1 original + 1 repair), NEVER 3!
    assert_eq!(call_count.load(Ordering::SeqCst), 2, "Must make exactly 2 requests");
    assert_eq!(env.schema_version, 1);
}

// ============================================================
// 27. TEST — TRANSLATION REGRESSION (No num_ctx or schema)
// ============================================================
#[tokio::test]
async fn test_section_27_translation_regression() {
    let mock_server = MockServer::start().await;

    // Verify Ollama receives translation request without num_ctx and without format
    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(move |req: &wiremock::Request| {
            let body_str = String::from_utf8_lossy(&req.body);
            assert!(!body_str.contains("num_ctx"), "Translation must NOT contain num_ctx: {}", body_str);
            assert!(!body_str.contains("\"format\":"), "Translation must NOT contain format schema: {}", body_str);
            ResponseTemplate::new(200).set_body_json(ollama_chat_response(
                r#"{"action":"translated","target_locale":"en","translated_text":"Translated desc"}"#,
            ))
        })
        .mount(&mock_server)
        .await;

    let client = reposhelf_lib::ai::OllamaClient::new(
        Some(&mock_server.uri()),
        "llama3.2",
        Duration::from_secs(10),
    )
    .unwrap();

    let res = translate_description(&client, "中文描述", "en", "ollama", "llama3.2").await;

    assert!(res.is_ok());
    assert_eq!(res.unwrap().translated_text, Some("Translated desc".to_string()));
}

// ============================================================
// 28. TEST — CLOUD PROVIDER REGRESSION
// ============================================================
#[tokio::test]
async fn test_section_28_cloud_provider_regression() {
    let mock_server = MockServer::start().await;
    let valid_json = make_valid_8_section_json("OpenAI overview", "OpenAI features");

    // OpenAI format: receives /v1/chat/completions, NO num_ctx, NO Ollama format
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(move |req: &wiremock::Request| {
            let body_str = String::from_utf8_lossy(&req.body);
            assert!(!body_str.contains("num_ctx"), "OpenAI must NOT receive num_ctx: {}", body_str);
            assert!(!body_str.contains("\"format\":"), "OpenAI must NOT receive format option: {}", body_str);
            ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "choices": [{
                    "message": { "content": valid_json }
                }]
            }))
        })
        .mount(&mock_server)
        .await;

    let (_dir, store) = make_test_settings_store(AiSettings {
        enabled: true,
        provider: Some(AiProvider::OpenaiCompatible),
        base_url: Some(format!("{}/v1", mock_server.uri())),
        model: Some("gpt-4o-mini".to_string()),
        repository_analysis_context_tokens: Some(32768),
    });
    let secrets = InMemorySecretStore::new();
    secrets.set_secret(SECRET_KEY_AI_API_KEY, "test-key").unwrap();
    let limiter = TranslationLimiter::new();
    let pack = create_oversized_synthetic_evidence_pack();

    let env = execute_repository_analysis(
        &pack,
        AnalysisLocale::En,
        &store,
        &secrets,
        &limiter,
        None,
        AnalysisExecutionOptions::default(),
    )
    .await
    .unwrap();

    assert_eq!(env.generator.provider, "openai_compatible");
}

// ============================================================
// 29. TEST — DETERMINISM
// ============================================================
#[test]
fn test_section_29_determinism() {
    let pack = create_oversized_synthetic_evidence_pack();

    let run1 = pack_evidence_for_model(&pack, AnalysisLocale::En, 8192).unwrap();
    let run2 = pack_evidence_for_model(&pack, AnalysisLocale::En, 8192).unwrap();

    assert_eq!(run1.estimated_input_tokens, run2.estimated_input_tokens);
    assert_eq!(run1.user_prompt, run2.user_prompt);
    assert_eq!(run1.sent_file_count, run2.sent_file_count);
    assert_eq!(run1.sent_evidence_records.len(), run2.sent_evidence_records.len());

    for (r1, r2) in run1.sent_evidence_records.iter().zip(&run2.sent_evidence_records) {
        assert_eq!(r1.id, r2.id);
        assert_eq!(r1.included_ranges, r2.included_ranges);
        assert_eq!(r1.included_content_hash, r2.included_content_hash);
        assert_eq!(r1.truncated, r2.truncated);
    }
}

// ============================================================
// 30. TEST — UTF-8 / CJK / CODE SAFETY
// ============================================================
#[test]
fn test_section_30_utf8_cjk_code_truncation_safety() {
    let mut mixed_lines = Vec::new();
    // Mix ASCII prose, CJK, Rust/JSON code, Unicode punctuation
    for i in 1..=500 {
        mixed_lines.push(format!("第{}行：Rust 结构体 struct AnalysisEnvelope {{ 字段: true }} —— “引文”", i));
        mixed_lines.push(format!("Line {}: 日本語のテストと한국어 테스트 mixed code snippets", i));
    }
    let mixed_text = mixed_lines.join("\n");

    let mut pack = create_oversized_synthetic_evidence_pack();
    pack.selected_texts[0].text_content = mixed_text;
    pack.selected_texts[0].included_ranges = vec![EvidenceRange { start_line: 1, end_line: 1000 }];

    let packed = pack_evidence_for_model(&pack, AnalysisLocale::ZhHans, 8192).unwrap();

    // Verify UTF-8 integrity of packed prompt
    assert!(std::str::from_utf8(packed.user_prompt.as_bytes()).is_ok(), "User prompt must be valid UTF-8");
    assert!(packed.estimated_input_tokens <= 8192);

    let truncated_record = packed.sent_evidence_records.iter().find(|e| e.id == "E003").unwrap();
    assert!(truncated_record.truncated);
    for r in &truncated_record.included_ranges {
        assert!(r.start_line <= r.end_line);
        assert!(r.start_line >= 1);
    }
}
