pub const CATEGORY_AI_AGENTS: &str = "AI & Agents";
pub const CATEGORY_DEV_TOOLS: &str = "Developer Tools";
pub const CATEGORY_WEB_SAAS: &str = "Web & SaaS";
pub const CATEGORY_DATA_DB: &str = "Data & Database";
pub const CATEGORY_DEVOPS_CLOUD: &str = "DevOps & Cloud";
pub const CATEGORY_SECURITY: &str = "Security & Privacy";
pub const CATEGORY_NETWORKING: &str = "Networking";
pub const CATEGORY_MULTIMEDIA: &str = "Multimedia";
pub const CATEGORY_AUTOMATION: &str = "Automation";
pub const CATEGORY_PRODUCTIVITY: &str = "Productivity";
pub const CATEGORY_DESIGN_UI: &str = "Design & UI";
pub const CATEGORY_MOBILE: &str = "Mobile";
pub const CATEGORY_GAMES: &str = "Games";
pub const CATEGORY_SYSTEM_UTILS: &str = "System Utilities";
pub const CATEGORY_LIBRARIES: &str = "Libraries & Frameworks";
pub const CATEGORY_OTHER: &str = "Other";

pub const ALL_CATEGORIES: [&str; 16] = [
    CATEGORY_AI_AGENTS,
    CATEGORY_DEV_TOOLS,
    CATEGORY_WEB_SAAS,
    CATEGORY_DATA_DB,
    CATEGORY_DEVOPS_CLOUD,
    CATEGORY_SECURITY,
    CATEGORY_NETWORKING,
    CATEGORY_MULTIMEDIA,
    CATEGORY_AUTOMATION,
    CATEGORY_PRODUCTIVITY,
    CATEGORY_DESIGN_UI,
    CATEGORY_MOBILE,
    CATEGORY_GAMES,
    CATEGORY_SYSTEM_UTILS,
    CATEGORY_LIBRARIES,
    CATEGORY_OTHER,
];

const TOPIC_PRIORITY: [&str; 15] = [
    CATEGORY_AI_AGENTS,
    CATEGORY_AUTOMATION,
    CATEGORY_MULTIMEDIA,
    CATEGORY_DATA_DB,
    CATEGORY_DEVOPS_CLOUD,
    CATEGORY_SECURITY,
    CATEGORY_NETWORKING,
    CATEGORY_GAMES,
    CATEGORY_MOBILE,
    CATEGORY_DESIGN_UI,
    CATEGORY_PRODUCTIVITY,
    CATEGORY_DEV_TOOLS,
    CATEGORY_WEB_SAAS,
    CATEGORY_SYSTEM_UTILS,
    CATEGORY_LIBRARIES,
];

