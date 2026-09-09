# RepoShelf

RepoShelf is a local-first, desktop-first, AI-optional GitHub open-source project discovery, search, and personal library desktop application.

*(The original internal project specification referred to this system as "GitHub Project Library"; the public product identity is consistently **RepoShelf**).*

---

## Screenshot

<!-- Screenshot Placeholder: RepoShelf desktop application showing Trending, Search, Library, and Settings views -->
> _Screenshot placeholder: The RepoShelf desktop application user interface displaying GitHub Trending discovery, filtered search results, and local favorite library management._

---

## Features

### 1. GitHub Trending Discovery
- **Periods**: Browse trending repositories across **Today**, **This Week**, and **This Month**.
- **Official GitHub Ranking**: Preserves GitHub's authentic trending order without arbitrary re-ranking.
- **Enriched Metadata**: Asynchronously retrieves metadata (topics, license, pushed date, repository ID) with bounded concurrency (<= 4) while handling rate limits gracefully.
- **Instant Actions**: Single-click and batch favoriting with duplicate detection.

### 2. Search & Selection History
- **Basic Keyword Search**: Query repositories with GitHub Best Match ranking.
- **Filters**: Minimum stars filter, last push activity date filter, and configurable result limits.
- **Exclusion Controls**:
  - **Exclude Selected**: Filters out repositories previously surfaced in search results (tracked automatically in `selection_history`).
  - **Exclude Favorites**: Filters out repositories already present in your local library.
- **Replenishment Pagination**: Automatically fetches subsequent pages up to a candidate budget to fulfill the requested result limit when exclusions reduce candidate counts.

### 3. Add Repository by URL
- **Manual Input**: Paste any GitHub repository URL (e.g., `https://github.com/owner/repo`).
- **Strict Normalization**: Strips trailing slashes, protocol discrepancies, and invalid subpaths; rejects non-repository URLs (PRs, issues, commits).
- **Metadata Preview**: Fetches repository information, topics, license, and stars for inspection before saving.
- **Duplicate Handling**: Clear 3-way modal resolution (`[Refresh Metadata]`, `[Open Favorite Record]`, `[Cancel]`).

### 4. Local Favorite Library Management
- **Local Desktop Table**: High information-density table displaying repository name, category, tags, stars, last active date, rating, and favorited date.
- **Unified Local Search**: Case-insensitive search across repository name, category, tags, description, and notes.
- **Multi-Dimensional Filters**: Filter by Category, Tag, and Rating.
- **Allowlisted Safe Sorting**: Sort by stars, user rating, favorited date, last push date, or repository name.
- **Metadata Ownership Protection**:
  - **GitHub-Owned Data**: Name, description, stars, license, pushed date, and `source='github'` topics.
  - **User-Owned Data**: Category, rating (1–5 stars), notes, `source='user'` tags, and `source='ai'` tags.
  - Refreshing metadata updates GitHub fields while strictly preserving user notes, rating, category, and custom tags.
- **Batch Operations**: Bounded concurrent refresh (`<= 4`) with live progress reporting; cascade deletion with confirmation.

### 5. Optional AI Capabilities
- **Strictly Optional**: Basic Mode runs 100% locally without AI. AI configuration is never required to start the app or manage favorites.
- **Supported Providers**: OpenAI Compatible, Anthropic, Google Gemini, and local Ollama.
- **Natural Language Search**: Translates natural language intent into up to 3 focused GitHub search queries and merges candidate results using Reciprocal Rank Fusion (RRF).
- **AI Category & Tag Suggestions**: On-demand metadata suggestions in the repository detail drawer with explicit user approval before applying.
- **Connection Testing**: Built-in test connection utility to verify endpoints, credentials, and models.

### 6. Secure Credential Storage
- GitHub Personal Access Tokens and AI API keys are stored exclusively in the operating system's secure credential store (Windows Credential Manager).
- Secrets are never stored in SQLite, `settings.json`, application logs, or git.
- Tauri IPC commands never expose plaintext secrets back to the frontend.

---

## Tech Stack

- **Desktop Framework**: Tauri v2
- **Frontend**: React 18, TypeScript, Vite, Tailwind CSS
- **Backend Core**: Rust (2021 edition)
- **Database**: SQLite with `sqlx` (versioned migration support)
- **Networking & Scraping**: `reqwest` (rustls-tls), `scraper` (HTML parsing)
- **Secure Keyring**: `keyring` crate with native Windows Credential Manager integration

---

## Requirements

### Runtime Requirements (Windows)
- **Operating System**: Windows 10 or Windows 11 (64-bit x64)
- **Web Runtime**: Microsoft Edge WebView2 (preinstalled on modern Windows)

### Development Prerequisites
- **Node.js**: v18.0 or higher (with `npm`)
- **Rust Toolchain**: Stable Rust (v1.75+ recommended, MSVC target)
- **Build Tools**: Visual Studio C++ Build Tools (required by Rust and Tauri on Windows)

---

## Development

Clone the repository and install frontend dependencies:

```bash
npm install
```

Start the application in development mode (starts Vite and Tauri concurrently):

```bash
npm run tauri dev
```

Run test suites and quality gates:

```bash
# Frontend tests
npm run test

# TypeScript type checking
npm run typecheck

# Frontend linting
npm run lint

# Rust test suites
cargo test --manifest-path src-tauri/Cargo.toml

# Rust formatting check
cargo fmt --check --manifest-path src-tauri/Cargo.toml

# Rust clippy static analysis
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```

---

## Build

To compile a production Windows release package:

```bash
npm run tauri build
```

This compiles optimized frontend and Rust binaries and generates Windows packaging artifacts in `src-tauri/target/release/`:

