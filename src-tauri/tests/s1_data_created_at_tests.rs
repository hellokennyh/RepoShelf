use std::collections::HashMap;
use std::fs;
use std::path::Path;
use tempfile::tempdir;
use sha2::{Digest, Sha256};
use sqlx::Row;

use reposhelf_lib::db::analysis::{
    derive_search_text, parse_db_analysis, validate_and_serialize_envelope,
    AnalysisContent, AnalysisCoverage, AnalysisEnvelopeV1, AnalysisGenerator,
    AnalysisItem, AnalysisLocale, AnalysisPatchInput, AnalysisReadStatus,
    AnalysisSection, AnalysisSource, ClaimBasis, EvidenceRange, EvidenceRecord, SectionStatus,
};
use reposhelf_lib::db::database::{
    ensure_pre_v1_1_2_backup, init_pool, validate_backup_file,
};
use reposhelf_lib::db::translation::{
    DescriptionTranslationInput, TranslationPatchInput,
};
use reposhelf_lib::github::models::GitHubRepository;
use reposhelf_lib::library::models::{
    FavoriteInput, MetadataPatch, UpdateFavoriteUserMetadataInput,
};
use reposhelf_lib::library::repository::{
    find_favorite_by_id, insert_favorite_tx, list_favorites, update_github_owned_metadata_tx,
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
// AC007 — 两列增量
// =============================================================================

#[tokio::test]
async fn test_ac007_two_column_increment_and_no_extraneous_tables_or_columns() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("fresh_006.db");

    let pool = init_pool(&db_path)
        .await
        .expect("Fresh init_pool must succeed");

    // 1. Verify PRAGMA table_info has exactly 21 columns (19 baseline + 2 new)
    let columns: Vec<(i64, String, String, i64, Option<String>, i64)> =
        sqlx::query_as("PRAGMA table_info(favorites)")
            .fetch_all(&pool)
            .await
            .unwrap();

    assert_eq!(
        columns.len(),
        21,
        "Favorites table must have exactly 21 columns (19 baseline + 2 new)"
    );

    let created_at_col = columns.iter().find(|c| c.1 == "repo_created_at");
    let ai_analysis_col = columns.iter().find(|c| c.1 == "ai_analysis");

    assert!(created_at_col.is_some(), "repo_created_at column must exist");
    let c_info = created_at_col.unwrap();
    assert_eq!(c_info.2, "TEXT", "repo_created_at must be TEXT");
    assert_eq!(c_info.3, 0, "repo_created_at must allow NULL");

    assert!(ai_analysis_col.is_some(), "ai_analysis column must exist");
    let a_info = ai_analysis_col.unwrap();
    assert_eq!(a_info.2, "TEXT", "ai_analysis must be TEXT");
    assert_eq!(a_info.3, 0, "ai_analysis must allow NULL");

    // 2. Verify NO separate analysis child tables, NO FTS tables, NO extraneous business tables
    let tables: Vec<(String,)> = sqlx::query_as(
        "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
    )
    .fetch_all(&pool)
    .await
    .unwrap();

    let table_names: Vec<String> = tables.into_iter().map(|t| t.0).collect();
    assert!(table_names.contains(&"favorites".to_string()));
    assert!(table_names.contains(&"favorite_tags".to_string()));
    assert!(table_names.contains(&"selection_history".to_string()));
    assert!(table_names.contains(&"_sqlx_migrations".to_string()));
    assert_eq!(
        table_names.len(),
        4,
        "Expected exactly 4 business/migration tables, found: {:?}",
        table_names
    );

    // 3. Verify no separate search_text column exists on favorites
    assert!(
        columns.iter().all(|c| c.1 != "search_text"),
        "search_text must NOT be a separate DB column"
    );

    pool.close().await;
}

// =============================================================================
// AC008 — 旧迁移不变
// =============================================================================

#[test]
fn test_ac008_historical_migration_integrity_and_hashes() {
    let migrations_dir = Path::new("migrations");
    let expected_hashes = [
        ("001_initial_schema.sql", "8F6CBCA185B7C7F5602B7C85039F2A9EFB1FAC8D6C994ADE796121BF14547AD1"),
        ("002_rating_suggestions.sql", "91D557895CA7F4CA153FA39C51A6C6D1ABDD07017431A21568D91082AA33C1B5"),
        ("003_ai_tag_dedup_cleanup.sql", "A38B429B23AD39C3B863ADD8AB86355010464B3CB95D81113B1D07BCC9491C59"),
        ("004_repository_metadata.sql", "9E601214F916ECADC5EA8C3BEE2667560ABC037401CBD94D3005460244654E34"),
        ("005_description_translation.sql", "92C23A718A620BDBF2BFC788EDFFE9196505DA082C2957483FDBEF59C2813BCD"),
        ("006_repo_created_at_and_ai_analysis.sql", "ABBC9E0BD1020EF963C14DF342E389B73F0EFFA8CFEDD459108C77726C8DDDE7"),
    ];

    for (file_name, expected_hash) in &expected_hashes {
        let file_path = migrations_dir.join(file_name);
        assert!(file_path.exists(), "Migration file '{}' must exist", file_name);

        let bytes = fs::read(&file_path).unwrap();
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        let actual_hash = format!("{:X}", hasher.finalize());

        assert_eq!(
            actual_hash, *expected_hash,
            "SHA256 mismatch for migration file '{}'",
            file_name
        );
    }
}

