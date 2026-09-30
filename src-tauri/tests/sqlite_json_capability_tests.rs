use tempfile::tempdir;
use reposhelf_lib::db::database::init_pool;
use sqlx::Row;

/// Verification of SQLite JSON runtime capabilities in RepoShelf's actual sqlx/SQLite toolchain.
/// Required by RepoShelf v1.1.2 S0 AC005.
/// All tests execute against an isolated temporary SQLite database created via `init_pool`.

#[tokio::test]
async fn test_sqlite_json_valid_capability() {
    let dir = tempdir().expect("Failed to create tempdir");
    let db_path = dir.path().join("test_json_valid.db");
    let pool = init_pool(&db_path)
        .await
        .expect("Failed to initialize isolated database pool");

    // 1. Valid JSON test
    let valid_json = r#"{"content":{"summary":"demo"},"search_text":"docker rust"}"#;
    let row: (i64,) = sqlx::query_as("SELECT json_valid(?)")
        .bind(valid_json)
        .fetch_one(&pool)
        .await
        .expect("json_valid on valid JSON should execute without error");
    assert_eq!(row.0, 1, "json_valid on valid JSON must return 1");

    // 2. Malformed JSON test (e.g. truncated opening bracket)
    let malformed_json = r#"{"content":"#;
    let row_malformed: (i64,) = sqlx::query_as("SELECT json_valid(?)")
        .bind(malformed_json)
        .fetch_one(&pool)
        .await
        .expect("json_valid on malformed JSON should return cleanly without crashing");
    assert_eq!(row_malformed.0, 0, "json_valid on malformed JSON must return 0");

    // 3. NULL input test
    let row_null: (Option<i64>,) = sqlx::query_as("SELECT json_valid(?)")
        .bind(None::<String>)
        .fetch_one(&pool)
        .await
        .expect("json_valid on NULL should execute without error");
    assert_eq!(row_null.0, None, "json_valid on NULL must return NULL");
}

#[tokio::test]
async fn test_sqlite_json_extract_search_text() {
    let dir = tempdir().expect("Failed to create tempdir");
    let db_path = dir.path().join("test_json_extract.db");
    let pool = init_pool(&db_path)
        .await
        .expect("Failed to initialize isolated database pool");

    let sample_ai_analysis = r#"{
  "schema_version": 1,
  "content": {
    "summary": "Repository analysis test",
    "features": ["A", "B"]
  },
  "search_text": "repository analysis docker rust"
}"#;

    // Direct extraction of $.search_text
    let row: (String,) = sqlx::query_as("SELECT json_extract(?, '$.search_text')")
        .bind(sample_ai_analysis)
        .fetch_one(&pool)
        .await
        .expect("json_extract on $.search_text must succeed");

    assert_eq!(
        row.0, "repository analysis docker rust",
        "json_extract must safely and accurately extract search_text"
    );

    // Extraction of schema_version
    let row_version: (i64,) = sqlx::query_as("SELECT json_extract(?, '$.schema_version')")
        .bind(sample_ai_analysis)
        .fetch_one(&pool)
        .await
        .expect("json_extract on $.schema_version must succeed");
    assert_eq!(row_version.0, 1);
}

#[tokio::test]
async fn test_sqlite_json_guarded_extraction_resilience() {
    let dir = tempdir().expect("Failed to create tempdir");
    let db_path = dir.path().join("test_json_guard.db");
    let pool = init_pool(&db_path)
        .await
        .expect("Failed to initialize isolated database pool");

    // Create a temporary table mimicking favorites with an ai_analysis TEXT column
    sqlx::query(
        "CREATE TABLE test_fixtures (
            id INTEGER PRIMARY KEY,
            ai_analysis TEXT
        )"
    )
    .execute(&pool)
    .await
    .expect("Failed to create test table");

    let valid_analysis = r#"{"schema_version": 1, "search_text": "extracted keywords"}"#;
    let malformed_analysis = r#"{"schema_version": 1, broken_json..."#;

    // Row 1: Valid JSON
    sqlx::query("INSERT INTO test_fixtures (id, ai_analysis) VALUES (?, ?)")
        .bind(1)
        .bind(valid_analysis)
        .execute(&pool)
        .await
        .unwrap();

    // Row 2: NULL
    sqlx::query("INSERT INTO test_fixtures (id, ai_analysis) VALUES (?, NULL)")
        .bind(2)
        .execute(&pool)
        .await
        .unwrap();

    // Row 3: Malformed JSON
    sqlx::query("INSERT INTO test_fixtures (id, ai_analysis) VALUES (?, ?)")
        .bind(3)
        .bind(malformed_analysis)
        .execute(&pool)
        .await
        .unwrap();

    // Guarded extraction query as specified in RepoShelf v1.1.2 Technical Spec §14.3
    let rows = sqlx::query(
        r#"
        SELECT id,
          CASE
            WHEN ai_analysis IS NOT NULL AND json_valid(ai_analysis) = 1
            THEN json_extract(ai_analysis, '$.search_text')
            ELSE NULL
          END AS extracted
        FROM test_fixtures
        ORDER BY id ASC
        "#
    )
    .fetch_all(&pool)
    .await
    .expect("Guarded extraction must execute smoothly without errors across all rows");

    assert_eq!(rows.len(), 3);

    // Row 1 (Valid) -> Some("extracted keywords")
    let r1_val: Option<String> = rows[0].get("extracted");
    assert_eq!(r1_val, Some("extracted keywords".to_string()));

    // Row 2 (NULL) -> None
    let r2_val: Option<String> = rows[1].get("extracted");
    assert_eq!(r2_val, None);

    // Row 3 (Malformed) -> None (guarded by json_valid, does NOT fail query)
    let r3_val: Option<String> = rows[2].get("extracted");
    assert_eq!(r3_val, None);
}

