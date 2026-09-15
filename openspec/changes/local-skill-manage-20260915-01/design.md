---
name: local-skill-manage-20260915-01
status: designed
---

# 技术方案：本地技能页托管非本仓库技能、软链接安全与来源跳转

## 方案概述

桌面端「本地技能」页当前的数据源 `list_installed_skills` → `scan_agent_skills()` 只返回带 `.skillhub/metadata.json` 的目录（`web/src-tauri/src/installer/metadata.rs:205`），导致三类问题：非本仓库来源的技能完全不可见、符号链接语义未建模（更新会摧毁链接）、缺少来源跳转入口。

本方案把本地扫描从「按目录逐个判断」升级为「**按真实目录聚合的技能实体 + 多位置**」模型，并确立一条贯穿全部实现的不变量：

> **所有会改变磁盘内容的操作，必须先 `canonicalize()` 拿到真实目录，再在真实目录上操作；链接本身只在「解除链接」这一个动作里被 `remove_file`。绝不依赖 std 对符号链接的隐式行为。**

已确认的四项设计决策：

| 决策 | 结论 |
|---|---|
| 非受管技能的能力边界 | 可查看 / 打开 / 复制路径 / **可卸载但强制备份**；无「更新」入口；不做「接管(adopt)」 |
| 跨 agent 同源展示 | **合并为一条记录 + 多位置标签**，可单独解除某一个链接 |
| 外挂 homepage 来源 | **仅用 `{registry}/space/{namespace}/{slug}` 兜底**；预读 `metadata.json.homepage` 字段但不写入 |
| frontmatter 解析 | **自写轻量提取器**（只读文件头 8KB），不引入 YAML 依赖 |

### 模块划分（方案 A，已确认）

| 文件 | 职责 |
|---|---|
| `installer/link.rs`（新增） | 链接语义：类型判定、真实路径解析、安全删除、路径合法性校验 |
| `installer/local_skills.rs`（新增） | 扫描 agent skill root、聚合为 `LocalSkill`、卸载编排 |
| `installer/frontmatter.rs`（新增） | 轻量 SKILL.md frontmatter 提取（当前只需 `homepage`） |
| `installer/homepage.rs`（新增） | homepage 三级优先级解析 + URL 校验 |
| `installer/metadata.rs` | 退回**纯元数据读写**；`InstalledMetadata` 增加 `homepage` 可选字段 |
| `installer/install.rs` | 安装/更新改为「解析真实目录后原子替换」 |
| `installer/agents.rs` | 不变（`AGENT_PROFILES` 继续作为扫描根与路径校验基准） |
| `commands.rs` | 命令签名调整 + 新增 `open_external_url` |

被否决的备选：全部逻辑堆进 `metadata.rs` / `install.rs`。否决理由——两个文件会同时承担元数据读写、链接语义、扫描聚合三种职责，`metadata.rs` 现已 233 行，叠加链接矩阵将超 450 行且测试难以定位，违反仓库「按领域组织、文件 <800 行」的规范。

## 详细设计

### 1. 数据模型

`InstalledSkill`（扁平）→ `LocalSkill`（实体 + 多位置）：

```rust
/// 位置类型
pub enum LocationKind {
    Dir,             // 普通目录
    Symlink,         // 有效符号链接，指向目录
    BrokenSymlink,   // 悬空链接（目标不存在）
    ForeignSymlink,  // 指向文件，或 canonicalize 报 ELOOP（链接环）
}

/// 一个技能实体在某一个 agent root 下的存在形式
pub struct SkillLocation {
    pub agent: String,       // agent id（AGENT_PROFILES 的 id）
    pub path: String,        // 该 root 下的条目路径（链接本身或目录本身）
    pub kind: LocationKind,
    pub target: Option<String>, // 链接指向的原始 target（保留相对链接原样，仅展示用）
}

pub enum SkillOrigin { Managed, Unmanaged }   // 决定是否有「更新」入口

pub enum HomepageSource { Frontmatter, Metadata, Registry }

pub struct LocalSkill {
    pub slug: String,                 // 展示名：目录名
    pub metadata_slug: Option<String>, // metadata 中的 slug；与目录名不一致时前端提示
    pub namespace: Option<String>,
    pub version: Option<String>,
    pub registry: Option<String>,
    pub origin: SkillOrigin,
    pub real_path: Option<String>,    // canonicalize 后的真实目录；BrokenSymlink 为 None
    pub locations: Vec<SkillLocation>,
    pub homepage: Option<String>,
    pub homepage_source: Option<HomepageSource>,
    pub unreadable: bool,             // 无权限读取，单项降级不炸整表
}
```

