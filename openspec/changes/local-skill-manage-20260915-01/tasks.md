---
name: local-skill-manage-20260915-01
status: implemented
---

# 执行任务：本地技能页托管非本仓库技能、软链接安全与来源跳转

> 实现不变量（贯穿全部任务）：
> **所有会改变磁盘内容的操作，必须先 `canonicalize()` 拿到真实目录，再在真实目录上操作；链接本身只在「解除链接」这一个动作里被 `remove_file`。绝不依赖 std 对符号链接的隐式行为。**

## 任务列表

### Milestone 1：链接语义与解析基础设施

- [x] 1.1 新增 `installer/link.rs`：`LocationKind` 四态、`classify`（显式 `symlink_metadata`）、`resolve_real_dir`（唯一写入解析入口）、`remove_location`（显式 `is_symlink()` 分支 → `remove_file`）、`ensure_under_agents_root(s)`。
- [x] 1.2 新增 `installer/frontmatter.rs`：只读 `SKILL.md` 头部 8KB 提取 `homepage`，处理引号/行尾注释/CRLF/嵌套键；非 UTF-8 有损解码。**不引入 YAML 依赖**。
- [x] 1.3 新增 `installer/homepage.rs`：三级优先级 `resolve_homepage` + `validate_external_url`（仅 http/https、拒控制字符与空白、长度上限）。
- [x] 1.4 `installer/mod.rs` 注册新模块。
- [x] 1.5 单测：`classify` 五种形态、`remove_location` 对链接只删链接、`ensure_under_roots` 越界/等于 root/`..`/非法 slug、frontmatter 提取（**含非 UTF-8**，见 7.9）、homepage 三级优先级、URL 白名单与拒绝项。

### Milestone 2：本地技能扫描与聚合

- [x] 2.1 新增 `installer/local_skills.rs`：`LocalSkill` / `SkillLocation` / `SkillOrigin`；`scan_roots` 遍历 agent roots、`read_dir` 失败只跳过该 root；跳过隐藏项与普通文件；以 canonicalize 真实路径为聚合键；metadata 不可解析 → `Unmanaged`。
- [x] 2.2 清理 `metadata.rs` 旧 API：删除 `scan_agent_skills`、`InstalledSkill`、`uninstall_dir`、`UninstallOutcome`；保留元数据读写与 `detect_status`。
- [x] 2.3 `detect_status` 增强：增加 `kind`；`unmanaged` 由 `exists()`（跟随链接）改为 `classify(...).is_some()`，修正悬空链接被误判为「未安装」。
- [x] 2.4 单测：扫描分类矩阵、跨 agent 聚合、独立真目录不聚合、root 不可读只跳过该 root（见 7.10）。
  - **注**：`LocalSkill.unreadable == true` 这条降级分支**无确定性测试**——它只在 `read_dir` 与 `canonicalize` 之间目录消失（竞态）时触发，无法在测试里稳定构造。该分支是防御性兜底，未验证，不要当作已验证行为。

### Milestone 3：安装与更新路径修正

- [x] 3.1 `install_skill` 改为真实路径写入，**绝不 touch 链接本身**。
- [x] 3.2 临时目录名 `{name}.skillhub-tmp-{pid}-{nanos}-{nonce}`，修复 `with_extension` 对含点号 slug 的撞名。
- [x] 3.3 `write_metadata` 改为合并式写回，保留 CLI 的 `versionId` / `fingerprint` / `files`。
- [x] 3.4 `InstalledMetadata` 增加 `homepage: Option<String>` 预留字段（`schemaVersion` 仍为 1）。
- [x] 3.5 覆盖语义保留；目标是链接时按真实目录处理并在 warnings 中说明。
- [x] 3.6 单测：更新链接后 `lstat` 仍为 symlink、目标未变、内容与 metadata 已更新；普通文件被备份而非删除；metadata 合并写回后 CLI 字段仍在。

### Milestone 4：命令层与安全边界

- [x] 4.1 `commands.rs`：`list_installed_skills` → `LocalSkillsPayload`；`detect_skill_status` 增加 `kind`；`uninstall_skill_command` 改为 `(dir, agent?)`；新增 `open_external_url`。
- [x] 4.2 `lib.rs` 注册 `open_external_url`。
- [x] 4.3 单测：`ensure_under_roots` 越界用例全部拒绝；URL 校验分支。

