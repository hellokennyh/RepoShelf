# RepoShelf

RepoShelf 是一个本地优先（local-first）、桌面优先（desktop-first）、AI 可选（AI-optional）、面向开源的 GitHub 开源项目发现与本地收藏管理工具。

## 当前状态 (Current Status)

```text
Current status: Phase 1 — Local persistence and settings foundation
GitHub API, Trending, and AI Provider network integration are not implemented yet.
```

本项目当前处于 **Phase 1（本地持久化与安全配置基础）** 阶段。已建立 SQLite 数据库、Migration 迁移系统、三张核心业务表（`favorites`, `favorite_tags`, `selection_history`）、非敏感设置持久化（`settings.json`）、统一 SecretStore 抽象（Windows Credential Manager 集成）以及前后端 Settings UI。

### 已实现 (Implemented in Phase 1)

- ✅ **SQLite 本地数据库**: 遵循 Tauri 应用数据目录标准，数据库文件为 `reposhelf.db`。
- ✅ **Migration 迁移系统**: 基于 `sqlx::migrate!` 运行版本化迁移 (`001_initial_schema.sql`)。
- ✅ **核心数据契约**: `favorites`、`favorite_tags`、`selection_history` 表结构及 7 个必需索引。
- ✅ **外键与级联删除**: 启用 `PRAGMA foreign_keys = ON;`，级联删除 (`ON DELETE CASCADE`) 已全面通过测试验证。
- ✅ **非敏感设置持久化**: `settings.json` 支持安全原子写、默认值回退、输入合法性校验与防损坏隔离。
- ✅ **操作系统安全凭据抽象 (SecretStore)**: 生产环境集成 Windows Credential Manager (`keyring` crate)，测试环境使用 `InMemorySecretStore`。
- ✅ **硬安全门禁**: 零明文回退（No Plaintext Fallback），前端绝不暴露明文 Secret 获取命令，仅返回配置状态布尔值。
- ✅ **Settings 设置界面**: 提供 GitHub 参数与 Token 管理、AI 偏好与 API Key 管理，输入保存后立即清空明文状态，且网络测试按钮明确置灰标识。
- ✅ **全套自动化测试**: 涵盖数据库迁移、外键级联、字段约束、配置读写与校验、密钥防泄漏门禁及前端 React 交互测试。

---

## 当前未实现功能 (Not Implemented Yet)

以下功能严格属于后续阶段，本阶段未实现任何外部网络访问逻辑：

- ❌ GitHub REST API / Token 有效性网络测试 / 限额管理 (Phase 2)
- ❌ GitHub Trending 网络抓取与 HTML 解析 (Phase 3)
- ❌ 本地收藏业务层、分类、标签、评分、备注完整管理流程 (Phase 4)
- ❌ 高级搜索与历史入选去重业务 (Phase 5)
- ❌ AI Provider 网络调用与自然语言搜索理解 (Phase 6)
