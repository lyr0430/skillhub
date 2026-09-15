---
name: local-skill-manage-20260915-01
created: 2026-09-15
status: draft
---

# 需求提案：本地技能页托管非本仓库技能、软链接安全与来源跳转

## 背景

桌面端「本地技能」页（`web/src/pages/dashboard/local-skills.tsx`）的数据来自 Tauri 命令 `list_installed_skills`，其实现 `scan_agent_skills()`（`web/src-tauri/src/installer/metadata.rs:205`）**只返回带 `.skillhub/metadata.json` 的目录**。由此产生四个具体问题。

### 1. 非本仓库来源的 skill 完全不可见

用户手工拷贝、从其他 agent 生态（`~/.cc-switch/skills`、`~/.agents/skills`）、或用第三方工具装到 `~/.claude/skills` 等目录的 skill，因为没有 `.skillhub/metadata.json`，在本地技能页**一条都看不到**。页面顶部还带着 `visibleSkills.length === 0` → 「暂无本地技能」的空态，与实际磁盘内容不符。用户既无法确认自己装了什么，也无法在页面里清理。

### 2. 软链接语义未建模，更新会破坏链接（数据一致性风险）

实测用户环境 `~/.claude/skills/` 下大量条目是符号链接，指向三类目标：

| 链接 | 目标 | 形式 |
|---|---|---|
| `agent-browser-2` | `/Users/zhenqi/.cc-switch/skills/agent-browser-2` | 绝对路径 |
| `agently-mail` | `../../.agents/skills/agently-mail` | 相对路径 |
| `brand-voice` | `/Users/zhenqi/play/codex/skill-manage/skills/brand-voice` | 工作区路径 |

现有实现对链接与真实目录**无任何区分**：

- **卸载**：`uninstall_dir()` 直接 `fs::remove_dir_all(install_dir)`。在 Rust 1.92 / Unix 下 `remove_dir_all` 对顶层链接只做 `unlink`（已核对 std 源码 `library/std/src/sys/fs/unix.rs:2568-2578`），**当前恰好安全**；但这是**未文档化**的平台细节（历史上曾跟随链接，CVE-2022-21658 即源于此），Windows 分支实现也不相同。当前代码把用户数据安全押在一个隐式行为上。
- **更新**：这条是真的坏。`install_skill()` 替换阶段执行 `remove_dir_all(target_dir)` → `fs::rename(effective, target_dir)`（`install.rs:127-146`），**rename 会在链接路径上新建一个真实目录**，链接关系被摧毁。结果：`~/.agents/skills/x` 里的真实技能仍是旧版，`~/.claude/skills/x` 变成一个独立副本，两边此后各自漂移。
- **同源重复展示**：同一份真实目录会被多个 agent root 扫到（`generic` 与 `claude-code`），页面出现两行内容相同、只有 agent 标签不同的记录；而卸载其中一行的影响范围完全不同（删链接 vs 删真实目录）。
- **悬空链接无感知**：`detect_status()` 用 `install_dir.exists()`（跟随链接）判断，链接失效时返回 `false`，条目直接消失，用户看不到任何「失效链接」线索。
- **附带缺陷**：临时目录名用 `target_dir.with_extension("tmp-install")`，slug 含点号时（如 `foo.bar`）会变成 `foo.tmp-install`，与另一个 slug 的临时目录撞名。

### 3. 缺少来源/主页跳转入口

部分 skill 的 `SKILL.md` frontmatter 带 `homepage`（源仓库地址）。本地技能页标题区只有 `@namespace/slug` 文本，没有任何跳转入口，用户无法回溯来源；对没有 `homepage` 的技能（本地自有、从别处拷贝）更是完全没有线索。

### 4. `.skillhub/metadata.json` 未记录来源地址

安装时写入的 metadata 只有 `registry/namespace/slug/version/source`（`metadata.rs:21`，实例见 `~/.claude/skills/weather/.skillhub/metadata.json`），没有可点击的来源地址，无法承担「外挂 homepage」的角色。

## 目标

### G1. 本地技能页托管所有本地 skill（不只是本仓库安装的）

- 扫描 agent skill root 下的**全部**条目并按可控性分类，而不是只挑有 metadata 的：
  - `managed`：有合法 `.skillhub/metadata.json`（skillhub 安装，可更新 / 可卸载）
  - `unmanaged`：无 metadata 或 metadata 损坏（本地自有 / 第三方来源，只能查看与安全清理）
  - `broken-link`：悬空链接（只提供「清理失效链接」）
- 页面可展示、搜索、打开目录、复制路径；**非受管项的破坏性操作必须与受管项在 UI 上明确区分**（文案 + 二次确认 + 备份语义）。