**聚合键 = `canonicalize()` 后的真实路径**。`~/.claude/skills/x`（链接）与 `~/.agents/skills/x`（真目录）聚合为一条记录、两个位置。若两个 agent root 下是两个**互相独立**的真实目录，canonicalize 结果不同 → 保持两条记录（正确行为）。

### 2. 扫描算法（`local_skills.rs`）

对 `AGENT_PROFILES` 中每个 profile 的 `profile_root()`：

1. `read_dir` 失败（不存在 / 无权限）→ 跳过该 root，记入 warnings，不影响其他 root。
2. 跳过以 `.` 开头的条目（`.skillhub`、`.DS_Store`）与既非目录也非链接的普通文件（如 `debug-issue.md`）。
3. `symlink_metadata(entry)` 判定：
   - `file_type().is_symlink()` → 链接分支
     - `canonicalize(entry)` 成功且目标是目录 → `Symlink`，继续通过链接读取 SKILL.md / metadata
     - `canonicalize(entry)` 成功但目标是文件 → `ForeignSymlink`
     - `Err(NotFound)` → `BrokenSymlink`
     - `Err(其他，含 ELOOP)` → `ForeignSymlink`
   - `file_type().is_dir()` → `Dir`
3.5 跳过我们自己的记账条目：`*.skillhub-backup-*`（未受管目录被卸载时的备份）与 `*.skillhub-tmp-*`（安装失败残留）都是 skills root 里的同级条目，若不过滤会作为「新的未受管技能」重新出现，并给用户一个卸载自己备份的按钮。

4. 对可读取的条目解析：
   - `metadata.json` 合法 → `origin = Managed`
   - 无 `metadata.json`、或 JSON 损坏、或字段校验失败 → `origin = Unmanaged`（**安全优先**：读不懂就当非受管，走备份路径而非删除）
5. 以 canonicalize 结果为 key 聚合到 `LocalSkill`；`BrokenSymlink` / `ForeignSymlink` 不聚合，各自成条。
6. 读取 `SKILL.md` 头部 8KB 提取 frontmatter `homepage`。
7. 解析 homepage 三级优先级（见第 4 节）。

**性能**：每个条目 1 次 `symlink_metadata` + 1 次 `canonicalize` + 最多 2 次小文件读取（`metadata.json`、`SKILL.md` 头部 8KB）。agent root 数量为 5，条目量级 < 200，无需缓存或并行。

### 3. 链接语义矩阵（核心）

#### 卸载（按**位置**粒度，`link.rs::remove_location`）

| 位置类型 | 行为 |
|---|---|
| `Dir` + `Managed` | `remove_dir_all(path)` |
| `Dir` + `Unmanaged` | **`backup_dir(path)` 重命名备份**，绝不删除 |
| `Symlink`（无论目标受管与否） | **仅 `remove_file(path)` 删除链接**，真实目录原封不动 |
| `BrokenSymlink` | 仅 `remove_file(path)`（「清理失效链接」） |
| `ForeignSymlink` | 仅 `remove_file(path)` |

```rust
pub struct LocationRemoval {
    pub removed_kind: LocationKind,
    /// 链接被移除时，仍然存在的真实目录路径（供 UI 明确告知用户）
    pub real_path_kept: Option<String>,
    /// 非受管目录被备份时的备份路径
    pub backup_dir: Option<String>,
}
```

实现必须**显式**判定：