/// Deterministic, zero-AI, keyword-based local category classifier.
/// Considers name, description, and topics.
pub fn classify_repository(
    name: &str,
    description: Option<&str>,
    topics: &[String],
) -> &'static str {
    // 1. Check topics in priority order
    let lower_topics: Vec<String> = topics.iter().map(|t| t.to_lowercase()).collect();
    for &cat in &TOPIC_PRIORITY {
        if lower_topics
            .iter()
            .any(|t| match_topic_exact(t) == Some(cat))
        {
            return cat;
        }
    }

    // 2. Build aggregated lowercase text from name, topics, and description
    let mut combined = format!(
        "{} {}",
        name.to_lowercase(),
        topics.join(" ").to_lowercase()
    );
    if let Some(desc) = description {
        combined.push(' ');
        combined.push_str(&desc.to_lowercase());
    }

    // 3. Rule matching in priority order
    if has_any_keyword(
        &combined,
        &[
            "llm",
            "agent",
            "mcp",
            "machine-learning",
            "deep-learning",
            "neural",
            "transformer",
            "gpt",
            "bert",
            "diffusion",
            "tts",
            "speech",
            "ai",
            "openai",
            "langchain",
            "rag",
            "nlp",
            "vision",
            "whisper",
            "embedding",
            "prompt",
            "artificial-intelligence",
        ],
    ) {
        return CATEGORY_AI_AGENTS;
    }

    if has_any_keyword(
        &combined,
        &[
            "crawler",
            "scraper",
            "bot",
            "scrape",
            "scraping",
            "rpa",
            "cron",
            "workflow",
            "automation",
        ],
    ) {
        return CATEGORY_AUTOMATION;
    }

    if has_any_keyword(
        &combined,
        &[
            "video", "audio", "ffmpeg", "media", "image", "music", "stream", "sound", "voice",
            "codec", "graphics", "photo", "canvas", "player",
        ],
    ) {
        return CATEGORY_MULTIMEDIA;
    }

    if has_any_keyword(
        &combined,
        &[
            "database",
            "sqlite",
            "postgres",
            "postgresql",
            "mysql",
            "redis",
            "mongodb",
            "nosql",
            "sql",
            "orm",
            "duckdb",
            "clickhouse",
            "cache",
            "vectordb",
            "vector-db",
        ],
    ) {
        return CATEGORY_DATA_DB;
    }

    if has_any_keyword(
        &combined,
        &[
            "devops",
            "docker",
            "kubernetes",
            "k8s",
            "terraform",
            "ansible",
            "ci/cd",
            "cicd",
            "container",
            "aws",
            "azure",
            "gcp",
            "serverless",
            "helm",
            "infra",
            "infrastructure",
        ],
    ) {
        return CATEGORY_DEVOPS_CLOUD;
    }

    if has_any_keyword(
        &combined,
        &[
            "security",
            "privacy",
            "cryptography",
            "crypto",
            "authentication",
            "oauth",
            "vulnerability",
            "penetration",
            "pentest",
            "firewall",
            "antivirus",
            "encryption",
            "hacker",
            "exploit",
            "sandbox",
        ],
    ) {
        return CATEGORY_SECURITY;
    }

    if has_any_keyword(
        &combined,
        &[
            "networking",
            "network",
            "http",
            "tcp",
            "udp",
            "grpc",
            "websocket",
            "dns",
            "proxy",
            "vpn",
            "router",
            "p2p",
            "socket",
            "protocol",
        ],
    ) {
        return CATEGORY_NETWORKING;
    }

    if has_any_keyword(
        &combined,
        &[
            "game", "gaming", "engine", "unity", "unreal", "godot", "arcade", "emulator", "2d",
            "3d", "rpg",
        ],
    ) {
        return CATEGORY_GAMES;
    }

    if has_any_keyword(
        &combined,
        &[
            "mobile",
            "android",
            "ios",
            "flutter",
            "react-native",
            "swift",
            "kotlin",
            "apk",
        ],
    ) {
        return CATEGORY_MOBILE;
    }

    if has_any_keyword(
        &combined,
        &[
            "design",
            "ui",
            "ux",
            "theme",
            "icon",
            "icons",
            "tailwind",
            "font",
            "figma",
            "component-library",
            "css-framework",
        ],
    ) {
        return CATEGORY_DESIGN_UI;
    }

    if has_any_keyword(
        &combined,
        &[
            "productivity",
            "todo",
            "note",
            "notes",
            "calendar",
            "planner",
            "knowledge-base",
            "wiki",
            "bookmark",
            "markdown-editor",
        ],
    ) {
        return CATEGORY_PRODUCTIVITY;
    }

    if has_any_keyword(
        &combined,
        &[
            "developer-tools",
            "devtools",
            "cli",
            "compiler",
            "debugger",
            "linter",
            "formatter",
            "terminal",
            "ide",
            "editor",
            "git",
            "sdk",
            "parser",
            "tooling",
        ],
    ) {
        return CATEGORY_DEV_TOOLS;
    }

    if has_any_keyword(
        &combined,
        &[
            "web",
            "frontend",
            "backend",
            "saas",
            "fullstack",
            "react",
            "vue",
            "svelte",
            "nextjs",
            "html",
            "css",
            "wasm",
            "webassembly",
            "dashboard",
        ],
    ) {
        return CATEGORY_WEB_SAAS;
    }

    if has_any_keyword(
        &combined,
        &[
            "system",
            "utility",
            "linux",
            "windows",
            "macos",
            "kernel",
            "driver",
            "filesystem",
            "memory",
            "performance",
            "process",
            "monitor",
            "benchmark",
            "sysadmin",
        ],
    ) {
        return CATEGORY_SYSTEM_UTILS;
    }

    if has_any_keyword(
        &combined,
        &[
            "library",
            "framework",
            "crate",
            "package",
            "module",
            "wrapper",
            "binding",
            "bindings",
        ],
    ) {
        return CATEGORY_LIBRARIES;
    }

    CATEGORY_OTHER
}