### Milestone 5：前端

- [x] 5.1 `tauri-installer.ts`：`LocalSkill` 及联合类型；`uninstallSkill(dir, agent?)`；`openExternalUrl`；`listInstalledSkills` 返回 `LocalSkillsPayload`。
- [x] 5.2 `local-skills.tsx` + 新增 `local-skill-card.tsx`：origin 徽章、homepage 跳转、位置 chips、按 `kind` 区分的卸载确认文案、仅 Managed 显示更新、失效链接清理、root warning 提示。
- [x] 5.3 `install-for-agent-button.tsx`：适配 `kind`；卸载按位置路径传参。
- [x] 5.4 i18n：三语言 key 对称（`localSkills` 50 个 key + `installForAgent.symlinkNotice`）。
- [x] 5.5 遵循 `docs/standards/frontend.md`：层级、hover/focus/active、语义化 HTML、ARIA、暗色主题。
- [x] 5.6 标题区软链接醒目标识（sky 描边徽章 + `Link2` + 数量 + tooltip）。

### Milestone 6：测试与验证（初版）

- [x] 6.1 `local-skills.test.tsx` 适配并扩充。
- [x] 6.2 `install-for-agent-button.test.tsx` 补充软链接提示与卸载位置路径用例。
- [x] 6.3 以 `#[ignore]` 诊断测试扫描真实 home（绝对/相对/悬空三类真实链接）。

### Milestone 7：评审修复（`/harness-review` 后追加）

初版实现通过编译与全部自测，但四位并行评审（Rust / TypeScript / Security / OpenSpec 对齐）报出 1 个 CRITICAL + 4 个 HIGH。以下为修复项：

- [x] 7.1 **[CRITICAL] wire key 名不匹配导致数据丢失**：前端发 `preserveExisting`，`InstallInput` 期望 `preserve_existing`，serde 静默取默认 `false` → 用户点「备份并安装」实际执行 `remove_dir_all`。修复：`#[serde(rename_all = "camelCase")]`。**先用失败测试证实**（`install_input_reads_the_backup_flag_the_web_view_sends`，修复前红）。
- [x] 7.2 **[HIGH] 「受管」两处定义漂移**：扫描用「能解析」、卸载与安装覆盖用「文件存在」→ metadata 损坏的目录 UI 承诺备份、实际被删除。修复：`has_metadata` 改为解析判定，三个调用点共用。
- [x] 7.3 **[HIGH] `install_skill` 的 `dir` / `registry` 未校验**：`dir` 可指向任意路径并触发递归删除。修复：`dir` 过 `ensure_under_agent_root`；`registry` 过 `validate_external_url`。
- [x] 7.4 **[HIGH] `resolve_install_target` 有副作用且早于下载**：下载失败也会先删掉既有条目；普通文件被归为 `ForeignSymlink` 静默 `remove_file`（由 fail-loud 退化为静默删除）。修复：解析改为无副作用 + 新增 `Predecessor`（`Absent` / `Directory` / `StaleLink` / `File`），清理移到下载成功之后；普通文件一律备份。
- [x] 7.5 **[HIGH] `tasks.md` 虚假勾选**：2.4（unreadable/root 不可读）、1.5（非 UTF-8）、AC9 均无对应测试。修复：补 7.9、7.10、7.11；`unreadable` 改为如实标注「无确定性测试」。
- [x] 7.6 备份与临时目录变成幽灵技能：扫描跳过 `*.skillhub-backup-*` / `*.skillhub-tmp-*`。
- [x] 7.7 多位置卸载默认预选 `locations[0]`（共享真实目录时可能默认删掉所有人的目录）：多位置时改为不预选，确认按钮保持 disabled；测试补上「必须选」的断言。
- [x] 7.8 卸载确认文案显示相对 `target`（如 `../../.agents/skills/x`）而非 `realPath`；`install-for-agent-button` 的软链接提示声称给出「真实目录」但实际给的是链接路径 → 前者改用 `realPath`，后者改为不声称路径。
- [x] 7.9 补 `read_frontmatter_value` 非 UTF-8 用例。
- [x] 7.10 补 `scan_roots` root 不可读（chmod 000）用例。
- [x] 7.11 **[AC9 真实证据]** 新增 `cli/test/unit/services/installed-skill-metadata.test.ts`：用 desktop 端 Rust 写出的 metadata 形状（含新增可选 `homepage`）跑 CLI 的 `readInstalledSkillMetadata`，含截断文件必须 `invalid` 的用例。此前该项目**没有任何**该函数的测试。
- [x] 7.12 `replace_directory`：`remove_dir_all` → `rename` 改为 `rename(old → old.skillhub-old-*)` → `rename(new → real)` → 删除 old，失败时把 old 改回。design.md 同步把「原子替换」改为准确表述。
- [x] 7.13 操作后 refresh 不再打回骨架屏（`refresh({ silent: true })`），保住滚动位置与键盘焦点。
- [x] 7.14 `uninstall_location` 拆出 `uninstall_location_under(dir, roots)`，使「删除 vs 备份」这一决策可被直接测试（此前该函数**零测试**）。
- [x] 7.15 `ensure_under_roots` 拒绝原始字符串以分隔符或 `/.` 结尾的路径（`Path` 抹掉该差异但内核不抹，会让「解除链接」落到真实目录上）；改为**只 canonicalize 父目录、最后一段按原样保留**，链接判定不再依赖分支。
- [x] 7.16 卸载路径只校验「单一路径分量是否安全」（`is_plain_entry_name`），不再要求 `validate_slug`——此前 `我的 技能` 这类真实条目能列出却必然卸载失败。
- [x] 7.17 `install-for-agent-button` 的 `if (result === null) return` 早于 `setAction('idle')`，会永久卡在 `uninstalling` 且所有控件禁用；顺带启用被注释掉的卸载结果详情。
- [x] 7.18 warning 分隔符 `'；'` 改为语言中立的 `' · '`。
- [x] 7.19 文档诚实化：`proposal.md` 非目标 5（Windows junction 未实现专门识别，实际是 fail-closed 且未验证）、`design.md` 的原子性表述、非 UTF-8 行为、`HomepageSource::Metadata` 无生产者。