```rust
if fs::symlink_metadata(path)?.file_type().is_symlink() {
    fs::remove_file(path)?;          // 只删链接，不解引用
} else {
    /* 目录分支 */
}
```

**禁止**对可能是链接的路径直接调用 `remove_dir_all`，即使当前 Rust 版本在 Unix 上恰好只做 `unlink`——该行为未写入文档（`library/std/src/fs.rs:3093` 的文档未承诺），历史上曾跟随链接（CVE-2022-21658 即源于此），且 Windows 分支实现不同。

#### 更新（按**技能实体**粒度，`install.rs::install_skill` 改造）

当前实现（`install.rs:127-146`）在替换阶段执行 `remove_dir_all(target_dir)` → `fs::rename(effective, target_dir)`，**rename 会在链接路径上新建真实目录，摧毁链接关系**。修正后流程：

```
(target, predecessor) = resolve_install_target(location.path)   // 无副作用：只判定，不动盘
download → tmp = real.parent()/{name}.skillhub-tmp-{pid}-{nanos}-{nonce} → extract_zip
clear_predecessor(target, predecessor)         // 失效链接→remove_file；普通文件→备份
replace_directory(real, effective)             // rename(real → real.skillhub-old-*) → rename(effective → real) → 删除 old
write_metadata(&real, ...)                     // 写到真实目录
```

关于「原子性」的准确表述：最终那一步 `rename` 本身是原子的，但**整个替换不是**——它由两次 `rename` 加一次删除组成。之所以先 `rename(real → old)` 而不是 `remove_dir_all(real)`，是为了让**每一个中间状态都有内容留在盘上**，且 `rename(effective → real)` 失败时能把 old 改回来；`remove_dir_all` 先行的写法会在失败时同时失去旧版本并让新版本卡在临时目录里。

`resolve_install_target` 刻意无副作用，清理放到**下载成功之后**：否则一次失败或不被授权的下载会先把用户原有的条目毁掉。

**绝不 touch 链接本身**。改完之后：`~/.claude/skills/x` 仍是链接、仍指向 `~/.agents/skills/x`，内容已是新版本，所有引用该真实目录的 agent 同步生效。

顺带修复两个现存缺陷：

1. **临时目录撞名**：现用 `target_dir.with_extension("tmp-install")`，slug 含点号时（`foo.bar` → `foo.tmp-install`）会与另一个 slug 的临时目录冲突。改用 `{name}.skillhub-tmp-{pid}-{nonce}`，其中 `nonce` 为进程内自增计数器 + 纳秒时间戳。
2. **metadata 覆盖丢失 CLI 字段**：CLI 的 metadata（`cli/src/services/installed-skill-metadata.ts:11-20`）支持 `versionId` / `fingerprint` / `files` / `agent`，而桌面端 `InstalledMetadata` 结构体只有 6 个字段，serde 默认忽略未知字段 → **更新时重写 metadata 会静默丢掉这些字段**。修正：`write_metadata` 改为「先读取原始 JSON 为 `serde_json::Map`，合并已知字段后再写回」，保留所有未知字段。

#### 安装（`install_skill` 的覆盖语义）

`preserve_existing` 语义保留，但判断分支前先解析真实目录：目标位置是链接时，按真实目录处理，不破坏链接。前端「安装到已存在同名软链接的位置」时，UI 提示为「将更新 `<真实目录>`」。

### 4. homepage 解析与跳转（`frontmatter.rs` + `homepage.rs`）

**提取器**（`frontmatter.rs`，不引入 YAML 依赖）：

- 只读 `SKILL.md` 头部 8KB（frontmatter 必在文件头）。
- 定位：首行须为 `---`，向下找到下一个 `---` 作为块结束；文件无合法 frontmatter → 返回 `None`。
- 在块内匹配 `^homepage\s*:\s*(.+?)\s*$`（多行缩进不算，本字段为标量）。
- 值清洗：去除包裹的 `'` / `"`；去除未被引号包裹的行尾 `# 注释`；trim。
- 找不到 / 文件不存在 → `None`（不报错）。
- 非 UTF-8 → 按字节有界读取后用 `String::from_utf8_lossy` **有损解码**（不是返回 `None`）：被截断的多字节字符不会让一个本来合法的文件解析失败，而首行不是 `---` 时自然仍返回 `None`。

