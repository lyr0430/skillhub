---
name: skill-install-mode-20260916-01
status: planned
created: 2026-09-16
scope: 切片一（设置入口 + 安装方式偏好 + 共享安装 + 已有技能添加到其他 agent）
---

# 执行任务：skill-install-mode-20260916-01

> 范围以 `design.md` 为准。**本 change 只做切片一**，覆盖 proposal 验收 1–7、12、13。
> G3（`.skillhub` 展示分类）、G4（线上版本对比 / 更新·替换）拆为独立 change，**不在本 tasks 内**，不得顺手实现。
>
> 项目规则：只允许实现本文件列出的内容；任何行为变化必须补测试（先写测试，后写实现）。

## 任务列表

### Milestone 1：安装方式偏好 + 设置入口（前端，不改变安装行为）

- [x] Task 1.1：新增 `web/src/shared/lib/install-mode.ts`
  - `InstallMode = 'shared' | 'copy'`；`DEFAULT_INSTALL_MODE = 'shared'`
  - `readStoredInstallMode()` / `saveInstallMode()` / `resolveInstallMode()`
  - localStorage 键 **`skillhub-install-mode`**（与 `skillhub-theme` 前缀一致）
  - 无效值/解析失败回退 `DEFAULT_INSTALL_MODE`，不抛错（对齐 `shared/lib/theme.ts` 范式）
  - 测试：读写往返；非法存储值回退默认；无存储时返回默认；写入后 `resolveInstallMode` 反映新值
- [x] Task 1.2：新增 `web/src/shared/hooks/use-install-mode.ts`，结构对齐 `use-theme.ts`（`{ mode, setMode }`）
- [x] Task 1.3：`web/src/shared/components/settings-trigger.tsx`（齿轮按钮，受控 `open`/`onToggle`）
  - `isTauri()` 为假时返回 `null`（不隐藏、不置灰、不注册路由）
  - **不在此组件内渲染弹窗**：header 带 `backdrop-blur`，会成为 `fixed` 后代的包含块，弹窗在 header 内无法相对视口居中
  - 测试：浏览器下不渲染；桌面端渲染；`aria-expanded` 跟随；点击回调
- [x] Task 1.4：`web/src/shared/components/settings-dialog.tsx`（**居中 modal + 遮罩**，由 `Layout` 渲染于 header 之外）
  - 用 Radix `Dialog`（自带遮罩/居中/焦点陷阱/Escape）；宽度 `min(calc(100vw-2rem), 46rem)`，左菜单 12rem
  - 左侧菜单**以数组渲染**（为后续多菜单预留），当前仅「Skill 配置」；**选中态为实心 pill**（`bg-primary text-primary-foreground`，与顶栏导航一致）——淡色方案在本主题下不可见（`--primary` 近黑）
  - 右侧内容抽 `SettingsSection`（标题 + 描述 + 分隔线）实现**模块区分**，后续新增配置 = 新增一个 section；当前两个模块：安装方式、切换行为
  - 安装方式为 `role="radiogroup"` 单选（`role="radio"` + `aria-checked`），**不设 switch**（选中态已可见，双控件会互相矛盾）
  - 测试：左菜单与两个模式均渲染；单选语义（radiogroup + 2 radio）；默认 shared；切换并持久化；读取已存偏好；`aria-current` 标记选中菜单；关闭时不渲染；Escape 请求关闭
- [x] Task 1.4b：订正——初版为 header 内联浮层，按反馈改为居中 modal；触发与弹窗**拆两个文件**，open 状态提升至 `Layout`
- [x] Task 1.5：`web/src/app/layout.tsx` 持有 `settingsOpen`，header 内渲染 `SettingsTrigger`、header 外渲染 `SettingsDialog`；补 i18n 文案（zh/en/ru）

### Milestone 2：Rust 安装方式分支（核心，最高风险）

- [x] Task 2.1：`install.rs` 新增 `InstallMode` enum（`#[serde(rename_all = "kebab-case")]`，`Default = Shared`）；`InstallInput` 增 `#[serde(default)] pub install_mode: InstallMode`
  - 测试：旧 payload（无 `installMode`）反序列化为 `Shared`；`"copy"`/`"shared"` 正确解析；未知值被拒（不让拼写错误静默退化成 Copy）
- [x] Task 2.2：`agents.rs` 新增 `repo_root()` / `repo_skill_dir(slug) -> PathBuf` → `~/.skillhub/skills/<slug>`
  - 测试：路径形状；与 CLI 的 `~/.skillhub/namespace-sync.json` 不冲突（该文件不在 `repo_root` 之内）
- [x] Task 2.3：`link.rs` 新增 `create_link()`（配 `LinkCreation` 结果）
  - target 为真实目录（`symlink_metadata` 校验非链接），`#[cfg]` 分平台 unix `symlink` / windows `symlink_dir`；失败返回 `InstallError`
  - 目标现状分类复用 `classify`：不存在→建；同源链接→幂等；其他（真实目录/异源链接/悬空链接）→不覆盖 + warning
  - 幂等比较**双侧 canonicalize**（`~` 与绝对路径、macOS `/System/Volumes/Data` 前缀不得被判为异源）
  - 测试：建链接且 target 正确；自动创建缺失父目录；同源幂等；异源拼写仍判同源；真实目录不覆盖且内容不变；异源链接不被改向；源非目录/源为链接均拒绝
