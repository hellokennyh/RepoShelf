# RepoShelf

RepoShelf 是一个本地优先（local-first）、桌面优先（desktop-first）、AI 可选（AI-optional）、面向开源的 GitHub 开源项目发现与本地收藏管理工具。

## 当前状态 (Current Status)

```text
Current status: Phase 4 — Favorites & Library
Complete local repository favorites persistence and library management implemented with FavoriteService, transactional SQLite operations, canonical URL duplicate detection, GitHub-owned vs User-owned metadata ownership protection, 3-way duplicate resolution modal, desktop library table with local multi-field search, filters, safe allowlisted sorting, batch refresh (bounded concurrency <= 4), favorite detail editing (16 categories, 1-5 rating, notes, source=user tags), cascade delete, and full integration across Search, Trending, and Add Repository.
Search completion, selection_history writing, and AI Providers are not implemented yet.
```

本项目当前处于 **Phase 4（Favorites & Library 收藏与本地库）** 阶段。已建立完整的本地项目收藏持久化与桌面端库管理能力：采用 `FavoriteService` 进行事务级 SQLite 数据管理；支持以 `github_repo_id`（优先）及规范化 `github_url`（次级）为依据的严格去重与并发安全保证；实现元数据所有权分立与单向覆盖保护契约（GitHub-owned 字段仅由刷新更新，用户分类、评级、备注、收藏时间及用户标签在刷新时严格保持）；提供三项可选的重复弹窗处理（刷新元数据 / 打开记录 / 取消）；提供功能完善的本地项目桌面表格（支持跨名称、分类、描述、备注、标签的联合本地搜索，分类/标签/评级过滤，安全白名单排序，行多选，受限并发批量更新）；提供收藏详情全量编辑（16 种冻结分类、1-5 可清除评级、Markdown/纯文本备注、仅针对 `source=user` 标签的增删）与带确认弹窗的级联删除；在 Search、Trending、Add Repository 三大入口完成收藏与重复处理接入，且严格保持原有排名顺序；全套测试验证通过，并在真实 Tauri 桌面端运行零残留验收。

### 已实现 (Implemented in Phase 4)

- ✅ **FavoriteService 事务化核心**: 封装单项与批量收藏、详情查询、本地列表过滤排序、用户元数据修改、级联删除与元数据刷新逻辑，所有写操作基于 SQLite 事务及写互斥锁，确保数据完整性与并发安全。
- ✅ **严格去重与规范化**: URL 统一规范化（小写主机名、剔除 `www.`、尾部斜杠及 `.git`），以 `github_repo_id` 为第一优先级、规范化 URL 为第二优先级进行冲突判定，准确识别重复收藏。
- ✅ **严格的元数据所有权契约 (Metadata Ownership Contract)**:
  - **GitHub 维护字段**: `name`, `description`, `stars`, `license`, `pushed_at`, `source='github'` 标签，以及 `metadata_refreshed_at`；在元数据刷新时进行单向增量更新与全量覆盖。
  - **用户独占字段**: `category`（首次收藏由分类器推断，收藏后转为用户资产）、`rating`（1-5 星，支持清空）、`notes`（个人备注）、`favorited_at`（不可变收藏时间）、`source='user'` 用户标签、`source='ai'` AI 标签（为 Phase 6 预留隔离）；在刷新时**绝对禁止**被覆盖或重置。
- ✅ **三项重复处理弹窗 (Duplicate Modal)**: 当用户在 Search、Trending、Add Repository 或批量操作中遇到已收藏项目时，提供明确的三种操作响应：`[刷新项目信息]`、`[打开收藏记录]`、`[取消]`。
- ✅ **本地库桌面表格 (Library Desktop Table)**:
  - **联合搜索**: 本地跨名称、分类、描述、备注、标签进行大小写不敏感检索。
  - **三维过滤**: 支持分类下拉筛选、独立标签下拉筛选、评级下拉筛选。
  - **安全白名单排序**: 支持按 `stars`, `rating`, `favorited_at`, `pushed_at`, `name` 升降序切换，严格白名单过滤杜绝 SQL 注入。
  - **行多选与批量更新**: 支持单行勾选与全选当前结果，支持 `更新选中` 与 `更新全部`。
  - **并发控制**: 批量刷新基于 `tokio::sync::Semaphore` 限制最大并发量 `<= 4`，遇 API 限额时安全跳过并展示部分完成汇总弹窗。
- ✅ **收藏详情与元数据编辑**: 支持在 16 种冻结分类间切换、1-5 星评级及清除评级、自由文本备注编辑、针对 `source=user` 标签的增加与删除（严格禁止修改或删除 `github` 标签）；提供单项刷新元数据按钮；提供带有明确警告弹窗的级联删除（标签连带清除）。
- ✅ **全入口无缝接入**:
  - **Search (项目搜索)**: 单项收藏按钮（更新为已收藏态）、多选与批量收藏、重复弹窗；保持 GitHub Best Match 排序不变。
  - **Trending (热门发现)**: 单项收藏按钮、多选与批量收藏、重复弹窗；严格保持 GitHub Trending 官方排名不变。
  - **Add Repository (添加项目)**: 预览卡片收藏按钮与重复弹窗处理。
- ✅ **零残留实时运行时验证**: 经真实 Tauri 运行时及生产数据库 smoke 验收，所有收藏、去重、编辑、刷新与删除流程顺畅，验证完成后数据库 `favorites`、`favorite_tags`、`selection_history` 行数精准回归为 0。

---

## 当前未实现功能 (Not Implemented Yet)

以下功能严格属于后续阶段，本阶段明确未实现：

- ❌ **Phase 5 — 搜索补全 (Search Completion)**: `selection_history` 写入（Phase 4 中只读/零写入）、排除以前入选、排除已收藏、搜索分页、RRF 排序融合、多查询融合补全。
- ❌ **Phase 6 — AI 增强能力 (AI Providers)**: AI Trending、AI 排序、AI 分类、AI 标签推荐、OpenAI、Anthropic、Gemini、Ollama 网络调用。
- ❌ **Non-Goals**: MCP、第三方/非官方 Trending API、浏览器无头自动化抓取（Puppeteer/Playwright 作为运行时）、代码克隆、Issue/PR 管理等。