**三级优先级**（`homepage.rs::resolve_homepage`）：

```
1. SKILL.md frontmatter 的 homepage      → HomepageSource::Frontmatter
2. metadata.json 的 homepage 字段        → HomepageSource::Metadata   （**本期无生产者**）
3. {registry}/space/{namespace}/{slug}   → HomepageSource::Registry   （路由见 web/src/app/router.tsx:309）
```

**关于第 2 级的准确表述**：它是一条**预留的读取路径**，本期没有任何代码写入该字段（`InstalledMetadata::new` 固定 `homepage: None`），因此生产环境中这一级不可达，`HomepageSource::Metadata` 只能由手工编辑的 metadata 触发。「安装时把外挂 homepage 写进 metadata」是原始需求的一部分，但已确认的决策是「仅用 registry URL 兜底、零安装流程改动」，故本期只保留读取端，写入端留待后续。**不要把它当作已实现的能力。**

第 3 级需要 registry 与 namespace 同时可用（即 `origin = Managed` 且 metadata 合法）；非受管技能若 frontmatter 无 homepage，则没有跳转按钮。

**打开方式**：新增 Tauri 命令 `open_external_url(url)`，沿用 `open_directory`（`commands.rs:158`）的 `std::process::Command` 模式：

| 平台 | 命令 |
|---|---|
| macOS | `open <url>` |
| Windows | `cmd /C start "" <url>` |
| Linux | `xdg-open <url>` |

**不引入 `tauri-plugin-opener`**：`capabilities/default.json` 目前只有 `core:default`，引入插件需要扩权限面，对单一功能不值得。

**Rust 侧强校验**（前端传入的任何值都不可信）：

- 解析为绝对 URL，scheme **仅允许 `http` / `https`**。
- 显式拒绝 `javascript:` / `data:` / `file:` / `vbscript:` 等。
- 拒绝含 ASCII 控制字符（`\0`、`\n`、`\r` 等，防命令注入）。
- 长度 ≤ 2048。
- 一律交给系统默认浏览器，**禁止 WebView 内导航**。

### 5. 命令与安全边界（`commands.rs`）

| 命令 | 变更 |
|---|---|
| `list_installed_skills` | 返回 `Vec<LocalSkill>`（结构变更） |
| `detect_skill_status` | 增加 `kind` 字段；修正悬空链接被 `exists()` 误判为「未安装」的问题 |
| `install_skill_command` | 内部走真实路径写入（第 3 节）；`preserve_existing` 语义不变（并修正 wire 上的 key 名，见下）；**`dir` 与 `registry` 改为可信边界内校验** |
| `uninstall_skill_command` | 签名由 `(agent, slug)` 改为 `(dir, agent?)` —— 因为要能单独解除某一个链接 |
| `open_external_url` | 新增 |

⚠️ **新增安全要点（必须实现）**：`uninstall_skill_command` 的 `dir` 现在来自前端，存在**任意路径删除**风险。校验规则：

0. **拒绝原始字符串以分隔符或 `/.` 结尾的路径**。`Path` 会抹掉这个差异（`"/a/b/"`、`"/a/b/."`、`"/a/b"` 的 `file_name()` 都是 `"b"`），但内核不会——对前两种形式它会解引用末尾软链接，导致「解除链接」的请求落到链接指向的真实目录上，删掉真目录却留下链接。这个差异只在原始文本里存在。
1. 只对 `parent()` 做 `canonicalize`，最后一段**按原样保留**（这正是链接仍是链接的原因；对整条路径 `canonicalize` 会解引用它，还会在悬空链接上直接失败）。
2. 规范化路径必须**位于某个 `AGENT_PROFILES` 的 `profile_root()` 之下**（前缀匹配，逐段比较而非字符串前缀，防 `/Users/x/.claude/skills-evil`）。
3. 规范化路径**不得等于**任何 `profile_root()` 本身。
4. `file_name()` 必须通过 `is_plain_entry_name`：非空、≤255 字节、不等于 `.` / `..`、不以 `.` 开头。**刻意比 `validate_slug` 宽松**——后者约束的是「允许创建什么 slug」，而这里只判断「这个名字是否可以安全操作」；用户自己命名的 `我的 技能` 是一个真实条目，必须仍然可卸载。它无法造成穿越：这是一个单一路径分量，天然不含分隔符。

