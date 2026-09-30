use std::path::Path;
use tempfile::tempdir;
use sqlx::Row;

use reposhelf_lib::db::analysis::{
    derive_search_text, parse_db_analysis, validate_and_serialize_envelope,
    AnalysisContent, AnalysisCoverage, AnalysisEnvelopeV1, AnalysisGenerator,
    AnalysisItem, AnalysisLocale, AnalysisPatchInput, AnalysisReadStatus,
    AnalysisSection, AnalysisSource, ClaimBasis, EvidenceRecord, SectionStatus,
};
use reposhelf_lib::db::database::{
    init_pool, validate_backup_file,
};
use reposhelf_lib::github::models::GitHubRepository;
use reposhelf_lib::library::models::{
    FavoriteInput, MetadataPatch, UpdateFavoriteUserMetadataInput,
};
use reposhelf_lib::library::repository::{
    find_favorite_by_id, insert_favorite_tx, update_github_owned_metadata_tx,
    update_user_metadata_tx,
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

fn sample_analysis_content() -> AnalysisContent {
    AnalysisContent {
        overview: sample_section("Overview: Fast and reliable system"),
        features: sample_section("Features: Async engine Memory safety"),
        use_cases: sample_section("Use Cases: Microservices"),
        tech_stack: sample_section("Tech Stack: Rust Tokio"),
        setup: sample_section("Setup: cargo build --release"),
        target_users: sample_section("Target Users: Backend developers"),
        maturity: sample_section("Maturity: Production ready"),
        limitations: sample_section("Limitations: Requires SQLite 3.38+"),
    }
}

fn sample_analysis_envelope() -> AnalysisEnvelopeV1 {
    AnalysisEnvelopeV1 {
        schema_version: 1,
        content: sample_analysis_content(),
        target_locale: AnalysisLocale::En,
        generated_at: "2024-03-01T12:00:00Z".to_string(),
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
            github_repo_id: "12345".to_string(),
            canonical_url: "https://github.com/owner/repo".to_string(),
            default_branch: Some("main".to_string()),
            commit_sha: Some("abc".to_string()),
            metadata_observed_at: "2024-03-01T12:00:00Z".to_string(),
            evidence_fingerprint: "fingerprint".to_string(),
        },
        evidence: vec![EvidenceRecord {
            id: "ev-meta-1".to_string(),
            kind: "metadata".to_string(),
            path: None,
            commit_sha: None,
            blob_sha: None,
            observed_at: "2024-03-01T12:00:00Z".to_string(),
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
        search_text: "arbitrary-search-text-to-be-overridden".to_string(),
    }
}

/// Helper to create a v1.1.1 database fixture (migrations 001..005) with representative data
async fn create_v1_1_1_fixture(db_path: &Path) -> sqlx::SqlitePool {
    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(db_path)
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal);
    let pool = sqlx::SqlitePool::connect_with(options).await.unwrap();

    // Create _sqlx_migrations tracking table
    sqlx::raw_sql(
        r#"
        CREATE TABLE IF NOT EXISTS _sqlx_migrations (
            version BIGINT PRIMARY KEY,
            description TEXT NOT NULL,
            installed_on TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
            success BOOLEAN NOT NULL,
            checksum BLOB NOT NULL,
            execution_time BIGINT NOT NULL
        );
        "#,
    )
    .execute(&pool)
    .await
    .unwrap();

    // Apply migrations 1 through 5 with exact sqlx checksums
    let migrator = sqlx::migrate!("./migrations");
    for m in migrator.iter() {
        if m.version <= 5 {
            sqlx::raw_sql(&m.sql).execute(&pool).await.unwrap();
            sqlx::query(
                "INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time) VALUES (?, ?, 1, ?, 10)"
            )
            .bind(m.version)
            .bind(&*m.description)
            .bind(&*m.checksum)
            .execute(&pool)
            .await
            .unwrap();
        }
    }

    // Insert representative v1.1.1 favorite records with all 19 columns
    sqlx::query(
        r#"
        INSERT INTO favorites (
            id, github_repo_id, github_url, name, description, category, stars, license,
            pushed_at, rating, rating_reason, rating_confidence, notes, favorited_at, metadata_refreshed_at,
            releases, languages, description_translations, description_translation_source_hash
        ) VALUES (
            1, 9901, 'https://github.com/fixture-owner/fixture-repo', 'fixture-repo',
            'Representative v1.1.1 description', 'Developer Tools', 1250, 'Apache-2.0',
            '2026-09-01T12:00:00Z', 5, 'Robust architecture and complete documentation', 'high',
            'User notes on fixture repo', '2026-09-02T10:00:00Z', '2026-09-02T10:00:00Z',
            42, '{"Rust":90.0,"C":10.0}', '{"zh-Hans":{"text":"翻译简介","target_locale":"zh-Hans","source_hash":"abc"}}', 'abc'
        )
        "#,
    )
    .execute(&pool)
    .await
    .unwrap();

    // Insert tags
    sqlx::query(
        r#"
        INSERT INTO favorite_tags (favorite_id, tag, source) VALUES
            (1, 'rust', 'github'),
            (1, 'tauri', 'github'),
            (1, 'productivity', 'user');
        "#,
    )
    .execute(&pool)
    .await
    .unwrap();

    // Insert selection_history
    sqlx::query(
        r#"
        INSERT INTO selection_history (github_repo_id, github_url, selected_at)
        VALUES (9901, 'https://github.com/fixture-owner/fixture-repo', '2026-09-02T10:05:00Z');
        "#,
    )
    .execute(&pool)
    .await
    .unwrap();

    pool
}

// =============================================================================
// 1. Fresh Database Migration Matrix (001 -> 006)
// =============================================================================

#[tokio::test]
async fn test_fresh_database_migration_001_to_006_and_schema_introspection() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("fresh_006.db");

    let pool = init_pool(&db_path)
        .await
        .expect("Fresh init_pool must succeed");

    // 1. Verify all 6 migrations applied successfully
    let versions: Vec<(i64,)> = sqlx::query_as(
        "SELECT version FROM _sqlx_migrations WHERE success = 1 ORDER BY version ASC",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(versions.len(), 6, "All 6 migrations must be applied");
    assert_eq!(versions[5].0, 6, "Latest migration version must be 6");

    // 2. PRAGMA table_info verification for exact columns and NULL-ability
    let columns: Vec<(i64, String, String, i64, Option<String>, i64)> =
        sqlx::query_as("PRAGMA table_info(favorites)")
            .fetch_all(&pool)
            .await
            .unwrap();

    let created_at_col = columns.iter().find(|c| c.1 == "repo_created_at");
    let ai_analysis_col = columns.iter().find(|c| c.1 == "ai_analysis");

    assert!(
        created_at_col.is_some(),
        "repo_created_at column must exist in favorites"
    );
    let created_at_info = created_at_col.unwrap();
    assert_eq!(created_at_info.2, "TEXT", "repo_created_at must be TEXT");
    assert_eq!(
        created_at_info.3, 0,
        "repo_created_at must allow NULL (notnull == 0)"
    );

    assert!(
        ai_analysis_col.is_some(),
        "ai_analysis column must exist in favorites"
    );
    let ai_analysis_info = ai_analysis_col.unwrap();
    assert_eq!(ai_analysis_info.2, "TEXT", "ai_analysis must be TEXT");
    assert_eq!(
        ai_analysis_info.3, 0,
        "ai_analysis must allow NULL (notnull == 0)"
    );

    // Verify exactly 21 columns in favorites (19 baseline + 2 new)
    assert_eq!(
        columns.len(),
        21,
        "Favorites table must have exactly 21 columns (19 baseline + 2 new)"
    );

    pool.close().await;
}

// =============================================================================
// 2. Fixture Upgrade (005 -> 006) and Pre-Migration Backup Gate
// =============================================================================

#[tokio::test]
async fn test_fixture_upgrade_from_005_to_006_and_backup_creation() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("reposhelf.db");

    // Create v1.1.1 fixture (migrations 001..005)
    let fixture_pool = create_v1_1_1_fixture(&db_path).await;
    fixture_pool.close().await;

    // Run init_pool which should trigger ensure_pre_v1_1_2_backup before running 006
    let pool = init_pool(&db_path)
        .await
        .expect("init_pool on v1.1.1 fixture must succeed");

    // 1. Verify pre-v1.1.2 backup exists and passes integrity check
    let backup_path = dir.path().join("backups").join("reposhelf-pre-v1.1.2.db");
    assert!(
        backup_path.exists(),
        "Pre-v1.1.2 backup must exist after upgrade"
    );
    validate_backup_file(&backup_path)
        .await
        .expect("Pre-v1.1.2 backup must pass validation");

    // 2. Verify all 6 migrations recorded
    let versions: Vec<(i64,)> = sqlx::query_as(
        "SELECT version FROM _sqlx_migrations WHERE success = 1 ORDER BY version ASC",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(versions.len(), 6);

    // 3. Verify all existing v1.1.1 data preserved unchanged
    let row = sqlx::query(
        r#"
        SELECT id, name, description, category, stars, license, pushed_at, rating,
               rating_reason, rating_confidence, notes, releases, languages,
               description_translations, description_translation_source_hash,
               repo_created_at, ai_analysis
        FROM favorites WHERE id = 1
        "#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    let name: String = row.get("name");
    let desc: Option<String> = row.get("description");
    let stars: i64 = row.get("stars");
    let releases: Option<i64> = row.get("releases");
    let repo_created_at: Option<String> = row.get("repo_created_at");
    let ai_analysis: Option<String> = row.get("ai_analysis");

    assert_eq!(name, "fixture-repo");
    assert_eq!(desc.as_deref(), Some("Representative v1.1.1 description"));
    assert_eq!(stars, 1250);
    assert_eq!(releases, Some(42));

    // 4. Verify NULL semantics on upgraded row
    assert!(
        repo_created_at.is_none(),
        "repo_created_at must be NULL for upgraded rows"
    );
    assert!(
        ai_analysis.is_none(),
        "ai_analysis must be NULL for upgraded rows"
    );

    // 5. Verify tags and selection history preserved
    let tag_count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM favorite_tags WHERE favorite_id = 1")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(tag_count.0, 3, "Tags must be preserved");

    let history_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM selection_history")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(history_count.0, 1, "Selection history must be preserved");

    pool.close().await;

    // 6. Reopen database: verify idempotency, no duplicate backup created
    let pool2 = init_pool(&db_path)
        .await
        .expect("Reopen init_pool must succeed");
    let versions2: Vec<(i64,)> = sqlx::query_as(
        "SELECT version FROM _sqlx_migrations WHERE success = 1 ORDER BY version ASC",
    )
    .fetch_all(&pool2)
    .await
    .unwrap();
    assert_eq!(versions2.len(), 6);
    pool2.close().await;
}

// =============================================================================
// 3. Created_at Domain Contract & Normalization Tests
// =============================================================================

#[tokio::test]
async fn test_repo_created_at_utc_normalization_and_invalid_rejection() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("created_at_test.db");
    let pool = init_pool(&db_path).await.unwrap();

    // 1. Insert with offset timezone: +08:00 should normalize to UTC 'Z'
    let input = FavoriteInput {
        github_repo_id: Some(1001),
        github_url: "https://github.com/owner/repo1".to_string(),
        name: "repo1".to_string(),
        description: Some("description".to_string()),
        category: "Developer Tools".to_string(),
        stars: 10,
        license: Some("MIT".to_string()),
        topics: vec![],
        pushed_at: Some("2024-03-01T08:00:00Z".to_string()),
        repo_created_at: Some("2024-03-01T16:00:00+08:00".to_string()),
        releases: None,
        languages: None,
        translation: None,
        ai_analysis: None,
    };

    let item = insert_favorite_tx(&pool, &input).await.unwrap();
    assert_eq!(
        item.repo_created_at.as_deref(),
        Some("2024-03-01T08:00:00Z"),
        "repo_created_at must be normalized to UTC ending with 'Z'"
    );

    // 2. Insert with invalid non-RFC3339 string should reject and store as None
    let input_invalid = FavoriteInput {
        github_repo_id: Some(1002),
        github_url: "https://github.com/owner/repo2".to_string(),
        name: "repo2".to_string(),
        description: None,
        category: "Developer Tools".to_string(),
        stars: 5,
        license: None,
        topics: vec![],
        pushed_at: None,
        repo_created_at: Some("not-a-valid-date".to_string()),
        releases: None,
        languages: None,
        translation: None,
        ai_analysis: None,
    };

    let item_invalid = insert_favorite_tx(&pool, &input_invalid).await.unwrap();
    assert_eq!(
        item_invalid.repo_created_at, None,
        "Invalid RFC3339 string must not be stored as created_at"
    );

    // 3. Metadata refresh Keep / Replace:
    // A) If refresh provides valid created_at, update to new value
    let update_valid = GitHubRepository {
        github_repo_id: 1001,
        github_url: "https://github.com/owner/repo1".to_string(),
        name: "repo1".to_string(),
        description: None,
        category: "Developer Tools".to_string(),
        stars: 20,
        license: None,
        topics: vec![],
        pushed_at: None,
        created_at: Some("2024-01-01T00:00:00Z".to_string()),
        releases: None,
        languages: None,
    };
    update_github_owned_metadata_tx(
        &pool,
        item.id,
        &update_valid,
        "2024-03-02T12:00:00Z",
        MetadataPatch::Keep,
        MetadataPatch::Keep,
    )
    .await
    .unwrap();

    let fetched = find_favorite_by_id(&pool, item.id).await.unwrap().unwrap();
    assert_eq!(
        fetched.repo_created_at.as_deref(),
        Some("2024-01-01T00:00:00Z")
    );

    // B) If refresh provides None or invalid created_at, KEEP existing value
    let update_none = GitHubRepository {
        github_repo_id: 1001,
        github_url: "https://github.com/owner/repo1".to_string(),
        name: "repo1".to_string(),
        description: None,
        category: "Developer Tools".to_string(),
        stars: 25,
        license: None,
        topics: vec![],
        pushed_at: None,
        created_at: None, // Failed to fetch created_at
        releases: None,
        languages: None,
    };
    update_github_owned_metadata_tx(
        &pool,
        item.id,
        &update_none,
        "2024-03-03T12:00:00Z",
        MetadataPatch::Keep,
        MetadataPatch::Keep,
    )
    .await
    .unwrap();

    let fetched_kept = find_favorite_by_id(&pool, item.id).await.unwrap().unwrap();
    assert_eq!(
        fetched_kept.repo_created_at.as_deref(),
        Some("2024-01-01T00:00:00Z"),
        "Missing created_at on refresh must KEEP existing value"
    );

    pool.close().await;
}

// =============================================================================
// 4. AI Analysis Persistence Foundation (Schema v1, Search Text, Validation)
// =============================================================================

#[test]
fn test_analysis_deterministic_search_text_and_validation() {
    let mut envelope = sample_analysis_envelope();
    envelope.search_text = "original-wrong-text".to_string();

    let derived = derive_search_text(&envelope.content);
    // Must contain overview, features, use_cases, tech_stack, setup, target_users, maturity, limitations
    assert!(derived.contains("Overview: Fast and reliable system"));
    assert!(derived.contains("Features: Async engine Memory safety"));
    assert!(derived.contains("Use Cases: Microservices"));
    assert!(derived.contains("Tech Stack: Rust Tokio"));
    assert!(derived.contains("Setup: cargo build --release"));
    assert!(derived.contains("Target Users: Backend developers"));
    assert!(derived.contains("Maturity: Production ready"));
    assert!(derived.contains("Limitations: Requires SQLite 3.38+"));

    // Validation & serialization: recomputes search_text ignoring caller's original-wrong-text
    let (serialized, derived_search) = validate_and_serialize_envelope(&envelope).unwrap();
    let reparsed: serde_json::Value = serde_json::from_str(&serialized).unwrap();
    assert_eq!(
        reparsed.get("search_text").unwrap().as_str().unwrap(),
        derived.as_str()
    );
    assert_eq!(derived_search, derived);

    // Item character limit check: item text > 360 chars rejected
    let mut invalid_envelope = sample_analysis_envelope();
    invalid_envelope.content.overview.items[0].text = "A".repeat(361);
    assert!(
        validate_and_serialize_envelope(&invalid_envelope).is_err(),
        "Item text exceeding 360 characters must be rejected"
    );

    // Schema version check: schema_version != 1 rejected
    let mut bad_version_envelope = sample_analysis_envelope();
    bad_version_envelope.schema_version = 2;
    assert!(
        validate_and_serialize_envelope(&bad_version_envelope).is_err(),
        "schema_version != 1 must be rejected"
    );
}

#[tokio::test]
async fn test_analysis_read_tolerance() {
    // 1. NULL in DB
    let (env, status) = parse_db_analysis(None);
    assert!(env.is_none());
    assert_eq!(status, AnalysisReadStatus::None);

    // 2. Valid Schema v1 JSON
    let valid_envelope = sample_analysis_envelope();
    let (valid_json, _) = validate_and_serialize_envelope(&valid_envelope).unwrap();
    let (env_valid, status_valid) = parse_db_analysis(Some(&valid_json));
    assert!(env_valid.is_some());
    assert_eq!(status_valid, AnalysisReadStatus::Valid);

    // 3. Corrupt JSON
    let corrupt_json = "{ invalid_json: true ";
    let (env_corrupt, status_corrupt) = parse_db_analysis(Some(corrupt_json));
    assert!(env_corrupt.is_none());
    assert!(matches!(status_corrupt, AnalysisReadStatus::Corrupt { .. }));

    // 4. Unsupported Schema Version
    let unsupported_json = r#"{"schema_version": 99, "content": {}, "search_text": ""}"#;
    let (env_unsupported, status_unsupported) = parse_db_analysis(Some(unsupported_json));
    assert!(env_unsupported.is_none());
    assert!(matches!(status_unsupported, AnalysisReadStatus::Unsupported { .. }));

    // 5. Valid JSON but not an object (scalar)
    let scalar_json = r#""just a string""#;
    let (env_scalar, status_scalar) = parse_db_analysis(Some(scalar_json));
    assert!(env_scalar.is_none());
    assert!(matches!(status_scalar, AnalysisReadStatus::Corrupt { .. }));
}

// =============================================================================
// 5. Atomic Favorite Persistence & User Metadata Keep/Replace
// =============================================================================

#[tokio::test]
async fn test_atomic_favorite_persistence_with_analysis() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("atomic_test.db");
    let pool = init_pool(&db_path).await.unwrap();

    let valid_envelope = sample_analysis_envelope();

    // 1. Successful atomic insertion
    let input = FavoriteInput {
        github_repo_id: Some(2001),
        github_url: "https://github.com/owner/atomic1".to_string(),
        name: "atomic1".to_string(),
        description: Some("desc".to_string()),
        category: "Developer Tools".to_string(),
        stars: 100,
        license: Some("MIT".to_string()),
        topics: vec!["db".to_string()],
        pushed_at: None,
        repo_created_at: Some("2024-01-01T12:00:00Z".to_string()),
        releases: None,
        languages: None,
        translation: None,
        ai_analysis: Some(valid_envelope.clone()),
    };

    let item = insert_favorite_tx(&pool, &input).await.unwrap();
    assert_eq!(item.analysis_status, Some(AnalysisReadStatus::Valid));
    assert!(item.ai_analysis.is_some());

    // 2. Failed atomic insertion rolls back whole favorite
    let mut invalid_envelope = sample_analysis_envelope();
    invalid_envelope.content.features.items[0].text = "OverLimit".repeat(100); // 900 chars > 360 chars

    let bad_input = FavoriteInput {
        github_repo_id: Some(2002),
        github_url: "https://github.com/owner/atomic2".to_string(),
        name: "atomic2".to_string(),
        description: None,
        category: "Developer Tools".to_string(),
        stars: 50,
        license: None,
        topics: vec![],
        pushed_at: None,
        repo_created_at: None,
        releases: None,
        languages: None,
        translation: None,
        ai_analysis: Some(invalid_envelope),
    };

    let insert_res = insert_favorite_tx(&pool, &bad_input).await;
    assert!(
        insert_res.is_err(),
        "Invalid analysis must fail favorite creation"
    );

    // Verify favorite 2002 was NOT persisted
    let check: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM favorites WHERE github_repo_id = 2002")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        check.0, 0,
        "Transaction rollback must ensure favorite is not created"
    );

    // 3. User metadata Keep semantics
    let update_keep = UpdateFavoriteUserMetadataInput {
        favorite_id: item.id,
        category: "Libraries & Frameworks".to_string(),
        rating: Some(4),
        rating_reason: None,
        rating_confidence: None,
        notes: "Updated notes".to_string(),
        user_tags: vec!["tag1".to_string()],
        ai_tags: None,
        translation_patch: None,
        analysis_patch: Some(AnalysisPatchInput::Keep),
    };
    update_user_metadata_tx(&pool, &update_keep)
        .await
        .unwrap();

    let fetched = find_favorite_by_id(&pool, item.id).await.unwrap().unwrap();
    assert_eq!(fetched.category, "Libraries & Frameworks");
    assert_eq!(
        fetched.analysis_status,
        Some(AnalysisReadStatus::Valid),
        "Keep semantics must preserve existing analysis"
    );

    // 4. User metadata Replace semantics
    let mut new_envelope = sample_analysis_envelope();
    new_envelope.content.overview.items[0].text = "Replaced overview content".to_string();
    let update_replace = UpdateFavoriteUserMetadataInput {
        favorite_id: item.id,
        category: "Libraries & Frameworks".to_string(),
        rating: Some(4),
        rating_reason: None,
        rating_confidence: None,
        notes: "Updated notes".to_string(),
        user_tags: vec!["tag1".to_string()],
        ai_tags: None,
        translation_patch: None,
        analysis_patch: Some(AnalysisPatchInput::Replace { envelope: new_envelope }),
    };
    update_user_metadata_tx(&pool, &update_replace)
        .await
        .unwrap();

    let fetched_replaced = find_favorite_by_id(&pool, item.id).await.unwrap().unwrap();
    assert_eq!(fetched_replaced.analysis_status, Some(AnalysisReadStatus::Valid));
    assert!(
        fetched_replaced
            .ai_analysis
            .unwrap()
            .search_text
            .contains("Replaced overview content"),
        "Replace semantics must update ai_analysis with new content"
    );

    pool.close().await;
}
