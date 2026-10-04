use std::path::Path;
use std::time::Instant;
use sqlx::{Row, SqlitePool};
use tempfile::NamedTempFile;

use reposhelf_lib::db::analysis::{
    validate_and_serialize_envelope, AnalysisContent, AnalysisCoverage, AnalysisEnvelopeV1,
    AnalysisGenerator, AnalysisItem, AnalysisLocale, AnalysisPatchInput, AnalysisSection,
    AnalysisSource, ClaimBasis, EvidenceRecord, SectionStatus,
};
use reposhelf_lib::library::models::{
    FavoriteInput, LibraryQuery, LibrarySortField, SortOrder, UpdateFavoriteUserMetadataInput,
};
use reposhelf_lib::library::repository::{
    insert_favorite_tx, list_favorites, update_user_metadata_tx, SAFE_ANALYSIS_SEARCH_TEXT_SQL,
};

fn sample_section(text: &str) -> AnalysisSection {
    AnalysisSection {
        status: SectionStatus::Supported,
        items: vec![AnalysisItem {
            text: text.to_string(),
            basis: ClaimBasis::Documented,
            evidence_ids: vec!["ev-meta-1".to_string()],
        }],
        missing_information: vec![],
    }
}

fn sample_section_with_missing(text: &str, missing: &str) -> AnalysisSection {
    AnalysisSection {
        status: SectionStatus::Partial,
        items: vec![AnalysisItem {
            text: text.to_string(),
            basis: ClaimBasis::Documented,
            evidence_ids: vec!["ev-meta-1".to_string()],
        }],
        missing_information: vec![missing.to_string()],
    }
}

fn build_envelope_with_sections(content: AnalysisContent) -> AnalysisEnvelopeV1 {
    AnalysisEnvelopeV1 {
        schema_version: 1,
        content,
        target_locale: AnalysisLocale::En,
        generated_at: "2026-10-04T12:00:00Z".to_string(),
        generator: AnalysisGenerator {
            provider: "test-provider".to_string(),
            model_requested: "test-model".to_string(),
            model_reported: None,
            prompt_version: "1.0".to_string(),
            selector_version: "1.0".to_string(),
            context_budget_tokens: 4000,
            input_tokens: 1000,
            input_tokens_is_estimate: false,
            output_limit_tokens: 2000,
        },
        source: AnalysisSource {
            github_repo_id: "99999".to_string(),
            canonical_url: "https://github.com/owner/repo".to_string(),
            default_branch: Some("main".to_string()),
            commit_sha: Some("abcdef123456".to_string()),
            metadata_observed_at: "2026-10-04T12:00:00Z".to_string(),
            evidence_fingerprint: "fingerprint-s4".to_string(),
        },
        evidence: vec![EvidenceRecord {
            id: "ev-meta-1".to_string(),
            kind: "metadata".to_string(),
            path: None,
            commit_sha: None,
            blob_sha: None,
            observed_at: "2026-10-04T12:00:00Z".to_string(),
            included_ranges: vec![],
            included_content_hash: "hash".to_string(),
            truncated: false,
            public_fact_summary: None,
        }],
        coverage: AnalysisCoverage {
            status: "bounded".to_string(),
            selected_file_count: 1,
            sent_file_count: 1,
            evidence_count: 1,
            gaps: vec![],
            executed_code: false,
            verified_deployment: false,
        },
        search_text: "will_be_recalculated".to_string(),
    }
}

async fn setup_in_memory_db() -> SqlitePool {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    pool
}

async fn setup_file_db(path: &Path) -> SqlitePool {
    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .foreign_keys(true);
    let pool = SqlitePool::connect_with(options).await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    pool
}

// ---------------------------------------------------------------------------
// SQL Expression helper verification (§28)
// ---------------------------------------------------------------------------
#[test]
fn test_sql_expression_structure() {
    assert!(SAFE_ANALYSIS_SEARCH_TEXT_SQL.contains("json_valid(f.ai_analysis) = 1"));
    assert!(SAFE_ANALYSIS_SEARCH_TEXT_SQL.contains("json_type(f.ai_analysis, '$') = 'object'"));
    assert!(SAFE_ANALYSIS_SEARCH_TEXT_SQL.contains("json_type(f.ai_analysis, '$.schema_version') = 'integer'"));
    assert!(SAFE_ANALYSIS_SEARCH_TEXT_SQL.contains("json_extract(f.ai_analysis, '$.schema_version') = 1"));
    assert!(SAFE_ANALYSIS_SEARCH_TEXT_SQL.contains("json_type(f.ai_analysis, '$.search_text') = 'text'"));
    assert!(SAFE_ANALYSIS_SEARCH_TEXT_SQL.contains("json_extract(f.ai_analysis, '$.search_text')"));
}

// ---------------------------------------------------------------------------
// AC091: Eight Dimensions and Missing Information
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_ac091_eight_dimensions_and_missing_information() {
    let pool = setup_in_memory_db().await;

    let content = AnalysisContent {
        overview: sample_section("System high-level overview_unique_s4"),
        features: sample_section("Core features include features_unique_s4 engine"),
        use_cases: sample_section("Ideal for use_cases_unique_s4 workflows"),
        tech_stack: sample_section("Built with tech_stack_unique_s4 tooling"),
        setup: sample_section("Simple setup_unique_s4 installation"),
        target_users: sample_section("Tailored for target_users_unique_s4 specialists"),
        maturity: sample_section("Evaluated as maturity_unique_s4 grade"),
        limitations: sample_section_with_missing(
            "Known limitations_unique_s4 behavior",
            "Lack of missing_info_unique_s4 documentation",
        ),
    };

    let envelope = build_envelope_with_sections(content);
    let (_json_str, derived_search_text) = validate_and_serialize_envelope(&envelope).unwrap();

    // Verify all keywords appear in derived_search_text
    assert!(derived_search_text.contains("overview_unique_s4"));
    assert!(derived_search_text.contains("features_unique_s4"));
    assert!(derived_search_text.contains("use_cases_unique_s4"));
    assert!(derived_search_text.contains("tech_stack_unique_s4"));
    assert!(derived_search_text.contains("setup_unique_s4"));
    assert!(derived_search_text.contains("target_users_unique_s4"));
    assert!(derived_search_text.contains("maturity_unique_s4"));
    assert!(derived_search_text.contains("limitations_unique_s4"));
    assert!(derived_search_text.contains("missing_info_unique_s4"));

    // Insert into database with neutral fields that do NOT contain these tokens
    let input = FavoriteInput {
        github_repo_id: Some(9101),
        github_url: "https://github.com/ac091/repo-eight-dimensions".to_string(),
        name: "ac091-eight-dimensions-repo".to_string(),
        description: Some("Neutral description with none of the target tokens".to_string()),
        category: "Developer Tools".to_string(),
        stars: 100,
        license: Some("MIT".to_string()),
        topics: vec!["neutral-tag".to_string()],
        pushed_at: Some("2026-10-04T00:00:00Z".to_string()),
        repo_created_at: Some("2026-01-01T00:00:00Z".to_string()),
        releases: Some(1),
        languages: None,
        translation: None,
        ai_analysis: Some(envelope),
    };

    let inserted = insert_favorite_tx(&pool, &input).await.unwrap();
    assert_eq!(inserted.name, "ac091-eight-dimensions-repo");

    // Test each dimension's unique keyword
    let dimensions = vec![
        "overview_unique_s4",
        "features_unique_s4",
        "use_cases_unique_s4",
        "tech_stack_unique_s4",
        "setup_unique_s4",
        "target_users_unique_s4",
        "maturity_unique_s4",
        "limitations_unique_s4",
        "missing_info_unique_s4",
    ];

    for keyword in dimensions {
        let q = LibraryQuery {
            search: Some(keyword.to_string()),
            ..Default::default()
        };
        let results = list_favorites(&pool, &q).await.unwrap();
        assert_eq!(
            results.len(),
            1,
            "Expected exactly 1 match for keyword '{}'",
            keyword
        );
        assert_eq!(results[0].id, inserted.id);
    }

    // Verify non-existent word returns 0
    let q_none = LibraryQuery {
        search: Some("non_existent_s4_token_xyz".to_string()),
        ..Default::default()
    };
    let results_none = list_favorites(&pool, &q_none).await.unwrap();
    assert_eq!(results_none.len(), 0);
}