#[tokio::test]
async fn test_sqlite_json_parameterized_like_compatibility() {
    let dir = tempdir().expect("Failed to create tempdir");
    let db_path = dir.path().join("test_json_like.db");
    let pool = init_pool(&db_path)
        .await
        .expect("Failed to initialize isolated database pool");

    sqlx::query(
        "CREATE TABLE test_search_fixtures (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            ai_analysis TEXT
        )"
    )
    .execute(&pool)
    .await
    .unwrap();

    let json_rust = r#"{"schema_version": 1, "search_text": "systems programming memory safety"}"#;
    let json_docker = r#"{"schema_version": 1, "search_text": "containerization deployment devops"}"#;
    let json_bad = r#"{"schema_version": broken"#;

    sqlx::query("INSERT INTO test_search_fixtures (id, name, ai_analysis) VALUES (?, ?, ?)")
        .bind(1).bind("rust-project").bind(json_rust).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO test_search_fixtures (id, name, ai_analysis) VALUES (?, ?, ?)")
        .bind(2).bind("docker-tool").bind(json_docker).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO test_search_fixtures (id, name, ai_analysis) VALUES (?, ?, ?)")
        .bind(3).bind("corrupt-entry").bind(json_bad).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO test_search_fixtures (id, name, ai_analysis) VALUES (?, ?, ?)")
        .bind(4).bind("null-entry").bind(None::<String>).execute(&pool).await.unwrap();

    // Query mimicking S4 local search with guarded json_extract inside LIKE clause
    let search_sql = r#"
        SELECT id, name FROM test_search_fixtures
        WHERE (
            name LIKE ?
            OR (
                CASE
                    WHEN ai_analysis IS NOT NULL AND json_valid(ai_analysis) = 1
                    THEN json_extract(ai_analysis, '$.search_text')
                    ELSE ''
                END
            ) LIKE ?
        )
        ORDER BY id ASC
    "#;

    // Search for "memory": should match row 1 via ai_analysis
    let pattern_memory = "%memory%";
    let results_memory: Vec<(i64, String)> = sqlx::query_as(search_sql)
        .bind(pattern_memory)
        .bind(pattern_memory)
        .fetch_all(&pool)
        .await
        .expect("Search query must execute without error");

    assert_eq!(results_memory.len(), 1);
    assert_eq!(results_memory[0].0, 1);
    assert_eq!(results_memory[0].1, "rust-project");

    // Search for "containerization": should match row 2 via ai_analysis
    let pattern_container = "%containerization%";
    let results_container: Vec<(i64, String)> = sqlx::query_as(search_sql)
        .bind(pattern_container)
        .bind(pattern_container)
        .fetch_all(&pool)
        .await
        .expect("Search query must execute without error");

    assert_eq!(results_container.len(), 1);
    assert_eq!(results_container[0].0, 2);
    assert_eq!(results_container[0].1, "docker-tool");

    // Search for "corrupt": matches row 3 via name, corrupt JSON does not blow up the query
    let pattern_corrupt = "%corrupt%";
    let results_corrupt: Vec<(i64, String)> = sqlx::query_as(search_sql)
        .bind(pattern_corrupt)
        .bind(pattern_corrupt)
        .fetch_all(&pool)
        .await
        .expect("Search query must execute without error");

    assert_eq!(results_corrupt.len(), 1);
    assert_eq!(results_corrupt[0].0, 3);
}