## 验收检查点

- [x] `cd web/src-tauri && cargo test`：**48 passed, 0 failed, 1 ignored**。
- [x] `cargo clippy --all-targets -- -D warnings`：0 警告；`cargo fmt --check` 干净。
- [x] `cd web && pnpm lint && pnpm typecheck && pnpm vitest run`：lint 0 warning、tsc 无错、**206 文件 / 800 用例**全通过。
- [x] `cd cli && bun test`：**528 passed / 48 files**（含新增 5 个 metadata 契约用例）。
- [x] 验收标准 2：卸载软链接后真实目录内文件 mtime 未变。
- [x] 验收标准 3：更新软链接后 `lstat` 仍为 symlink 且链接目标未变。
- [x] 验收标准 4：跨 agent 同源聚合为一条记录多位置（实机 104 链接位置聚合进 19 个技能）。
- [x] 验收标准 6：非受管目录走备份（含 metadata 损坏的情况，见 7.2）。
- [x] 验收标准 8：非 http/https 一律拒绝。
- [x] 验收标准 9：CLI 校验器接受 desktop 写出的 metadata（7.11）。
- [x] wire 契约锁定：`local_skill_serializes_to_the_web_contract` + `install_input_reads_the_backup_flag_the_web_view_sends`。
- [x] 无新增外部依赖（无 YAML crate、无 Tauri 插件）。
- [x] 受保护路径零变更。
- [ ] **Windows 实机冒烟待执行**（无 Windows 环境）。Unix 专属链接用例以 `#[cfg(unix)]` 跳过；`open_external_url` 在 Windows 用 `explorer.exe` 而非 `cmd /C start`（已由安全评审确认是真实改善）。
- [ ] 二次评审待执行。

## 执行偏差记录

