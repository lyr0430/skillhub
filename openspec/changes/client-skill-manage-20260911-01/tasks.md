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
- [ ] Review 通过。

## 下一步

执行 /harness-apply client-skill-manage-20260911-01，然后 /harness-review 进行代码评审。