fn match_topic_exact(topic: &str) -> Option<&'static str> {
    match topic {
        "ai" | "llm" | "agent" | "mcp" | "machine-learning" | "deep-learning" | "nlp" | "gpt"
        | "tts" | "speech" | "langchain" | "transformers" => Some(CATEGORY_AI_AGENTS),
        "crawler" | "scraper" | "scraping" | "automation" | "bot" => Some(CATEGORY_AUTOMATION),
        "video" | "audio" | "ffmpeg" | "media" | "image" | "music" => Some(CATEGORY_MULTIMEDIA),
        "database" | "sqlite" | "postgres" | "postgresql" | "mysql" | "redis" | "sql"
        | "duckdb" => Some(CATEGORY_DATA_DB),
        "devops" | "docker" | "kubernetes" | "k8s" | "terraform" | "ci-cd" | "cicd" => {
            Some(CATEGORY_DEVOPS_CLOUD)
        }
        "security" | "privacy" | "cryptography" | "crypto" | "pentest" => Some(CATEGORY_SECURITY),
        "networking" | "network" | "http" | "grpc" | "websocket" | "proxy" | "vpn" => {
            Some(CATEGORY_NETWORKING)
        }
        "game" | "games" | "gamedev" | "unity" | "godot" => Some(CATEGORY_GAMES),
        "mobile" | "android" | "ios" | "flutter" | "react-native" => Some(CATEGORY_MOBILE),
        "design" | "ui" | "ux" | "icons" | "tailwind" => Some(CATEGORY_DESIGN_UI),
        "productivity" | "todo" | "notes" | "wiki" => Some(CATEGORY_PRODUCTIVITY),
        "cli" | "compiler" | "devtools" | "developer-tools" | "terminal" | "linter" => {
            Some(CATEGORY_DEV_TOOLS)
        }
        "web" | "frontend" | "backend" | "saas" | "react" | "vue" | "nextjs" => {
            Some(CATEGORY_WEB_SAAS)
        }
        "system" | "kernel" | "linux" | "windows" | "utility" => Some(CATEGORY_SYSTEM_UTILS),
        "library" | "framework" | "crate" | "sdk" => Some(CATEGORY_LIBRARIES),
        _ => None,
    }
}

fn has_any_keyword(text: &str, keywords: &[&str]) -> bool {
    for &kw in keywords {
        if contains_word_or_phrase(text, kw) {
            return true;
        }
    }
    false
}

