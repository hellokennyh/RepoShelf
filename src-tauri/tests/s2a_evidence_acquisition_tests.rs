use std::sync::Arc;
use base64::Engine;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use reposhelf_lib::db::analysis::EvidenceRecord;
use reposhelf_lib::github::evidence::{
    build_github_contents_url, compute_evidence_fingerprint, is_binary_content, is_excluded_path,
    is_sensitive_secret_path, process_file_content, redact_secret_values,
    select_evidence_files, validate_redirect_target, validate_repository_target,
    AcquisitionCancelToken, AcquisitionOptions, GitTreeEntry,
    RepositoryEvidenceService, SELECTOR_VERSION,
};

use reposhelf_lib::github::GitHubClient;

fn encode_base64(s: &str) -> String {
    base64::engine::general_purpose::STANDARD.encode(s.as_bytes())
}

// -----------------------------------------------------------------------------
// AC033: Public Repository Identity Verification
// -----------------------------------------------------------------------------

#[test]
fn test_ac033_valid_public_repo_identity() {
    // Canonical URL
    let id1 = validate_repository_target("https://github.com/rust-lang/rust", None).unwrap();
    assert_eq!(id1.owner, "rust-lang");
    assert_eq!(id1.repo, "rust");
    assert_eq!(id1.canonical_url, "https://github.com/rust-lang/rust");

    // Trailing slash
    let id2 = validate_repository_target("https://github.com/tokio-rs/tokio/", None).unwrap();
    assert_eq!(id2.owner, "tokio-rs");
    assert_eq!(id2.repo, "tokio");

    // .git suffix
    let id3 = validate_repository_target("https://github.com/facebook/react.git", None).unwrap();
    assert_eq!(id3.owner, "facebook");
    assert_eq!(id3.repo, "react");

    // Owner/repo slug
    let id4 = validate_repository_target("tauri-apps/tauri", Some(12345)).unwrap();
    assert_eq!(id4.owner, "tauri-apps");
    assert_eq!(id4.repo, "tauri");
    assert_eq!(id4.canonical_url, "https://github.com/tauri-apps/tauri");
    assert_eq!(id4.expected_repo_id, Some(12345));
}

#[test]
fn test_ac033_invalid_and_non_github_targets_rejected() {
    // Non-GitHub hosts
    assert!(validate_repository_target("https://gitlab.com/owner/repo", None).is_err());
    assert!(validate_repository_target("https://bitbucket.org/owner/repo", None).is_err());
    assert!(validate_repository_target("https://example.com/owner/repo", None).is_err());

    // Localhost and loopback
    assert!(validate_repository_target("http://localhost/owner/repo", None).is_err());
    assert!(validate_repository_target("http://127.0.0.1/owner/repo", None).is_err());
    assert!(validate_repository_target("http://127.0.0.1:8080/owner/repo", None).is_err());
    assert!(validate_repository_target("127.0.0.1/owner/repo", None).is_err());

    // Cloud metadata endpoint (SSRF)
    assert!(validate_repository_target("http://169.254.169.254/latest/meta-data/", None).is_err());

    // Empty and invalid slugs
    assert!(validate_repository_target("", None).is_err());
    assert!(validate_repository_target("   ", None).is_err());
    assert!(validate_repository_target("just-an-owner", None).is_err());
    assert!(validate_repository_target("owner/repo/subpath", None).is_err());

    // Subpages
    assert!(validate_repository_target("https://github.com/owner/repo/issues", None).is_err());
    assert!(validate_repository_target("https://github.com/owner/repo/pull/1", None).is_err());
    assert!(validate_repository_target("https://github.com/owner/repo/blob/main/README.md", None).is_err());
}

#[tokio::test]
async fn test_ac033_private_repo_and_expected_id_mismatch() {
    let mock_server = MockServer::start().await;

    // Mock private repository response
    Mock::given(method("GET"))
        .and(path("/repos/org/private-repo"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": 1001,
            "html_url": "https://github.com/org/private-repo",
            "name": "private-repo",
            "private": true,
            "stargazers_count": 50,
            "topics": []
        })))
        .mount(&mock_server)
        .await;

    let github = Arc::new(GitHubClient::with_base_url(&mock_server.uri()));
    let service = RepositoryEvidenceService::new(github);

    let err = service
        .acquire_evidence("org/private-repo", None, AcquisitionOptions::default(), None)
        .await
        .unwrap_err();

    assert!(
        err.to_string().contains("Private repositories are unsupported"),
        "Expected private repo error, got: {}",
        err
    );

    // Mock ID mismatch response
    Mock::given(method("GET"))
        .and(path("/repos/org/mismatch-repo"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": 2002,
            "html_url": "https://github.com/org/mismatch-repo",
            "name": "mismatch-repo",
            "private": false,
            "stargazers_count": 100,
            "topics": []
        })))
        .mount(&mock_server)
        .await;

    let err_mismatch = service
        .acquire_evidence(
            "org/mismatch-repo",
            None,
            AcquisitionOptions {
                expected_repo_id: Some(9999),
                ..Default::default()
            },
            None,
        )
        .await
        .unwrap_err();

    assert!(
        err_mismatch.to_string().contains("Repository ID mismatch"),
        "Expected ID mismatch error, got: {}",
        err_mismatch
    );
}

// -----------------------------------------------------------------------------
// AC034: Fixed Commit Snapshot & Empty Repo Handling
// -----------------------------------------------------------------------------

