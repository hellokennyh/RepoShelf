use std::collections::HashSet;

use crate::db::analysis::AnalysisGap;
use crate::github::evidence::budget::BUDGET_MAX_SELECTED_FILES;
use crate::github::evidence::tree::GitTreeEntry;

pub const SELECTOR_VERSION: &str = "1.1.2-s2a-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FileRole {
    Readme = 1,
    SkillOrPluginOrMcp = 2,
    Manifest = 3,
    Documentation = 4,
    EntryCore = 5,
    Example = 6,
    OtherText = 7,
}

impl FileRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            FileRole::Readme => "readme",
            FileRole::SkillOrPluginOrMcp => "skill_or_plugin_or_mcp",
            FileRole::Manifest => "manifest",
            FileRole::Documentation => "documentation",
            FileRole::EntryCore => "entry_core",
            FileRole::Example => "example",
            FileRole::OtherText => "other_text",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectedCandidate {
    pub path: String,
    pub blob_sha: String,
    pub role: FileRole,
    pub size: Option<usize>,
}

pub struct FileSelectionPlan {
    pub selected: Vec<SelectedCandidate>,
    pub gaps: Vec<AnalysisGap>,
    pub is_monorepo: bool,
}

/// Evaluates git tree entries and selects up to 20 representative text files deterministically.
pub fn select_evidence_files(
    entries: &[GitTreeEntry],
    submodules: &[String],
    symlinks: &[String],
) -> FileSelectionPlan {
    let mut gaps = Vec::new();

    // 1. Record gaps for special entries that are not recursively expanded
    for sub in submodules {
        gaps.push(AnalysisGap {
            kind: "skipped".to_string(),
            subject: sub.clone(),
            reason_code: "git_submodule_external_crawl_prohibited".to_string(),
        });
    }

    for sym in symlinks {
        gaps.push(AnalysisGap {
            kind: "skipped".to_string(),
            subject: sym.clone(),
            reason_code: "symlink_resolution_prohibited".to_string(),
        });
    }

    // 2. Identify candidate blobs excluding generated, binary, and sensitive files
    let mut eligible_blobs: Vec<&GitTreeEntry> = Vec::new();
    let mut sensitive_detected = false;

    for entry in entries {
        if !entry.is_blob() || entry.is_submodule() || entry.is_symlink() {
            continue;
        }

        if is_sensitive_secret_path(&entry.path) {
            sensitive_detected = true;
            continue;
        }

        if is_excluded_path(&entry.path) {
            continue;
        }

        eligible_blobs.push(entry);
    }

    if sensitive_detected {
        gaps.push(AnalysisGap {
            kind: "skipped".to_string(),
            subject: "sensitive_credential_files".to_string(),
            reason_code: "excluded_by_security_policy".to_string(),
        });
    }

    // 3. Detect monorepo shape
    let monorepo_packages = detect_monorepo_packages(&eligible_blobs);
    let is_monorepo = !monorepo_packages.is_empty();

    let allowed_monorepo_packages: Option<HashSet<String>> = if is_monorepo {
        let mut sorted_packages = monorepo_packages.clone();
        sorted_packages.sort();

        // Deterministically select up to 2 representative packages/apps
        let selected_pkgs: Vec<String> = sorted_packages.iter().take(2).cloned().collect();
        let unselected_pkgs: Vec<String> = sorted_packages.iter().skip(2).cloned().collect();

        if !unselected_pkgs.is_empty() {
            gaps.push(AnalysisGap {
                kind: "skipped".to_string(),
                subject: "monorepo_unselected_packages".to_string(),
                reason_code: format!(
                    "Monorepo detected: only representative modules [{}] selected; modules [{}] not covered",
                    selected_pkgs.join(", "),
                    unselected_pkgs.join(", ")
                ),
            });
        }

        Some(selected_pkgs.into_iter().collect())
    } else {
        None
    };

    // 4. Score and categorize candidates
    let mut scored_candidates: Vec<SelectedCandidate> = Vec::new();

    for entry in eligible_blobs {
        // If monorepo, only allow root files or files within selected representative packages
        if let Some(ref allowed_pkgs) = allowed_monorepo_packages {
            if let Some(pkg) = get_package_prefix(&entry.path) {
                if !allowed_pkgs.contains(&pkg) {
                    continue;
                }
            }
        }

        let role = classify_file_role(&entry.path);
        scored_candidates.push(SelectedCandidate {
            path: entry.path.clone(),
            blob_sha: entry.sha.clone(),
            role,
            size: entry.size,
        });
    }

    // 5. Deterministic sorting:
    // Primary: Role tier (Readme < SkillOrPluginOrMcp < Manifest < Documentation < EntryCore < Example < OtherText)
    // Secondary: Directory depth ascending (root files first)
    // Tertiary: Path alphabetical ascending (stable tie-breaking)
    scored_candidates.sort_by(|a, b| {
        a.role
            .cmp(&b.role)
            .then_with(|| {
                let depth_a = a.path.matches('/').count();
                let depth_b = b.path.matches('/').count();
                depth_a.cmp(&depth_b)
            })
            .then_with(|| a.path.cmp(&b.path))
    });

    // 6. Apply diversity caps across categories
    let mut selected = Vec::new();
    let mut selected_paths = HashSet::new();
    let mut count_readme = 0;
    let mut count_manifest = 0;
    let mut count_skill = 0;
    let mut count_docs = 0;
    let mut count_core = 0;
    let mut count_examples = 0;

    // Pass 1: Initial diversity selection
    for candidate in &scored_candidates {
        if selected.len() >= BUDGET_MAX_SELECTED_FILES {
            break;
        }

        match candidate.role {
            FileRole::Readme => {
                if count_readme >= 1 {
                    continue;
                }
                count_readme += 1;
            }
            FileRole::SkillOrPluginOrMcp => {
                if count_skill >= 3 {
                    continue;
                }
                count_skill += 1;
            }
            FileRole::Manifest => {
                if count_manifest >= 4 {
                    continue;
                }
                count_manifest += 1;
            }
            FileRole::Documentation => {
                if count_docs >= 6 {
                    continue;
                }
                count_docs += 1;
            }
            FileRole::EntryCore => {
                if count_core >= 6 {
                    continue;
                }
                count_core += 1;
            }
            FileRole::Example => {
                if count_examples >= 3 {
                    continue;
                }
                count_examples += 1;
            }
            FileRole::OtherText => {
                if selected.len() >= 18 {
                    continue;
                }
            }
        }

        selected_paths.insert(candidate.path.clone());
        selected.push(candidate.clone());
    }

    // Pass 2: If slots remain under the 20 file cap, fill with remaining eligible docs/core/other files
    if selected.len() < BUDGET_MAX_SELECTED_FILES {
        for candidate in &scored_candidates {
            if selected.len() >= BUDGET_MAX_SELECTED_FILES {
                break;
            }
            if selected_paths.contains(&candidate.path) {
                continue;
            }
            if candidate.role == FileRole::Readme {
                // Never take a second README
                continue;
            }

            selected_paths.insert(candidate.path.clone());
            selected.push(candidate.clone());
        }
    }

    // Note if README is missing
    if count_readme == 0 {
        gaps.push(AnalysisGap {
            kind: "missing".to_string(),
            subject: "README".to_string(),
            reason_code: "repository_has_no_readme".to_string(),
        });
    }

    FileSelectionPlan {
        selected,
        gaps,
        is_monorepo,
    }
}

