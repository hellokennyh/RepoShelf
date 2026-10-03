use tempfile::tempdir;
use reposhelf_lib::ai::analysis::task::AnalysisTaskManager;
use reposhelf_lib::db::analysis::*;
use reposhelf_lib::db::database::init_pool;
use reposhelf_lib::error::AppError;
use reposhelf_lib::library::models::{
    DescriptionTranslationInput, FavoriteInput, TranslationPatchInput,
    UpdateFavoriteUserMetadataInput,
};
use reposhelf_lib::library::repository::{
    delete_favorite_by_id, find_favorite_by_id, insert_favorite_tx, update_user_metadata_tx,
};

async fn setup_test_db() -> (tempfile::TempDir, sqlx::SqlitePool) {
    let dir = tempdir().expect("Failed to create tempdir");
    let db_path = dir.path().join("test_s3_persistence.db");
    let pool = init_pool(&db_path)
        .await
        .expect("Failed to initialize test pool");
    (dir, pool)
}

fn create_test_section(name: &str, text: &str) -> AnalysisSection {
    AnalysisSection {
        status: SectionStatus::Supported,
        items: vec![AnalysisItem {
            text: format!("{}: {}", name, text),
            basis: ClaimBasis::Documented,
            evidence_ids: vec!["E001".to_string()],
        }],
        missing_information: vec![],
    }
}