fn contains_word_or_phrase(text: &str, needle: &str) -> bool {
    if needle.contains(' ') || needle.contains('-') || needle.contains('/') {
        return text.contains(needle);
    }

    // Single word check with boundary delimiters
    for part in text.split(|c: char| !c.is_alphanumeric() && c != '_' && c != '-') {
        if part.eq_ignore_ascii_case(needle) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_category_ai_agents() {
        assert_eq!(
            classify_repository(
                "super-agent",
                Some("An autonomous LLM agent built with MCP and RAG"),
                &["ai".to_string(), "llm".to_string()]
            ),
            CATEGORY_AI_AGENTS
        );
    }

    #[test]
    fn test_category_developer_tools() {
        assert_eq!(
            classify_repository(
                "fast-linter",
                Some("Command line linter and compiler tool for Rust"),
                &["cli".to_string(), "developer-tools".to_string()]
            ),
            CATEGORY_DEV_TOOLS
        );
    }

    #[test]
    fn test_category_web_saas() {
        assert_eq!(
            classify_repository(
                "react-dashboard",
                Some("SaaS web frontend dashboard application"),
                &["web".to_string(), "react".to_string()]
            ),
            CATEGORY_WEB_SAAS
        );
    }

    #[test]
    fn test_category_data_database() {
        assert_eq!(
            classify_repository(
                "my-db",
                Some("Embedded SQLite database driver and vector database"),
                &["database".to_string(), "sqlite".to_string()]
            ),
            CATEGORY_DATA_DB
        );
    }

    #[test]
    fn test_category_devops_cloud() {
        assert_eq!(
            classify_repository(
                "k8s-deployer",
                Some("Deploy Docker containers onto Kubernetes clusters via Terraform"),
                &["k8s".to_string(), "docker".to_string()]
            ),
            CATEGORY_DEVOPS_CLOUD
        );
    }

    #[test]
    fn test_category_security_privacy() {
        assert_eq!(
            classify_repository(
                "vault-guard",
                Some("End-to-end cryptography and security audit penetration tool"),
                &["security".to_string(), "crypto".to_string()]
            ),
            CATEGORY_SECURITY
        );
    }

    #[test]
    fn test_category_networking() {
        assert_eq!(
            classify_repository(
                "p2p-proxy",
                Some("High-performance TCP, UDP, and HTTP networking proxy"),
                &["networking".to_string(), "proxy".to_string()]
            ),
            CATEGORY_NETWORKING
        );
    }

    #[test]
    fn test_category_multimedia() {
        assert_eq!(
            classify_repository(
                "video-transcoder",
                Some("FFmpeg wrapper for audio and video compression"),
                &["multimedia".to_string(), "video".to_string()]
            ),
            CATEGORY_MULTIMEDIA
        );
    }

    #[test]
    fn test_category_automation() {
        assert_eq!(
            classify_repository(
                "insta-scraper",
                Some("Web crawler and bot for automated data scraping"),
                &["scraper".to_string(), "automation".to_string()]
            ),
            CATEGORY_AUTOMATION
        );
    }

    #[test]
    fn test_category_productivity() {
        assert_eq!(
            classify_repository(
                "daily-journal",
                Some("Personal markdown notes, wiki, and todo planner"),
                &["productivity".to_string(), "notes".to_string()]
            ),
            CATEGORY_PRODUCTIVITY
        );
    }

    #[test]
    fn test_category_design_ui() {
        assert_eq!(
            classify_repository(
                "tailwind-icons",
                Some("Modern UI icon component library and Tailwind CSS themes"),
                &["design".to_string(), "icons".to_string()]
            ),
            CATEGORY_DESIGN_UI
        );
    }

    #[test]
    fn test_category_mobile() {
        assert_eq!(
            classify_repository(
                "flutter-reader",
                Some("Cross platform mobile app for Android and iOS devices"),
                &["mobile".to_string(), "flutter".to_string()]
            ),
            CATEGORY_MOBILE
        );
    }

    #[test]
    fn test_category_games() {
        assert_eq!(
            classify_repository(
                "godot-platformer",
                Some("2D arcade platformer game engine demo built with Godot"),
                &["games".to_string(), "gamedev".to_string()]
            ),
            CATEGORY_GAMES
        );
    }

    #[test]
    fn test_category_system_utilities() {
        assert_eq!(
            classify_repository(
                "sys-monitor",
                Some("Linux kernel process, memory, and filesystem utility"),
                &["system".to_string(), "linux".to_string()]
            ),
            CATEGORY_SYSTEM_UTILS
        );
    }

    #[test]
    fn test_category_libraries_frameworks() {
        assert_eq!(
            classify_repository(
                "http-helpers",
                Some("Reusable utility library, SDK bindings, and crate framework"),
                &["library".to_string(), "crate".to_string()]
            ),
            CATEGORY_LIBRARIES
        );
    }

    #[test]
    fn test_category_other_fallback() {
        // Must fallback to Other when no matching topics or keywords exist
        assert_eq!(
            classify_repository(
                "random-repo-12345",
                Some("Just a repository with arbitrary text and no matched terms"),
                &["unrelated-topic".to_string()]
            ),
            CATEGORY_OTHER
        );

        assert_eq!(
            classify_repository("unclassified-empty", None, &[]),
            CATEGORY_OTHER
        );
    }

    #[test]
    fn test_category_precedence_deterministic() {
        // AI rule has higher precedence than Dev Tools or Web
        assert_eq!(
            classify_repository(
                "ai-cli-tool",
                Some("A CLI tool that queries LLM models"),
                &["cli".to_string(), "llm".to_string()]
            ),
            CATEGORY_AI_AGENTS
        );
    }
}