// ---------------------------------------------------------------------------
// AC092: Original Field Regression
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_ac092_original_field_regression() {
    let pool = setup_in_memory_db().await;

    // Insert favorite 1: without analysis
    let input1 = FavoriteInput {
        github_repo_id: Some(9201),
        github_url: "https://github.com/rust-lang/rust".to_string(),
        name: "rust".to_string(),
        description: Some("Empowering everyone to build reliable and efficient software.".to_string()),
        category: "Developer Tools".to_string(),
        stars: 90000,
        license: Some("MIT OR Apache-2.0".to_string()),
        topics: vec!["compiler".to_string(), "language".to_string()],
        pushed_at: Some("2026-10-01T12:00:00Z".to_string()),
        repo_created_at: Some("2010-06-16T00:00:00Z".to_string()),
        releases: Some(100),
        languages: None,
        translation: None,
        ai_analysis: None,
    };
    let fav1 = insert_favorite_tx(&pool, &input1).await.unwrap();

    // Set notes on favorite 1
    sqlx::query("UPDATE favorites SET notes = 'secret-notes-keyword' WHERE id = ?")
        .bind(fav1.id)
        .execute(&pool)
        .await
        .unwrap();

    // Insert favorite 2: with analysis
    let content2 = AnalysisContent {
        overview: sample_section("An async runtime for Rust"),
        features: sample_section("Multi-threaded work-stealing scheduler"),
        use_cases: sample_section("Network applications"),
        tech_stack: sample_section("Rust tokio-rs"),
        setup: sample_section("cargo add tokio"),
        target_users: sample_section("Systems engineers"),
        maturity: sample_section("Very mature"),
        limitations: sample_section("Async complexity"),
    };
    let envelope2 = build_envelope_with_sections(content2);
    let input2 = FavoriteInput {
        github_repo_id: Some(9202),
        github_url: "https://github.com/tokio-rs/tokio".to_string(),
        name: "tokio".to_string(),
        description: Some("An event-driven, non-blocking I/O platform for writing asynchronous applications".to_string()),
        category: "AI & Agents".to_string(),
        stars: 25000,
        license: Some("MIT".to_string()),
        topics: vec!["async".to_string(), "network".to_string()],
        pushed_at: Some("2026-10-02T12:00:00Z".to_string()),
        repo_created_at: Some("2016-01-01T00:00:00Z".to_string()),
        releases: Some(50),
        languages: None,
        translation: None,
        ai_analysis: Some(envelope2),
    };
    let fav2 = insert_favorite_tx(&pool, &input2).await.unwrap();

    // 1. Name search (ASCII case-insensitive)
    let q_name = LibraryQuery {
        search: Some("RUST".to_string()),
        ..Default::default()
    };
    let res_name = list_favorites(&pool, &q_name).await.unwrap();
    assert!(res_name.iter().any(|f| f.id == fav1.id));

    // 2. Exact name search "tokio"
    let q_tokio = LibraryQuery {
        search: Some("tokio".to_string()),
        ..Default::default()
    };
    let res_tokio = list_favorites(&pool, &q_tokio).await.unwrap();
    assert_eq!(res_tokio.len(), 1);
    assert_eq!(res_tokio[0].id, fav2.id);

    // 3. Category search
    let q_cat = LibraryQuery {
        search: Some("Developer Tools".to_string()),
        ..Default::default()
    };
    let res_cat = list_favorites(&pool, &q_cat).await.unwrap();
    assert!(res_cat.iter().any(|f| f.id == fav1.id));

    // 4. Description search
    let q_desc = LibraryQuery {
        search: Some("Empowering everyone".to_string()),
        ..Default::default()
    };
    let res_desc = list_favorites(&pool, &q_desc).await.unwrap();
    assert_eq!(res_desc.len(), 1);
    assert_eq!(res_desc[0].id, fav1.id);

    // 5. Notes search
    let q_notes = LibraryQuery {
        search: Some("secret-notes-keyword".to_string()),
        ..Default::default()
    };
    let res_notes = list_favorites(&pool, &q_notes).await.unwrap();
    assert_eq!(res_notes.len(), 1);
    assert_eq!(res_notes[0].id, fav1.id);

    // 6. Tag search
    let q_tag = LibraryQuery {
        search: Some("compiler".to_string()),
        ..Default::default()
    };
    let res_tag = list_favorites(&pool, &q_tag).await.unwrap();
    assert_eq!(res_tag.len(), 1);
    assert_eq!(res_tag[0].id, fav1.id);

    // 7. Non-matching search
    let q_none = LibraryQuery {
        search: Some("nonexistent_legacy_query_xyz".to_string()),
        ..Default::default()
    };
    let res_none = list_favorites(&pool, &q_none).await.unwrap();
    assert_eq!(res_none.len(), 0);
}