### 5.1 新增/修正的 wire 契约

`InstallInput` 增加 `#[serde(rename_all = "camelCase")]`。**这是一个数据丢失级别的修复**：前端发的是 `preserveExisting`，而结构体字段是 `preserve_existing`，serde 默认忽略未知字段并取 `#[serde(default)]` 的 `false`——于是用户点「备份并安装」，实际执行的是 `remove_dir_all`。回归测试：`install_input_reads_the_backup_flag_the_web_view_sends`。

### 5.2 「受管」只有一个定义

`metadata::has_metadata` 从「`metadata.json` 存在」改为「**能被解析**」（`matches!(read_metadata(..), Ok(Some(_)))`）。此前扫描用「能解析」、卸载与安装覆盖用「存在」，两处定义漂移，导致 metadata 损坏的目录在 UI 上显示为未受管（承诺会备份）而实际被 `remove_dir_all` 删除。现在扫描、卸载、安装覆盖共用同一个函数，无法再漂移。

### 6. 前端变更

#### `web/src/features/skill/tauri-installer.ts`

- `InstalledSkill` → `LocalSkill`（含 `LocationKind` / `SkillOrigin` / `HomepageSource` 联合类型）。
- `uninstallSkill(agent, slug)` → `uninstallSkill(dir, agent?)`。
- 新增 `openExternalUrl(url)`。

#### `web/src/pages/dashboard/local-skills.tsx`

- **列表项**：标题行 = `@namespace/slug`（或目录名）+ `origin` 徽章 + **homepage 跳转按钮**（有值时）
  - 跳转按钮用 lucide `ExternalLink` 图标，带 `aria-label` / `title` / 可见 focus ring；明暗双主题。
- **位置标签**：每个 `location` 一个 agent chip；hover 显示完整路径；`AgentBrandIcon` 复用现有组件。
- **操作**：
  - 卸载 → 弹窗内**先选择要移除的位置**（多位置时），并根据 `kind` 显示不同后果文案：
    - `Symlink`：「仅移除链接 `~/.claude/skills/x`，真实目录 `<real_path>` 不受影响」
    - `BrokenSymlink`：「清理失效链接」
    - `Unmanaged Dir`：「原目录将被备份到 `<name>.skillhub-backup-<ts>`」
    - `Managed Dir`：「将删除该技能目录」
  - 更新 → **仅 `Managed` 显示**；`Symlink` 位置提示「将更新真实目录 `<real_path>`」。
  - 打开目录 / 复制路径 → 沿用。
- **悬空链接**：`BrokenSymlink` 单独徽章标记，可一键清理。
- **空态/加载态**：沿用现有 shimmer 骨架与空态卡片；空态文案需修正——「本地技能」页现在应展示所有技能，无技能时才是真空态。
- 遵守 `docs/standards/frontend.md` 与设计质量标准：层级、hover/focus/active 态、语义化 HTML、ARIA、暗色主题。

#### `web/src/features/skill/install-for-agent-button.tsx`

适配 `detect_skill_status` 新增的 `kind`：目标是软链接时，安装文案提示「将更新真实目录」。

#### i18n

`web/src/i18n/locales/{zh,en,ru}.json` 同步补齐：`origin` 徽章、各 `kind` 的卸载文案、homepage 按钮、失效链接、非受管无更新入口的说明。

## 影响范围

