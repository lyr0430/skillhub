---
name: client-skill-manage-20260911-01
status: implemented
---

# 技术方案：客户端 Skill 安装（Tauri 桌面应用）

## 背景与问题

网页端「安装到 agent」按钮当前只是把 `npx @astron-team/skillhub install ...` 命令复制到剪贴板，由用户自行打开 agent 粘贴执行。核心问题：**「部署到本地」这一价值没有被网页端直接承接**。

技术约束（已确认）：浏览器沙箱无法访问用户文件系统，无法解析 `~/.claude/skills` 等主目录路径，也无法枚举本地已安装的 agent。File System Access API 虽可写文件，但要求用户每次手动挑选目录、仅 Chrome/Edge、无法解析 `~` 路径——**无法满足"点选 agent 一键安装"**。

## 方案概述

采用 **Tauri v2 桌面应用**承载"一键安装到本地"能力：Tauri 壳加载**现有 React 前端**（复用 90%+ 代码），通过 Rust 原生命令提供本地安装引擎。技能浏览/数据仍走远端 registry（复用现有 web 的 HTTP 查询与下载接口），仅"写到本地 agent 目录"这一步走 Rust 原生命令。

**技术路线（已确认的决策）**：
- 安装引擎：**Rust 原生实现**（不依赖用户预装 Node/npm）。
- 与前端关系：**Tauri 壳 + 现有前端**（一套代码两用，浏览器端回退为现有复制行为）。

## 核心原则

- **网页端保持可用**：浏览器环境无 Tauri 时，`InstallForAgentButton` 回退为现有"复制命令"行为，不破坏当前网页体验。
- **跨平台**：Windows + macOS（符合 Rust/Tauri 原生支持）。
- **目录约定复用现有 CLI**：agent 目录映射与 `cli/src/agents/profiles/` 保持一致（`.claude/skills`、`.agents/skills` 等），避免两处漂移。
- **安全边界**：安装写操作局限在用户 agent 目录内；不触碰系统级路径。

## 架构变更

### 新增 `web/src-tauri/`（Tauri 应用壳）

```
web/src-tauri/
├─ Cargo.toml
├─ tauri.conf.json            # 嵌入 web 构建产物，窗口/打包配置
├─ build.rs
├─ capabilities/              # Tauri v2 权限（前端 invoke 白名单）
└─ src/
   ├─ main.rs                 # 入口
   ├─ lib.rs                  # 注册命令 + 状态
   └─ installer/
      ├─ mod.rs
      ├─ agents.rs            # agent 目录映射（跨平台 PathBuf）
      ├─ detect.rs            # detect_agents 命令
      ├─ install.rs           # install_skill 命令（下载+解压+写入）
      └─ error.rs             # 错误类型映射
```

### Rust 命令（核心）

| 命令 | 输入 | 输出 | 说明 |
|---|---|---|---|
| `detect_agents` | — | `[{ id, name, dir, installed }]` | 解析各 agent 的 skill 目录并探测是否存在 |
| `install_skill` | `{ namespace, slug, version, agentId, dir? }` | `{ ok, dir, agent, warnings }` | 下载 zip → 解压 → 写入目标目录 |

- **下载**：复用后端 zip 接口 `GET /api/v1/.../{ns}/{slug}/versions/{version}/download`，Rust 侧 `reqwest` 拉取。
- **MVP 范围**：先支持 **PUBLIC 技能匿名安装**（无需登录/Token）；团队/私有技能安装延后。

### Agent 目录映射（跨平台）

| agent | macOS / Linux | Windows |
|---|---|---|
| Claude Code | `~/.claude/skills/<slug>` | `%USERPROFILE%\.claude\skills\<slug>` |
| Codex / OpenCode / OpenClaw / Cursor / Windsurf / Gemini CLI … | 镜像 CLI profile 相对路径 | 同相对路径 |
| 默认全局 skill 位置（generic） | `~/.agents/skills/<slug>` | `%USERPROFILE%\.agents\skills\<slug>` |

- 统一用 `dirs::home_dir()`（或 `tauri::path`）解析家目录，`PathBuf::join` 组装，**禁止硬编码/字符串拼接**。
- agent 相对路径与 `cli/agents/profiles` 一致；个别平台差异（如 Windows 上 roo/trae）按 profile 映射。
- 因 CLI 已内置 16+ profile，本期先在 Rust 侧实现核心 4-5 个（Claude Code、Codex、OpenCode、OpenClaw）+ generic，其余后续按相同模式补充（tasks 中注明）。

### Tauri 打包分发（跨平台）

- Windows：NSIS / MSI 安装包。
- macOS：DMG / .app。
- 本期用开发签名（dev bundle）即可运行；notarization / code-sign 作为发布期配置项在 `tauri.conf.json` 标注，不改当前功能范围。

## 前端交互变更