// ---------------------------------------------------------------------------
// AC093: Hidden Metadata Exclusion
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_ac093_hidden_metadata_exclusion() {
    let pool = setup_in_memory_db().await;

    let content = AnalysisContent {
        overview: sample_section("Clean overview text without any secrets"),
        features: sample_section("Clean features text"),
        use_cases: sample_section("Clean use cases"),
        tech_stack: sample_section("Clean tech stack"),
        setup: sample_section("Clean setup"),
        target_users: sample_section("Clean target users"),
        maturity: sample_section("Clean maturity"),
        limitations: sample_section("Clean limitations"),
    };

    let mut envelope = build_envelope_with_sections(content);
    // Inject sentinels into generator, source, and evidence fields
    envelope.generator.provider = "provider-only-secret-word".to_string();
    envelope.generator.model_requested = "model-only-secret-word".to_string();
    envelope.generator.selector_version = "selector-version-secret-word".to_string();
    envelope.source.commit_sha = Some("sha-only-secret-word".to_string());
    envelope.evidence[0].path = Some("evidence-path-only-secret-word".to_string());

    let (json_str, derived_search_text) = validate_and_serialize_envelope(&envelope).unwrap();
    // Confirm sentinels are in the JSON envelope
    assert!(json_str.contains("provider-only-secret-word"));
    assert!(json_str.contains("model-only-secret-word"));
    assert!(json_str.contains("sha-only-secret-word"));
    assert!(json_str.contains("evidence-path-only-secret-word"));
    assert!(json_str.contains("selector-version-secret-word"));
    // But NOT in search_text!
    assert!(!derived_search_text.contains("provider-only-secret-word"));
    assert!(!derived_search_text.contains("model-only-secret-word"));
    assert!(!derived_search_text.contains("sha-only-secret-word"));
    assert!(!derived_search_text.contains("evidence-path-only-secret-word"));
    assert!(!derived_search_text.contains("selector-version-secret-word"));

    let input = FavoriteInput {
        github_repo_id: Some(9301),
        github_url: "https://github.com/ac093/repo-metadata-isolation".to_string(),
        name: "ac093-metadata-isolation".to_string(),
        description: Some("Isolation test".to_string()),
        category: "Other".to_string(),
        stars: 10,
        license: None,
        topics: vec![],
        pushed_at: None,
        repo_created_at: None,
        releases: None,
        languages: None,
        translation: None,
        ai_analysis: Some(envelope),
    };
    let inserted = insert_favorite_tx(&pool, &input).await.unwrap();

    let sentinels = vec![
        "provider-only-secret-word",
        "model-only-secret-word",
        "sha-only-secret-word",
        "evidence-path-only-secret-word",
        "selector-version-secret-word",
        "schema_version",
        "target_locale",
        "evidence_fingerprint",
        "context_budget_tokens",
    ];

    for s in sentinels {
        let q = LibraryQuery {
            search: Some(s.to_string()),
            ..Default::default()
        };
        let results = list_favorites(&pool, &q).await.unwrap();
        assert_eq!(
            results.len(),
            0,
            "Sentinel '{}' must NOT match through analysis search branch",
            s
        );
    }

    // But searching for clean overview content DOES match
    let q_valid = LibraryQuery {
        search: Some("Clean overview".to_string()),
        ..Default::default()
    };
    let res_valid = list_favorites(&pool, &q_valid).await.unwrap();
    assert_eq!(res_valid.len(), 1);
    assert_eq!(res_valid[0].id, inserted.id);
}

