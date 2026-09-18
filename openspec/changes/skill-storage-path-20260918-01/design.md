---
name: skill-storage-path-20260918-01
status: designed
---

# 技术方案：skill-storage-path-20260918-01

## 方案概述

在「Skill 配置」设置页新增「技能存储路径」配置项，与「技能安装方式」视觉分离；支持选择新目录（先二次确认 → 目录选择 → 整库移动 + 更新软链接）、恢复默认路径。

现状：技能仓库根路径 `repo_root()`（`web/src-tauri/src/installer/agents.rs:83`）返回常量 `~/.skillhub/skills`，用户无法更改；设置页「注意」区块用框式画法看起来像配置项。

核心思路：把仓库根路径从**常量**改为**可覆盖值**（存应用配置目录），新增一条迁移命令，按**整库整体移动 + 预检中止 + 仅同盘 rename** 的语义迁移，并重写所有指向旧仓库目录的软链接。

## 详细设计

### 架构变更

#### 1. 配置持久化（新增 `web/src-tauri/src/installer/storage_path.rs`）

- 存储文件：`dirs::config_dir().join("skillhub").join("desktop-config.json")`，JSON 结构：

  ```json
  { "skillStoragePath": "/absolute/path/to/repo" }
  ```

- **刻意不写入 `~/.skillhub`**。原因：`~/.skillhub` 有三层含义（共享仓库根 `skills/`、CLI 工作区 `namespace-sync.json`、技能内 `metadata.json`），且硬约束「桌面端只写 `skills/` 子目录、不碰 CLI 工作区」（`implicit-contracts.md` 第 91 行）。用 `dirs::config_dir()`（纯函数，不依赖 Tauri App 句柄，可单测）隔离在桌面端自己的地盘。
- 模块 API：
  - `configured_repo_root() -> Option<PathBuf>`：读配置覆盖值；文件不存在 / 解析失败 / 未设置 → `None`。
  - `write_skill_storage_path(path: &Path)`：写覆盖值。
  - `clear_skill_storage_path()`：清空覆盖值（恢复默认用）。
  - `StorageConfig`：`#[serde(rename_all = "camelCase")]`，字段 `skill_storage_path: Option<String>`。写回必须**合并式**（保留未知字段），对齐 `metadata.json` 契约习惯。

#### 2. `repo_root()` 改造（`agents.rs`）

- `repo_root()` 由常量改为：`configured_repo_root().unwrap_or_else(default_repo_root)`。
- 新增 `pub fn default_repo_root() -> PathBuf`：原常量实现 `home_dir().join(".skillhub").join("skills")`。
- 所有消费方（`install` 共享安装 `repo_skill_dir`、`attach` 的 `skill_source_roots`、`local_skills` 扫描/仓库卸载、`agents`）不变，因它们都经 `repo_root()`；改一处全链路生效。
- 测试不受影响：现有测试走 `_under` 变体（显式 `from`/`to`/roots），不读真实配置。

#### 3. 迁移命令（`storage_path.rs` 内核心 + `commands.rs` 暴露）

**核心纯函数**（可测试，显式入参）：

```rust
pub struct StorageMigration {
    pub new_path: String,          // 新仓库根（绝对路径）
    pub moved_slugs: Vec<String>,  // 被移动的技能目录名
    pub updated_links: Vec<String>,// 被重写指向的软链接路径
    pub warnings: Vec<String>,     // 读失败 / 链接重写失败等
}

pub fn migrate_repo_root(
    from: &Path,
    to: &Path,
    agent_roots: &[(String, PathBuf)],
) -> Result<StorageMigration, InstallError>
```

流程：
1. **预检**（任一失败即中止，不落盘）：
   - `from` 存在且为目录；`to` 绝对路径且 `to != from`。
   - `to` **不在任何 agent skills root 之内**（对每个 agent root 做 canonicalize 后 `to.starts_with(root)` 判定，避免仓库嵌进 agent 根造成扫描重复归并）。
   - `to` 不存在，或存在且为空；若存在且**非空** → 报冲突错误（对应「预检中止」决策 D4）。
2. **枚举旧仓库技能**：`from` 下 `is_skill_entry`（目录或符号链接、非 dot、非 `.skillhub-*` 兄弟）的 `<slug>`。
3. **预采集软链接**（rename 前，因 rename 后旧路径已不可解析）：对每个 `<slug>`，用 `find_links_to_target_under(from/<slug>, agent_roots)` 取 `(agent, link_path)`，收集 `slug → [link_path]`；`find_links` 自身的 warning 并入汇总。
4. **整库移动**：`fs::rename(from, to)`。跨设备（`EXDEV`）时上抛明确中文错误「目标磁盘不同，仅支持同盘迁移」；`to` 已存在且为空时先 `remove_dir(to)` 再 rename。
5. **重写软链接**：对每个记录的 `link_path`，先 `remove_file(link_path)`（只删链接），再 `create_link(link_path, to/<slug>)`（复用 `create_link` 的「源必须是真实目录」校验）；单个失败并入 warning，不中断。
6. 返回 `StorageMigration`。

**组合逻辑**（`commands.rs` 暴露，`spawn_blocking` 执行，仿 `install_skill_command`）：

- `get_skill_storage_path() -> CommandResult<String>`：返回 `repo_root()` 当前生效值。
- `set_skill_storage_path(path: String) -> CommandResult<StorageMigration>`：
  - `from = repo_root()`；`to = Path::new(&path)`；`agent_roots` 取 `AGENT_PROFILES` 映射。
  - `migrate_repo_root(&from, &to, &roots)` 成功后，`write_skill_storage_path(&to)`（rename 成功才写配置）。