### G2. 完整建模并安全处理符号链接

- 扫描时区分 `dir` / `symlink` / `broken-link` / `junction`，并解析真实路径。
- **卸载只删链接本身**：显式以 `symlink_metadata().file_type().is_symlink()` 判定后 `remove_file`，不依赖 std 隐式行为；真实目录原封不动，结果中返还真实目录位置供 UI 告知用户。
- **更新写真实位置**：解析到真实目录后，在**真实目录的同级**建临时目录并原子替换，链接保持不变；多个 agent 因共享同一真实目录而同步生效。
- 以**真实路径**为单位聚合去重：一个技能实体关联多个「位置」（agent + 路径 + 类型），页面显示为一条记录 + 多个 agent 标签，且可只解除其中某一个链接。
- 悬空链接、链接环、指向非目录的链接、无权限目录，均不得导致扫描崩溃或误删。

### G3. homepage 跳转与「外挂 homepage」

- 本地技能页标题区提供跳转按钮，可跳转到源仓库地址。
- 地址优先级：`SKILL.md` frontmatter 的 `homepage` > `.skillhub/metadata.json` 中记录的外挂 `homepage` > 由 `registry + namespace + slug` 推导的 SkillHub 技能页地址。
- 安装时把来源地址写入 `.skillhub/metadata.json`（外挂 homepage），使「内容里没有 homepage」的技能也有可跳转的来源。
- 跳转必须是安全的外部打开：**仅允许 http/https**（拒绝 `javascript:` / `data:` / `file:` 等），走系统默认浏览器，**不允许在 WebView 内直接导航**。

### G4. 保持 CLI 与桌面端共享的 metadata 契约向后兼容

- 新增字段必须是**可选字段**，不改变 `schemaVersion: 1` 的语义；旧版 CLI 读到新增字段必须仍判定为 valid。（已核对 `cli/src/services/installed-skill-metadata.ts:27-61`：只校验已知字段的类型，不拒绝未知字段，因此天然兼容。）

## 非目标

1. 不新增 / 不修改后端 API 与数据库表。后端 `SkillDetailResponse` 当前没有 homepage / sourceUrl 字段，本期不追加（由 G3 的推导兜底覆盖）。
2. 不做「把本地非受管技能发布 / 上传到 SkillHub」的能力。
3. 不实现技能内容编辑、版本回退、批量升级。
4. 不改造 CLI 的安装 / 更新行为（只要求 metadata 读写兼容）。
5. 不保证 Windows 上以 junction / `.lnk` 建立的链接可被安全删除。**本期未实现专门的 junction 识别**：Windows 上目录 junction 会被 `is_symlink()` 判为链接，因而走 `remove_file` 分支——该操作对目录型 reparse point 会失败，所以是 fail-closed（报错而非删掉目标），但这依赖 std/平台行为且**未在 Windows 上验证**。原表述「正确识别并降级为不可管理的链接」与实现不符，已按实际行为修正。
6. 不处理技能目录**内部**的符号链接（只处理 skill 目录这一层）。
7. 不做 agent 自动发现（仍沿用 `AGENT_PROFILES` 静态列表）。

## 利益相关方

- **桌面端用户**（主要）：本地技能可见性、清理体验与数据安全的直接受益者。
- **已有大量软链接工作流的用户**：更新语义修正后不再出现「副本漂移」。
- **CLI 用户**：与桌面端共享 `.skillhub/metadata.json`，受 G4 契约约束。
- **前端 / 桌面端开发**：本地技能页数据结构从扁平列表变为「实体 + 位置」模型，改动面覆盖 Rust 命令、TS 类型、页面组件、i18n。
- **平台治理**：来源跳转为 skill 溯源提供线索。

## 验收标准