// =============================================================================
// AC009 — 全新库
// =============================================================================

#[tokio::test]
async fn test_ac009_fresh_database_nullable_columns() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("fresh_ac009.db");

    let pool = init_pool(&db_path).await.unwrap();

    // Verify all 6 migrations applied
    let versions: Vec<(i64,)> = sqlx::query_as(
        "SELECT version FROM _sqlx_migrations WHERE success = 1 ORDER BY version ASC",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(versions.len(), 6);

    // Insert record with NULL in both new fields
    let input = FavoriteInput {
        github_repo_id: Some(8881),
        github_url: "https://github.com/owner/fresh-repo".to_string(),
        name: "fresh-repo".to_string(),
        description: Some("Fresh repo description".to_string()),
        category: "Developer Tools".to_string(),
        stars: 100,
        license: Some("MIT".to_string()),
        topics: vec!["rust".to_string()],
        pushed_at: None,
        repo_created_at: None,
        releases: None,
        languages: None,
        translation: None,
        ai_analysis: None,
    };

    let item = insert_favorite_tx(&pool, &input).await.unwrap();
    assert_eq!(item.repo_created_at, None);
    assert_eq!(item.ai_analysis, None);
    assert_eq!(item.analysis_status, Some(AnalysisReadStatus::None));

    // Introspect DB raw values
    let row = sqlx::query("SELECT repo_created_at, ai_analysis FROM favorites WHERE id = ?")
        .bind(item.id)
        .fetch_one(&pool)
        .await
        .unwrap();

    let db_created_at: Option<String> = row.get("repo_created_at");
    let db_analysis: Option<String> = row.get("ai_analysis");
    assert!(db_created_at.is_none(), "DB repo_created_at must be SQL NULL");
    assert!(db_analysis.is_none(), "DB ai_analysis must be SQL NULL");

    pool.close().await;
}

// =============================================================================
// AC010 — 旧库升级
// =============================================================================

#[tokio::test]
async fn test_ac010_fixture_upgrade_from_005_and_backup_creation() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("reposhelf.db");

    let fixture_pool = create_v1_1_1_fixture(&db_path).await;
    fixture_pool.close().await;

    // Upgrading to 006 must trigger pre-v1.1.2 backup
    let pool = init_pool(&db_path)
        .await
        .expect("init_pool on v1.1.1 fixture must succeed");

    let backup_path = dir.path().join("backups").join("reposhelf-pre-v1.1.2.db");
    assert!(backup_path.exists(), "Pre-v1.1.2 backup must exist");
    validate_backup_file(&backup_path)
        .await
        .expect("Backup must pass integrity check");

    // Verify all 19 columns of existing record are intact
    let row = sqlx::query(
        r#"
        SELECT id, github_repo_id, github_url, name, description, category, stars, license,
               pushed_at, rating, rating_reason, rating_confidence, notes, favorited_at,
               metadata_refreshed_at, releases, languages, description_translations,
               description_translation_source_hash, repo_created_at, ai_analysis
        FROM favorites WHERE id = 1
        "#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(row.get::<String, _>("name"), "fixture-repo");
    assert_eq!(row.get::<i64, _>("stars"), 1250);
    assert_eq!(row.get::<Option<i64>, _>("releases"), Some(42));
    assert!(row.get::<Option<String>, _>("repo_created_at").is_none());
    assert!(row.get::<Option<String>, _>("ai_analysis").is_none());

    pool.close().await;
}

// =============================================================================
// AC011 — 迁移失败与重启
// =============================================================================