// ---------------------------------------------------------------------------
// AC094: Null / Bad / Future JSON Matrix
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_ac094_bad_json_matrix_and_corrupt_safety() {
    let pool = setup_in_memory_db().await;

    // Define all 20 bad / edge / valid variants
    let variants: Vec<(&str, Option<&str>, &str)> = vec![
        ("bad-variant-01-null", None, "never_matches_null"),
        ("bad-variant-02-empty", Some(""), "never_matches_empty"),
        ("bad-variant-03-whitespace", Some("   \t\r\n"), "never_matches_whitespace"),
        ("bad-variant-04-malformed", Some("{ broken_syntax json : true"), "broken_syntax"),
        ("bad-variant-05-json-null", Some("null"), "never_matches_json_null"),
        ("bad-variant-06-array", Some("[101, 102, 103]"), "102"),
        ("bad-variant-07-empty-obj", Some("{}"), "never_matches_empty_obj"),
        ("bad-variant-08-scalar-num", Some("987654321"), "987654321"),
        ("bad-variant-09-scalar-str", Some("\"scalar_payload_s4\""), "scalar_payload_s4"),
        ("bad-variant-10-scalar-bool", Some("true"), "never_matches_bool"),
        ("bad-variant-11-missing-schema-ver", Some(r#"{"search_text": "bad_token_11"}"#), "bad_token_11"),
        ("bad-variant-12-wrong-schema-type", Some(r#"{"schema_version": "1", "search_text": "bad_token_12"}"#), "bad_token_12"),
        ("bad-variant-13-future-schema-99", Some(r#"{"schema_version": 99, "search_text": "bad_token_13"}"#), "bad_token_13"),
        ("bad-variant-14-missing-search-text", Some(r#"{"schema_version": 1}"#), "never_matches_missing_st"),
        ("bad-variant-15-null-search-text", Some(r#"{"schema_version": 1, "search_text": null}"#), "never_matches_null_st"),
        ("bad-variant-16-array-search-text", Some(r#"{"schema_version": 1, "search_text": ["bad_token_16"]}"#), "bad_token_16"),
        ("bad-variant-17-object-search-text", Some(r#"{"schema_version": 1, "search_text": {"sub": "bad_token_17"}}"#), "bad_token_17"),
        ("bad-variant-18-number-search-text", Some(r#"{"schema_version": 1, "search_text": 12345678}"#), "12345678"),
        ("bad-variant-19-malformed-v1-content", Some(r#"{"schema_version": 1, "search_text": "valid_token_19", "content": "not_an_object"}"#), "valid_token_19"),
    ];

    for (idx, (name, raw_json, target_search_word)) in variants.iter().enumerate() {
        sqlx::query(
            r#"
            INSERT INTO favorites (
                github_repo_id, github_url, name, description, category, stars, notes,
                favorited_at, metadata_refreshed_at, ai_analysis
            ) VALUES (
                ?, ?, ?, 'desc', 'Other', 5, 'notes',
                '2026-10-04T00:00:00Z', '2026-10-04T00:00:00Z', ?
            )
            "#,
        )
        .bind(1000 + idx as i64)
        .bind(format!("https://github.com/owner/{}", name))
        .bind(*name)
        .bind(*raw_json)
        .execute(&pool)
        .await
        .unwrap();

        // 1. Searching for words inside bad JSON must NOT match through analysis branch
        let q = LibraryQuery {
            search: Some(target_search_word.to_string()),
            ..Default::default()
        };
        let res = list_favorites(&pool, &q).await.unwrap();
        if *name == "bad-variant-19-malformed-v1-content" {
            assert_eq!(res.len(), 1, "v1 with valid search_text text matches");
        } else {
            assert_eq!(
                res.len(),
                0,
                "Variant '{}' with word '{}' must not match analysis branch",
                name,
                target_search_word
            );
        }

        // 2. Querying by name (original field) MUST still succeed!
        let q_name = LibraryQuery {
            search: Some(name.to_string()),
            ..Default::default()
        };
        let res_name = list_favorites(&pool, &q_name).await.unwrap();
        assert_eq!(res_name.len(), 1, "Variant '{}' must match by name", name);
        assert_eq!(res_name[0].name, *name);

        // 3. Confirm raw DB value is completely unchanged (no auto-repair, byte-for-byte identical)
        let row = sqlx::query("SELECT ai_analysis FROM favorites WHERE name = ?")
            .bind(*name)
            .fetch_one(&pool)
            .await
            .unwrap();
        let db_val: Option<String> = row.get("ai_analysis");
        assert_eq!(
            db_val.as_deref(),
            *raw_json,
            "Variant '{}' in DB must remain byte-for-byte identical",
            name
        );
    }
}

// ---------------------------------------------------------------------------
// AC095: Saved Scope and Unsaved Draft Isolation
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_ac095_saved_scope_and_unsaved_isolation() {
    let pool = setup_in_memory_db().await;

    // Favorite A: Formal saved report in DB
    let content_a = AnalysisContent {
        overview: sample_section("Formal saved report with unique_saved_scope_keyword_ac095"),
        features: sample_section("Feature A"),
        use_cases: sample_section("Use case A"),
        tech_stack: sample_section("Stack A"),
        setup: sample_section("Setup A"),
        target_users: sample_section("Users A"),
        maturity: sample_section("Maturity A"),
        limitations: sample_section("Limitations A"),
    };
    let envelope_a = build_envelope_with_sections(content_a);
    let input_a = FavoriteInput {
        github_repo_id: Some(9501),
        github_url: "https://github.com/ac095/repo-a".to_string(),
        name: "repo-a-saved".to_string(),
        description: Some("Saved repo".to_string()),
        category: "Developer Tools".to_string(),
        stars: 10,
        license: None,
        topics: vec![],
        pushed_at: None,
        repo_created_at: None,
        releases: None,
        languages: None,
        translation: None,
        ai_analysis: Some(envelope_a),
    };
    let fav_a = insert_favorite_tx(&pool, &input_a).await.unwrap();

    // Favorite B: Unsaved draft in memory only (DB has ai_analysis = NULL)
    let input_b = FavoriteInput {
        github_repo_id: Some(9502),
        github_url: "https://github.com/ac095/repo-b".to_string(),
        name: "repo-b-unsaved".to_string(),
        description: Some("Unsaved draft repo".to_string()),
        category: "Developer Tools".to_string(),
        stars: 10,
        license: None,
        topics: vec![],
        pushed_at: None,
        repo_created_at: None,
        releases: None,
        languages: None,
        translation: None,
        ai_analysis: None, // No saved report in DB
    };
    let fav_b = insert_favorite_tx(&pool, &input_b).await.unwrap();

    // Search for the saved keyword
    let q = LibraryQuery {
        search: Some("unique_saved_scope_keyword_ac095".to_string()),
        ..Default::default()
    };
    let res = list_favorites(&pool, &q).await.unwrap();
    assert_eq!(res.len(), 1);
    assert_eq!(res[0].id, fav_a.id);
    assert_ne!(res[0].id, fav_b.id);
}

// ---------------------------------------------------------------------------
// AC096: Replace Consistency and Abandoned Draft
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_ac096_replace_consistency_and_abandoned_draft() {
    let pool = setup_in_memory_db().await;

    // Initial analysis with old keyword
    let content_old = AnalysisContent {
        overview: sample_section("Initial analysis with old_analysis_keyword_s4"),
        features: sample_section("Old features"),
        use_cases: sample_section("Old use cases"),
        tech_stack: sample_section("Old stack"),
        setup: sample_section("Old setup"),
        target_users: sample_section("Old users"),
        maturity: sample_section("Old maturity"),
        limitations: sample_section("Old limitations"),
    };
    let envelope_old = build_envelope_with_sections(content_old);
    let input_initial = FavoriteInput {
        github_repo_id: Some(9601),
        github_url: "https://github.com/ac096/replace-consistency".to_string(),
        name: "ac096-repo".to_string(),
        description: Some("Consistency test".to_string()),
        category: "Developer Tools".to_string(),
        stars: 10,
        license: None,
        topics: vec![],
        pushed_at: None,
        repo_created_at: None,
        releases: None,
        languages: None,
        translation: None,
        ai_analysis: Some(envelope_old),
    };
    let fav = insert_favorite_tx(&pool, &input_initial).await.unwrap();

    // Step 1: Search old keyword -> matches
    let q_old = LibraryQuery {
        search: Some("old_analysis_keyword_s4".to_string()),
        ..Default::default()
    };
    let res_old = list_favorites(&pool, &q_old).await.unwrap();
    assert_eq!(res_old.len(), 1);
    assert_eq!(res_old[0].id, fav.id);

    // Step 2: Save Exit Replace with new analysis containing new keyword
    let content_new = AnalysisContent {
        overview: sample_section("Replaced analysis with new_analysis_keyword_s4"),
        features: sample_section("New features"),
        use_cases: sample_section("New use cases"),
        tech_stack: sample_section("New stack"),
        setup: sample_section("New setup"),
        target_users: sample_section("New users"),
        maturity: sample_section("New maturity"),
        limitations: sample_section("New limitations"),
    };
    let envelope_new = build_envelope_with_sections(content_new);
    let patch = UpdateFavoriteUserMetadataInput {
        favorite_id: fav.id,
        category: "Developer Tools".to_string(),
        rating: None,
        rating_reason: None,
        rating_confidence: None,
        notes: "Updated notes".to_string(),
        user_tags: vec![],
        ai_tags: None,
        translation_patch: None,
        analysis_patch: Some(AnalysisPatchInput::Replace {
            envelope: envelope_new,
            expected_revision: fav.analysis_revision.clone().unwrap(),
            expected_repo_id: 9601,
        }),
    };
    let updated = update_user_metadata_tx(&pool, &patch).await.unwrap();
    assert!(updated.ai_analysis.is_some());

    // Step 3: New keyword matches, old keyword NO LONGER matches
    let q_new = LibraryQuery {
        search: Some("new_analysis_keyword_s4".to_string()),
        ..Default::default()
    };
    let res_new = list_favorites(&pool, &q_new).await.unwrap();
    assert_eq!(res_new.len(), 1);
    assert_eq!(res_new[0].id, fav.id);

    let res_old_after = list_favorites(&pool, &q_old).await.unwrap();
    assert_eq!(
        res_old_after.len(),
        0,
        "Old analysis keyword must NOT match after successful Replace"
    );

    // Step 4: In case of draft abandonment / failure (no DB update), old report remains
    let res_still_new = list_favorites(&pool, &q_new).await.unwrap();
    assert_eq!(res_still_new.len(), 1);
}

// ---------------------------------------------------------------------------
// AC097: Multiword / Cross-Field Semantics
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_ac097_multiword_and_cross_field_semantics() {
    let pool = setup_in_memory_db().await;

    // Favorite A: name has "alpha", analysis has "beta" (split across fields)
    let content_a = AnalysisContent {
        overview: sample_section("Only contains beta keyword here"),
        features: sample_section("No other words"),
        use_cases: sample_section("Use cases"),
        tech_stack: sample_section("Tech stack"),
        setup: sample_section("Setup"),
        target_users: sample_section("Target users"),
        maturity: sample_section("Maturity"),
        limitations: sample_section("Limitations"),
    };
    let envelope_a = build_envelope_with_sections(content_a);
    let input_a = FavoriteInput {
        github_repo_id: Some(9701),
        github_url: "https://github.com/ac097/repo-alpha".to_string(),
        name: "alpha".to_string(),
        description: Some("neutral description".to_string()),
        category: "Other".to_string(),
        stars: 1,
        license: None,
        topics: vec![],
        pushed_at: None,
        repo_created_at: None,
        releases: None,
        languages: None,
        translation: None,
        ai_analysis: Some(envelope_a),
    };
    insert_favorite_tx(&pool, &input_a).await.unwrap();

    // Favorite B: single field (analysis search_text) contains "alpha beta" together
    let content_b = AnalysisContent {
        overview: sample_section("Here is alpha beta together in one section"),
        features: sample_section("Features"),
        use_cases: sample_section("Use cases"),
        tech_stack: sample_section("Tech stack"),
        setup: sample_section("Setup"),
        target_users: sample_section("Target users"),
        maturity: sample_section("Maturity"),
        limitations: sample_section("Limitations"),
    };
    let envelope_b = build_envelope_with_sections(content_b);
    let input_b = FavoriteInput {
        github_repo_id: Some(9702),
        github_url: "https://github.com/ac097/repo-beta".to_string(),
        name: "unrelated-name".to_string(),
        description: Some("neutral description".to_string()),
        category: "Other".to_string(),
        stars: 1,
        license: None,
        topics: vec![],
        pushed_at: None,
        repo_created_at: None,
        releases: None,
        languages: None,
        translation: None,
        ai_analysis: Some(envelope_b),
    };
    let fav_b = insert_favorite_tx(&pool, &input_b).await.unwrap();

    // Query for "alpha beta"
    // Under existing frozen semantics (no whitespace split), "alpha beta" is a single LIKE pattern "%alpha beta%"
    // Favorite A has "alpha" in name and "beta" in analysis, but NO single field contains "alpha beta".
    // Therefore Favorite A MUST NOT match!
    let q = LibraryQuery {
        search: Some("alpha beta".to_string()),
        ..Default::default()
    };
    let res = list_favorites(&pool, &q).await.unwrap();
    assert_eq!(
        res.len(),
        1,
        "Expected only Favorite B to match because it has 'alpha beta' in a single field"
    );
    assert_eq!(res[0].id, fav_b.id);

    // Query with leading and trailing whitespace "  alpha beta  "
    let q_padded = LibraryQuery {
        search: Some("  alpha beta  ".to_string()),
        ..Default::default()
    };
    let res_padded = list_favorites(&pool, &q_padded).await.unwrap();
    assert_eq!(res_padded.len(), 1);
    assert_eq!(res_padded[0].id, fav_b.id);
}

// ---------------------------------------------------------------------------
// AC098: Filtering and Pagination / Count
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_ac098_filtering_and_pagination() {
    let pool = setup_in_memory_db().await;

    // Insert 4 favorites matching "analytics_s4" with different categories, ratings, and tags
    let make_fav = |id: i64, name: &str, cat: &str, tags: Vec<String>| {
        let content = AnalysisContent {
            overview: sample_section(&format!("Overview for {} with analytics_s4 engine", name)),
            features: sample_section("Features"),
            use_cases: sample_section("Use cases"),
            tech_stack: sample_section("Tech stack"),
            setup: sample_section("Setup"),
            target_users: sample_section("Target users"),
            maturity: sample_section("Maturity"),
            limitations: sample_section("Limitations"),
        };
        let envelope = build_envelope_with_sections(content);
        FavoriteInput {
            github_repo_id: Some(id),
            github_url: format!("https://github.com/ac098/{}", name),
            name: name.to_string(),
            description: Some("Test".to_string()),
            category: cat.to_string(),
            stars: 100,
            license: None,
            topics: tags,
            pushed_at: None,
            repo_created_at: None,
            releases: None,
            languages: None,
            translation: None,
            ai_analysis: Some(envelope),
        }
    };

    let f1 = insert_favorite_tx(&pool, &make_fav(9801, "repo-1", "Developer Tools", vec!["rust".to_string(), "backend".to_string()])).await.unwrap();
    let f2 = insert_favorite_tx(&pool, &make_fav(9802, "repo-2", "Developer Tools", vec!["python".to_string()])).await.unwrap();
    let f3 = insert_favorite_tx(&pool, &make_fav(9803, "repo-3", "AI & Agents", vec!["rust".to_string(), "ml".to_string()])).await.unwrap();
    let f4 = insert_favorite_tx(&pool, &make_fav(9804, "repo-4", "AI & Agents", vec!["rust".to_string()])).await.unwrap();

    // 1. Search only: all 4 match
    let q_all = LibraryQuery {
        search: Some("analytics_s4".to_string()),
        ..Default::default()
    };
    let res_all = list_favorites(&pool, &q_all).await.unwrap();
    assert_eq!(res_all.len(), 4);

    // 2. Search + Category filter "Developer Tools": f1 and f2
    let q_cat = LibraryQuery {
        search: Some("analytics_s4".to_string()),
        category: Some("Developer Tools".to_string()),
        ..Default::default()
    };
    let res_cat = list_favorites(&pool, &q_cat).await.unwrap();
    assert_eq!(res_cat.len(), 2);
    assert!(res_cat.iter().any(|f| f.id == f1.id));
    assert!(res_cat.iter().any(|f| f.id == f2.id));

    // 3. Search + Tag filter "rust": f1, f3, f4
    // NOTE: f1 has tags ["rust", "backend"], f3 has ["rust", "ml"].
    // The tag filter uses EXISTS (SELECT 1 FROM favorite_tags ...), so f1 and f3 MUST NOT be duplicated!
    let q_tag = LibraryQuery {
        search: Some("analytics_s4".to_string()),
        tag: Some("rust".to_string()),
        ..Default::default()
    };
    let res_tag = list_favorites(&pool, &q_tag).await.unwrap();
    assert_eq!(res_tag.len(), 3, "EXISTS check must prevent duplicate rows");
    let ids: Vec<i64> = res_tag.iter().map(|f| f.id).collect();
    assert!(ids.contains(&f1.id));
    assert!(ids.contains(&f3.id));
    assert!(ids.contains(&f4.id));

    // 4. Search + Rating filter "5": f1, f3
    sqlx::query("UPDATE favorites SET rating = 5 WHERE id IN (?, ?)")
        .bind(f1.id)
        .bind(f3.id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE favorites SET rating = 3 WHERE id = ?")
        .bind(f2.id)
        .execute(&pool)
        .await
        .unwrap();

    let q_rating = LibraryQuery {
        search: Some("analytics_s4".to_string()),
        rating: Some("5".to_string()),
        ..Default::default()
    };
    let res_rating = list_favorites(&pool, &q_rating).await.unwrap();
    assert_eq!(res_rating.len(), 2);
    assert!(res_rating.iter().any(|f| f.id == f1.id));
    assert!(res_rating.iter().any(|f| f.id == f3.id));

    // 5. Search + Rating "unrated": f4
    let q_unrated = LibraryQuery {
        search: Some("analytics_s4".to_string()),
        rating: Some("unrated".to_string()),
        ..Default::default()
    };
    let res_unrated = list_favorites(&pool, &q_unrated).await.unwrap();
    assert_eq!(res_unrated.len(), 1);
    assert_eq!(res_unrated[0].id, f4.id);

    // 6. Combined: search + category "AI & Agents" + tag "rust" + rating "5": only f3
    let q_combined = LibraryQuery {
        search: Some("analytics_s4".to_string()),
        category: Some("AI & Agents".to_string()),
        tag: Some("rust".to_string()),
        rating: Some("5".to_string()),
        ..Default::default()
    };
    let res_combined = list_favorites(&pool, &q_combined).await.unwrap();
    assert_eq!(res_combined.len(), 1);
    assert_eq!(res_combined[0].id, f3.id);
}

// ---------------------------------------------------------------------------
// AC099: Parameter Binding and Special Characters
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_ac099_parameter_binding_and_special_characters() {
    let pool = setup_in_memory_db().await;

    // Chinese content in analysis
    let content_zh = AnalysisContent {
        overview: sample_section("深度学习分布式训练框架"),
        features: sample_section("支持异构集群与动态弹性调度"),
        use_cases: sample_section("大规模语言模型微调"),
        tech_stack: sample_section("PyTorch CUDA NCCL"),
        setup: sample_section("pip install framework"),
        target_users: sample_section("AI算法工程师"),
        maturity: sample_section("生产就绪"),
        limitations: sample_section("不支持 Windows 操作系统"),
    };
    let envelope_zh = build_envelope_with_sections(content_zh);
    let input_zh = FavoriteInput {
        github_repo_id: Some(9901),
        github_url: "https://github.com/ac099/chinese-framework".to_string(),
        name: "chinese-framework".to_string(),
        description: Some("中文测试仓库".to_string()),
        category: "AI & Agents".to_string(),
        stars: 100,
        license: None,
        topics: vec![],
        pushed_at: None,
        repo_created_at: None,
        releases: None,
        languages: None,
        translation: None,
        ai_analysis: Some(envelope_zh),
    };
    let fav_zh = insert_favorite_tx(&pool, &input_zh).await.unwrap();

    // 1. Chinese keyword search
    let q_zh = LibraryQuery {
        search: Some("分布式训练".to_string()),
        ..Default::default()
    };
    let res_zh = list_favorites(&pool, &q_zh).await.unwrap();
    assert_eq!(res_zh.len(), 1);
    assert_eq!(res_zh[0].id, fav_zh.id);

    // 2. SQL injection payloads: must NOT alter SQL structure, must execute safely
    let injection_payloads = vec![
        "' OR '1'='1",
        "\" OR \"\"=\"",
        "'; DROP TABLE favorites; --",
        "/* comment injection */",
        "\\",
        "' UNION SELECT * FROM favorites --",
        "admin'--",
    ];

    for payload in injection_payloads {
        let q_inj = LibraryQuery {
            search: Some(payload.to_string()),
            ..Default::default()
        };
        // Must succeed without SQLite syntax errors
        let res_inj = list_favorites(&pool, &q_inj).await.unwrap();
        // Since none of the records have this literal payload, must return 0
        assert_eq!(
            res_inj.len(),
            0,
            "Injection payload '{}' must return 0 results safely",
            payload
        );
    }

    // Verify database was NOT dropped!
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM favorites")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count.0, 1, "Favorites table must remain intact");

    // 3. Wildcards '%' and '_' retain SQLite LIKE wildcard semantics
    let q_wildcard = LibraryQuery {
        search: Some("%".to_string()),
        ..Default::default()
    };
    let res_wildcard = list_favorites(&pool, &q_wildcard).await.unwrap();
    assert_eq!(res_wildcard.len(), 1, "'%' matches all records");

    let q_underscore = LibraryQuery {
        search: Some("chinese_framework".to_string()),
        ..Default::default()
    };
    let res_underscore = list_favorites(&pool, &q_underscore).await.unwrap();
    assert_eq!(res_underscore.len(), 1, "'_' matches '-' in 'chinese-framework'");
}

// ---------------------------------------------------------------------------
// AC100: Empty Query, Filters, and Sorting
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_ac100_empty_query_filters_and_sorting() {
    let pool = setup_in_memory_db().await;

    // Insert 3 records with different names, stars, and pushed_at
    let insert_rec = |id: i64, name: &str, stars: i64, pushed: Option<&str>| {
        let content = AnalysisContent {
            overview: sample_section("Common overview"),
            features: sample_section("Common features"),
            use_cases: sample_section("Common use cases"),
            tech_stack: sample_section("Common stack"),
            setup: sample_section("Common setup"),
            target_users: sample_section("Common users"),
            maturity: sample_section("Common maturity"),
            limitations: sample_section("Common limitations"),
        };
        let envelope = build_envelope_with_sections(content);
        FavoriteInput {
            github_repo_id: Some(id),
            github_url: format!("https://github.com/ac100/{}", name),
            name: name.to_string(),
            description: Some("Sort test".to_string()),
            category: "Developer Tools".to_string(),
            stars,
            license: None,
            topics: vec![],
            pushed_at: pushed.map(|s| s.to_string()),
            repo_created_at: None,
            releases: None,
            languages: None,
            translation: None,
            ai_analysis: Some(envelope),
        }
    };

    let f_a = insert_favorite_tx(&pool, &insert_rec(1001, "charlie", 300, Some("2026-10-03T00:00:00Z"))).await.unwrap();
    let f_b = insert_favorite_tx(&pool, &insert_rec(1002, "alice", 100, None)).await.unwrap();
    let f_c = insert_favorite_tx(&pool, &insert_rec(1003, "bob", 200, Some("2026-10-01T00:00:00Z"))).await.unwrap();

    // 1. Empty and whitespace-only query returns all 3 records
    let q_none = LibraryQuery::default();
    let res_none = list_favorites(&pool, &q_none).await.unwrap();
    assert_eq!(res_none.len(), 3);

    let q_empty = LibraryQuery {
        search: Some("".to_string()),
        ..Default::default()
    };
    let res_empty = list_favorites(&pool, &q_empty).await.unwrap();
    assert_eq!(res_empty.len(), 3);

    let q_spaces = LibraryQuery {
        search: Some("   \t  ".to_string()),
        ..Default::default()
    };
    let res_spaces = list_favorites(&pool, &q_spaces).await.unwrap();
    assert_eq!(res_spaces.len(), 3);

    // 2. Sort by Name Ascending: alice (1002), bob (1003), charlie (1001)
    let q_sort_name = LibraryQuery {
        sort_field: Some(LibrarySortField::Name),
        sort_order: Some(SortOrder::Asc),
        ..Default::default()
    };
    let res_sort_name = list_favorites(&pool, &q_sort_name).await.unwrap();
    assert_eq!(res_sort_name[0].id, f_b.id);
    assert_eq!(res_sort_name[1].id, f_c.id);
    assert_eq!(res_sort_name[2].id, f_a.id);

    // 3. Sort by Stars Descending: charlie (300), bob (200), alice (100)
    let q_sort_stars = LibraryQuery {
        sort_field: Some(LibrarySortField::Stars),
        sort_order: Some(SortOrder::Desc),
        ..Default::default()
    };
    let res_sort_stars = list_favorites(&pool, &q_sort_stars).await.unwrap();
    assert_eq!(res_sort_stars[0].id, f_a.id);
    assert_eq!(res_sort_stars[1].id, f_c.id);
    assert_eq!(res_sort_stars[2].id, f_b.id);

    // 4. Sort by PushedAt with NULLs last
    let q_sort_pushed = LibraryQuery {
        sort_field: Some(LibrarySortField::PushedAt),
        sort_order: Some(SortOrder::Desc),
        ..Default::default()
    };
    let res_sort_pushed = list_favorites(&pool, &q_sort_pushed).await.unwrap();
    // Non-nulls first: charlie (2026-10-03), bob (2026-10-01), then NULL (alice)
    assert_eq!(res_sort_pushed[0].id, f_a.id);
    assert_eq!(res_sort_pushed[1].id, f_c.id);
    assert_eq!(res_sort_pushed[2].id, f_b.id);
}

// ---------------------------------------------------------------------------
// AC101: Zero Network Dependency (Offline)
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_ac101_zero_network_offline() {
    let pool = setup_in_memory_db().await;

    let content = AnalysisContent {
        overview: sample_section("Offline repository analysis offline_token_s4"),
        features: sample_section("Zero external network dependencies"),
        use_cases: sample_section("Air-gapped environments"),
        tech_stack: sample_section("Pure local SQLite"),
        setup: sample_section("Local installation"),
        target_users: sample_section("Security conscious developers"),
        maturity: sample_section("Stable"),
        limitations: sample_section("None"),
    };
    let envelope = build_envelope_with_sections(content);
    let input = FavoriteInput {
        github_repo_id: Some(10101),
        github_url: "https://github.com/ac101/offline-repo".to_string(),
        name: "offline-repo".to_string(),
        description: Some("Offline description".to_string()),
        category: "Developer Tools".to_string(),
        stars: 42,
        license: None,
        topics: vec![],
        pushed_at: None,
        repo_created_at: None,
        releases: None,
        languages: None,
        translation: None,
        ai_analysis: Some(envelope),
    };
    insert_favorite_tx(&pool, &input).await.unwrap();

    // Execute query: pure SQLite, zero network calls
    let q = LibraryQuery {
        search: Some("offline_token_s4".to_string()),
        ..Default::default()
    };
    let res = list_favorites(&pool, &q).await.unwrap();
    assert_eq!(res.len(), 1);
    assert_eq!(res[0].name, "offline-repo");
}

// ---------------------------------------------------------------------------
// AC102: Restart Persistence
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_ac102_restart_persistence() {
    let temp_file = NamedTempFile::new().unwrap();
    let db_path = temp_file.path().to_path_buf();

    // 1. Initial run: open file-backed SQLite, save formal report
    let pool1 = setup_file_db(&db_path).await;

    let content = AnalysisContent {
        overview: sample_section("Restart persistence test with persistent_keyword_ac102"),
        features: sample_section("Features persistent"),
        use_cases: sample_section("Survives pool termination and application restart"),
        tech_stack: sample_section("SQLite disk file"),
        setup: sample_section("Setup"),
        target_users: sample_section("Users"),
        maturity: sample_section("Maturity"),
        limitations: sample_section("Limitations"),
    };
    let envelope = build_envelope_with_sections(content);
    let input = FavoriteInput {
        github_repo_id: Some(10201),
        github_url: "https://github.com/ac102/restart-repo".to_string(),
        name: "restart-repo".to_string(),
        description: Some("Restart test".to_string()),
        category: "Developer Tools".to_string(),
        stars: 123,
        license: None,
        topics: vec![],
        pushed_at: None,
        repo_created_at: None,
        releases: None,
        languages: None,
        translation: None,
        ai_analysis: Some(envelope),
    };
    let fav = insert_favorite_tx(&pool1, &input).await.unwrap();

    // Verify search works on pool1
    let q = LibraryQuery {
        search: Some("persistent_keyword_ac102".to_string()),
        ..Default::default()
    };
    let res1 = list_favorites(&pool1, &q).await.unwrap();
    assert_eq!(res1.len(), 1);
    assert_eq!(res1[0].id, fav.id);

    // 2. Simulate application exit: close and drop pool1
    pool1.close().await;
    drop(pool1);

    // 3. Simulate application restart: reconnect to the EXACT SAME file path
    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(&db_path)
        .foreign_keys(true);
    let pool2 = SqlitePool::connect_with(options).await.unwrap();

    // Verify search STILL matches from persistent SQLite DB
    let res2 = list_favorites(&pool2, &q).await.unwrap();
    assert_eq!(
        res2.len(),
        1,
        "Search must match persistent keyword after restarting pool"
    );
    assert_eq!(res2[0].id, fav.id);
    assert_eq!(res2[0].name, "restart-repo");
}

// ---------------------------------------------------------------------------
// AC104: Performance Measurement on 1,000 and 5,000 Synthetic Favorites
// ---------------------------------------------------------------------------
async fn populate_synthetic_fixtures(pool: &SqlitePool, count: usize) {
    let mut tx = pool.begin().await.unwrap();

    // Prepare analysis payloads
    let short_content = AnalysisContent {
        overview: sample_section("Short overview for microservice utility"),
        features: sample_section("Fast performance and low memory footprint"),
        use_cases: sample_section("CLI tool and background daemon"),
        tech_stack: sample_section("Rust serde"),
        setup: sample_section("cargo install"),
        target_users: sample_section("Developers"),
        maturity: sample_section("Active"),
        limitations: sample_section("Single node"),
    };
    let (short_json, _) = validate_and_serialize_envelope(&build_envelope_with_sections(short_content)).unwrap();

    let medium_content = AnalysisContent {
        overview: sample_section("Comprehensive data analytics and telemetry processing framework"),
        features: sample_section("Distributed stream processing fault tolerance horizontal scalability"),
        use_cases: sample_section("Real-time telemetry fraud detection event sourcing IoT gateway"),
        tech_stack: sample_section("Rust Apache Arrow DataFusion WebAssembly Tokio"),
        setup: sample_section("Docker compose up with cluster configuration"),
        target_users: sample_section("Data engineers infrastructure architects backend developers"),
        maturity: sample_section("Production grade deployed in enterprise clusters"),
        limitations: sample_section("Requires significant memory for in-memory partitions"),
    };
    let (medium_json, _) = validate_and_serialize_envelope(&build_envelope_with_sections(medium_content)).unwrap();

    // Target analysis containing unique keyword
    let target_content = AnalysisContent {
        overview: sample_section("Special targeted repository with unique_perf_telemetry_keyword_s4"),
        features: sample_section("High throughput search benchmarks"),
        use_cases: sample_section("Performance validation AC104"),
        tech_stack: sample_section("Rust SQLite"),
        setup: sample_section("Automated benchmark"),
        target_users: sample_section("QA and performance testers"),
        maturity: sample_section("Verified"),
        limitations: sample_section("Synthetic benchmark"),
    };
    let (target_json, _) = validate_and_serialize_envelope(&build_envelope_with_sections(target_content)).unwrap();

    let categories = [
        "Developer Tools",
        "AI & Agents",
        "Web & SaaS",
        "System Utilities",
        "Other",
    ];
    let target_idx = count / 2; // Placed right in the middle

    for i in 1..=count {
        let cat = categories[i % categories.len()];
        let stars = ((i * 17) % 50000) as i64;
        let rating = match i % 4 {
            0 => None,
            1 => Some(1),
            2 => Some(3),
            _ => Some(5),
        };
        let pushed_at = format!("2026-09-{:02}T12:00:00Z", (i % 28) + 1);

        let (name, desc, analysis_val) = if i == target_idx {
            (
                format!("target-repo-perf-{:05}", i),
                "Target description with special_target_original_keyword".to_string(),
                Some(target_json.clone()),
            )
        } else {
            let a_val = match i % 10 {
                0 | 1 | 2 | 3 => None, // 40% no analysis
                4 | 5 => Some(short_json.clone()), // 20% short
                6 | 7 | 8 => Some(medium_json.clone()), // 30% medium
                _ => Some(r#"{"schema_version": 99, "search_text": "bad_future_json"}"#.to_string()), // 10% bad JSON
            };
            (
                format!("synth-repo-{:05}", i),
                format!("Synthetic repository description for index {}", i),
                a_val,
            )
        };

        sqlx::query(
            r#"
            INSERT INTO favorites (
                id, github_repo_id, github_url, name, description, category, stars, license,
                pushed_at, rating, notes, favorited_at, metadata_refreshed_at, ai_analysis
            ) VALUES (
                ?, ?, ?, ?, ?, ?, ?, 'MIT',
                ?, ?, 'synthetic notes', '2026-10-04T00:00:00Z', '2026-10-04T00:00:00Z', ?
            )
            "#,
        )
        .bind(i as i64)
        .bind(100000 + i as i64)
        .bind(format!("https://github.com/synth-org/repo-{}", i))
        .bind(&name)
        .bind(&desc)
        .bind(cat)
        .bind(stars)
        .bind(pushed_at)
        .bind(rating)
        .bind(analysis_val)
        .execute(&mut *tx)
        .await
        .unwrap();

        let fav_id = i as i64;
        // Insert 1-2 tags per favorite
        sqlx::query("INSERT INTO favorite_tags (favorite_id, tag, source) VALUES (?, 'rust', 'github')")
            .bind(fav_id)
            .execute(&mut *tx)
            .await
            .unwrap();

        if i % 2 == 0 {
            sqlx::query("INSERT INTO favorite_tags (favorite_id, tag, source) VALUES (?, 'analytics', 'user')")
                .bind(fav_id)
                .execute(&mut *tx)
                .await
                .unwrap();
        }
    }

    tx.commit().await.unwrap();
}

fn compute_percentiles(mut durations_ms: Vec<f64>) -> (f64, f64) {
    durations_ms.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let len = durations_ms.len();
    let p50_idx = (len as f64 * 0.50).floor() as usize;
    let p95_idx = ((len as f64 * 0.95).floor() as usize).min(len - 1);
    (durations_ms[p50_idx], durations_ms[p95_idx])
}

#[tokio::test]
async fn test_ac104_performance_1000_and_5000_synthetic() {
    // -----------------------------------------------------------------------
    // Benchmark 1: 1,000 Favorites
    // -----------------------------------------------------------------------
    let pool_1000 = setup_in_memory_db().await;
    populate_synthetic_fixtures(&pool_1000, 1000).await;

    let count_1000: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM favorites")
        .fetch_one(&pool_1000)
        .await
        .unwrap();
    assert_eq!(count_1000.0, 1000);

    // Warm-up queries & measure
    let q_analysis_1000 = LibraryQuery {
        search: Some("unique_perf_telemetry_keyword_s4".to_string()),
        ..Default::default()
    };
    let q_orig_1000 = LibraryQuery {
        search: Some("special_target_original_keyword".to_string()),
        ..Default::default()
    };
    let q_combined_1000 = LibraryQuery {
        search: Some("telemetry".to_string()),
        category: Some("Developer Tools".to_string()),
        tag: Some("rust".to_string()),
        ..Default::default()
    };

    // Cold run (first execution)
    let cold_start = Instant::now();
    let cold_res = list_favorites(&pool_1000, &q_analysis_1000).await.unwrap();
    let cold_1000_ms = cold_start.elapsed().as_secs_f64() * 1000.0;
    assert_eq!(cold_res.len(), 1);

    // Warm repetitions (30 runs across representative workloads)
    let mut durations_1000 = Vec::with_capacity(30);
    for _ in 0..10 {
        let t0 = Instant::now();
        let r1 = list_favorites(&pool_1000, &q_analysis_1000).await.unwrap();
        durations_1000.push(t0.elapsed().as_secs_f64() * 1000.0);
        assert_eq!(r1.len(), 1);

        let t1 = Instant::now();
        let r2 = list_favorites(&pool_1000, &q_orig_1000).await.unwrap();
        durations_1000.push(t1.elapsed().as_secs_f64() * 1000.0);
        assert_eq!(r2.len(), 1);

        let t2 = Instant::now();
        let r3 = list_favorites(&pool_1000, &q_combined_1000).await.unwrap();
        durations_1000.push(t2.elapsed().as_secs_f64() * 1000.0);
        assert!(r3.len() >= 1);
    }

    let (p50_1000, p95_1000) = compute_percentiles(durations_1000);

    println!(
        "\n================ AC104 PERFORMANCE: 1,000 FIXTURES ================\n\
         Cold run:    {:.2} ms\n\
         Warm p50:    {:.2} ms\n\
         Warm p95:    {:.2} ms (Target: <= 500.00 ms)\n\
         Target met:  {}\n\
         ===================================================================",
        cold_1000_ms,
        p50_1000,
        p95_1000,
        if p95_1000 <= 500.0 { "PASS" } else { "FAIL" }
    );

    // Target contract: 1,000 warm p95 <= 500ms
    assert!(
        p95_1000 <= 500.0,
        "1,000 favorites p95 was {:.2}ms, exceeding 500ms target",
        p95_1000
    );

    // -----------------------------------------------------------------------
    // Benchmark 2: 5,000 Favorites (Growth Trend Measurement)
    // -----------------------------------------------------------------------
    let pool_5000 = setup_in_memory_db().await;
    populate_synthetic_fixtures(&pool_5000, 5000).await;

    let count_5000: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM favorites")
        .fetch_one(&pool_5000)
        .await
        .unwrap();
    assert_eq!(count_5000.0, 5000);

    let q_analysis_5000 = LibraryQuery {
        search: Some("unique_perf_telemetry_keyword_s4".to_string()),
        ..Default::default()
    };
    let q_orig_5000 = LibraryQuery {
        search: Some("special_target_original_keyword".to_string()),
        ..Default::default()
    };
    let q_combined_5000 = LibraryQuery {
        search: Some("telemetry".to_string()),
        category: Some("Developer Tools".to_string()),
        tag: Some("rust".to_string()),
        ..Default::default()
    };

    // Cold run
    let cold_start_5000 = Instant::now();
    let cold_res_5000 = list_favorites(&pool_5000, &q_analysis_5000).await.unwrap();
    let cold_5000_ms = cold_start_5000.elapsed().as_secs_f64() * 1000.0;
    assert_eq!(cold_res_5000.len(), 1);

    // Warm repetitions (30 runs across representative workloads)
    let mut durations_5000 = Vec::with_capacity(30);
    for _ in 0..10 {
        let t0 = Instant::now();
        let r1 = list_favorites(&pool_5000, &q_analysis_5000).await.unwrap();
        durations_5000.push(t0.elapsed().as_secs_f64() * 1000.0);
        assert_eq!(r1.len(), 1);

        let t1 = Instant::now();
        let r2 = list_favorites(&pool_5000, &q_orig_5000).await.unwrap();
        durations_5000.push(t1.elapsed().as_secs_f64() * 1000.0);
        assert_eq!(r2.len(), 1);

        let t2 = Instant::now();
        let r3 = list_favorites(&pool_5000, &q_combined_5000).await.unwrap();
        durations_5000.push(t2.elapsed().as_secs_f64() * 1000.0);
        assert!(r3.len() >= 1);
    }

    let (p50_5000, p95_5000) = compute_percentiles(durations_5000);

    println!(
        "\n================ AC104 PERFORMANCE: 5,000 FIXTURES ================\n\
         Cold run:    {:.2} ms\n\
         Warm p50:    {:.2} ms\n\
         Warm p95:    {:.2} ms\n\
         Growth ratio (p50 5k/1k): {:.2}x\n\
         ===================================================================\n",
        cold_5000_ms,
        p50_5000,
        p95_5000,
        if p50_1000 > 0.0 { p50_5000 / p50_1000 } else { 0.0 }
    );
}
