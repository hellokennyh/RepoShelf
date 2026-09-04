# RepoShelf

RepoShelf 是一个本地优先（local-first）、桌面优先（desktop-first）、AI 可选（AI-optional）、面向开源的 GitHub 开源项目发现与本地收藏管理工具。

## 当前状态 (Current Status)

```text
Current status: Phase 2 — GitHub Core
GitHub REST API client, URL normalizer, local category classifier, connection testing, rate limit handling, search, and repository preview implemented.
Trending, Favorites persistence workflow, and AI Provider network integration are not implemented yet.
```

本项目当前处于 **Phase 2（GitHub 核心访问层）** 阶段。已建立 GitHub REST API Version 2026-03-10 客户端、Anonymous 与 Authenticated 双模式支持、Windows Credential Manager 安全凭据读取、GitHub Connection Test 状态展示、Rate Limit 实时跟踪解析、GitHub 仓库 URL 规范化与解析、本地确定性零成本 Category 分类器、基础项目搜索（关键词/Stars/最后活跃时间/结果上限）、手动添加项目元数据预览、系统默认浏览器外部链接打开、全套本地 HTTP Mock 自动化测试以及受限的匿名 Runtime Smoke 验证。

### 已实现 (Implemented in Phase 2)

- ✅ **GitHub REST 客户端**: 显式锁定 `GitHub REST API Version: 2026-03-10`，采用 `reqwest` 实现轻量客户端，集中管理 15 秒超时、安全重定向与统一 Request Headers (`Accept`, `X-GitHub-Api-Version`, `User-Agent`)。
- ✅ **Anonymous / Authenticated 双模式**: 未配置 Token 时自动以匿名模式发起公共请求；已配置 Token 时在 Rust 后端内部安全读取并添加 `Authorization: Bearer <token>` 请求头。
- ✅ **严格安全凭据边界**: Token 仅留存于系统凭据管理器与 Rust 后端内部，绝不向前端回传明文，绝不进入日志、错误信息、URL 参数、`settings.json` 或 SQLite。
- ✅ **GitHub Connection Test**: 设置页面提供完整的连接测试，支持展示连接状态（Connected/Failed）、运行模式（Anonymous/Authenticated）、Core 与 Search 的配额/剩余/重置时间。
- ✅ **Rate Limit 状态与限额耗尽处理**: 请求响应实时解析 `x-ratelimit-*` 响应头并缓存在内存中；遇到 403/429 时准确区分主限额耗尽与 Secondary Rate Limit 并友好反馈重置时间。
- ✅ **GitHub 仓库 URL 规范化 (URL Normalizer)**: 支持标准/带斜杠/HTTP/www/带查询参数及锚点的仓库地址，规范化为 `https://github.com/owner/repo`；严格拒绝根路径、组织页、Issues/PRs/Tree/Blob/Commit 子路径及非 GitHub 域名。
- ✅ **本地确定性分类器 (Local Category Classifier)**: 完全本地、零 AI 成本，基于项目名称、简介与 Topics 关键词，确定性映射至 16 个固定预设分类之一（未命中归为 `Other`）。
- ✅ **公开项目强制过滤 (Public Only)**: 仅支持公开开源项目，搜索结果与手动添加严格过滤私有仓库（`private == true` 拦截为 `PrivateRepositoryUnsupported`）。
- ✅ **Search 基础搜索界面**: 支持关键词输入、最低 Stars 过滤、最后活跃时间预设（一周/一月/半年/一年/不限制）、返回数量限制（10/20/30/50/100）；完整覆盖 Idle、Loading、Success、Empty、Error、Rate Limited 六大状态；AI 搜索模式明确置灰提示 Phase 6。
- ✅ **Add Repository 项目预览**: 手动输入 GitHub 地址即时获取元数据并在前端预览项目卡片（名称、分类、简介、Stars、License、Topics、最后活跃时间、GitHub 链接），尚未持久化到本地库。
- ✅ **安全外部链接打开**: 点击 GitHub 链接通过经过安全白名单校验的 Tauri IPC 在系统默认浏览器中安全打开。
- ✅ **全套自动化测试**: Rust 端 35 个测试（含 12 个 WireMock HTTP Mock 集成测试与 13 个纯函数单元测试），前端 34 个 Vitest 测试全部通过。

---

## 当前未实现功能 (Not Implemented Yet)

以下功能严格属于后续阶段，本阶段未实现：

- ❌ GitHub Trending 网络抓取、HTML 解析与周期热门项目 (Phase 3)
- ❌ 点击收藏写入本地 SQLite、批量收藏、收藏库 CRUD、标签、评分、备注与信息刷新 (Phase 4)
- ❌ 排除以前入选、排除已收藏、selection_history 写入、自动翻页补齐、多查询 RRF 融合 (Phase 5)
- ❌ AI Provider 网络调用（OpenAI, Anthropic, Gemini, Ollama）与自然语言搜索 (Phase 6)
- ❌ MCP、README 深度抓取、代码克隆、Issue/PR 管理等 Non-Goals
