# RepoShelf

RepoShelf 是一个本地优先（local-first）、桌面优先（desktop-first）、AI 可选（AI-optional）、面向开源的 GitHub 开源项目发现与本地收藏管理工具。

## 当前状态 (Current Status)

```text
Current status: Phase 3 — Trending
Official GitHub Trending discovery capability implemented with daily/weekly/monthly period support, pure deterministic HTML parser, strictly preserved DOM ranking, period stars parsing, bounded concurrency REST metadata enrichment (max 4 concurrent), graceful degradation on single-item failures or API rate limits, and dedicated Trending desktop UI.
Favorites persistence, Library management, Search completion, and AI Providers are not implemented yet.
```

本项目当前处于 **Phase 3（Trending 热门项目发现）** 阶段。已建立基于 GitHub 官方页面（`https://github.com/trending`）的直接发现能力，支持 `daily`（Today）、`weekly`（This week）、`monthly`（This month）三种周期；采用成熟的 `scraper` 库实现纯粹、确定性、不依赖浏览器的 HTML Parser；严格保持 GitHub Trending 官方 DOM 出现顺序作为排名权威；准确解析当前周期的增量 Stars（Period Stars）；复用 Phase 2 的 `GitHubClient` 进行最大并发为 4 的受限异步 REST 元数据补充（`github_repo_id`, canonical URL, name, description, stars, license, topics, pushed_at）；对单个项目失败及 GitHub REST 限额触发执行友好降级与部分警示（Partial Warning）；复用 Phase 2 本地 16 分类分类器；提供完整的 Trending 桌面 UI（Idle / Loading / Success / Empty / Error / Partial Warning / Short Source Notice）；配套全套离线 HTML Fixture 测试、WireMock HTTP Mock 自动化测试以及受限的真实匿名运行时 Smoke 验证。

### 已实现 (Implemented in Phase 3)

- ✅ **TrendingProvider 抽象**: 建立精简的 Provider 抽象架构，提供统一的异步 `get_trending(period, limit)` 契约，仅实现官方 `GitHubTrendingProvider`，无过度工程。
- ✅ **官方 GitHub Trending HTML 获取**: 直接请求 `https://github.com/trending?since=...`，严格使用匿名公共请求，超时统一控制在 15 秒，安全重定向最多 5 次；绝对不向 `github.com/trending` 发送 Authorization、Token、Cookies 或会话凭据。
- ✅ **全周期支持 (Period Contract)**: 完整支持 `daily`（Today）、`weekly`（This week）、`monthly`（This month），UI 初始周期读取 Phase 1 已有的 `github.default_trending_period` 设置。
- ✅ **纯函数独立 HTML Parser**: 使用成熟的 Rust `scraper` 库，纯内存、确定性解析 HTML 字符串；集中维护 CSS 选择器；对异常卡片具备容错跳过能力；对卡片整体识别失败明确返回 `TrendingParse` 错误（不伪装为 Empty）。
- ✅ **官方排名严格保持 (Ranking Authority)**: 列表顺序严格以 GitHub Trending DOM 出现顺序（1, 2, 3...）为权威，REST 元数据补充后绝对不重新按 Stars、活跃时间或任何指标重排。
- ✅ **周期增量 Stars (Period Stars)**: 精确解析官方周期增量文案（`stars today`, `stars this week`, `stars this month`）为数值，由请求周期决定展示文案。
- ✅ **REST 元数据补充 (Enrichment)**: 复用 Phase 2 已审核的 `GitHubClient`，对入选项目通过 `GET /repos/{owner}/{repo}` 获取标准化规范信息（SPDX License、GitHub Topics、pushed_at 等）。
- ✅ **受限并发 (Max Concurrency = 4)**: 使用 `tokio::sync::Semaphore` 将 REST 元数据补充的最大并发量严格限制在 4，并通过自动化测试验证实际观测并发量 `<= 4`。
- ✅ **单项目失败降级与限额熔断**: 单个项目 REST 补充失败（404、超时等）不影响整个页面，保留 HTML 数据并将缺失字段优雅回退为 Unknown；遇到 GitHub REST API 限额耗尽时立即停止未开始的补充，保留已解析项目，并向用户展示明确的部分数据警示与重置时间。
- ✅ **本地分类器复用**: 复用 Phase 2 本地零成本确定性 16 分类规则，REST 成功时结合 GitHub Topics 分类，REST 失败时基于 HTML 名称与简介分类。
- ✅ **完整 Trending 桌面 UI**: 提供周期与返回数量（10/20/30/50）下拉选择；页面初始化为 `Idle` 状态（不强制自动发网）；完整覆盖 `Idle`、`Loading`、`Success`、`Empty`、`Error` 状态；当 GitHub 实际返回少于请求数时友好提示（`当前数据源只返回了 N 个热门项目`）；严格不包含收藏（Favorite）动作。
- ✅ **安全外部链接**: 继续复用 Phase 2 经安全域名校验的系统默认浏览器打开指令。
- ✅ **全套离线测试与 Mock**: 提供 Daily/Weekly/Monthly/Empty/Malformed 离线 HTML Fixture 测试；全套 WireMock HTTP Mock 测试验证参数、请求头、无 Authorization 边界、超时、状态码、截断、并发及降级行为。

---

## 当前未实现功能 (Not Implemented Yet)

以下功能严格属于后续阶段，本阶段明确未实现：

- ❌ **Phase 4 — 收藏与本地库 (Favorites & Library)**: 收藏按钮的真实收藏行为、SQLite 数据库写入、批量收藏、收藏库 CRUD、Rating、Notes、用户自定义标签、元数据刷新。本阶段禁止任何数据库写入。
- ❌ **Phase 5 — 搜索补全 (Search Completion)**: selection_history 写入、排除以前入选、排除已收藏、搜索分页、RRF 排序融合、多查询融合补全。
- ❌ **Phase 6 — AI 增强能力 (AI Providers)**: AI Trending、AI 排序、AI 分类、AI 标签推荐、OpenAI、Anthropic、Gemini、Ollama 网络调用。
- ❌ **Non-Goals**: MCP、第三方/非官方 Trending API、浏览器无头自动化抓取（Puppeteer/Playwright 作为运行时）、代码克隆、Issue/PR 管理等。