| 项 | 计划 | 实际 | 理由 |
|---|---|---|---|
| `metadata.rs` 清理范围 | 退役 `scan_agent_skills` / `InstalledSkill` | 额外删除 `uninstall_dir` / `UninstallOutcome` | 它是不感知软链接的第二条删除路径，保留会成为绕过 `link::remove_location` 的后门 |
| 3.4 `homepage` 字段 | 排在 Milestone 3 | 提前到 Milestone 1 | `homepage.rs` 依赖该字段，否则无法编译 |
| 新增测试切口 | 未列 | `resolve_install_target`、`swap_into_place`、`ensure_under_roots`、`uninstall_location_under` 拆为可测函数 | 下载与文件系统操作耦合时，「更新不破坏链接」「删除 vs 备份」这些核心契约只能靠联网或间接验证 |
| serde 命名 | 未列 | 线上结构加 `rename_all = "camelCase"` | 前端契约需要；顺带修正 `UninstallResult.backup_dir` 与 TS `backupDir` 长期不一致 |
| 复制路径实现 | 未列 | 复用 `copyToClipboard` helper | 非安全上下文下 `navigator.clipboard` 会失败，仓库已有带回退的 helper |
| `agents.rs` | design 列为「不变」 | 有 1 行改动 | 仅 fmt 移除一行尾随空白 |
| `detect_status` 签名 | 2.3 只写「增加 kind」 | 同时删除死参数 `(agent_id, slug)`、`unmanaged` 语义由 `exists()` 改为 `classify().is_some()` | 后者正是悬空链接误判的修复本身 |
| 5.6 标题徽章 | design 未设计 | 新增 UI + 2 个 i18n key | 实现后按用户要求追加 |
| `cli/` 目录 | 非目标 4：不改 CLI | 新增 1 个**测试文件**（无行为改动） | AC9 需要真实的跨语言验证；此前该函数无任何测试 |

## 已知遗留（本次未修，按约定范围延后）

| # | 级别 | 问题 |
|---|---|---|
| 1 | LOW | 聚合 key 是 lossy `String`（`to_string_lossy`）：含无效 UTF-8 的两个不同目录名可能塌缩为同一个 key 被误合并。改 `HashMap<PathBuf, usize>` 即可，但概率极低 |
| 2 | LOW | macOS 大小写不敏感文件系统上，`canonicalize` 不修正最后一段大小写，符号链接目标拼写与真实 dirent 不一致时可能一分为二 |
| 3 | LOW | testid 以 slug 结尾，两个同名但独立的技能会重复（React key 本身没问题） |
| 4 | LOW | 无 host 白名单：恶意 `SKILL.md` 可诱导点击 `http://127.0.0.1:8080/...` 打本机服务（需用户点击 + 本机存在脆弱服务） |
| 5 | LOW | URL 校验用 trim 后的值、执行未 trim 的原值（无中间 shell，不可注入；但保证不精确） |
| 6 | LOW | `open_external_url` / `open_directory` 的 `.spawn()` 未 `wait()`，每次点击留一个未回收子进程 |
| 7 | LOW | 可访问性：位置 radio 组缺 `role="radiogroup"` 与关联标签；`title` 提示在非可聚焦 `<span>` 上键盘不可达；装饰性图标未 `aria-hidden` |
| 8 | LOW | 测试隔离：`clearAllMocks` 不清空 `mockResolvedValueOnce` 队列；`install-for-agent-button.test.tsx` 重定义 `navigator.clipboard` 未还原 |
| 9 | LOW | 零 `locations` 的技能在 UI 上是死胡同（类型未约束非空，Rust 实际总会给 1 个） |
| 10 | LOW | 聚合技能的 `slug` 取自 `AGENT_PROFILES` 中先被扫描到的 root，顺序未在 design 中约定 |
| 11 | LOW | `install_skill` 无端到端测试（需要联网） |
| 12 | LOW | 前端大量断言比较的是 mock 的 i18n key 而非渲染文本，无法捕捉 `zh.json` 里的错别字 |
| 13 | — | Windows junction 需实机验证（见非目标 5 的修正表述） |

## 风险提示（实现时重点自检）

1. 任何删除路径都必须过 `link::remove_location`，**不得**在别处直接对可能为链接的路径调用 `fs::remove_dir_all`。
2. `uninstall_skill_command` 与 `install_skill` 的 `dir` 都来自前端，路径校验不可省略——两个入口必须对称。
3. `metadata.json` 写回必须是合并式，否则静默丢失 CLI 字段。
4. 「受管」只有一个定义（`metadata::has_metadata`，解析判定）；不要新增第二个。
5. wire 上新增多词字段时，Rust 结构体必须带 `rename_all = "camelCase"`，否则 serde 静默丢弃。

## 下一步

`/harness-review local-skill-manage-20260915-01` 二次评审。