- [x] Task 2.4：`install_skill()` 接入分支（判定抽为纯函数 `should_share_to_repo` 以便免网络测试）
  - `Copy` → 保持现状路径，零行为变化
  - `Shared` → 落盘目标换为 `repo_skill_dir(slug)`，`swap_into_place` + `write_metadata` 全部作用于该真实目录；**落盘成功后**再在 agent 目录建链接
  - **不变量**：写入目标恒为真实目录，绝不写穿链接（隐性契约 §10 / R1）
  - 测试：`should_share_to_repo` 真值表（含「显式 `dir` 时即便 shared 也不走仓库」——保住既有写穿链接契约）
- [x] Task 2.5：copy 模式不触碰仓库目录（由 Task 2.4 真值表覆盖）
- [x] Task 2.6：卸载确认——既有测试 `uninstall_detaches_a_link_rather_than_its_target` 覆盖「只删链接、真实文件保留」

### Milestone 3：前端安装链路传递安装方式

- [x] Task 3.1：`features/skill/tauri-installer.ts` 的 `InstallSkillInput` 增 `installMode?: InstallMode`
- [x] Task 3.2：`install-for-agent-button.tsx` 以 `resolveInstallMode()` 在**点击时**取值并传入
  - 测试：默认（未设置）传 `shared`；已存偏好传 `copy`；**弹窗打开后再改偏好，安装时取到新值**（防挂载期 state 过期）

### Milestone 4：已有技能添加到其他 agent（验收 12）

- [x] Task 4.1：新增 `installer/attach.rs`：`attach_skill_to_agent` / `attach_under` / `resolve_attach_source` / `copy_dir_recursive`
  - 入参 `source_dir`、`agent`、`mode`；纯本地操作，不下载、不查 registry
  - **源校验用 `is_skill_package()`**（改为 `pub(crate)` 复用，保持单一定义）+ canonicalize 为目录；**不用 `ensure_under_agent_root`**
  - 目标路径由 `agent_root.join(slug)` 构造，`slug` 取源目录名（用户重命名过的技能按自己的名字出现）
  - `shared` → 复用 `create_link`；`copy` → `copy_dir_recursive`（链接**重建**而非跟随，防复制拉入外部内容）
  - `attach_under` 拆出以便测试注入 temp root（**避免测试写真实 home**）
  - 测试：源在 agent root 之外**通过**校验；缺 `SKILL.md`/不存在/是文件均拒绝；未知 agent 拒绝；shared 建链接且 target 正确；shared 幂等；shared 目标占用不覆盖且用户内容不变；copy 复制完整树；copy 目标占用不覆盖；copy 保留链接语义
- [x] Task 4.2：`commands.rs` 注册 `attach_skill_to_agent_command`（`spawn_blocking` + `CommandResult` 信封）+ `lib.rs` 注册
- [x] Task 4.3：`tauri-installer.ts` 新增 `attachSkillToAgent()` + `AttachSkillInput`/`AttachSkillResult`
- [x] Task 4.4：`local-skill-card.tsx` 新增「添加到其他 agent」入口（`attachTargetCount === 0` 或链接不可解析时不渲染）
- [x] Task 4.5：agent 多选弹窗（只列未持有该技能的 agent；未选时确认禁用）
- [x] Task 4.6：`local-skills.tsx` 接线，逐 agent 调用（**单个失败不中断其余**），成功后 `refresh({ silent: true })`
  - 测试：有其他 agent 时出现入口 / 全部持有时隐藏；只列未持有者；未选禁用确认；逐个调用且 mode 正确；已存偏好生效；两选中一个失败仍尝试另一个且页面仍刷新

### Milestone 5：验证与文档

- [x] Task 5.1：**自动化**验收——前端 `pnpm test` 837 通过 / `pnpm lint` / `pnpm typecheck` 干净；Rust `cargo test` 87 通过（1 ignored）、`cargo clippy -D warnings` 干净
  - ⚠️ **未做**：在真实桌面应用中手工点选验收（需启动 Tauri app，本环境无法完成）——留待人工
- [x] Task 5.2：CLI 回归——`cli/` 零改动；新增测试断言 `repo_root()` 与 `~/.skillhub/namespace-sync.json` 不重叠
- [x] Task 5.3：隐性契约 §10 复核——`has_metadata` 仍是「受管」唯一定义（未新增）；`create_link` 仅 2 个调用点且只建链接；`attach.rs` 无 `swap_into_place`/`remove_dir_all`/`fs::rename`
- [x] Task 5.4：知识回写——已确认 4 条写入 `docs/architecture/implicit-contracts.md` §10，并标注来源变更

## 验收检查点

- [x] Rust 编译通过（`cargo check` / `cargo clippy -D warnings` 干净）
- [x] 前端类型检查通过（`pnpm typecheck`）+ lint 通过（`--max-warnings 0`）
- [x] Rust 单元测试通过（106 passed / 1 ignored）
- [x] 前端单元测试通过（851 passed，210 files）
- [ ] 手工验收 proposal 标准 1–7、12、13 通过 —— **需在真实桌面应用中点选，本环境无法完成**
- [x] CLI 回归通过（`cli/` 零改动 + 路径不重叠断言）
- [x] 确认**未触碰**受保护路径（`server/`、`sql/`、`deploy/`、`infra/`、`application*.yml`、`db/`）
- [x] Review 通过（`.claude/REVIEW.md`）—— 四位并行评审（Rust / TypeScript / 安全 / OpenSpec 对齐），发现 1 HIGH（attach 源位置边界）+ 2 HIGH（未注册 dialog 依赖、死 i18n key）+ 2 MEDIUM（i18n 缺失/未实现、fmt），已全部修复并补测试；见归档记录「评审结论」

## 下一步

Review：`/harness-review skill-install-mode-20260916-01`

> 切片二（`.skillhub` 展示分类）与切片三（线上版本对比 / 更新·替换）完成后另行 `/harness-propose` 建独立 change。