#[tokio::test]
async fn test_ac034_fixed_commit_resolution_and_pinned_requests() {
    let mock_server = MockServer::start().await;

    // 1. Repo metadata declaring default_branch: "develop"
    Mock::given(method("GET"))
        .and(path("/repos/test-owner/test-repo"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": 5555,
            "html_url": "https://github.com/test-owner/test-repo",
            "name": "test-repo",
            "default_branch": "develop",
            "private": false,
            "stargazers_count": 42,
            "topics": ["rust"]
        })))
        .mount(&mock_server)
        .await;

    // 2. Commit resolution: develop -> exact commit SHA
    let exact_sha = "a1b2c3d4e5f6789012345678901234567890abcd";
    Mock::given(method("GET"))
        .and(path("/repos/test-owner/test-repo/commits/develop"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "sha": exact_sha
        })))
        .mount(&mock_server)
        .await;

    // 3. Tree request pinned to exact_sha
    let tree_path = format!("/repos/test-owner/test-repo/git/trees/{}", exact_sha);
    Mock::given(method("GET"))
        .and(path(&tree_path))
        .and(query_param("recursive", "1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "sha": exact_sha,
            "tree": [
                {
                    "path": "README.md",
                    "mode": "100644",
                    "type": "blob",
                    "sha": "blob111",
                    "size": 100
                }
            ],
            "truncated": false
        })))
        .mount(&mock_server)
        .await;

    // 4. File content request pinned to ref={exact_sha}
    Mock::given(method("GET"))
        .and(path("/repos/test-owner/test-repo/contents/README.md"))
        .and(query_param("ref", exact_sha))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "name": "README.md",
            "path": "README.md",
            "sha": "blob111",
            "size": 30,
            "type": "file",
            "content": encode_base64("# Test Project\nAn amazing tool."),
            "encoding": "base64"
        })))
        .mount(&mock_server)
        .await;

    // 5. Releases endpoint
    Mock::given(method("GET"))
        .and(path("/repos/test-owner/test-repo/releases"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .mount(&mock_server)
        .await;

    let github = Arc::new(GitHubClient::with_base_url(&mock_server.uri()));
    let service = RepositoryEvidenceService::new(github);

    let pack = service
        .acquire_evidence("test-owner/test-repo", None, AcquisitionOptions::default(), None)
        .await
        .unwrap();

    assert_eq!(pack.source.commit_sha.as_deref(), Some(exact_sha));
    assert_eq!(pack.source.default_branch.as_deref(), Some("develop"));
    assert_eq!(pack.selected_texts.len(), 1);
    assert_eq!(pack.selected_texts[0].commit_sha.as_deref(), Some(exact_sha));
}

#[tokio::test]
async fn test_ac034_empty_repository_handling() {
    let mock_server = MockServer::start().await;

    // Repo metadata: size = 0, default_branch = null
    Mock::given(method("GET"))
        .and(path("/repos/empty-org/empty-repo"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": 6666,
            "html_url": "https://github.com/empty-org/empty-repo",
            "name": "empty-repo",
            "size": 0,
            "default_branch": null,
            "private": false,
            "stargazers_count": 0,
            "topics": []
        })))
        .mount(&mock_server)
        .await;

    // Releases endpoint
    Mock::given(method("GET"))
        .and(path("/repos/empty-org/empty-repo/releases"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .mount(&mock_server)
        .await;

    let github = Arc::new(GitHubClient::with_base_url(&mock_server.uri()));
    let service = RepositoryEvidenceService::new(github);

    let pack = service
        .acquire_evidence("empty-org/empty-repo", None, AcquisitionOptions::default(), None)
        .await
        .unwrap();

    assert!(pack.metadata_summary.is_empty);
    assert_eq!(pack.source.commit_sha, None);
    assert!(pack.coverage.gaps.iter().any(|g| g.subject == "commit_sha"));
}

// -----------------------------------------------------------------------------
// AC035: README and Long Document Handling
// -----------------------------------------------------------------------------

#[test]
fn test_ac035_heading_aware_bounded_selection_preserves_qualifiers() {
    let long_markdown = r#"# Big Library
Welcome to the big library project.

## Architecture
Architecture is clean and modular.

## Benchmarks
Detailed benchmark results with lots of numbers.
Line 1 of benchmarks.
Line 2 of benchmarks.
Line 3 of benchmarks.

## Limitations and Status
WARNING: This project is experimental and not recommended for production use.
Support for Windows is currently unsupported and planned for v2.0.
Legacy v1 API is deprecated.

## Internal Changelog
Thousands of lines of internal changelog notes.
"#;

    // Extract bounded markdown with small byte limit
    let (extracted, truncated, ranges) =
        reposhelf_lib::github::evidence::extract_heading_aware_bounded_markdown(long_markdown, 500);

    assert!(truncated, "Expected document to be truncated");
    assert!(!ranges.is_empty(), "Expected non-empty included ranges");

    // Must preserve preamble and Overview/Architecture
    assert!(extracted.contains("# Big Library"));

    // Critical: MUST preserve negative qualifiers and limitation statements!
    assert!(extracted.contains("WARNING: This project is experimental"));
    assert!(extracted.contains("not recommended for production use"));
    assert!(extracted.contains("unsupported"));
    assert!(extracted.contains("deprecated"));
    assert!(extracted.contains("planned for v2.0"));
}

#[tokio::test]
async fn test_ac035_readme_absent_does_not_fail_task() {
    let mock_server = MockServer::start().await;
    let sha = "1111222233334444555566667777888899990000";

    Mock::given(method("GET"))
        .and(path("/repos/org/no-readme"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": 7777,
            "html_url": "https://github.com/org/no-readme",
            "name": "no-readme",
            "default_branch": "main",
            "private": false,
            "stargazers_count": 10,
            "topics": []
        })))
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/repos/org/no-readme/commits/main"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "sha": sha
        })))
        .mount(&mock_server)
        .await;

    // Tree with only Cargo.toml, no README
    Mock::given(method("GET"))
        .and(path(format!("/repos/org/no-readme/git/trees/{}", sha)))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "sha": sha,
            "tree": [
                {
                    "path": "Cargo.toml",
                    "mode": "100644",
                    "type": "blob",
                    "sha": "blob_cargo",
                    "size": 120
                }
            ],
            "truncated": false
        })))
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/repos/org/no-readme/contents/Cargo.toml"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "name": "Cargo.toml",
            "path": "Cargo.toml",
            "sha": "blob_cargo",
            "size": 120,
            "type": "file",
            "content": encode_base64("[package]\nname = \"no-readme\"\nversion = \"0.1.0\""),
            "encoding": "base64"
        })))
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/repos/org/no-readme/releases"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .mount(&mock_server)
        .await;

    let github = Arc::new(GitHubClient::with_base_url(&mock_server.uri()));
    let service = RepositoryEvidenceService::new(github);

    let pack = service
        .acquire_evidence("org/no-readme", None, AcquisitionOptions::default(), None)
        .await
        .unwrap();

    // Verify task succeeded and recorded missing README gap
    assert!(pack.coverage.gaps.iter().any(|g| g.subject == "README" && g.kind == "missing"));
    assert_eq!(pack.selected_texts.len(), 1);
    assert_eq!(pack.selected_texts[0].path, "Cargo.toml");
}

