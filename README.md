# RepoShelf

RepoShelf 是一个本地优先（local-first）、桌面优先（desktop-first）、AI 可选（AI-optional）、面向开源的 GitHub 开源项目发现与本地收藏管理工具。

## 当前状态 (Current Status)

```text
Current status: Phase 0 scaffold.
GitHub discovery, persistence and AI features are not implemented yet.
```

本项目当前处于 **Phase 0（工程初始化与基线验证）** 阶段。已完成桌面应用工程基线、Tauri 2 + React + TypeScript + Vite + Tailwind CSS 栈搭建、基础目录与导航 Shell、IPC 基础通信验证、前后端测试与 Windows 构建验证。

**Phase 1+ 业务功能（数据库、GitHub API、Trending 解析、AI Provider 等）尚未实现。**

---

## 技术栈 (Tech Stack)

- **Desktop Framework**: Tauri 2 (stable)
- **Frontend**: React 18, TypeScript, Vite, Tailwind CSS
- **Backend**: Rust
- **Package Manager**: npm

---

## 环境准备 (Prerequisites)

- **Node.js**: >= 18 (已验证 v24.15.0)
- **npm**: >= 9 (已验证 11.12.1)
- **Rust & Cargo**: >= 1.77 (已验证 1.98.1)
- **C++ Build Tools**: Visual Studio 2022 C++ Build Tools (MSVC `link.exe`)
- **WebView2**: Windows 10/11 预装或 Evergreen Runtime

---

## 安装与开发指令 (Scripts)

### 1. 安装依赖

```bash
npm install
```

### 2. 启动开发模式

```bash
npm run tauri dev
```

前端独立预览模式：

```bash
npm run dev
```

### 3. 类型检查 (Typecheck)

```bash
npm run typecheck
```

### 4. 代码规范检查 (Lint)

```bash
npm run lint
```

### 5. 运行测试 (Test)

前端测试 (Vitest + React Testing Library):

```bash
npm run test
```

后端 Rust 单元测试:

```bash
cd src-tauri
cargo test
```

### 6. 构建生产包 (Build)

仅前端打包：

```bash
npm run build
```

Tauri 桌面端完整构建（生成 Windows 安装包与可执行文件）：

```bash
npm run tauri build
```

---

## 当前未实现功能 (Not Implemented Yet)

以下功能均严格属于后续阶段，当前阶段未实现任何业务逻辑：

- ❌ SQLite 数据库与数据迁移 (Phase 1)
- ❌ GitHub REST API / Token / 限额管理 (Phase 2)
- ❌ GitHub Trending 网络抓取与 HTML 解析 (Phase 3)
- ❌ 本地收藏、分类、标签、评分、备注 (Phase 4)
- ❌ 高级搜索与历史入选去重 (Phase 5)
- ❌ AI Provider 集成 (Phase 6)
- ❌ 安全凭据存储 (OS Keyring) (Phase 1/6)