#[tokio::test]
async fn test_ac011_migration_failure_injection_recovery_and_idempotence() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("reposhelf.db");

    // 1. Create clean v1.1.1 fixture database
    let fixture_pool = create_v1_1_1_fixture(&db_path).await;

    // 2. Pre-create the pre-v1.1.2 backup from the clean v1.1.1 state
    ensure_pre_v1_1_2_backup(&fixture_pool, &db_path)
        .await
        .expect("pre-backup of clean v1.1.1 must succeed");

    // 3. Inject failure: add conflicting column `repo_created_at INTEGER` to favorites table
    sqlx::query("ALTER TABLE favorites ADD COLUMN repo_created_at INTEGER")
        .execute(&fixture_pool)
        .await
        .unwrap();
    fixture_pool.close().await;

    // 4. Attempt init_pool: migration 006 must fail because column `repo_created_at` already exists
    let init_result = init_pool(&db_path).await;
    assert!(
        init_result.is_err(),
        "Migration 006 must fail due to conflicting column"
    );

    let err_msg = init_result.err().unwrap().to_string();
    assert!(
        err_msg.contains("duplicate column name: repo_created_at") || err_msg.contains("migration"),
        "Failure must be diagnosable, received: {}",
        err_msg
    );

    // 4. Verify that pre-v1.1.2 backup was safely created BEFORE migration attempted
    let backup_path = dir.path().join("backups").join("reposhelf-pre-v1.1.2.db");
    assert!(
        backup_path.exists(),
        "Pre-v1.1.2 backup must exist even when migration fails"
    );
    validate_backup_file(&backup_path)
        .await
        .expect("Pre-v1.1.2 backup must be valid");

    // 5. Connect directly to check database state:
    // - version 6 must NOT be recorded as success = 1 in _sqlx_migrations
    // - old business data in favorites, tags, and selection_history is intact
    let raw_options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(&db_path)
        .read_only(true);
    let direct_pool = sqlx::SqlitePool::connect_with(raw_options).await.unwrap();

    let v6_row: Option<(i64, bool)> = sqlx::query_as(
        "SELECT version, success FROM _sqlx_migrations WHERE version = 6",
    )
    .fetch_optional(&direct_pool)
    .await
    .unwrap();
    assert!(
        v6_row.is_none() || !v6_row.unwrap().1,
        "Migration version 6 must NOT be recorded as successful"
    );

    let fav_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM favorites")
        .fetch_one(&direct_pool)
        .await
        .unwrap();
    assert_eq!(fav_count.0, 1, "Existing business data must not be lost");

    let tag_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM favorite_tags")
        .fetch_one(&direct_pool)
        .await
        .unwrap();
    assert_eq!(tag_count.0, 3, "Existing tags must not be lost");
    direct_pool.close().await;

    // 6. Fix condition: restore clean database from the verified pre-v1.1.2 backup
    let recovered_db_path = dir.path().join("recovered.db");
    fs::copy(&backup_path, &recovered_db_path).expect("Restore from backup must succeed");

    // 7. Re-initialize: migration 006 must now succeed cleanly on restored database
    let recovered_pool = init_pool(&recovered_db_path)
        .await
        .expect("init_pool after recovery must succeed");

    let versions: Vec<(i64,)> = sqlx::query_as(
        "SELECT version FROM _sqlx_migrations WHERE success = 1 ORDER BY version ASC",
    )
    .fetch_all(&recovered_pool)
    .await
    .unwrap();
    assert_eq!(versions.len(), 6, "All 6 migrations must now be recorded");
    assert_eq!(versions[5].0, 6);

    recovered_pool.close().await;

    // 8. Reopen database: verify idempotence, no duplicate migration run
    let reopened_pool = init_pool(&recovered_db_path)
        .await
        .expect("Reopening database must succeed idempotently");
    let versions2: Vec<(i64,)> = sqlx::query_as(
        "SELECT version FROM _sqlx_migrations WHERE success = 1 ORDER BY version ASC",
    )
    .fetch_all(&reopened_pool)
    .await
    .unwrap();
    assert_eq!(versions2.len(), 6);
    reopened_pool.close().await;
}

// =============================================================================
// AC012 — 创建时间来源
// =============================================================================

#[tokio::test]
async fn test_ac012_created_at_source_distinctness_and_normalization() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("ac012.db");
    let pool = init_pool(&db_path).await.unwrap();

    let input = FavoriteInput {
        github_repo_id: Some(1012),
        github_url: "https://github.com/owner/ac012-repo".to_string(),
        name: "ac012-repo".to_string(),
        description: Some("description".to_string()),
        category: "Developer Tools".to_string(),
        stars: 10,
        license: Some("MIT".to_string()),
        topics: vec![],
        pushed_at: Some("2024-03-01T08:00:00Z".to_string()),
        repo_created_at: Some("2020-05-15T16:00:00+08:00".to_string()),
        releases: None,
        languages: None,
        translation: None,
        ai_analysis: None,
    };

    let item = insert_favorite_tx(&pool, &input).await.unwrap();

    // Must be normalized to UTC ending with 'Z'
    assert_eq!(
        item.repo_created_at.as_deref(),
        Some("2020-05-15T08:00:00Z"),
        "repo_created_at must be normalized from +08:00 to UTC Z"
    );

    // Must be distinct from favorited_at and pushed_at
    assert_ne!(
        item.repo_created_at.as_deref(),
        Some(item.favorited_at.as_str()),
        "repo_created_at must not equal favorited_at"
    );
    assert_ne!(
        item.repo_created_at.as_deref(),
        item.pushed_at.as_deref(),
        "repo_created_at must not equal pushed_at"
    );

    // Invalid non-RFC3339 string stored as None
    let bad_input = FavoriteInput {
        github_repo_id: Some(1013),
        github_url: "https://github.com/owner/ac012-bad".to_string(),
        name: "ac012-bad".to_string(),
        description: None,
        category: "Developer Tools".to_string(),
        stars: 1,
        license: None,
        topics: vec![],
        pushed_at: None,
        repo_created_at: Some("invalid-date-string".to_string()),
        releases: None,
        languages: None,
        translation: None,
        ai_analysis: None,
    };

    let bad_item = insert_favorite_tx(&pool, &bad_input).await.unwrap();
    assert_eq!(bad_item.repo_created_at, None);

    pool.close().await;
}