// -----------------------------------------------------------------------------
// AC036: Deterministic File Selector & Cap
// -----------------------------------------------------------------------------

#[test]
fn test_ac036_deterministic_selector_priority_and_20_file_cap() {
    let mut entries = Vec::new();

    // Add 30 markdown documentation files
    for i in 1..=30 {
        entries.push(GitTreeEntry {
            path: format!("docs/doc_{:02}.md", i),
            mode: "100644".to_string(),
            entry_type: "blob".to_string(),
            sha: format!("sha_doc_{}", i),
            size: Some(500),
        });
    }

    // Add 10 source files
    for i in 1..=10 {
        entries.push(GitTreeEntry {
            path: format!("src/core_{:02}.rs", i),
            mode: "100644".to_string(),
            entry_type: "blob".to_string(),
            sha: format!("sha_src_{}", i),
            size: Some(1000),
        });
    }

    // Add high-priority files
    entries.push(GitTreeEntry {
        path: "README.md".to_string(),
        mode: "100644".to_string(),
        entry_type: "blob".to_string(),
        sha: "sha_readme".to_string(),
        size: Some(2000),
    });
    entries.push(GitTreeEntry {
        path: "Cargo.toml".to_string(),
        mode: "100644".to_string(),
        entry_type: "blob".to_string(),
        sha: "sha_cargo".to_string(),
        size: Some(300),
    });
    entries.push(GitTreeEntry {
        path: "SKILL.md".to_string(),
        mode: "100644".to_string(),
        entry_type: "blob".to_string(),
        sha: "sha_skill".to_string(),
        size: Some(800),
    });

    // Run selector multiple times to verify exact reproducibility
    let plan1 = select_evidence_files(&entries, &[], &[]);
    let plan2 = select_evidence_files(&entries, &[], &[]);

    // Exactly 20 files selected
    assert_eq!(plan1.selected.len(), 20);
    assert_eq!(plan2.selected.len(), 20);

    // Exact identity and ordering match
    for (a, b) in plan1.selected.iter().zip(plan2.selected.iter()) {
        assert_eq!(a.path, b.path);
        assert_eq!(a.role, b.role);
    }

    // High priority files are guaranteed to be selected
    let selected_paths: Vec<&str> = plan1.selected.iter().map(|c| c.path.as_str()).collect();
    assert!(selected_paths.contains(&"README.md"));
    assert!(selected_paths.contains(&"Cargo.toml"));
    assert!(selected_paths.contains(&"SKILL.md"));
}

// -----------------------------------------------------------------------------
// AC037: Monorepo Bounded Selection
// -----------------------------------------------------------------------------

#[test]
fn test_ac037_monorepo_bounded_selection_and_coverage_gap() {
    let entries = vec![
        GitTreeEntry {
            path: "README.md".to_string(),
            mode: "100644".to_string(),
            entry_type: "blob".to_string(),
            sha: "sha_rm".to_string(),
            size: Some(100),
        },
        GitTreeEntry {
            path: "package.json".to_string(),
            mode: "100644".to_string(),
            entry_type: "blob".to_string(),
            sha: "sha_pkg".to_string(),
            size: Some(200),
        },
        GitTreeEntry {
            path: "packages/alpha/package.json".to_string(),
            mode: "100644".to_string(),
            entry_type: "blob".to_string(),
            sha: "sha_a".to_string(),
            size: Some(150),
        },
        GitTreeEntry {
            path: "packages/beta/package.json".to_string(),
            mode: "100644".to_string(),
            entry_type: "blob".to_string(),
            sha: "sha_b".to_string(),
            size: Some(150),
        },
        GitTreeEntry {
            path: "packages/gamma/package.json".to_string(),
            mode: "100644".to_string(),
            entry_type: "blob".to_string(),
            sha: "sha_c".to_string(),
            size: Some(150),
        },
        GitTreeEntry {
            path: "packages/delta/package.json".to_string(),
            mode: "100644".to_string(),
            entry_type: "blob".to_string(),
            sha: "sha_d".to_string(),
            size: Some(150),
        },
    ];

    let plan = select_evidence_files(&entries, &[], &[]);
    assert!(plan.is_monorepo, "Expected monorepo detection");

    let selected_paths: Vec<&str> = plan.selected.iter().map(|c| c.path.as_str()).collect();

    // Representative packages alpha and beta selected
    assert!(selected_paths.contains(&"packages/alpha/package.json"));
    assert!(selected_paths.contains(&"packages/beta/package.json"));

    // Packages gamma and delta not selected
    assert!(!selected_paths.contains(&"packages/gamma/package.json"));
    assert!(!selected_paths.contains(&"packages/delta/package.json"));

    // Must have gap for unselected packages
    assert!(plan
        .gaps
        .iter()
        .any(|g| g.subject == "monorepo_unselected_packages"));
}

// -----------------------------------------------------------------------------
// AC038: Truncated Tree & Directory Limit Detection
// -----------------------------------------------------------------------------