/// Classifies a path into its file role category for deterministic prioritization.
pub fn classify_file_role(path: &str) -> FileRole {
    let lower = path.to_lowercase();
    let filename = lower.split('/').last().unwrap_or(&lower);

    // 1. README
    if filename.starts_with("readme")
        && (filename.ends_with(".md")
            || filename.ends_with(".rst")
            || filename.ends_with(".txt")
            || filename == "readme")
    {
        return FileRole::Readme;
    }

    // 2. Skill / Plugin / MCP / Tool descriptors
    if filename == "skill.md"
        || filename == "plugin.json"
        || filename == "mcp.json"
        || filename == "tool.json"
        || filename == "manifest.json"
    {
        return FileRole::SkillOrPluginOrMcp;
    }

    // 3. Manifests / build files
    if matches!(
        filename,
        "package.json"
            | "pyproject.toml"
            | "cargo.toml"
            | "go.mod"
            | "pom.xml"
            | "build.gradle"
            | "build.gradle.kts"
            | "requirements.txt"
            | "setup.py"
            | "setup.cfg"
            | "pnpm-workspace.yaml"
            | "lerna.json"
            | "makefile"
            | "cmakelists.txt"
            | "dockerfile"
            | "compose.yaml"
            | "compose.yml"
            | "docker-compose.yml"
    ) {
        return FileRole::Manifest;
    }

    // 4. Documentation
    if lower.contains("docs/")
        || lower.contains("documentation/")
        || lower.starts_with("doc/")
        || matches!(
            filename,
            "architecture.md"
                | "contributing.md"
                | "installation.md"
                | "install.md"
                | "usage.md"
                | "getting-started.md"
                | "quickstart.md"
                | "deployment.md"
                | "configuration.md"
                | "guide.md"
        )
    {
        return FileRole::Documentation;
    }

    // 5. Entry / Core source files
    if matches!(
        filename,
        "main.rs"
            | "lib.rs"
            | "main.go"
            | "main.py"
            | "app.py"
            | "index.ts"
            | "index.js"
            | "main.ts"
            | "main.js"
            | "app.ts"
            | "app.js"
            | "cli.ts"
            | "cli.js"
            | "mod.rs"
    ) || lower.starts_with("src/lib/")
        || lower.starts_with("src/core/")
        || lower.starts_with("lib/core/")
    {
        return FileRole::EntryCore;
    }

    // 6. Examples
    if lower.contains("examples/") || lower.contains("samples/") {
        return FileRole::Example;
    }

    FileRole::OtherText
}