- `reset_skill_storage_path() -> CommandResult<StorageMigration>`：
  - `from = repo_root()`；`to = default_repo_root()`。
  - 若 `from == to` → no-op 成功。
  - 迁移成功 → `clear_skill_storage_path()`（配置回默认）。

### 接口变更

| 命令 | 参数 | 返回 | 说明 |
|------|------|------|------|
| `get_skill_storage_path` | 无 | `CommandResult<String>` | 当前仓库根路径 |
| `set_skill_storage_path` | `path: String` | `CommandResult<StorageMigration>` | 迁移到新路径 |
| `reset_skill_storage_path` | 无 | `CommandResult<StorageMigration>` | 迁回默认 |

- `StorageMigration` 及内部结构体字段带 `#[serde(rename_all = "camelCase")]`。
- **命令参数 JS 侧 camelCase**（`implicit-contracts` 第 97 行）：`set_skill_storage_path(path)`。

### 前端变更

#### `web/src/features/skill/tauri-installer.ts`
- 新增 `getSkillStoragePath()`、`setSkillStoragePath(path)`、`resetSkillStoragePath()`，封装 `invokeTauri`。

#### `web/src/shared/components/settings-dialog.tsx`
- **重构 Skill 配置页分区**：
  - 分区一「技能安装方式」：保留空调面板；把 `note` 从 `bg-muted/50` 盒子里拿出来，改为该分区标题下的**小号淡色说明文字**（不再像配置项）。
  - 分区二「技能存储路径」（新增 `SettingsSection`，与分区一用分隔线/分组标题明确分开）：
    - 只读 `Input`（展示当前路径，`get_skill_storage_path` 加载）+ 右侧 `FolderOpen` 图标按钮。
    - 图标按钮 → 自定义 `AlertDialog` 二次确认（文案「修改技能存储路径会把当前路径的技能都移动到修改后的目录，是否要继续？」）。
    - 确认 → `open({ directory: true })`（`@tauri-apps/plugin-dialog`）→ 选中目录则 `set_skill_storage_path(chosen)`。
    - 「恢复默认路径」次要按钮：当前路径 ≠ 默认时显示；点击后 `AlertDialog` 确认 → `reset_skill_storage_path()`。
    - 结果用 `CommandResult` 的 `ok/error/warnings` 呈现（成功提示 + 警告列表；失败展示中文原因）。
  - 仅 Tauri 环境（整弹窗已 Tauri 限定）。
- 加载/提交状态：禁用按钮 + 提示，避免重复触发。

#### i18n（`web/src/i18n/`）
- `settings.storagePath.*`：`label`（技能存储路径）、`hint`、`confirmTitle`/`confirmBody`、`restoreDefault`、`loading`、`success`、`warnings`、`error`。
- 调整 `settings.installMode.*` 的 note 文案使其作为说明文字呈现。

## 影响范围

- 受影响模块：
  - Rust：`web/src-tauri/src/installer/agents.rs`（`repo_root`/`default_repo_root`）、新增 `installer/storage_path.rs`、`web/src-tauri/src/commands.rs`（3 条命令）、`web/src-tauri/src/lib.rs`（mod + invoke_handler 注册）。
  - 前端：`web/src/shared/components/settings-dialog.tsx`、`web/src/features/skill/tauri-installer.ts`、i18n 资源。
- 受保护路径变更：无（不触碰 `~/.skillhub/namespace-sync.json`、数据库、部署/配置）。
- 向后兼容性：
  - 未配置覆盖值时 `repo_root()` 仍返回 `~/.skillhub/skills`，行为与现状一致。
  - 现有 `_under` 测试路径、扫描、安装、卸载均不受影响。
  - CLI 完全不受影响（桌面端只动自己的配置目录）。

## 风险评估

| 风险 | 等级 | 缓解方案 |
|------|------|----------|
| 迁移时软链接重写失败，个别 agent 链接悬空 | 中 | 重写失败记入 warning；`local_skills` 会以 `BrokenSymlink` 显示，用户可 re-attach；核心仓库已迁走、配置已更新 |
| `repo_root()` 每次读文件，性能 | 低 | 调用频率低（每条命令/每次扫描一次），文件极小；无共享可变状态 |
| 目标目录非空被选 | 中 | 预检中止并给出明确中文原因，不落盘 |
| 跨盘迁移 rename 失败 | 中 | 预检/rename 阶段捕获 `EXDEV`，上抛「仅支持同盘迁移」 |
| 配置写回丢 CLI/未知字段 | 低 | 写回合并式（保留未知字段），对齐 `metadata.json` 契约 |

## 事务与数据

- 事务边界：整库迁移在单次 `spawn_blocking` 内完成；**rename 成功才写配置**，rename 失败则配置不变。
- 数据迁移：仓库根目录整体 rename，不逐技能复制；无数据库变更。
- 回滚方案：无自动回滚（rename 原子且不可逆）。若配置已写但个别链接重写失败，用户可在本地技能页重新 attach 对应技能。

## 测试策略

- Rust（`storage_path` + `agents`）：
  - 配置模块读写合并式单测（覆盖值、清空、文件缺失）。
  - `migrate_repo_root`（temp tree，显式 `from`/`to`/roots）：整库搬移 + 链接重写、预检冲突（`to` 非空/嵌 agent 根/`to==from`/非绝对）中止、`to==from` no-op、rename 失败上抛。
  - `repo_root()` 默认回退单测（借用 `_under` 变体隔离真实 home）。
- 前端：
  - `settings-dialog` 存储路径区渲染当前路径、点图标弹确认、确认后调命令、恢复默认；mock `invokeTauri` 与 dialog 插件。
  - `tauri-installer` 3 个新命令封装的断言。
- 手动：`pnpm tauri:dev` 验证真实迁移 + 软链接更新。