#[tokio::test]
async fn test_ac038_truncated_tree_detection_and_candidate_limit() {
    let mock_server = MockServer::start().await;
    let sha = "commit_tree_truncated";

    // Tree API returns truncated = true with 600 items
    let mut tree_items = Vec::new();
    for i in 1..=600 {
        tree_items.push(serde_json::json!({
            "path": format!("file_{}.txt", i),
            "mode": "100644",
            "type": "blob",
            "sha": format!("sha_{}", i),
            "size": 10
        }));
    }

    Mock::given(method("GET"))
        .and(path(format!("/repos/owner/trunc/git/trees/{}", sha)))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "sha": sha,
            "tree": tree_items,
            "truncated": true
        })))
        .mount(&mock_server)
        .await;

    let github = Arc::new(GitHubClient::with_base_url(&mock_server.uri()));
    let transport = reposhelf_lib::github::evidence::transport::AcquisitionTransport::new(github);
    let budget = reposhelf_lib::github::evidence::AcquisitionBudgetTracker::new(None);

    let res = reposhelf_lib::github::evidence::discover_bounded_tree(
        &transport, "owner", "trunc", sha, None, &budget,
    )
    .await
    .unwrap();

    assert!(res.truncated, "Expected truncated to be true");
    assert_eq!(
        res.candidate_paths.len(),
        500,
        "Candidate paths must be capped at 500"
    );
    assert_eq!(res.total_tree_entries, 600);
}

// -----------------------------------------------------------------------------
// AC039: Special Entries (Submodule, Symlink, Git LFS)
// -----------------------------------------------------------------------------

#[test]
fn test_ac039_special_entries_safe_detection() {
    let entries = vec![
        GitTreeEntry {
            path: "external_sub".to_string(),
            mode: "160000".to_string(),
            entry_type: "commit".to_string(),
            sha: "sub_sha".to_string(),
            size: None,
        },
        GitTreeEntry {
            path: "symlink_file".to_string(),
            mode: "120000".to_string(),
            entry_type: "blob".to_string(),
            sha: "sym_sha".to_string(),
            size: Some(20),
        },
        GitTreeEntry {
            path: "normal_file.txt".to_string(),
            mode: "100644".to_string(),
            entry_type: "blob".to_string(),
            sha: "norm_sha".to_string(),
            size: Some(100),
        },
    ];

    let plan = select_evidence_files(
        &entries,
        &["external_sub".to_string()],
        &["symlink_file".to_string()],
    );

    // Verify submodules and symlinks are never selected as text files
    assert!(!plan.selected.iter().any(|c| c.path == "external_sub"));
    assert!(!plan.selected.iter().any(|c| c.path == "symlink_file"));

    // Verify gaps recorded
    assert!(plan
        .gaps
        .iter()
        .any(|g| g.subject == "external_sub" && g.reason_code.contains("submodule")));
    assert!(plan
        .gaps
        .iter()
        .any(|g| g.subject == "symlink_file" && g.reason_code.contains("symlink")));

    // Git LFS Pointer Detection test
    let lfs_pointer_content = "version https://git-lfs.github.com/spec/v1\noid sha256:4b5b7b9f...\nsize 123456789\n";
    let res = process_file_content(lfs_pointer_content, None, false).unwrap();
    assert!(res.is_lfs_pointer, "Expected Git LFS pointer detection");
}

// -----------------------------------------------------------------------------
// AC040: Exclusion Rules, Binary Rejection, and Secret Redaction
// -----------------------------------------------------------------------------

#[test]
fn test_ac040_exclusion_rules() {
    // Excluded paths
    assert!(is_excluded_path("node_modules/express/index.js"));
    assert!(is_excluded_path("target/debug/build.rs"));
    assert!(is_excluded_path("dist/bundle.min.js"));
    assert!(is_excluded_path("vendor/autoload.php"));
    assert!(is_excluded_path(".cache/turbo/123"));
    assert!(is_excluded_path("package-lock.json"));
    assert!(is_excluded_path("yarn.lock"));
    assert!(is_excluded_path("Cargo.lock"));
    assert!(is_excluded_path("images/hero.png"));
    assert!(is_excluded_path("assets/font.woff2"));
    assert!(is_excluded_path("model/weights.safetensors"));
    assert!(is_excluded_path("data/dataset.parquet"));

    // Sensitive files
    assert!(is_sensitive_secret_path(".env"));
    assert!(is_sensitive_secret_path(".env.local"));
    assert!(is_sensitive_secret_path(".env.production"));
    assert!(is_sensitive_secret_path("id_rsa"));
    assert!(is_sensitive_secret_path("id_ed25519"));
    assert!(is_sensitive_secret_path("server.key"));
    assert!(is_sensitive_secret_path("cert.pem"));
    assert!(is_sensitive_secret_path("credentials.json"));
    assert!(is_sensitive_secret_path("token.json"));

    // Eligible files
    assert!(!is_excluded_path("src/main.rs"));
    assert!(!is_excluded_path("README.md"));
    assert!(!is_excluded_path("Cargo.toml"));
    assert!(!is_excluded_path("package.json"));
    assert!(!is_sensitive_secret_path("src/main.rs"));
}

#[test]
fn test_ac040_binary_rejection_and_secret_redaction() {
    // Binary rejection
    let binary_bytes = vec![0x48, 0x65, 0x6c, 0x6c, 0x6f, 0x00, 0x57, 0x6f, 0x72, 0x6c, 0x64];
    assert!(is_binary_content(&binary_bytes));

    let b64_binary = base64::engine::general_purpose::STANDARD.encode(&binary_bytes);
    let err = process_file_content(&b64_binary, Some("base64"), false).unwrap_err();
    assert!(err.to_string().contains("Binary file content detected"));

    // Secret redaction
    let config_sample = r#"
    {
        "github_token": "ghp_1234567890abcdef1234567890abcdef1234",
        "api_key": "sk-abcdefghijklmnopqrstuvwxyz123456",
        "private_key": "-----BEGIN RSA PRIVATE KEY-----\nMIIEowIBAAKCAQEA...\n-----END RSA PRIVATE KEY-----"
    }
    "#;

    let redacted = redact_secret_values(config_sample);
    assert!(!redacted.contains("ghp_1234567890abcdef"));
    assert!(!redacted.contains("sk-abcdefghijklmnopqrstuvwxyz"));
    assert!(!redacted.contains("MIIEowIBAAKCAQEA"));
    assert!(redacted.contains("[REDACTED_SECRET]"));
    assert!(redacted.contains("[REDACTED_PRIVATE_KEY]"));
}