// =============================================================================
// AC013 — 时间本地化与跨日跨月时区转换
// =============================================================================

#[tokio::test]
async fn test_ac013_timezone_offsets_and_leap_day_normalization() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("ac013.db");
    let pool = init_pool(&db_path).await.unwrap();

    // 1. Positive timezone with leap day: 2024-02-29 16:30 +08:00 -> 2024-02-29 08:30:00Z
    let in1 = FavoriteInput {
        github_repo_id: Some(1014),
        github_url: "https://github.com/owner/leap-pos".to_string(),
        name: "leap-pos".to_string(),
        category: "Developer Tools".to_string(),
        stars: 5,
        repo_created_at: Some("2024-02-29T16:30:00+08:00".to_string()),
        ..Default::default()
    };
    let item1 = insert_favorite_tx(&pool, &in1).await.unwrap();
    assert_eq!(
        item1.repo_created_at.as_deref(),
        Some("2024-02-29T08:30:00Z")
    );

    // 2. Negative timezone across month boundary: 2024-03-01 02:00 -05:00 -> 2024-03-01 07:00:00Z
    let in2 = FavoriteInput {
        github_repo_id: Some(1015),
        github_url: "https://github.com/owner/month-neg".to_string(),
        name: "month-neg".to_string(),
        category: "Developer Tools".to_string(),
        stars: 5,
        repo_created_at: Some("2024-03-01T02:00:00-05:00".to_string()),
        ..Default::default()
    };
    let item2 = insert_favorite_tx(&pool, &in2).await.unwrap();
    assert_eq!(
        item2.repo_created_at.as_deref(),
        Some("2024-03-01T07:00:00Z")
    );

    // 3. Positive timezone across month boundary into leap day: 2024-03-01 02:00 +08:00 -> 2024-02-29 18:00:00Z
    let in3 = FavoriteInput {
        github_repo_id: Some(1016),
        github_url: "https://github.com/owner/month-leap-cross".to_string(),
        name: "month-leap-cross".to_string(),
        category: "Developer Tools".to_string(),
        stars: 5,
        repo_created_at: Some("2024-03-01T02:00:00+08:00".to_string()),
        ..Default::default()
    };
    let item3 = insert_favorite_tx(&pool, &in3).await.unwrap();
    assert_eq!(
        item3.repo_created_at.as_deref(),
        Some("2024-02-29T18:00:00Z")
    );

    pool.close().await;
}

// =============================================================================
// AC016 — 旧记录补齐
// =============================================================================

#[tokio::test]
async fn test_ac016_old_favorite_backfill_semantics() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("ac016.db");
    let pool = init_pool(&db_path).await.unwrap();

    // 1. Initial state: favorite exists with repo_created_at = NULL
    let input = FavoriteInput {
        github_repo_id: Some(1017),
        github_url: "https://github.com/owner/ac016-repo".to_string(),
        name: "ac016-repo".to_string(),
        category: "Developer Tools".to_string(),
        stars: 10,
        repo_created_at: None,
        ..Default::default()
    };
    let item = insert_favorite_tx(&pool, &input).await.unwrap();
    assert_eq!(item.repo_created_at, None);

    // Listing favorites does not modify or fill repo_created_at
    let list_res = list_favorites(&pool, &Default::default()).await.unwrap();
    let listed = list_res.into_iter().find(|f| f.id == item.id).unwrap();
    assert_eq!(listed.repo_created_at, None);

    // 2. User explicitly triggers metadata refresh with valid created_at -> REPLACES/backfills
    let update_valid = GitHubRepository {
        github_repo_id: 1017,
        github_url: "https://github.com/owner/ac016-repo".to_string(),
        name: "ac016-repo".to_string(),
        description: None,
        category: "Developer Tools".to_string(),
        stars: 15,
        license: None,
        topics: vec![],
        pushed_at: None,
        created_at: Some("2022-07-20T09:00:00Z".to_string()),
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
        Some("2022-07-20T09:00:00Z"),
        "Refresh with valid created_at must backfill repo_created_at"
    );

    // 3. User refreshes again, but GitHub returns None -> KEEP existing value
    let update_none = GitHubRepository {
        created_at: None,
        ..update_valid.clone()
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
        Some("2022-07-20T09:00:00Z"),
        "Missing created_at on refresh must KEEP existing value"
    );

    // 4. User refreshes with invalid date string -> KEEP existing value
    let update_invalid = GitHubRepository {
        created_at: Some("not-valid-date".to_string()),
        ..update_valid.clone()
    };
    update_github_owned_metadata_tx(
        &pool,
        item.id,
        &update_invalid,
        "2024-03-04T12:00:00Z",
        MetadataPatch::Keep,
        MetadataPatch::Keep,
    )
    .await
    .unwrap();

    let fetched_kept_again = find_favorite_by_id(&pool, item.id).await.unwrap().unwrap();
    assert_eq!(
        fetched_kept_again.repo_created_at.as_deref(),
        Some("2022-07-20T09:00:00Z"),
        "Invalid created_at on refresh must KEEP existing value"
    );

    pool.close().await;
}