- **受影响模块**：
  - `web/src-tauri/src/installer/`：新增 `link.rs`、`local_skills.rs`、`frontmatter.rs`、`homepage.rs`；改 `metadata.rs`、`install.rs`、`mod.rs`、`installer_tests.rs`
  - `web/src-tauri/src/commands.rs`、`lib.rs`（注册新命令）
  - `web/src/features/skill/tauri-installer.ts`、`install-for-agent-button.tsx`
  - `web/src/pages/dashboard/local-skills.tsx`、`local-skills.test.tsx`
  - `web/src/i18n/locales/{zh,en,ru}.json`
- **受保护路径变更**：**无**。不涉及 `application*.yml`、`bootstrap*.yml`、`db/`、`sql/`、`deploy/`、`infra/`、`secrets/`。
- **数据库/后端**：**无任何变更**。不新增 API、不改表、不动 `SkillDetailResponse`。
- **新外部依赖**：**无**。frontmatter 用自写提取器，外部打开复用 `std::process::Command`，不引入 YAML crate 或 Tauri 插件。
- **向后兼容性**：
  - `InstalledSkill` → `LocalSkill` 是破坏性变更，但唯一消费方是 `local-skills.tsx`（已 grep 确认），前端与 Rust 同二进制分发，无跨版本问题。
  - `.skillhub/metadata.json` 新增 `homepage` 为**可选字段**，`schemaVersion` 保持 `1`。已核对 CLI 校验器（`cli/src/services/installed-skill-metadata.ts:27-61`）只校验已知字段类型、不拒绝未知字段 → **CLI 零改动**。
  - `metadata.json` 写回改为「合并未知字段」，修复现有 CLI 字段丢失缺陷，对 CLI 是纯改善。

## 风险评估

| 风险 | 等级 | 缓解方案 |
|---|---|---|
| 误删用户真实技能目录（最高危） | **高** | 全部删除路径统一走 `link.rs`，显式 `symlink_metadata` 判定；非受管目录一律备份重命名；验收标准 2 用真实目录内文件 mtime/inode 未变作硬判据；Rust 单测覆盖完整链接矩阵 |
| `uninstall_skill_command` 前端传参导致任意路径删除 | **高** | 第 5 节的四步路径校验（规范化 + 逐段前缀匹配 + 不等于 root + `validate_slug`）；单测覆盖越界用例 |
| 更新链接时误改链接本身 | **高** | 不变量：先 `canonicalize` 再操作；验收标准 3 用 `lstat` 仍是 symlink + 目标路径未变作硬判据 |
| metadata 写回丢失 CLI 字段（现存缺陷） | 中 | 改为合并式写入；补回归测试 |
| 链接环 / 权限不足导致扫描崩溃 | 中 | `canonicalize` 错误归为 `ForeignSymlink`；`read_dir` 失败只跳过该 root 并记 warning；单项 `unreadable` 降级 |
| homepage 被写成非 http scheme 导致 XSS / 命令注入 | 中 | Rust 侧白名单校验 + 控制字符拒绝；前端不直接 `window.open` |
| 数据模型变更导致本地技能页回归 | 中 | `local-skills.test.tsx` 全量适配；改动仅限两个消费方 |
| Windows 上 junction / `.lnk` 语义差异 | 低 | 本期只要求正确识别并降级为「不可管理的链接」，不提供删除入口；Unix 专属链接单测用 `#[cfg(unix)]` 跳过 |
| 相对软链接（`../../.agents/skills/x`）解析错误 | 低 | 一律用 `canonicalize` 解析（自动处理相对/绝对）；`target` 仅原样保留用于展示 |

## 事务与数据

- **事务边界**：无数据库事务。本地文件系统操作按「先备份/先写真实目录、后删除」的顺序编排：
  - 更新：`extract → tmp`（失败则清理 tmp，原目录未动）→ `remove 旧 → rename 新`（rename 失败则保留 tmp 并报错，便于人工恢复）→ `write_metadata`。
  - 卸载非受管：`rename` 到备份名（原子，失败则原目录未动）。
  - 卸载链接：`remove_file`（原子）。
