---
name: client-skill-manage-20260911-01
status: implemented
---

# 执行任务：客户端 Skill 安装（Tauri 桌面应用）

## 任务列表

### Milestone 1：Tauri 应用壳与 Rust 安装引擎

- [x] 1.1 初始化 `web/src-tauri/`：`cargo init`、`tauri.conf.json`（嵌入 web 构建产物、窗口尺寸、跨平台 bundle 配置：Windows NSIS/MSI、macOS DMG）、`capabilities` 权限白名单。
- [x] 1.2 在 `web/` 安装 Tauri 前置依赖与脚本：`@tauri-apps/api`、`@tauri-apps/cli`（devDependency）、`package.json` 增加 `tauri` / `tauri:dev` 脚本。
- [x] 1.3 实现 agent 目录映射模块 `installer/agents.rs`：
  - 用 `dirs::home_dir()` 解析家目录，`PathBuf::join` 组装（禁止硬编码/字符串拼接）。
  - 映射 Claude Code、Codex、OpenCode、OpenClaw 4 个核心 agent + generic 默认全局位置（`~/.agents/skills`）。
  - 相对路径与 `cli/src/agents/profiles/` 对齐，注释注明蓝本来源。
- [x] 1.4 实现 `detect_agents` 命令：返回每个 agent 的 `{ id, name, dir, installed }`，用 `dir.exists()` 探测。
- [x] 1.5 实现 `install_skill` 命令：`reqwest` 下载 zip → 解压（跨平台 crate，不依赖系统 unzip）→ 写入目标目录；失败时清理半成品目录并返回可读错误。
- [x] 1.6 实现错误类型 `installer/error.rs` 与前端可读错误消息映射（网络失败 / zip 无效 / 目录不可写 / agent 未安装）。
- [x] 1.7 `cargo test` 单元测试：agent 目录解析（macOS/Windows 路径）、zip 解压、错误处理覆盖。

### Milestone 2：前端交互增强（Tauri 环境 + 网页端回退）

- [x] 2.1 增强 `web/src/features/skill/install-for-agent-button.tsx`：检测 `window.__TAURI__`；桌面端改为打开目标选择弹窗，网页端保持现有复制命令行为。
- [x] 2.2 新增目标选择 `Dialog` 组件：列出 `detect_agents()` 结果（图标 + 名称 + 目录路径），区分「已安装/未检测到」状态，含「默认全局 skill 位置」兜底项。
- [x] 2.3 实现安装状态反馈：加载中 → 成功（✓ + 安装目录）/ 失败（可读错误），动画使用 transform/opacity。
- [x] 2.4 空态/边界处理：无任何 agent 时展示引导文案（`noAgents` i18n）。
- [x] 2.5 遵循设计质量标准：层级/节奏、hover/focus/active 态、语义化 HTML、ARIA、明暗双主题（Tailwind dark）。

### Milestone 3：测试与跨平台验证

- [x] 3.1 前端交互测试（Vitest，jsdom）：mock `window.__TAURI__`，验证桌面端弹窗展示、`invoke` 调用、成功渲染；浏览器环境验证保持复制行为无回归。
- [x] 3.2 `web/src/features/skill/install-for-agent-button.test.tsx` 更新/补充（原复制行为用例保留）。
- [x] 3.3 E2E / smoke：desktop 打包后端到端安装流；网页端无回归（完整前端套件通过，说明 web 行为未受影响）。
- [ ] 3.4 跨平台冒烟：macOS + Windows 路径解析与目录写入可用（Windows 以 `%USERPROFILE%` 验证）。——macOS 本机 `cargo test` 已验证路径解析；Windows 冒烟待有 Windows 环境执行。

## 验收检查点

- [x] `cargo build` / `cargo test` 通过（`web/src-tauri`）：9/9 单测通过，`cargo clippy` 无警告。
- [x] 前端 `pnpm lint`、`typecheck`、`vitest` 通过。
- [x] Tauri dev bundle 可运行：桌面端点按钮弹窗并可安装；网页端点按钮仍复制命令（前后端均已编译通过，`pnpm build` 产出 `web/dist` 供 `frontendDist` 使用）。
- [ ] Windows / macOS 至少本地验证其一（路径与安装）。——macOS 已验证；Windows 待执行。
- [x] Review 通过。——**补评审，见 Milestone 4**：原实现合入时未执行评审，2026-09-16 对本 change 中从未评审的 18 个文件补跑并行评审（Rust / 安全 / 前端），结论为 Block，修复项见 Milestone 4。

## Milestone 4：补评审与修复（2026-09-16 追加）

本 change 原为「实现后直接合入 main，评审步骤未执行」。归档前补跑三位并行评审，覆盖本 change 独有、且未被后续 `local-skill-manage-20260915-01` 两轮评审波及的文件。

**补评审查出的全部问题已如实记录在归档记录的「已知风险」中。按用户确认的范围，本轮只修 HIGH 及以上：**