// =============================================================================
// AC017 — 只改旧字段不丢失新字段
// =============================================================================

#[tokio::test]
async fn test_ac017_old_field_updates_preserve_created_at_and_analysis() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("ac017.db");
    let pool = init_pool(&db_path).await.unwrap();

    let valid_envelope = sample_analysis_envelope();
    let original_created_at = "2023-05-10T08:00:00Z";

    let input = FavoriteInput {
        github_repo_id: Some(1018),
        github_url: "https://github.com/owner/ac017-repo".to_string(),
        name: "ac017-repo".to_string(),
        description: Some("Base description".to_string()),
        category: "Developer Tools".to_string(),
        stars: 50,
        license: Some("MIT".to_string()),
        topics: vec!["initial-tag".to_string()],
        pushed_at: Some("2024-01-01T00:00:00Z".to_string()),
        repo_created_at: Some(original_created_at.to_string()),
        releases: Some(1),
        languages: None,
        translation: None,
        ai_analysis: Some(valid_envelope.clone()),
    };

    let item = insert_favorite_tx(&pool, &input).await.unwrap();
    assert_eq!(item.repo_created_at.as_deref(), Some(original_created_at));
    assert_eq!(item.analysis_status, Some(AnalysisReadStatus::Valid));

    // 1. Update notes only
    let mut update_input = UpdateFavoriteUserMetadataInput {
        favorite_id: item.id,
        category: "Developer Tools".to_string(),
        rating: None,
        rating_reason: None,
        rating_confidence: None,
        notes: "Modified notes only".to_string(),
        user_tags: vec![],
        ai_tags: None,
        translation_patch: None,
        analysis_patch: None, // Missing patch = Keep
    };
    update_user_metadata_tx(&pool, &update_input).await.unwrap();

    let f1 = find_favorite_by_id(&pool, item.id).await.unwrap().unwrap();
    assert_eq!(f1.notes, "Modified notes only");
    assert_eq!(f1.repo_created_at.as_deref(), Some(original_created_at));
    assert_eq!(f1.analysis_status, Some(AnalysisReadStatus::Valid));

    // 2. Update category only
    update_input.category = "Libraries & Frameworks".to_string();
    update_user_metadata_tx(&pool, &update_input).await.unwrap();
    let f2 = find_favorite_by_id(&pool, item.id).await.unwrap().unwrap();
    assert_eq!(f2.category, "Libraries & Frameworks");
    assert_eq!(f2.repo_created_at.as_deref(), Some(original_created_at));
    assert_eq!(f2.analysis_status, Some(AnalysisReadStatus::Valid));

    // 3. Update user tags only
    update_input.user_tags = vec!["custom-tag-1".to_string(), "custom-tag-2".to_string()];
    update_user_metadata_tx(&pool, &update_input).await.unwrap();
    let f3 = find_favorite_by_id(&pool, item.id).await.unwrap().unwrap();
    assert_eq!(f3.tags.len(), 3); // 1 github + 2 user
    assert_eq!(f3.repo_created_at.as_deref(), Some(original_created_at));
    assert_eq!(f3.analysis_status, Some(AnalysisReadStatus::Valid));

    // 4. Update rating, rating_reason, rating_confidence
    update_input.rating = Some(5);
    update_input.rating_reason = Some("Great tool".to_string());
    update_input.rating_confidence = Some("high".to_string());
    update_user_metadata_tx(&pool, &update_input).await.unwrap();
    let f4 = find_favorite_by_id(&pool, item.id).await.unwrap().unwrap();
    assert_eq!(f4.rating, Some(5));
    assert_eq!(f4.repo_created_at.as_deref(), Some(original_created_at));
    assert_eq!(f4.analysis_status, Some(AnalysisReadStatus::Valid));

    // 5. Update ai_tags
    update_input.ai_tags = Some(vec!["ai-inferred-tag".to_string()]);
    update_user_metadata_tx(&pool, &update_input).await.unwrap();
    let f5 = find_favorite_by_id(&pool, item.id).await.unwrap().unwrap();
    assert_eq!(f5.repo_created_at.as_deref(), Some(original_created_at));
    assert_eq!(f5.analysis_status, Some(AnalysisReadStatus::Valid));

    // 6. Update translation via translation_patch
    update_input.translation_patch = Some(TranslationPatchInput::Replace {
        text: "中文翻译".to_string(),
        target_locale: "zh-Hans".to_string(),
        source_text: "Base description".to_string(),
    });
    update_user_metadata_tx(&pool, &update_input).await.unwrap();
    let f6 = find_favorite_by_id(&pool, item.id).await.unwrap().unwrap();
    assert!(f6.description_translation.is_some());
    assert_eq!(f6.repo_created_at.as_deref(), Some(original_created_at));
    assert_eq!(f6.analysis_status, Some(AnalysisReadStatus::Valid));

    // 7. Normal metadata refresh (releases, stars, pushed_at) preserves ai_analysis
    let gh_update = GitHubRepository {
        github_repo_id: 1018,
        github_url: "https://github.com/owner/ac017-repo".to_string(),
        name: "ac017-repo".to_string(),
        description: Some("Base description".to_string()),
        category: "Developer Tools".to_string(),
        stars: 999,
        license: Some("MIT".to_string()),
        topics: vec![],
        pushed_at: Some("2024-03-01T00:00:00Z".to_string()),
        created_at: Some(original_created_at.to_string()),
        releases: Some(10),
        languages: None,
    };
    update_input.analysis_patch = Some(AnalysisPatchInput::Keep);
    update_github_owned_metadata_tx(
        &pool,
        item.id,
        &gh_update,
        "2024-03-05T12:00:00Z",
        MetadataPatch::Replace(10),
        MetadataPatch::Keep,
    )
    .await
    .unwrap();

    let f7 = find_favorite_by_id(&pool, item.id).await.unwrap().unwrap();
    assert_eq!(f7.stars, 999);
    assert_eq!(f7.releases, Some(10));
    assert_eq!(f7.repo_created_at.as_deref(), Some(original_created_at));
    assert_eq!(f7.analysis_status, Some(AnalysisReadStatus::Valid));

    pool.close().await;
}