- **数据迁移**：无。旧版 `metadata.json`（无 `homepage`）正常读取。
- **回滚方案**：代码回滚即恢复旧行为；本地磁盘上唯一新增产物是备份目录（`<name>.skillhub-backup-<ts>`）与可能的 `HOME` 之外的 `.skillhub-tmp-*`（失败时保留并给出路径）。无不可逆操作。

## 测试策略

### Rust 单元测试（`web/src-tauri/src/installer/`，`cargo test`）

用 `std::env::temp_dir()` 建临时树（沿用现有 `installer_tests.rs` 风格），Unix 专属用例以 `#[cfg(unix)]` + `std::os::unix::fs::symlink` 标注，Windows 跳过。

1. **扫描分类矩阵**：受管目录 / 受管链接 / 非受管目录 / 非受管链接 / 悬空链接 / 指向文件的链接 / 隐藏项（`.DS_Store`）/ 普通文件（`debug-issue.md`）→ 各自归类正确，且隐藏项与普通文件不出现。
2. **跨 agent 聚合**：真目录 + 指向它的链接 → **一条** `LocalSkill`、两个 `locations`；两个独立真目录 → 两条记录。
3. **卸载链接（核心回归）**：记录真实目录内某文件的 mtime 与内容 → 卸载链接 → 链接消失、**真实目录及文件完全未变**、`real_path_kept` 正确返回。
4. **更新链接（核心回归）**：`lstat` 仍为 symlink、`read_link` 目标未变、真实目录内容为新版本、`metadata.json` 版本已更新。
5. **非受管目录卸载**：备份重命名成功、备份目录内原内容可读、原路径不存在。
6. **悬空链接清理**：只删链接，不报错。
7. **metadata 合并写回**：预置含 `fingerprint`/`files`/`versionId` 的 metadata → 更新后这些字段仍在；新增 `homepage` 后 CLI 校验器判定 valid。
8. **frontmatter 提取**：有 `homepage` / 无 / 带单双引号 / 带行尾注释 / 无合法 frontmatter / 超大文件（只读前 8KB）/ 非 UTF-8 → 各自结果。
9. **homepage 优先级**：三级各自命中与回退顺序。
10. **URL 校验**：`http`/`https` 通过；`javascript:`/`data:`/`file:`/含控制字符/超长 → 拒绝。
11. **路径校验（安全）**：`dir` 指向 agent root 之外、等于 root 本身、含 `..`、非法 slug → 全部拒绝。

### 前端测试（Vitest + jsdom）

- `local-skills.test.tsx` 适配并扩充：
  - 三类 `origin`（Managed / Unmanaged / BrokenSymlink）渲染与徽章。
  - 多位置标签渲染；多位置时卸载弹窗需先选位置。
  - homepage 按钮：有值渲染 + 点击调用 `invoke('open_external_url')`；无值不渲染。
  - 非受管行**无**「更新」按钮。
  - 各类 `kind` 的卸载确认文案不同。
- `install-for-agent-button.test.tsx`：软链接已安装时的文案提示。

### 手工验证清单（macOS，用现成样本）

`~/.claude/skills` 下三种链接形式逐一验证：绝对链接（`agent-browser-2`）、相对链接（`agently-mail`）、指向工作区路径的链接（`brand-voice`）。

### 验收命令

```bash
cd web/src-tauri && cargo test && cargo clippy -- -D warnings && cargo fmt --check
cd web && pnpm lint && pnpm typecheck && pnpm vitest run
```

## 参考文档

- `openspec/changes/client-skill-manage-20260911-01/`（前置变更：本地安装能力与本地技能页初版）
- `docs/architecture/implicit-contracts.md` 第 10 节「客户端安装（桌面/Tauri）约定」
- `docs/standards/frontend.md`、`docs/standards/testing.md`
- `cli/src/services/installed-skill-metadata.ts`（共享 metadata 契约）
- Rust std 源码 `library/std/src/sys/fs/unix.rs:2568-2578`（`remove_dir_all` 对顶层链接的隐式行为，证明不应依赖）