// -----------------------------------------------------------------------------
// AC041 & AC042: Network Budgets, Oversize Rejection & Cancellation
// -----------------------------------------------------------------------------

#[tokio::test]
async fn test_ac041_oversized_response_rejected() {
    let mock_server = MockServer::start().await;

    // Response with body size > 2 MiB (2,097,152 bytes)
    let large_body = vec![b'A'; 2 * 1024 * 1024 + 1024];

    Mock::given(method("GET"))
        .and(path("/repos/org/huge/contents/giant.txt"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(large_body))
        .mount(&mock_server)
        .await;

    let github = Arc::new(GitHubClient::with_base_url(&mock_server.uri()));
    let transport = reposhelf_lib::github::evidence::transport::AcquisitionTransport::new(github);
    let budget = reposhelf_lib::github::evidence::AcquisitionBudgetTracker::new(None);

    let err = transport
        .get_bounded_bytes("/repos/org/huge/contents/giant.txt", None, &budget)
        .await
        .unwrap_err();

    assert!(
        err.to_string().contains("exceeds single response budget limit")
            || err.to_string().contains("Single response exceeded"),
        "Expected oversize rejection, got: {}",
        err
    );
}

#[tokio::test]
async fn test_ac042_cancellation_aborts_immediately() {
    let cancel_token = AcquisitionCancelToken::new();
    let budget = reposhelf_lib::github::evidence::AcquisitionBudgetTracker::new(Some(cancel_token.clone()));

    assert!(budget.check_cancel_and_deadline().is_ok());

    // Cancel token
    cancel_token.cancel();

    let err = budget.check_cancel_and_deadline().unwrap_err();
    assert!(err.to_string().contains("cancelled"));
}

// -----------------------------------------------------------------------------
// AC043: Recent Releases Sample
// -----------------------------------------------------------------------------

#[tokio::test]
async fn test_ac043_recent_releases_sample_bounded() {
    let mock_server = MockServer::start().await;

    // Mock 7 releases (1 draft, 6 published)
    let mut releases_json = Vec::new();
    for i in 1..=7 {
        releases_json.push(serde_json::json!({
            "id": i,
            "tag_name": format!("v1.0.{}", i),
            "name": format!("Release 1.0.{}", i),
            "body": format!("Release notes for version 1.0.{}", i),
            "draft": i == 1, // first is draft
            "prerelease": false,
            "published_at": "2026-03-01T00:00:00Z"
        }));
    }

    Mock::given(method("GET"))
        .and(path("/repos/owner/repo/releases"))
        .respond_with(ResponseTemplate::new(200).set_body_json(releases_json))
        .mount(&mock_server)
        .await;

    let github = Arc::new(GitHubClient::with_base_url(&mock_server.uri()));
    let transport = reposhelf_lib::github::evidence::transport::AcquisitionTransport::new(github);
    let budget = reposhelf_lib::github::evidence::AcquisitionBudgetTracker::new(None);

    let sample = reposhelf_lib::github::evidence::release::fetch_recent_releases(
        &transport, "owner", "repo", None, &budget,
    )
    .await
    .unwrap();

    // Draft excluded, max 5 releases
    assert_eq!(sample.len(), 5);
    assert!(!sample.iter().any(|r| r.tag_name == "v1.0.1")); // draft was v1.0.1
    assert_eq!(sample[0].tag_name, "v1.0.2");
}

// -----------------------------------------------------------------------------
// AC044: Partial Failure Handling
// -----------------------------------------------------------------------------

#[tokio::test]
async fn test_ac044_partial_failure_coverage() {
    let mock_server = MockServer::start().await;
    let sha = "partial_fail_commit_sha";

    Mock::given(method("GET"))
        .and(path("/repos/owner/partial"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": 8888,
            "html_url": "https://github.com/owner/partial",
            "name": "partial",
            "default_branch": "main",
            "private": false,
            "stargazers_count": 5,
            "topics": []
        })))
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/repos/owner/partial/commits/main"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "sha": sha
        })))
        .mount(&mock_server)
        .await;

    // Tree with README and bad_file
    Mock::given(method("GET"))
        .and(path(format!("/repos/owner/partial/git/trees/{}", sha)))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "sha": sha,
            "tree": [
                {
                    "path": "README.md",
                    "mode": "100644",
                    "type": "blob",
                    "sha": "b_rm",
                    "size": 50
                },
                {
                    "path": "bad_file.md",
                    "mode": "100644",
                    "type": "blob",
                    "sha": "b_bad",
                    "size": 50
                }
            ],
            "truncated": false
        })))
        .mount(&mock_server)
        .await;

    // README succeeds
    Mock::given(method("GET"))
        .and(path("/repos/owner/partial/contents/README.md"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "name": "README.md",
            "path": "README.md",
            "sha": "b_rm",
            "size": 50,
            "type": "file",
            "content": encode_base64("# Partial Project"),
            "encoding": "base64"
        })))
        .mount(&mock_server)
        .await;

    // bad_file returns 404
    Mock::given(method("GET"))
        .and(path("/repos/owner/partial/contents/bad_file.md"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/repos/owner/partial/releases"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .mount(&mock_server)
        .await;

    let github = Arc::new(GitHubClient::with_base_url(&mock_server.uri()));
    let service = RepositoryEvidenceService::new(github);

    let pack = service
        .acquire_evidence("owner/partial", None, AcquisitionOptions::default(), None)
        .await
        .unwrap();

    // Partial failure records gap and marks coverage as "partial"
    assert_eq!(pack.coverage.status, "partial");
    assert!(pack.coverage.gaps.iter().any(|g| g.subject == "bad_file.md" && g.kind == "failed"));
    assert_eq!(pack.selected_texts.len(), 1);
    assert_eq!(pack.selected_texts[0].path, "README.md");
}

// -----------------------------------------------------------------------------
// AC045 & AC046: Retention Scope & Fresh Acquisition
// -----------------------------------------------------------------------------

#[test]
fn test_ac045_evidence_record_retention_scope() {
    let rec = EvidenceRecord {
        id: "E003".to_string(),
        kind: "file".to_string(),
        path: Some("src/main.rs".to_string()),
        commit_sha: Some("abc123456789".to_string()),
        blob_sha: Some("blob123456".to_string()),
        observed_at: "2026-10-02T08:00:00Z".to_string(),
        included_ranges: vec![reposhelf_lib::db::analysis::EvidenceRange {
            start_line: 1,
            end_line: 50,
        }],
        included_content_hash: "d41d8cd98f00b204e9800998ecf8427e".to_string(),
        truncated: false,
        public_fact_summary: Some("Path: src/main.rs | Lines: 50".to_string()),
    };

    // Public facts and hashes retained, full source text is NOT stored in EvidenceRecord
    assert_eq!(rec.id, "E003");
    assert_eq!(rec.kind, "file");
    assert!(rec.public_fact_summary.is_some());
    assert!(!rec.included_content_hash.is_empty());
}

// -----------------------------------------------------------------------------
// Deterministic Fingerprint
// -----------------------------------------------------------------------------

#[test]
fn test_deterministic_fingerprint_consistency() {
    let rec1 = EvidenceRecord {
        id: "E001".to_string(),
        kind: "metadata".to_string(),
        path: None,
        commit_sha: Some("commit1".to_string()),
        blob_sha: None,
        observed_at: "2026-10-02T08:00:00Z".to_string(),
        included_ranges: vec![],
        included_content_hash: "hash1".to_string(),
        truncated: false,
        public_fact_summary: Some("meta".to_string()),
    };

    let text1 = reposhelf_lib::github::evidence::SelectedTextEvidence {
        evidence_id: "E003".to_string(),
        path: "README.md".to_string(),
        text_content: "# Title\nHello world".to_string(),
        content_hash: "hash_text1".to_string(),
        commit_sha: Some("commit1".to_string()),
        blob_sha: Some("blob1".to_string()),
        truncated: false,
        included_ranges: vec![],
        role: "readme".to_string(),
    };

    let fp1 = compute_evidence_fingerprint(
        "https://github.com/org/repo",
        12345,
        Some("main"),
        Some("commit1"),
        SELECTOR_VERSION,
        &[rec1.clone()],
        &[text1.clone()],
    );

    let fp2 = compute_evidence_fingerprint(
        "https://github.com/org/repo",
        12345,
        Some("main"),
        Some("commit1"),
        SELECTOR_VERSION,
        &[rec1.clone()],
        &[text1.clone()],
    );

    assert_eq!(fp1, fp2, "Fingerprints must be identical for identical inputs");
    assert_eq!(fp1.len(), 64, "SHA-256 hex string must be 64 characters");

    // Alter text content -> fingerprint must change
    let mut text_altered = text1.clone();
    text_altered.text_content = "# Title\nHello world modified".to_string();

    let fp_altered = compute_evidence_fingerprint(
        "https://github.com/org/repo",
        12345,
        Some("main"),
        Some("commit1"),
        SELECTOR_VERSION,
        &[rec1],
        &[text_altered],
    );

    assert_ne!(fp1, fp_altered, "Altered content must change fingerprint");
}

// -----------------------------------------------------------------------------
// Security Checks (AC105, AC106, AC107, AC110)
// -----------------------------------------------------------------------------

#[test]
fn test_ac107_redirect_safety_and_token_stripping() {
    let from_url = url::Url::parse("https://api.github.com/repos/owner/repo/tarball").unwrap();

    // Same host redirect: token preserved
    let same_host = url::Url::parse("https://api.github.com/repos/owner/repo/tarball/v1").unwrap();
    assert!(validate_redirect_target(&from_url, &same_host, None).unwrap());

    // Redirect to codeload.github.com: allowed host, but different origin -> token must be stripped!
    let codeload_url = url::Url::parse("https://codeload.github.com/owner/repo/tar.gz/v1").unwrap();
    let keep_token = validate_redirect_target(&from_url, &codeload_url, None).unwrap();
    assert!(!keep_token, "Token MUST be stripped on cross-host redirect!");

    // Redirect to malicious internal target -> BLOCKED!
    let ssrf_target = url::Url::parse("http://169.254.169.254/latest/meta-data").unwrap();
    assert!(validate_redirect_target(&from_url, &ssrf_target, None).is_err());

    let localhost_target = url::Url::parse("http://127.0.0.1:8080/admin").unwrap();
    assert!(validate_redirect_target(&from_url, &localhost_target, None).is_err());

    let private_ip = url::Url::parse("http://192.168.1.1/secret").unwrap();
    assert!(validate_redirect_target(&from_url, &private_ip, None).is_err());
}

#[tokio::test]
async fn test_branch_moving_after_commit_resolution_does_not_alter_snapshot() {
    let mock_server = MockServer::start().await;
    let commit_v1 = "1111111111111111111111111111111111111111";

    Mock::given(method("GET"))
        .and(path("/repos/owner/moving"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": 9901,
            "html_url": "https://github.com/owner/moving",
            "name": "moving",
            "default_branch": "main",
            "private": false,
            "stargazers_count": 50,
            "topics": []
        })))
        .mount(&mock_server)
        .await;

    // First call to /commits/main returns commit_v1
    Mock::given(method("GET"))
        .and(path("/repos/owner/moving/commits/main"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "sha": commit_v1
        })))
        .mount(&mock_server)
        .await;

    // Tree pinned to commit_v1
    Mock::given(method("GET"))
        .and(path(format!("/repos/owner/moving/git/trees/{}", commit_v1)))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "sha": commit_v1,
            "tree": [
                {
                    "path": "README.md",
                    "mode": "100644",
                    "type": "blob",
                    "sha": "blob_v1",
                    "size": 50
                }
            ],
            "truncated": false
        })))
        .mount(&mock_server)
        .await;

    // File content pinned to ref=commit_v1
    Mock::given(method("GET"))
        .and(path("/repos/owner/moving/contents/README.md"))
        .and(query_param("ref", commit_v1))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "name": "README.md",
            "path": "README.md",
            "sha": "blob_v1",
            "size": 50,
            "type": "file",
            "content": encode_base64("# Pinned at commit v1"),
            "encoding": "base64"
        })))
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/repos/owner/moving/releases"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .mount(&mock_server)
        .await;

    let github = Arc::new(GitHubClient::with_base_url(&mock_server.uri()));
    let service = RepositoryEvidenceService::new(github);

    let pack = service
        .acquire_evidence("owner/moving", None, AcquisitionOptions::default(), None)
        .await
        .unwrap();

    assert_eq!(pack.source.commit_sha.as_deref(), Some(commit_v1));
    assert_eq!(pack.selected_texts[0].commit_sha.as_deref(), Some(commit_v1));
    assert!(pack.selected_texts[0].text_content.contains("Pinned at commit v1"));
}