- **Standalone Executable**: `src-tauri/target/release/reposhelf.exe`
- **NSIS Installer**: `src-tauri/target/release/bundle/nsis/RepoShelf_<version>_x64-setup.exe` (e.g., `RepoShelf_0.1.0_x64-setup.exe`)
- **MSI Package**: `src-tauri/target/release/bundle/msi/RepoShelf_<version>_x64_en-US.msi` (e.g., `RepoShelf_0.1.0_x64_en-US.msi`)

*(The exact filename version reflects `tauri.conf.json` package configuration, e.g. `0.1.0`).*

---

## GitHub Token Configuration

- **Optional**: A GitHub Personal Access Token is **not required**. RepoShelf functions anonymously for public repositories out of the box.
- **Rate Limits**: Anonymous GitHub API requests are limited to 60 requests per hour. Configuring a token increases this limit to 5,000 requests per hour.
- **Permissions**: RepoShelf V1 performs **zero repository write operations**. Only public read access (or fine-grained read permissions for public repositories) is needed. Never grant repository-write or administration scopes.
- **Storage**: Tokens are entered via **Settings → GitHub** and saved directly into Windows Credential Manager. Plaintext tokens are never stored in settings files or passed back to the user interface.

---

## AI Provider Configuration

AI features are strictly **optional**. When unconfigured or disabled, Basic Mode remains fully operational.

### Supported Providers
1. **OpenAI Compatible**: Any service adhering to the OpenAI `/chat/completions` specification.
2. **Anthropic**: Claude models via the Anthropic Messages API.
3. **Google Gemini**: Gemini models via Google AI Studio API.
4. **Ollama**: Local models running on your machine.

### Configuration Steps
Navigate to **Settings → AI Settings**:
1. Toggle **Enable AI** to ON.
2. Select your provider.
3. Specify the **Model** name (e.g., `gpt-4o-mini`, `claude-3-5-haiku-latest`, `gemini-1.5-flash`, `llama3.2`).
4. Enter the **Base URL** (if using a custom proxy or Ollama).
5. Enter the **API Key** (stored in Windows Credential Manager; not needed for Ollama).
6. Click **Test Connection** to verify availability before using AI features.

---

## Ollama Configuration

RepoShelf supports offline, private AI features through local [Ollama](https://ollama.com/) instances:

- **Base URL**: Enter your local Ollama address, typically:
  ```text
  http://127.0.0.1:11434
  ```
- **API Key**: Ollama runs locally without authentication; leave the API Key field empty.
- **Model**: Specify the name of a model you have already pulled locally (e.g., `llama3.2`, `qwen2.5`, `mistral`).
- Ensure the Ollama service is running before clicking **Test Connection**.

---

## Data Location

All user data is stored locally on your machine under the application identifier `com.reposhelf.desktop`:

- **SQLite Database**:
  ```text
  %APPDATA%\com.reposhelf.desktop\reposhelf.db
  ```
  Contains your favorites, tags, and selection history.
- **Settings File**:
  ```text
  %APPDATA%\com.reposhelf.desktop\settings.json
  ```
  Contains non-sensitive application preferences (e.g., default search limits, preferred trending period, AI provider and model choices).
- **Secrets & API Keys**:
  Stored securely in the **Windows Credential Manager** under the service name `com.reposhelf.desktop`. Secrets are never written to `settings.json` or SQLite.

---

## Privacy

- **Local-First Architecture**: Your favorites, personal notes, ratings, and tags are stored strictly in your local SQLite database.
- **Zero Cloud Backend**: RepoShelf operates no centralized server, telemetry, or user account backend.
- **No AI in Basic Mode**: When using Basic Mode, no data is ever transmitted to an AI provider.
- **Data Minimization for AI**:
  - When you invoke **AI Natural Language Search**, only your search query text is sent to the configured AI provider to produce keyword queries.
  - When you invoke **AI Category/Tag Suggestions**, only the repository `name`, `description`, and `topics` are transmitted.
  - Your personal notes, ratings, custom user tags, existing AI tags, internal database IDs, and credentials are **never** sent to any AI provider.

---

## Known Limitations

- **Public GitHub Repositories Only**: RepoShelf V1 is designed for discovering and cataloging public open-source repositories.
- **GitHub API Rate Limits**: Dependent on GitHub's API rate limiting policies (60 req/hr anonymous, 5,000 req/hr authenticated).
- **GitHub Trending HTML Structure**: Trending data is scraped from GitHub's public trending pages, which may change if GitHub modifies its markup.
- **No Git Clone / Push Operations**: RepoShelf is a discovery and library management tool, not a Git client. It does not clone repositories, pull code, or commit changes.
- **No Issues / PR Management**: RepoShelf does not manage or track GitHub Issues or Pull Requests.
- **No Automatic Cloud Sync**: RepoShelf stores data locally; multi-device synchronization is not included in V1.
- **AI Output Quality**: Suggestion accuracy and query quality depend on the capabilities of the specific AI provider and model chosen by the user.
- **Target Platform**: Windows 10/11 (x64) is the verified target environment for V1 desktop packaging.

---

## Roadmap

*(The following items are potential future enhancements and are not part of the V1 release contract):*

- **Cross-Platform Packaging**: Validated builds and testing for macOS and Linux.
- **Alternative Trending Sources**: Extensible providers for trending software lists.
- **Full-Text Search (FTS5)**: Enhanced SQLite full-text search indexing for personal collections exceeding tens of thousands of repositories.
- **Export & Import**: Exporting personal libraries to JSON/Markdown and backup restoration.
- **Custom Categorization Rules**: User-configurable keyword rules for automatic category assignment.