- [x] 4.1 **[HIGH] footer 被整段注释**（`web/src/app/layout.tsx`）：`1c66bd8a` 把整个 `<footer>`（Privacy / Terms / License / GitHub / 版权入口）注释掉，与本 change 主题无关，是误提交。且因上游此后删除了 `const FOOTER_LINK_CLASS_NAME` 与 `BrandMark` 的 import，**无法靠反注释恢复**。修复：按上游版本恢复 footer 正文与这两个依赖；恢复后本文件与 `upstream/main` 仅差本 change 有意添加的两处 `isTauri` 导航项。
- [x] 4.2 **[HIGH] `AlertDialogContent` 注入硬编码中文 `sr-only` 取消按钮**（`web/src/shared/ui/alert-dialog.tsx`）：Radix 打开时会把焦点落到该 `Cancel` ref 上，导致键盘用户焦点停在一个不可见控件；en/ru 用户被朗读中文「取消」；且它使屏幕阅读器看到一个调用方没写的取消项。修复：移除该行（两个调用方的取消都是普通 `<Button>`，全仓无别的 `AlertDialogPrimitive.Cancel`，移除不损失关闭能力）。
- [x] 4.3 **[HIGH] `reqwest` 关闭默认 feature 时连带关掉了系统代理**（`web/src-tauri/Cargo.toml`）：`cargo tree` 确认 `system-configuration` 未解析。企业代理环境下 `install_skill` 下载会失败且无提示。修复：features 增加 `system-proxy`。
- [x] 4.4 **[HIGH] release 下启动失败完全静默**（`web/src-tauri/src/lib.rs` + `main.rs`）：`run()` 走 `.expect()`，而 `main.rs` 在 release 设置了 `windows_subsystem = "windows"`，stderr 不可见 —— 用户双击后无窗口、无提示、无日志。修复：失败时写日志文件并弹系统对话框。
- [x] 4.5 **[HIGH] IPC 错误通道把远端可控字符串原样送到 UI**（`web/src-tauri/src/installer/error.rs`）：`network_error` / `zip_error` 直接 `format!("{err}")`，把 `reqwest::Error` 的 `Display`（内嵌完整请求 URL）与 `ZipError::InvalidArchive` 的条目名（来自远端 zip、长度不受限）送进 toast。修复：网络错误改为只回显状态码 + 主机名；zip 错误截断到 200 字符。

  **对评审结论的一处纠正**：评审称「`registry` 带 userinfo 时凭据会进入错误消息」。实测（reqwest 0.12.28）**不成立** —— reqwest 渲染 URL 时会剥掉 `user:password@`，实测原始 `Display` 为 `error sending request for url (http://127.0.0.1:1/nope)`。因此凭据断言是空转的（旧代码也能通过），已改为断言真实可复现的部分：**不把请求 URL 回显进 UI**。该测试在旧行为下确实变红（`the request URL must not be echoed into the UI: 下载技能包失败: error sending request for url (http://127.0.0.1:1/nope)`）。
- [x] 4.6 顺带清理：`tauri.conf.json` 的 `app.withGlobalTauri` 改为 `false`。全仓**没有任何代码**读取 `window.__TAURI__`（仅注释提及「为何不用它」），实际走 `@tauri-apps/api/core`。这是死配置，移除后同时收窄了「页面脚本可无条件调用文件系统命令」的暴露面。

### 补测试（本 Milestone 的行为变化均附回归测试，且每条都验证过「去掉修复即变红」）

- [x] 4.7 `src/app/layout.test.ts` 由「仅断言导出存在」改为**真实渲染** `Layout` 并断言 footer（含 `/privacy`、`/terms`、LICENSE、GitHub）在输出中。把 `layout.tsx` stash 回注释状态后该测试变红：`the global footer must be rendered: expected null not to be null`。
- [x] 4.8 新增 `src/shared/ui/alert-dialog.test.tsx`：断言弹窗只渲染调用方给的按钮、且不含硬编码 `取消`。把那行 `sr-only` Cancel 加回去后两个用例均变红：`expected [ 'keep', '取消' ] to deeply equal [ 'keep' ]`。
- [x] 4.9 `error.rs` 新增 4 个单测：短串原样、长串截断且带截断标记、按**字符**而非字节截断（中日韩文本）、网络错误不回显请求 URL。

### 验证

- [x] `cargo test`：**57 passed / 1 ignored**（原 53）；`cargo clippy --all-targets -- -D warnings` 0 问题；`cargo fmt --check` 干净。
- [x] `tsc --noEmit`、`eslint --max-warnings 0` 无错；`vitest run`：**207 文件 / 804 用例**全通过（原 206 / 800）。

**按用户确认的范围未修、已记入归档记录「已知风险」**：`capabilities/default.json` 对自定义命令不生效（安全模型整改，需单独 change）· `csp: null`（需实测后设定）· `bundle.identifier` 用上游反向域名 · `dirs` 版本重复 · `reqwest` 无用 `json` feature · `zip` 仅启用 `deflate` · `/local-skills` 无环境守卫 · `tooltip` portal 不一致 · `agent-icons` 无默认可访问名 · `set_icon` 丢弃 Result · `package.json` 的 Tauri API 进入 web 包 · 7 处文档漂移 · zip 安全公告未验证。

## 下一步

执行 `/harness-archive client-skill-manage-20260911-01` 完成归档。