#[tokio::test]
async fn test_ac041_40_request_cap_and_retry_accounting() {
    let budget = reposhelf_lib::github::evidence::AcquisitionBudgetTracker::new(None);

    // Record 40 requests
    for _ in 0..40 {
        assert!(budget.record_request().is_ok());
    }

    // 41st request must exceed budget limit!
    let err = budget.record_request().unwrap_err();
    assert!(
        err.to_string().contains("GitHub request limit exceeded"),
        "Expected request limit exceeded, got: {}",
        err
    );
}

#[tokio::test]
async fn test_ac041_redirect_cap_exceeded() {
    let mock_server = MockServer::start().await;

    // Set up a redirect loop exceeding 3 hops
    Mock::given(method("GET"))
        .and(path("/redirect/1"))
        .respond_with(ResponseTemplate::new(302).insert_header("Location", "/redirect/2"))
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/redirect/2"))
        .respond_with(ResponseTemplate::new(302).insert_header("Location", "/redirect/3"))
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/redirect/3"))
        .respond_with(ResponseTemplate::new(302).insert_header("Location", "/redirect/4"))
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/redirect/4"))
        .respond_with(ResponseTemplate::new(302).insert_header("Location", "/redirect/5"))
        .mount(&mock_server)
        .await;

    let github = Arc::new(GitHubClient::with_base_url(&mock_server.uri()));
    let transport = reposhelf_lib::github::evidence::transport::AcquisitionTransport::new(github);
    let budget = reposhelf_lib::github::evidence::AcquisitionBudgetTracker::new(None);

    let err = transport
        .get_bounded_bytes("/redirect/1", None, &budget)
        .await
        .unwrap_err();

    assert!(
        err.to_string().contains("Redirect limit (3) exceeded"),
        "Expected redirect limit exceeded, got: {}",
        err
    );
}