/// Identifies whether a path is excluded (binaries, node_modules, lockfiles, cache, etc.).
pub fn is_excluded_path(path: &str) -> bool {
    let lower = path.to_lowercase();
    let filename = lower.split('/').last().unwrap_or(&lower);

    // Excluded directory segments
    let segments: Vec<&str> = lower.split('/').collect();
    for seg in &segments {
        if matches!(
            *seg,
            "node_modules"
                | "target"
                | "dist"
                | "build"
                | "vendor"
                | ".cache"
                | ".git"
                | "coverage"
                | ".next"
                | ".nuxt"
                | "out"
                | "__snapshots__"
                | ".idea"
                | ".vscode"
                | "bin"
                | "obj"
        ) {
            return true;
        }
    }

    // Excluded lock files
    if matches!(
        filename,
        "package-lock.json"
            | "yarn.lock"
            | "pnpm-lock.yaml"
            | "cargo.lock"
            | "poetry.lock"
            | "composer.lock"
            | "gemfile.lock"
            | "mix.lock"
            | "go.sum"
            | "flake.lock"
    ) {
        return true;
    }

    // Excluded minified code
    if filename.ends_with(".min.js")
        || filename.ends_with(".min.css")
        || filename.ends_with(".bundle.js")
    {
        return true;
    }

    // Excluded binary and media extensions
    if matches_extension(
        filename,
        &[
            "png", "jpg", "jpeg", "gif", "svg", "ico", "webp", "bmp", "tiff",
            "mp3", "wav", "ogg", "mp4", "webm", "avi", "mov", "mkv",
            "zip", "tar", "gz", "tgz", "bz2", "xz", "7z", "rar",
            "pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx",
            "exe", "dll", "so", "dylib", "wasm", "class", "jar", "pyc", "o", "a",
            "bin", "safetensors", "onnx", "pt", "pth", "gguf", "h5", "parquet", "arrow",
            "ttf", "woff", "woff2", "eot",
        ],
    ) {
        return true;
    }

    false
}

/// Identifies whether a path is sensitive (secrets, credentials, private keys).
pub fn is_sensitive_secret_path(path: &str) -> bool {
    let lower = path.to_lowercase();
    let filename = lower.split('/').last().unwrap_or(&lower);

    if filename == ".env" || filename.starts_with(".env.") {
        return true;
    }

    if filename == "id_rsa"
        || filename == "id_rsa.pub"
        || filename == "id_ed25519"
        || filename == "id_ed25519.pub"
        || filename == "id_ecdsa"
    {
        return true;
    }

    if matches_extension(filename, &["pem", "key", "crt", "p12", "pfx", "keystore"]) {
        return true;
    }

    if matches!(
        filename,
        "credentials.json"
            | "service-account.json"
            | "token.json"
            | "secrets.json"
            | "auth.json"
    ) || (filename.starts_with("secret")
        && (filename.ends_with(".json")
            || filename.ends_with(".yml")
            || filename.ends_with(".yaml")))
    {
        return true;
    }

    false
}

fn matches_extension(filename: &str, extensions: &[&str]) -> bool {
    if let Some(idx) = filename.rfind('.') {
        let ext = &filename[idx + 1..];
        extensions.iter().any(|e| *e == ext)
    } else {
        false
    }
}

fn detect_monorepo_packages(entries: &[&GitTreeEntry]) -> Vec<String> {
    let mut packages = HashSet::new();

    for entry in entries {
        let lower = entry.path.to_lowercase();
        if lower.starts_with("packages/") || lower.starts_with("apps/") || lower.starts_with("crates/") {
            let parts: Vec<&str> = entry.path.split('/').collect();
            if parts.len() >= 2 {
                packages.insert(format!("{}/{}", parts[0], parts[1]));
            }
        }
    }

    // Only consider monorepo if 2 or more distinct subpackages exist
    if packages.len() >= 2 {
        packages.into_iter().collect()
    } else {
        Vec::new()
    }
}

fn get_package_prefix(path: &str) -> Option<String> {
    let lower = path.to_lowercase();
    if lower.starts_with("packages/") || lower.starts_with("apps/") || lower.starts_with("crates/") {
        let parts: Vec<&str> = path.split('/').collect();
        if parts.len() >= 2 {
            return Some(format!("{}/{}", parts[0], parts[1]));
        }
    }
    None
}