1. 在一个同时包含「受管目录、受管软链接、非受管目录、非受管软链接、悬空链接、`.DS_Store` 等杂项」的 `~/.claude/skills` 上打开本地技能页，每一类条目都能被正确列出或正确归类；页面不报错、不漏项、不误判。
2. 卸载一个**软链接**形式的技能后：链接消失，**真实目录及其全部文件完好无损**（以真实目录内文件的 mtime / inode 未被改变作为判定依据），且 UI 提示真实目录位置。
3. 更新一个**软链接**形式的受管技能后：该路径 `lstat` 仍为 symlink，链接目标仍是原真实路径，真实目录内容更新到目标版本，`metadata.json` 版本同步更新。
4. 同一个真实目录被 `~/.agents/skills` 与 `~/.claude/skills`（链接）同时引用时，页面显示为**一条**技能记录并带两个位置标签，而不是两条重复记录。
5. 悬空链接可被识别并可一键清理，清理只删除链接。
6. 非受管技能不出现「更新」入口；其卸载路径有明确的二次确认与「备份而非删除」或「仅删链接」的行为说明。
7. 带 `homepage` frontmatter 的技能在标题区显示跳转按钮并能在系统浏览器打开；无 `homepage` 的技能，若 `metadata.json` 有来源地址则同样显示跳转按钮；两者都没有时，跳转到由 registry 推导的技能页地址。
8. 外部跳转对 `javascript:`、`data:`、`file:` 等非 http/https 地址一律拒绝。
9. 新增字段后的 `metadata.json` 仍能被 CLI 判定为 valid（`readInstalledSkillMetadata` 返回 `status: 'valid'`）。
10. 以上行为均有自动化测试覆盖：Rust 侧软链接矩阵以单元测试覆盖（`std::os::unix::fs::symlink`，Windows 用 `#[cfg]` 跳过）；前端页面对应交互为 Vitest 组件测试。

## 待确认问题（设计阶段决定）

1. **是否需要「接管 / adopt」能力**：为本地非受管 skill 生成 `.skillhub/metadata.json` 以纳入托管？风险是给用户自制技能写入本不存在的来源信息。倾向于不做，或做成显式的「标记为本地技能」。
2. **卸载受管软链接后真实目录仍在**：是否追加提示「真实目录仍存在于 &lt;path&gt;，是否一并删除？」，默认不删。
3. **frontmatter 解析方式**：新增 YAML 依赖（`serde_yaml` 已废弃，候选 `serde_norway` / `yaml-rust2`），还是自写轻量 `key: value` 提取器（只读文件头 8KB，覆盖引号 / 注释 / 行尾）？需在 design.md 说明取舍与依赖理由。
4. **目录名与 `metadata.slug` 不一致时**（用户改名）如何展示与处理。
5. **外挂 homepage 的取值来源**：① zip 内 `SKILL.md` 的 `homepage` ② 后端未来字段 ③ registry 技能页 URL。本期至少落 ③，①② 是否纳入需确认。
6. **技能详情页 / 技能卡片**是否也需要同样的跳转按钮（当前需求只提本地技能页标题）。

## 备注

### 相关现有实现

| 文件 | 关注点 |
|---|---|
| `web/src-tauri/src/installer/metadata.rs` | `scan_agent_skills` / `detect_status` / `uninstall_dir` / `backup_dir` |
| `web/src-tauri/src/installer/install.rs` | `install_skill` 的替换与 rename 逻辑、`flatten_single_root` |
| `web/src-tauri/src/installer/agents.rs` | `AGENT_PROFILES`、`skill_dir`、`validate_slug` |
| `web/src-tauri/src/commands.rs` | `list_installed_skills` / `detect_skill_status` / `uninstall_skill_command` / `open_directory` |
| `web/src/features/skill/tauri-installer.ts` | TS 契约与 `InstalledSkill` 类型 |
| `web/src/pages/dashboard/local-skills.tsx` + `.test.tsx` | 页面与组件测试 |
| `cli/src/services/installed-skill-metadata.ts` | 共享 metadata 契约 |
| `web/src-tauri/capabilities/default.json` | 仅 `core:default`；外部打开沿用 `open_directory` 的 `Command` 模式可避免新增 Tauri 插件 |

### 前置变更

`openspec/changes/client-skill-manage-20260911-01`（本地安装能力与本地技能页初版）。

### 安全边界

本次改动全部位于 `web/src-tauri` 与 `web/src`，**不涉及任何受保护路径**（`application*.yml`、`db/`、`sql/`、`deploy/`、`infra/`、`secrets/`）。

### 已核对的事实（供 design 阶段直接引用）

- Rust 1.92 Unix `remove_dir_all` 对顶层符号链接执行 `unlink`（`library/std/src/sys/fs/unix.rs:2568-2578`），属未文档化行为，不应依赖。
- CLI 的 metadata 校验不拒绝未知字段，新增可选字段向后兼容。
- `dirs::home_dir()` 已用于家目录解析；`AGENT_PROFILES` 目前只有 5 个 profile（generic / claude-code / codex / opencode / openclaw）。
- 代码库中目前**没有任何** `homepage` 的解析或展示逻辑（`web/`、`cli/`、`server/` 均无命中）。

### 其他

- i18n：新增文案需同时补齐 `zh.json` / `en.json` / `ru.json`。
- 手工验证样本：`~/.claude/skills` 下同时存在绝对链接（`agent-browser-2`）、相对链接（`agently-mail`）与指向工作区路径的链接（`brand-voice`）。