#[test]
fn test_ac040_invalid_base64_error() {
    let invalid_b64 = "This is not valid base64!@#$%^&*()";
    let err = process_file_content(invalid_b64, Some("base64"), false).unwrap_err();
    assert!(
        err.to_string().contains("Invalid base64 encoding"),
        "Expected invalid base64 error, got: {}",
        err
    );
}

#[test]
fn test_ac040_oversized_decoded_file_bounded() {
    // Generate 300 KiB of text (> 256 KiB limit)
    let large_line = "A".repeat(100) + "\n";
    let large_content = large_line.repeat(3100); // ~310 KB
    let b64 = base64::engine::general_purpose::STANDARD.encode(&large_content);

    let res = process_file_content(&b64, Some("base64"), false).unwrap();
    assert!(res.truncated, "File over 256 KiB must be marked truncated");
    assert!(
        res.text_content.len() <= 256 * 1024,
        "Decoded content must be bounded to <= 256 KiB, got {} bytes",
        res.text_content.len()
    );
}

#[test]
fn test_ac041_total_byte_budget_exceeded() {
    let budget = reposhelf_lib::github::evidence::AcquisitionBudgetTracker::new(None);

    // Record 4 responses of 2 MiB each (total 8 MiB)
    for _ in 0..4 {
        assert!(budget.record_bytes(2 * 1024 * 1024).is_ok());
    }

    // 5th response pushes over 8 MiB total!
    let err = budget.record_bytes(1024).unwrap_err();
    assert!(
        err.to_string().contains("Total decompressed responses exceeded"),
        "Expected total response byte limit exceeded, got: {}",
        err
    );
}

#[tokio::test]
async fn test_ac106_no_credentials_in_evidence_pack_or_diagnostics() {
    let mock_server = MockServer::start().await;
    let sha = "sec_test_commit_sha";

    Mock::given(method("GET"))
        .and(path("/repos/sec/test"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": 9999,
            "html_url": "https://github.com/sec/test",
            "name": "test",
            "default_branch": "main",
            "private": false,
            "stargazers_count": 10,
            "topics": []
        })))
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/repos/sec/test/commits/main"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "sha": sha
        })))
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path(format!("/repos/sec/test/git/trees/{}", sha)))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "sha": sha,
            "tree": [
                {
                    "path": "README.md",
                    "mode": "100644",
                    "type": "blob",
                    "sha": "rm_sha",
                    "size": 50
                }
            ],
            "truncated": false
        })))
        .mount(&mock_server)
        .await;

    // README contains an accidentally committed token string
    let readme_content = "# Project\nExample token: ghp_111122223333444455556666777788889999\n";
    Mock::given(method("GET"))
        .and(path("/repos/sec/test/contents/README.md"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "name": "README.md",
            "path": "README.md",
            "sha": "rm_sha",
            "size": readme_content.len(),
            "type": "file",
            "content": encode_base64(readme_content),
            "encoding": "base64"
        })))
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/repos/sec/test/releases"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .mount(&mock_server)
        .await;

    let github = Arc::new(GitHubClient::with_base_url(&mock_server.uri()));
    let service = RepositoryEvidenceService::new(github);

    // Call acquire_evidence with a dummy caller token
    let dummy_user_token = "ghp_USER_SECRET_TOKEN_DO_NOT_LEAK";
    let pack = service
        .acquire_evidence("sec/test", Some(dummy_user_token), AcquisitionOptions::default(), None)
        .await
        .unwrap();

    let pack_json = serde_json::to_string(&pack).unwrap();

    // Verify user caller token is NEVER in EvidencePack JSON
    assert!(!pack_json.contains(dummy_user_token));
    assert!(!pack.evidence_fingerprint.contains(dummy_user_token));

    // Verify token found in README content was redacted
    assert!(!pack_json.contains("ghp_111122223333444455556666777788889999"));
    assert!(pack.selected_texts[0].text_content.contains("[REDACTED_SECRET]"));
}