### `web/src/features/skill/install-for-agent-button.tsx` 增强

- 新增运行时检测：`window.__TAURI__` 是否存在。
- **桌面端**：按钮点击 → 弹出目标选择 `Dialog` → 列出 `detect_agents()` 结果（含默认全局位置）→ 选择后调 `install_skill()` → 展示成功/失败/警告（含安装目录）。
- **网页端**：保持现有 `copy(buildAgentInstallPrompt(...))` 行为不变（回退）。

### UI 美观要求

- **目标选择弹窗**：每个 agent 显示图标 + 名称 + 目录路径（`~/.claude/skills` 等）；区分「检测到已安装」/「未检测到」两种状态，已安装置顶；底部提供「默认全局 skill 位置」兜底项。
- 交互态：选中项有设计过的 focus/hover/active 态；有层级与节奏，非默认卡片网格。
- **安装状态反馈**：加载中 → 成功（✓ + 目录）/ 失败（可读错误 + 重试），动画仅用 transform/opacity 等 compositor 属性。
- **空态/边界**：无任何 agent 时展示引导文案，不空白。
- 语义化 HTML、键盘导航与 ARIA、明暗双主题（Tailwind dark 变体）遵循 `docs/standards/frontend.md`。

## 数据模型变更

**无数据库变更。** 本功能完全在客户端（Tauri + 前端）实现，不新增后端 API、不改动数据表。下载复用现有 zip 接口。

## 接口变更

- **后端**：无新增/变更（复用现有 `download` 接口）。
- **前端新增类型**（`web/src-tauri` 与前端共享的 TS 类型由 `@tauri-apps/api` + 手写 `Commands` 类型定义）。
- 前端与 Rust 通过 `window.__TAURI__.core.invoke` 通信；命令入参/出参类型在 `web/src-tauri` 及前端 `src-tauri` 类型声明中定义。

## 影响范围

- 受影响模块：
  - 新增 `web/src-tauri/`（Rust 应用）。
  - 前端：`web/src/features/skill/install-for-agent-button.tsx`（增强）、新增目标选择 Dialog 组件、相关 unit test。
  - `web/package.json`（新增 `@tauri-apps/api` 依赖、tauri CLI dev 脚本）。
  - `web/vite.config.ts`（Tauri 开发服务器端口/配置，如需要）。
- 受保护路径变更：**无**（不涉及 `application*.yml`、`db/`、`sql/`、`deploy/`、`infra/`、`secrets/`）。
- 向后兼容性：
  - 网页端行为完全不变（无 Tauri 时回退为复制命令）。
  - 现有 `buildAgentInstallPrompt` 函数保留，网页端复用。

## 风险评估

| 风险 | 等级 | 缓解方案 |
|------|------|----------|
| Tauri 需维护独立桌面壳，构建/打包链路长 | 中 | 本期聚焦可运行 dev bundle；打包配置仅标注，不内联 CI 签名 |
| agent 目录映射在 Rust 侧与现有 CLI 重复，可能漂移 | 中 | 相对路径对齐 CLI profile；核心 agent 先行，后续按 profile 增量补齐 |
| 网络下载失败 / zip 无效 / 目录不可写 | 中 | Rust 侧全面错误映射，返回可读错误；前端展示重试 |
| 无 agent 安装时体验断裂 | 低 | 提供默认全局位置兜底 + 空态引导 |
| 私有/团队技能需 Token，本期不支持 | 低 | 明确 MVP 仅 PUBLIC 匿名安装，私有无权限时前端禁用并提示 |

## 事务与数据

- 事务边界：本功能无数据库写入。Rust 侧下载+解压+写入作为一次安装操作，失败时清理已写入的半成品目录（保留告警信息）。
- 数据迁移：无。
- 回滚方案：前端按钮行为在网页端无变化；桌面端卸载技能即删除对应目录（本期不做卸载 UI，仅安装 + 失败清理）。

## 测试策略

- **Rust 单元测试**（`cargo test`，在 `web/src-tauri`）：agent 目录解析（macOS/Windows 路径）、zip 解压、错误处理（agent 未安装 / zip 无效 / 网络失败 / 目录不可写）。
- **前端交互测试**（Vitest，jsdom）：mock `window.__TAURI__`，验证桌面端弹窗展示、调用 `invoke`、成功/失败渲染；浏览器环境验证保持复制行为。
- **E2E / smoke**：desktop 打包后端到端安装流（下载 zip → 写入目标目录）；网页端无回归。
- 遵循项目测试规范：行为变更必补测试，Service/组件层优先单测，API 行为优先集成。

## 参考文档

- `docs/07-skill-protocol.md`、`docs/08-frontend-architecture.md`、`docs/14-skill-lifecycle.md`
- `docs/standards/frontend.md`（UI/设计质量标准）
- 现有 `cli/src/agents/profiles/`（agent 目录映射蓝本）