// =============================================================================
// AC018 — 报告持久化边界与非法值拒绝
// =============================================================================

#[test]
fn test_ac018_analysis_storage_boundary_and_rejections() {
    let mut envelope = sample_analysis_envelope();

    // 1. Deterministic search text re-derivation (tampering ignored)
    envelope.search_text = "tampered-search-text-attacker-provided".to_string();
    let (serialized, derived_search) = validate_and_serialize_envelope(&envelope).unwrap();
    assert!(!serialized.contains("tampered-search-text"));
    assert!(serialized.contains("Overview: Fast and reliable system"));
    assert_eq!(derived_search, derive_search_text(&envelope.content));

    // 2. Reject non-1 schema_version
    let mut bad_v = envelope.clone();
    bad_v.schema_version = 2;
    assert!(validate_and_serialize_envelope(&bad_v).is_err());

    // 3. Reject non-UTC generated_at
    let mut bad_time = envelope.clone();
    bad_time.generated_at = "2024-03-01T12:00:00+08:00".to_string();
    assert!(validate_and_serialize_envelope(&bad_time).is_err());

    // 4. Reject duplicate evidence IDs
    let mut dup_ev = envelope.clone();
    dup_ev.evidence.push(EvidenceRecord {
        id: "ev-meta-1".to_string(), // duplicate!
        kind: "metadata".to_string(),
        path: None,
        commit_sha: None,
        blob_sha: None,
        observed_at: "2024-03-01T12:00:00Z".to_string(),
        included_ranges: vec![],
        included_content_hash: "hash2".to_string(),
        truncated: false,
        public_fact_summary: None,
    });
    dup_ev.coverage.evidence_count = 2;
    assert!(validate_and_serialize_envelope(&dup_ev).is_err());

    // 5. Reject invalid evidence range
    let mut bad_range = envelope.clone();
    bad_range.evidence[0].included_ranges.push(EvidenceRange {
        start_line: 10,
        end_line: 5, // end < start
    });
    assert!(validate_and_serialize_envelope(&bad_range).is_err());

    // 6. Reject executed_code = true
    let mut exec_code = envelope.clone();
    exec_code.coverage.executed_code = true;
    assert!(validate_and_serialize_envelope(&exec_code).is_err());

    // 7. Reject verified_deployment = true
    let mut ver_dep = envelope.clone();
    ver_dep.coverage.verified_deployment = true;
    assert!(validate_and_serialize_envelope(&ver_dep).is_err());

    // 8. Reject item text exceeding 360 characters
    let mut long_item = envelope.clone();
    long_item.content.overview.items[0].text = "X".repeat(361);
    assert!(validate_and_serialize_envelope(&long_item).is_err());

    // 9. Reject missing_information exceeding 200 characters
    let mut long_missing = envelope.clone();
    long_missing.content.overview.missing_information = vec!["Y".repeat(201)];
    assert!(validate_and_serialize_envelope(&long_missing).is_err());

    // 10. Reject code_observed basis referencing non-file evidence
    let mut bad_basis = envelope.clone();
    bad_basis.content.features.items[0].basis = ClaimBasis::CodeObserved;
    // evidence 0 is kind: "metadata", not "file"
    assert!(validate_and_serialize_envelope(&bad_basis).is_err());

    // 11. Reject oversized envelope (> 128 KiB)
    let mut big_env = envelope.clone();
    big_env.evidence.clear();
    for i in 0..64 {
        big_env.evidence.push(EvidenceRecord {
            id: format!("ev-{}", i),
            kind: "metadata".to_string(),
            path: None,
            commit_sha: None,
            blob_sha: None,
            observed_at: "2024-03-01T12:00:00Z".to_string(),
            included_ranges: vec![],
            included_content_hash: "H".repeat(2000), // inflate size
            truncated: false,
            public_fact_summary: Some("Fact".repeat(50)),
        });
    }
    big_env.coverage.evidence_count = 64;
    assert!(validate_and_serialize_envelope(&big_env).is_err());

    // 12. Boundary values tested against parse_db_analysis
    let (env_none, status_none) = parse_db_analysis(None);
    assert!(env_none.is_none());
    assert_eq!(status_none, AnalysisReadStatus::None);

    let (env_empty, status_empty) = parse_db_analysis(Some("   "));
    assert!(env_empty.is_none());
    assert_eq!(status_empty, AnalysisReadStatus::None);

    let (env_null, status_null) = parse_db_analysis(Some("null"));
    assert!(env_null.is_none());
    assert_eq!(status_null, AnalysisReadStatus::None);
}