fn create_test_envelope(repo_id: i64, custom_text: &str) -> AnalysisEnvelopeV1 {
    let content = AnalysisContent {
        overview: create_test_section("overview", custom_text),
        features: create_test_section("features", custom_text),
        use_cases: create_test_section("use_cases", custom_text),
        tech_stack: create_test_section("tech_stack", custom_text),
        setup: create_test_section("setup", custom_text),
        target_users: create_test_section("target_users", custom_text),
        maturity: create_test_section("maturity", custom_text),
        limitations: create_test_section("limitations", custom_text),
    };

    AnalysisEnvelopeV1 {
        schema_version: 1,
        content,
        target_locale: AnalysisLocale::En,
        generated_at: "2026-10-03T00:00:00Z".to_string(),
        generator: AnalysisGenerator {
            provider: "mock".to_string(),
            model_requested: "mock-model".to_string(),
            model_reported: Some("mock-model".to_string()),
            prompt_version: "v1.1.2".to_string(),
            selector_version: "v1.1.2".to_string(),
            context_budget_tokens: 32768,
            input_tokens: 1000,
            input_tokens_is_estimate: false,
            output_limit_tokens: 4096,
        },
        source: AnalysisSource {
            github_repo_id: repo_id.to_string(),
            canonical_url: format!("https://github.com/owner/repo-{}", repo_id),
            default_branch: Some("main".to_string()),
            commit_sha: Some("1234567890abcdef1234567890abcdef12345678".to_string()),
            metadata_observed_at: "2026-10-03T00:00:00Z".to_string(),
            evidence_fingerprint: "test_evidence_fingerprint".to_string(),
        },
        evidence: vec![EvidenceRecord {
            id: "E001".to_string(),
            kind: "metadata".to_string(),
            path: None,
            commit_sha: None,
            blob_sha: None,
            observed_at: "2026-10-03T00:00:00Z".to_string(),
            included_ranges: vec![],
            included_content_hash: "meta_hash".to_string(),
            truncated: false,
            public_fact_summary: Some("Repo metadata".to_string()),
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
        search_text: format!("Search text for repo-{}", repo_id),
    }
}

// =============================================================================
// AC075 - AC080: Optimistic Concurrency and Draft Persistence Tests
// =============================================================================

#[tokio::test]
async fn test_analysis_patch_replace_with_null_revision_succeeds_on_new_record() {
    let (_dir, pool) = setup_test_db().await;

    // 1. Insert favorite without analysis
    let input = FavoriteInput {
        github_repo_id: Some(101),
        github_url: "https://github.com/owner/repo-101".to_string(),
        name: "repo-101".to_string(),
        description: Some("Test repo 101".to_string()),
        category: "Developer Tools".to_string(),
        stars: 42,
        license: Some("MIT".to_string()),
        topics: vec!["rust".to_string()],
        pushed_at: None,
        repo_created_at: None,
        releases: None,
        languages: None,
        translation: None,
        ai_analysis: None,
    };

    let inserted = insert_favorite_tx(&pool, &input)
        .await
        .expect("Failed to insert favorite");
    let fav_id = inserted.id;

    let fav = find_favorite_by_id(&pool, fav_id)
        .await
        .expect("Failed to find favorite")
        .expect("Favorite not found");

    assert_eq!(
        fav.analysis_revision.as_deref(),
        Some(NULL_ANALYSIS_REVISION)
    );
    assert!(fav.ai_analysis.is_none());

    // 2. Replace analysis with expected_revision = NULL_ANALYSIS_REVISION
    let envelope = create_test_envelope(101, "initial analysis");
    let patch = AnalysisPatchInput::Replace {
        envelope: envelope.clone(),
        expected_revision: NULL_ANALYSIS_REVISION.to_string(),
        expected_repo_id: 101,
    };

    let update_input = UpdateFavoriteUserMetadataInput {
        favorite_id: fav_id,
        category: "Developer Tools".to_string(),
        rating: Some(5),
        rating_reason: Some("Great repo".to_string()),
        rating_confidence: Some("high".to_string()),
        notes: "Some notes".to_string(),
        user_tags: vec!["cli".to_string()],
        ai_tags: None,
        translation_patch: None,
        analysis_patch: Some(patch),
    };

    let updated = update_user_metadata_tx(&pool, &update_input)
        .await
        .expect("Failed to update with analysis");

    assert!(updated.ai_analysis.is_some());
    assert_ne!(
        updated.analysis_revision.as_deref(),
        Some(NULL_ANALYSIS_REVISION)
    );

    // Verify stored analysis in DB
    let reloaded = find_favorite_by_id(&pool, fav_id)
        .await
        .expect("Failed to reload")
        .expect("Not found");
    assert_eq!(
        reloaded.analysis_revision,
        updated.analysis_revision
    );
}

#[tokio::test]
async fn test_analysis_patch_replace_conflict_on_mismatched_revision() {
    let (_dir, pool) = setup_test_db().await;

    // 1. Insert favorite with initial analysis
    let envelope1 = create_test_envelope(102, "v1 analysis");
    let input = FavoriteInput {
        github_repo_id: Some(102),
        github_url: "https://github.com/owner/repo-102".to_string(),
        name: "repo-102".to_string(),
        description: Some("Test repo 102".to_string()),
        category: "Developer Tools".to_string(),
        stars: 100,
        license: None,
        topics: vec![],
        pushed_at: None,
        repo_created_at: None,
        releases: None,
        languages: None,
        translation: None,
        ai_analysis: Some(envelope1),
    };

    let inserted = insert_favorite_tx(&pool, &input)
        .await
        .expect("Failed to insert favorite");
    let fav_id = inserted.id;

    let fav = find_favorite_by_id(&pool, fav_id)
        .await
        .expect("Failed to find favorite")
        .expect("Favorite not found");

    let current_rev = fav.analysis_revision.unwrap();

    // 2. Try to replace with wrong expected_revision
    let envelope2 = create_test_envelope(102, "v2 analysis");
    let patch = AnalysisPatchInput::Replace {
        envelope: envelope2,
        expected_revision: "mismatched_revision_token".to_string(),
        expected_repo_id: 102,
    };

    let update_input = UpdateFavoriteUserMetadataInput {
        favorite_id: fav_id,
        category: "AI & Agents".to_string(),
        rating: None,
        rating_reason: None,
        rating_confidence: None,
        notes: "Changed notes".to_string(),
        user_tags: vec![],
        ai_tags: None,
        translation_patch: None,
        analysis_patch: Some(patch),
    };

    let result = update_user_metadata_tx(&pool, &update_input).await;
    match result {
        Err(AppError::AnalysisConflict(msg)) => {
            assert!(msg.contains("has been modified by another operation"));
        }
        other => panic!("Expected AnalysisConflict, got: {:?}", other),
    }

    // Verify entire transaction rolled back: category and notes unchanged
    let unchanged = find_favorite_by_id(&pool, fav_id)
        .await
        .expect("Failed to reload")
        .expect("Not found");
    assert_eq!(unchanged.category, "Developer Tools");
    assert_eq!(unchanged.notes, "");
    assert_eq!(unchanged.analysis_revision.as_deref(), Some(current_rev.as_str()));
}

#[tokio::test]
async fn test_analysis_patch_replace_conflict_on_mismatched_repo_id() {
    let (_dir, pool) = setup_test_db().await;

    let input = FavoriteInput {
        github_repo_id: Some(103),
        github_url: "https://github.com/owner/repo-103".to_string(),
        name: "repo-103".to_string(),
        description: None,
        category: "Other".to_string(),
        stars: 10,
        license: None,
        topics: vec![],
        pushed_at: None,
        repo_created_at: None,
        releases: None,
        languages: None,
        translation: None,
        ai_analysis: None,
    };

    let inserted = insert_favorite_tx(&pool, &input).await.unwrap();
    let fav_id = inserted.id;

    let envelope = create_test_envelope(103, "analysis");
    let patch = AnalysisPatchInput::Replace {
        envelope,
        expected_revision: NULL_ANALYSIS_REVISION.to_string(),
        expected_repo_id: 999, // Mismatched repo_id
    };

    let update_input = UpdateFavoriteUserMetadataInput {
        favorite_id: fav_id,
        category: "Other".to_string(),
        rating: None,
        rating_reason: None,
        rating_confidence: None,
        notes: "".to_string(),
        user_tags: vec![],
        ai_tags: None,
        translation_patch: None,
        analysis_patch: Some(patch),
    };

    let result = update_user_metadata_tx(&pool, &update_input).await;
    match result {
        Err(AppError::AnalysisConflict(msg)) => {
            assert!(msg.contains("Repository identity mismatch"));
        }
        other => panic!("Expected AnalysisConflict, got: {:?}", other),
    }
}

#[tokio::test]
async fn test_analysis_patch_keep_preserves_concurrent_analysis() {
    let (_dir, pool) = setup_test_db().await;

    // 1. Insert favorite with initial analysis
    let envelope1 = create_test_envelope(104, "initial analysis");
    let input = FavoriteInput {
        github_repo_id: Some(104),
        github_url: "https://github.com/owner/repo-104".to_string(),
        name: "repo-104".to_string(),
        description: None,
        category: "Developer Tools".to_string(),
        stars: 10,
        license: None,
        topics: vec![],
        pushed_at: None,
        repo_created_at: None,
        releases: None,
        languages: None,
        translation: None,
        ai_analysis: Some(envelope1),
    };

    let inserted = insert_favorite_tx(&pool, &input).await.unwrap();
    let fav_id = inserted.id;
    let initial_fav = find_favorite_by_id(&pool, fav_id).await.unwrap().unwrap();
    let initial_rev = initial_fav.analysis_revision.unwrap();

    // 2. Perform metadata update with AnalysisPatchInput::Keep
    let update_input = UpdateFavoriteUserMetadataInput {
        favorite_id: fav_id,
        category: "Productivity".to_string(),
        rating: Some(4),
        rating_reason: Some("Nice tool".to_string()),
        rating_confidence: Some("medium".to_string()),
        notes: "Updated notes".to_string(),
        user_tags: vec!["tag1".to_string()],
        ai_tags: None,
        translation_patch: None,
        analysis_patch: Some(AnalysisPatchInput::Keep),
    };

    let updated = update_user_metadata_tx(&pool, &update_input)
        .await
        .expect("Keep update should succeed");

    assert_eq!(updated.category, "Productivity");
    assert_eq!(updated.notes, "Updated notes");
    // Analysis is completely preserved and untouched
    assert_eq!(updated.analysis_revision.as_deref(), Some(initial_rev.as_str()));
    assert!(updated.ai_analysis.is_some());
}

#[tokio::test]
async fn test_analysis_delete_recreate_resets_revision_to_null() {
    let (_dir, pool) = setup_test_db().await;

    let envelope = create_test_envelope(105, "initial");
    let input = FavoriteInput {
        github_repo_id: Some(105),
        github_url: "https://github.com/owner/repo-105".to_string(),
        name: "repo-105".to_string(),
        description: None,
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

    let inserted1 = insert_favorite_tx(&pool, &input).await.unwrap();
    let fav_id1 = inserted1.id;
    let fav1 = find_favorite_by_id(&pool, fav_id1).await.unwrap().unwrap();
    let old_rev = fav1.analysis_revision.unwrap();

    // Delete the favorite
    delete_favorite_by_id(&pool, fav_id1).await.unwrap();

    // Recreate favorite without analysis
    let input2 = FavoriteInput {
        github_repo_id: Some(105),
        github_url: "https://github.com/owner/repo-105".to_string(),
        name: "repo-105".to_string(),
        description: None,
        category: "Other".to_string(),
        stars: 10,
        license: None,
        topics: vec![],
        pushed_at: None,
        repo_created_at: None,
        releases: None,
        languages: None,
        translation: None,
        ai_analysis: None,
    };

    let inserted2 = insert_favorite_tx(&pool, &input2).await.unwrap();
    let fav_id2 = inserted2.id;
    let fav2 = find_favorite_by_id(&pool, fav_id2).await.unwrap().unwrap();

    // New row revision is NULL_ANALYSIS_REVISION
    assert_eq!(
        fav2.analysis_revision.as_deref(),
        Some(NULL_ANALYSIS_REVISION)
    );

    // Old revision cannot be used to update new row
    let patch = AnalysisPatchInput::Replace {
        envelope: create_test_envelope(105, "attempted overwrite"),
        expected_revision: old_rev,
        expected_repo_id: 105,
    };

    let update_input = UpdateFavoriteUserMetadataInput {
        favorite_id: fav_id2,
        category: "Other".to_string(),
        rating: None,
        rating_reason: None,
        rating_confidence: None,
        notes: "".to_string(),
        user_tags: vec![],
        ai_tags: None,
        translation_patch: None,
        analysis_patch: Some(patch),
    };

    let result = update_user_metadata_tx(&pool, &update_input).await;
    assert!(matches!(result, Err(AppError::AnalysisConflict(_))));
}

// =============================================================================
// AC081 - AC085: Task Manager, Cancellation and Identity Protection Tests
// =============================================================================

#[test]
fn test_analysis_task_manager_registration_and_cancellation() {
    let manager = AnalysisTaskManager::new();

    let task_id = "task-session-123";
    let token = manager.register(task_id).expect("register task");

    assert!(!token.is_cancelled());

    // Looking up non-existent task
    assert!(!manager.cancel("non-existent"));

    // Cancel the registered task
    assert!(manager.cancel(task_id));
    assert!(token.is_cancelled());

    // Task is cleanly unregistered
    manager.unregister(task_id);
    assert!(!manager.cancel(task_id));
}

#[tokio::test]
async fn test_new_favorite_with_preview_atomic_persistence() {
    let (_dir, pool) = setup_test_db().await;

    let envelope = create_test_envelope(106, "preview result");
    let input = FavoriteInput {
        github_repo_id: Some(106),
        github_url: "https://github.com/owner/repo-106".to_string(),
        name: "repo-106".to_string(),
        description: Some("Atomic persistence test".to_string()),
        category: "AI & Agents".to_string(),
        stars: 500,
        license: Some("Apache-2.0".to_string()),
        topics: vec!["ai".to_string(), "agents".to_string()],
        pushed_at: None,
        repo_created_at: None,
        releases: Some(3),
        languages: None,
        translation: None,
        ai_analysis: Some(envelope.clone()),
    };

    let inserted = insert_favorite_tx(&pool, &input)
        .await
        .expect("Atomic insert with analysis should succeed");
    let fav_id = inserted.id;

    let fav = find_favorite_by_id(&pool, fav_id)
        .await
        .unwrap()
        .unwrap();

    assert!(fav.ai_analysis.is_some());
    let loaded = fav.ai_analysis.unwrap();
    assert_eq!(loaded.schema_version, 1);
    assert_eq!(loaded.source.github_repo_id, "106");
    assert_ne!(
        fav.analysis_revision.as_deref(),
        Some(NULL_ANALYSIS_REVISION)
    );
}

// =============================================================================
// Section 4.3 & AC086 - AC088: Mandatory Replace Baselines and Concurrency Hardening
// =============================================================================

#[test]
fn test_replace_json_missing_expected_revision_rejected() {
    let envelope = create_test_envelope(201, "rejection test");
    let envelope_json = serde_json::to_string(&envelope).unwrap();
    let bad_json = format!(
        r#"{{"action":"replace","envelope":{},"expected_repo_id":201}}"#,
        envelope_json
    );
    let result = serde_json::from_str::<AnalysisPatchInput>(&bad_json);
    assert!(result.is_err(), "Replace missing expected_revision must fail deserialization");
}

#[test]
fn test_replace_json_missing_expected_repo_id_rejected() {
    let envelope = create_test_envelope(202, "rejection test");
    let envelope_json = serde_json::to_string(&envelope).unwrap();
    let bad_json = format!(
        r#"{{"action":"replace","envelope":{},"expected_revision":"rev-123"}}"#,
        envelope_json
    );
    let result = serde_json::from_str::<AnalysisPatchInput>(&bad_json);
    assert!(result.is_err(), "Replace missing expected_repo_id must fail deserialization");
}

#[test]
fn test_malicious_ipc_dto_missing_both_cannot_deserialize() {
    let envelope = create_test_envelope(203, "rejection test");
    let envelope_json = serde_json::to_string(&envelope).unwrap();
    let bad_patch_json = format!(
        r#"{{"action":"replace","envelope":{}}}"#,
        envelope_json
    );
    let result = serde_json::from_str::<AnalysisPatchInput>(&bad_patch_json);
    assert!(result.is_err(), "Replace missing both expected_revision and expected_repo_id must fail");

    let bad_dto_json = format!(
        r#"{{"favorite_id":1,"category":"Other","notes":"","user_tags":[],"analysis_patch":{}}}"#,
        bad_patch_json
    );
    let dto_result = serde_json::from_str::<UpdateFavoriteUserMetadataInput>(&bad_dto_json);
    assert!(dto_result.is_err(), "Full DTO with bypass Replace cannot deserialize");
}

#[tokio::test]
async fn test_replace_valid_existing_revision_and_correct_repo_id_succeeds() {
    let (_dir, pool) = setup_test_db().await;

    let envelope_a = create_test_envelope(204, "version A");
    let input = FavoriteInput {
        github_repo_id: Some(204),
        github_url: "https://github.com/owner/repo-204".to_string(),
        name: "repo-204".to_string(),
        category: "Developer Tools".to_string(),
        stars: 10,
        ai_analysis: Some(envelope_a),
        ..Default::default()
    };
    let inserted = insert_favorite_tx(&pool, &input).await.unwrap();
    let fav = find_favorite_by_id(&pool, inserted.id).await.unwrap().unwrap();
    let rev_a = fav.analysis_revision.unwrap();

    let envelope_b = create_test_envelope(204, "version B");
    let patch = AnalysisPatchInput::Replace {
        envelope: envelope_b,
        expected_revision: rev_a,
        expected_repo_id: 204,
    };
    let update_input = UpdateFavoriteUserMetadataInput {
        favorite_id: inserted.id,
        category: "Developer Tools".to_string(),
        rating: None,
        rating_reason: None,
        rating_confidence: None,
        notes: "Updated with valid revision".to_string(),
        user_tags: vec![],
        ai_tags: None,
        translation_patch: None,
        analysis_patch: Some(patch),
    };
    let updated = update_user_metadata_tx(&pool, &update_input).await.expect("Valid replace must succeed");
    assert!(updated.ai_analysis.is_some());
    assert_eq!(updated.notes, "Updated with valid revision");
}

#[tokio::test]
async fn test_ac086_double_instance_full_transaction_rollback() {
    let (_dir, pool) = setup_test_db().await;

    // Initial DB state:
    // analysis = A, notes = "original notes", rating = 3, tags = ["original-tag"], translation = "original translation"
    let envelope_a = create_test_envelope(301, "analysis A");
    let input = FavoriteInput {
        github_repo_id: Some(301),
        github_url: "https://github.com/owner/repo-301".to_string(),
        name: "repo-301".to_string(),
        description: Some("Base description".to_string()),
        category: "Developer Tools".to_string(),
        stars: 100,
        license: Some("MIT".to_string()),
        topics: vec![],
        pushed_at: None,
        repo_created_at: None,
        releases: None,
        languages: None,
        translation: Some(DescriptionTranslationInput {
            text: "original translation".to_string(),
            target_locale: "zh-Hans".to_string(),
        }),
        ai_analysis: Some(envelope_a),
    };

    let inserted = insert_favorite_tx(&pool, &input).await.unwrap();
    let fav_id = inserted.id;

    // Update notes and rating to establish the exact initial baseline
    let setup_input = UpdateFavoriteUserMetadataInput {
        favorite_id: fav_id,
        category: "Developer Tools".to_string(),
        rating: Some(3),
        rating_reason: Some("Initial reason".to_string()),
        rating_confidence: Some("medium".to_string()),
        notes: "original notes".to_string(),
        user_tags: vec!["original-tag".to_string()],
        ai_tags: None,
        translation_patch: None,
        analysis_patch: Some(AnalysisPatchInput::Keep),
    };
    let initial_fav = update_user_metadata_tx(&pool, &setup_input).await.unwrap();
    let baseline_rev_a = initial_fav.analysis_revision.unwrap();

    // Instance 1:
    // baseline revision A -> Replace B, notes = "N1 notes"
    let envelope_b = create_test_envelope(301, "analysis B");
    let patch_1 = AnalysisPatchInput::Replace {
        envelope: envelope_b,
        expected_revision: baseline_rev_a.clone(),
        expected_repo_id: 301,
    };
    let input_instance_1 = UpdateFavoriteUserMetadataInput {
        favorite_id: fav_id,
        category: "Developer Tools".to_string(),
        rating: Some(3),
        rating_reason: Some("Initial reason".to_string()),
        rating_confidence: Some("medium".to_string()),
        notes: "N1 notes".to_string(),
        user_tags: vec!["original-tag".to_string()],
        ai_tags: None,
        translation_patch: None,
        analysis_patch: Some(patch_1),
    };
    let res_1 = update_user_metadata_tx(&pool, &input_instance_1).await;
    assert!(res_1.is_ok(), "Instance 1 save must succeed");

    // Instance 2:
    // Still holding baseline revision A -> Replace C, notes = "N2 notes", rating = 5, tags = ["changed-tag"], translation = "changed translation"
    let envelope_c = create_test_envelope(301, "analysis C");
    let patch_2 = AnalysisPatchInput::Replace {
        envelope: envelope_c,
        expected_revision: baseline_rev_a, // Stale baseline revision A!
        expected_repo_id: 301,
    };
    let input_instance_2 = UpdateFavoriteUserMetadataInput {
        favorite_id: fav_id,
        category: "AI & Agents".to_string(),
        rating: Some(5),
        rating_reason: Some("Changed reason".to_string()),
        rating_confidence: Some("high".to_string()),
        notes: "N2 notes".to_string(),
        user_tags: vec!["changed-tag".to_string()],
        ai_tags: None,
        translation_patch: Some(TranslationPatchInput::Replace {
            text: "changed translation".to_string(),
            target_locale: "zh-Hans".to_string(),
            source_text: "Base description".to_string(),
        }),
        analysis_patch: Some(patch_2),
    };
    let res_2 = update_user_metadata_tx(&pool, &input_instance_2).await;

    // Must be AnalysisConflict
    match res_2 {
        Err(AppError::AnalysisConflict(msg)) => {
            assert!(msg.contains("has been modified by another operation"));
        }
        other => panic!("Expected AnalysisConflict on instance 2, got: {:?}", other),
    }

    // Assert full transaction rollback: re-read DB and assert ALL fields remain strictly from Instance 1
    let db_record = find_favorite_by_id(&pool, fav_id).await.unwrap().unwrap();
    let analysis_obj = db_record.ai_analysis.expect("Analysis B must be present");
    assert_eq!(
        analysis_obj.content.overview.items[0].text,
        "overview: analysis B",
        "Analysis must be B (from Instance 1)"
    );
    assert_eq!(db_record.notes, "N1 notes", "Notes must remain N1 (from Instance 1)");
    assert_eq!(db_record.rating, Some(3), "Rating must remain 3 (from Instance 1)");
    assert_eq!(db_record.category, "Developer Tools", "Category must remain Developer Tools");

    let tags = db_record.tags.into_iter().map(|t| t.tag).collect::<Vec<_>>();
    assert_eq!(tags, vec!["original-tag".to_string()], "Tags must remain original-tag");

    let trans = db_record.description_translation.expect("Translation must exist");
    assert_eq!(trans.text, "original translation", "Translation must not be overwritten by instance 2");
}

#[tokio::test]
async fn test_ac087_keep_concurrency_preserves_concurrent_replace() {
    let (_dir, pool) = setup_test_db().await;

    let envelope_a = create_test_envelope(302, "analysis A");
    let input = FavoriteInput {
        github_repo_id: Some(302),
        github_url: "https://github.com/owner/repo-302".to_string(),
        name: "repo-302".to_string(),
        category: "Developer Tools".to_string(),
        stars: 10,
        ai_analysis: Some(envelope_a),
        ..Default::default()
    };
    let inserted = insert_favorite_tx(&pool, &input).await.unwrap();
    let fav_id = inserted.id;
    let initial_fav = find_favorite_by_id(&pool, fav_id).await.unwrap().unwrap();
    let rev_a = initial_fav.analysis_revision.unwrap();

    // Instance 1: Replace B
    let envelope_b = create_test_envelope(302, "analysis B");
    let patch_b = AnalysisPatchInput::Replace {
        envelope: envelope_b,
        expected_revision: rev_a,
        expected_repo_id: 302,
    };
    let input_1 = UpdateFavoriteUserMetadataInput {
        favorite_id: fav_id,
        category: "Developer Tools".to_string(),
        rating: None,
        rating_reason: None,
        rating_confidence: None,
        notes: "N1 notes".to_string(),
        user_tags: vec![],
        ai_tags: None,
        translation_patch: None,
        analysis_patch: Some(patch_b),
    };
    update_user_metadata_tx(&pool, &input_1).await.expect("Instance 1 Replace B succeeds");

    // Instance 2: Keep (older baseline, but only updating user notes)
    let input_2 = UpdateFavoriteUserMetadataInput {
        favorite_id: fav_id,
        category: "Developer Tools".to_string(),
        rating: Some(4),
        rating_reason: None,
        rating_confidence: None,
        notes: "N2 notes".to_string(),
        user_tags: vec![],
        ai_tags: None,
        translation_patch: None,
        analysis_patch: Some(AnalysisPatchInput::Keep),
    };
    let updated_2 = update_user_metadata_tx(&pool, &input_2).await.expect("Instance 2 Keep succeeds");

    assert_eq!(updated_2.notes, "N2 notes");
    assert_eq!(updated_2.rating, Some(4));
    let analysis_b = updated_2.ai_analysis.expect("Analysis B preserved");
    assert_eq!(analysis_b.content.overview.items[0].text, "overview: analysis B");
}

#[tokio::test]
async fn test_ac088_delete_recreate_stale_instance_rejected() {
    let (_dir, pool) = setup_test_db().await;

    // Favorite X with github_repo_id G
    let envelope_a = create_test_envelope(303, "analysis A");
    let input_x = FavoriteInput {
        github_repo_id: Some(303),
        github_url: "https://github.com/owner/repo-303".to_string(),
        name: "repo-303".to_string(),
        category: "Developer Tools".to_string(),
        stars: 10,
        ai_analysis: Some(envelope_a),
        ..Default::default()
    };
    let inserted_x = insert_favorite_tx(&pool, &input_x).await.unwrap();
    let row_id_x = inserted_x.id;
    let fav_x = find_favorite_by_id(&pool, row_id_x).await.unwrap().unwrap();
    let stale_rev_a = fav_x.analysis_revision.unwrap();

    // Delete row X
    delete_favorite_by_id(&pool, row_id_x).await.unwrap();

    // Recreate same repo as new favorite Y (row_id_y != row_id_x)
    let input_y = FavoriteInput {
        github_repo_id: Some(303),
        github_url: "https://github.com/owner/repo-303".to_string(),
        name: "repo-303".to_string(),
        category: "Developer Tools".to_string(),
        stars: 10,
        ai_analysis: None,
        ..Default::default()
    };
    let inserted_y = insert_favorite_tx(&pool, &input_y).await.unwrap();
    let row_id_y = inserted_y.id;
    assert_ne!(row_id_x, row_id_y);

    let fav_y = find_favorite_by_id(&pool, row_id_y).await.unwrap().unwrap();
    assert_eq!(fav_y.analysis_revision.as_deref(), Some(NULL_ANALYSIS_REVISION));

    // Old detail instance attempts to save to old row id X:
    let old_detail_save_x = UpdateFavoriteUserMetadataInput {
        favorite_id: row_id_x,
        category: "Developer Tools".to_string(),
        rating: None,
        rating_reason: None,
        rating_confidence: None,
        notes: "Old instance note".to_string(),
        user_tags: vec![],
        ai_tags: None,
        translation_patch: None,
        analysis_patch: Some(AnalysisPatchInput::Replace {
            envelope: create_test_envelope(303, "stale save"),
            expected_revision: stale_rev_a.clone(),
            expected_repo_id: 303,
        }),
    };
    let err_x = update_user_metadata_tx(&pool, &old_detail_save_x).await;
    assert!(matches!(err_x, Err(AppError::FavoriteNotFound(_))), "Save to deleted row X must return FavoriteNotFound");

    // Even if stale client tried to target row Y with old baseline revision A:
    let stale_detail_save_y = UpdateFavoriteUserMetadataInput {
        favorite_id: row_id_y,
        category: "Developer Tools".to_string(),
        rating: None,
        rating_reason: None,
        rating_confidence: None,
        notes: "Stale revision note".to_string(),
        user_tags: vec![],
        ai_tags: None,
        translation_patch: None,
        analysis_patch: Some(AnalysisPatchInput::Replace {
            envelope: create_test_envelope(303, "stale save"),
            expected_revision: stale_rev_a,
            expected_repo_id: 303,
        }),
    };
    let err_y = update_user_metadata_tx(&pool, &stale_detail_save_y).await;
    assert!(matches!(err_y, Err(AppError::AnalysisConflict(_))), "Stale revision must be rejected by row Y with AnalysisConflict");

    let y_after = find_favorite_by_id(&pool, row_id_y).await.unwrap().unwrap();
    assert!(y_after.ai_analysis.is_none(), "Row Y ai_analysis must remain null");
}

#[test]
fn test_analysis_task_manager_duplicate_task_id_rejected() {
    let manager = AnalysisTaskManager::new();

    let task_id = "duplicate-test-task";

    // 1. Register X -> success
    let token1 = manager.register(task_id).expect("First registration must succeed");
    assert!(!token1.is_cancelled());

    // 2. Register X again -> second rejected with AnalysisTaskAlreadyExists
    let second_res = manager.register(task_id);
    match second_res {
        Err(AppError::AnalysisTaskAlreadyExists(msg)) => {
            assert!(msg.contains("is already running"));
        }
        other => panic!("Expected AnalysisTaskAlreadyExists, got: {:?}", other),
    }

    // 3. Cancel X -> cancels original token
    assert!(manager.cancel(task_id));
    assert!(token1.is_cancelled());

    // 4. Unregister original -> cleans up
    manager.unregister(task_id);

    // 5. Afterward, register X again -> success
    let token2 = manager.register(task_id).expect("Re-registration after unregister must succeed");
    assert!(!token2.is_cancelled());
    manager.unregister(task_id);
}