// -----------------------------------------------------------------------------
// S2A MINOR FINDINGS HARDENING TESTS (S2B Pre-requisite)
// -----------------------------------------------------------------------------

#[test]
fn test_finding_s2a_min_01_url_path_encoding() {
    let base = "https://api.github.com";

    // 1. Path with space
    let u1 = build_github_contents_url(base, "owner", "repo", "docs/my notes.md", Some("abc1234")).unwrap();
    assert_eq!(u1.path(), "/repos/owner/repo/contents/docs/my%20notes.md");
    assert_eq!(u1.query(), Some("ref=abc1234"));
    assert!(u1.fragment().is_none());
    assert_eq!(u1.host_str(), Some("api.github.com"));

    // 2. Path with '#' (must not become fragment)
    let u2 = build_github_contents_url(base, "owner", "repo", "src/hash#tag.rs", Some("abc1234")).unwrap();
    assert_eq!(u2.path(), "/repos/owner/repo/contents/src/hash%23tag.rs");
    assert!(u2.fragment().is_none());
    assert_eq!(u2.query(), Some("ref=abc1234"));

    // 3. Path with '?' (must not alter query)
    let u3 = build_github_contents_url(base, "owner", "repo", "faq/what?why.md", Some("abc1234")).unwrap();
    assert_eq!(u3.path(), "/repos/owner/repo/contents/faq/what%3Fwhy.md");
    assert_eq!(u3.query(), Some("ref=abc1234"));
    assert!(!u3.query().unwrap().contains("why"));

    // 4. Path with '%' (must be properly percent-encoded)
    let u4 = build_github_contents_url(base, "owner", "repo", "stats/100%_done.txt", None).unwrap();
    assert_eq!(u4.path(), "/repos/owner/repo/contents/stats/100%25_done.txt");
    assert!(u4.query().is_none());

    // 5. Path with Unicode characters
    let u5 = build_github_contents_url(base, "owner", "repo", "文档/指南.md", Some("sha40")).unwrap();
    assert!(u5.path().starts_with("/repos/owner/repo/contents/"));
    assert!(u5.path().contains("%E6%96%87%E6%A1%A3"));
    assert!(u5.path().contains("%E6%8C%87%E5%8D%97.md"));
    assert_eq!(u5.query(), Some("ref=sha40"));

    // 6. Security: cannot jump out of /repos/{owner}/{repo}/contents/ with ".."
    let u6 = build_github_contents_url(base, "owner", "repo", "a/../../etc/passwd", None);
    assert!(u6.is_err());
    assert!(u6.unwrap_err().to_string().contains("Path traversal"));

    // 7. Base URL host cannot be changed by malicious path
    let u7 = build_github_contents_url(base, "owner", "repo", "@evil.com/leak", None).unwrap();
    assert_eq!(u7.host_str(), Some("api.github.com"));
    assert!(u7.path().starts_with("/repos/owner/repo/contents/"));
}

#[tokio::test]
async fn test_finding_s2a_min_02_tiny_deadline_timeout() {
    let mock_server = MockServer::start().await;

    // Simulate endpoint that takes 100ms
    Mock::given(method("GET"))
        .and(path("/repos/delay/test"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(std::time::Duration::from_millis(100))
                .set_body_json(serde_json::json!({
                    "id": 1,
                    "name": "test",
                    "html_url": "https://github.com/delay/test",
                    "description": "test",
                    "stargazers_count": 10,
                    "default_branch": "main",
                    "archived": false,
                    "private": false,
                    "size": 10
                })),
        )
        .mount(&mock_server)
        .await;

    let github = Arc::new(GitHubClient::with_base_url(&mock_server.uri()));
    let service = RepositoryEvidenceService::new(github);

    let start = std::time::Instant::now();
    // Inject tiny deadline of 10ms
    let options = AcquisitionOptions {
        custom_timeout: Some(std::time::Duration::from_millis(10)),
        ..Default::default()
    };

    let result = service.acquire_evidence("delay/test", None, options, None).await;
    let elapsed = start.elapsed();

    // Must return timeout error
    assert!(result.is_err());
    let err_str = result.unwrap_err().to_string();
    assert!(
        err_str.to_lowercase().contains("timed out")
            || err_str.to_lowercase().contains("timeout")
            || err_str.contains("deadline"),
        "Expected timeout error, got: {}",
        err_str
    );

    // Must finish quickly without waiting indefinitely (e.g. well below 1 second)
    assert!(elapsed < std::time::Duration::from_secs(1));

    // Verify retry does not reset overall deadline:
    // If deadline is already elapsed, check_cancel_and_deadline immediately returns timeout
    let budget = reposhelf_lib::github::evidence::AcquisitionBudgetTracker::with_custom_deadline(
        std::time::Duration::from_millis(5),
        None,
    );
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    let err = budget.check_cancel_and_deadline().unwrap_err();
    assert!(err.to_string().contains("deadline"));
    assert!(matches!(err, reposhelf_lib::error::AppError::GitHubTimeout(_)));
}