// =============================================================================
// AC019 — 坏数据容错
// =============================================================================

#[tokio::test]
async fn test_ac019_bad_data_tolerance_across_all_variants() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("ac019.db");
    let pool = init_pool(&db_path).await.unwrap();

    let valid_envelope = sample_analysis_envelope();
    let (valid_json, _) = validate_and_serialize_envelope(&valid_envelope).unwrap();

    // 8 distinct test variants for ai_analysis column:
    // 1. SQL NULL
    // 2. Valid Schema v1 JSON
    // 3. Malformed JSON
    // 4. JSON null literal "null"
    // 5. JSON array "[1, 2, 3]"
    // 6. JSON scalar string "\"scalar string\""
    // 7. Known schema 1 but missing required sections (corrupt shape)
    // 8. Future/unknown schema version (schema_version 99)
    let test_variants: Vec<(i64, &str, Option<&str>)> = vec![
        (1, "repo-null", None),
        (2, "repo-valid-v1", Some(&valid_json)),
        (3, "repo-malformed", Some("{ broken json : true")),
        (4, "repo-json-null", Some("null")),
        (5, "repo-array", Some("[1, 2, 3]")),
        (6, "repo-scalar", Some("\"scalar string\"")),
        (7, "repo-wrong-shape", Some(r#"{"schema_version": 1, "bad_content": true}"#)),
        (8, "repo-future-schema", Some(r#"{"schema_version": 99, "content": {}}"#)),
    ];

    for (id, name, analysis_val) in &test_variants {
        sqlx::query(
            r#"
            INSERT INTO favorites (
                id, github_repo_id, github_url, name, description, category, stars, license,
                favorited_at, metadata_refreshed_at, repo_created_at, ai_analysis
            ) VALUES (?, ?, ?, ?, 'Sample desc', 'Developer Tools', 10, 'MIT',
                      '2024-01-01T00:00:00Z', '2024-01-01T00:00:00Z', '2023-01-01T00:00:00Z', ?)
            "#,
        )
        .bind(id)
        .bind(id * 100)
        .bind(format!("https://github.com/owner/{}", name))
        .bind(name)
        .bind(analysis_val)
        .execute(&pool)
        .await
        .unwrap();
    }

    // A. list_favorites MUST NOT crash or fail, and must return all 8 records
    let list = list_favorites(&pool, &Default::default()).await.unwrap();
    assert_eq!(list.len(), 8, "All 8 favorites must be returned despite bad data");

    for fav in &list {
        assert_eq!(fav.repo_created_at.as_deref(), Some("2023-01-01T00:00:00Z"));
        assert_eq!(fav.category, "Developer Tools");

        match fav.id {
            1 => {
                assert_eq!(fav.analysis_status, Some(AnalysisReadStatus::None));
                assert!(fav.ai_analysis.is_none());
            }
            2 => {
                assert_eq!(fav.analysis_status, Some(AnalysisReadStatus::Valid));
                assert!(fav.ai_analysis.is_some());
            }
            3 => {
                assert!(matches!(fav.analysis_status, Some(AnalysisReadStatus::Corrupt { .. })));
                assert!(fav.ai_analysis.is_none());
            }
            4 => {
                assert_eq!(fav.analysis_status, Some(AnalysisReadStatus::None));
                assert!(fav.ai_analysis.is_none());
            }
            5 => {
                assert!(matches!(fav.analysis_status, Some(AnalysisReadStatus::Corrupt { .. })));
                assert!(fav.ai_analysis.is_none());
            }
            6 => {
                assert!(matches!(fav.analysis_status, Some(AnalysisReadStatus::Corrupt { .. })));
                assert!(fav.ai_analysis.is_none());
            }
            7 => {
                assert!(matches!(fav.analysis_status, Some(AnalysisReadStatus::Corrupt { .. })));
                assert!(fav.ai_analysis.is_none());
            }
            8 => {
                assert!(matches!(
                    fav.analysis_status,
                    Some(AnalysisReadStatus::Unsupported { schema_version: Some(99), .. })
                ));
                assert!(fav.ai_analysis.is_none());
            }
            _ => unreachable!(),
        }
    }

    // B. find_favorite_by_id MUST NOT crash for any variant
    for (id, _, _) in &test_variants {
        let single = find_favorite_by_id(&pool, *id).await.unwrap();
        assert!(single.is_some());
    }

    // C. Database raw column values MUST NOT be modified or wiped by reading
    for (id, _, expected_val) in &test_variants {
        let row = sqlx::query("SELECT ai_analysis FROM favorites WHERE id = ?")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
        let db_val: Option<String> = row.get("ai_analysis");
        assert_eq!(
            db_val.as_deref(),
            *expected_val,
            "Raw database value for id {} must remain completely untouched",
            id
        );
    }

    pool.close().await;
}

// =============================================================================
// AC020 — 原子收藏与失败回滚
// =============================================================================

#[tokio::test]
async fn test_ac020_atomic_favorite_persistence_and_failure_rollback() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("ac020.db");
    let pool = init_pool(&db_path).await.unwrap();

    let valid_envelope = sample_analysis_envelope();

    // 1. Successful atomic insertion with all features
    let mut langs = HashMap::new();
    langs.insert("Rust".to_string(), 85.0);
    langs.insert("TypeScript".to_string(), 15.0);

    let full_input = FavoriteInput {
        github_repo_id: Some(2020),
        github_url: "https://github.com/owner/atomic-full".to_string(),
        name: "atomic-full".to_string(),
        description: Some("Full description".to_string()),
        category: "Developer Tools".to_string(),
        stars: 500,
        license: Some("Apache-2.0".to_string()),
        topics: vec!["topic1".to_string(), "topic2".to_string()],
        pushed_at: Some("2024-03-01T00:00:00Z".to_string()),
        repo_created_at: Some("2022-01-01T00:00:00Z".to_string()),
        releases: Some(5),
        languages: Some(langs),
        translation: Some(DescriptionTranslationInput {
            text: "完整描述翻译".to_string(),
            target_locale: "zh-Hans".to_string(),
        }),
        ai_analysis: Some(valid_envelope),
    };

    let item = insert_favorite_tx(&pool, &full_input).await.unwrap();
    assert_eq!(item.repo_created_at.as_deref(), Some("2022-01-01T00:00:00Z"));
    assert_eq!(item.releases, Some(5));
    assert!(item.description_translation.is_some());
    assert_eq!(item.analysis_status, Some(AnalysisReadStatus::Valid));

    // Verify tag table committed
    let tag_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM favorite_tags WHERE favorite_id = ?")
        .bind(item.id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(tag_count.0, 2);

    // 2. Failure injection: invalid analysis envelope validation failure rolls back entire transaction
    let mut bad_envelope = sample_analysis_envelope();
    bad_envelope.content.overview.items[0].text = "Z".repeat(500); // 500 > 360 chars!

    let bad_input = FavoriteInput {
        github_repo_id: Some(2021),
        github_url: "https://github.com/owner/atomic-bad".to_string(),
        name: "atomic-bad".to_string(),
        description: Some("Description".to_string()),
        category: "Developer Tools".to_string(),
        stars: 10,
        license: None,
        topics: vec!["should-not-persist".to_string()],
        pushed_at: None,
        repo_created_at: Some("2022-01-01T00:00:00Z".to_string()),
        releases: None,
        languages: None,
        translation: None,
        ai_analysis: Some(bad_envelope),
    };

    let bad_res = insert_favorite_tx(&pool, &bad_input).await;
    assert!(bad_res.is_err(), "Invalid analysis must abort insertion");

    // Verify 0 rows persisted for 2021
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM favorites WHERE github_repo_id = 2021")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count.0, 0, "Transaction rollback must ensure no favorite row created");

    let orphaned_tags: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM favorite_tags WHERE tag = 'should-not-persist'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(orphaned_tags.0, 0, "No orphaned tags must be created");

    // 3. Failure injection: translation provided without repository description
    let bad_trans_input = FavoriteInput {
        github_repo_id: Some(2022),
        github_url: "https://github.com/owner/atomic-no-desc".to_string(),
        name: "atomic-no-desc".to_string(),
        description: None, // No description!
        category: "Developer Tools".to_string(),
        stars: 10,
        license: None,
        topics: vec![],
        pushed_at: None,
        repo_created_at: None,
        releases: None,
        languages: None,
        translation: Some(DescriptionTranslationInput {
            text: "orphan translation".to_string(),
            target_locale: "zh-Hans".to_string(),
        }),
        ai_analysis: None,
    };
    let trans_res = insert_favorite_tx(&pool, &bad_trans_input).await;
    assert!(trans_res.is_err());

    let count2: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM favorites WHERE github_repo_id = 2022")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count2.0, 0);

    pool.close().await;
}
