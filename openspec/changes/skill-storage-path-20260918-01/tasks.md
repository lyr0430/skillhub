---
name: skill-storage-path-20260918-01
status: planned
---

# 执行任务：skill-storage-path-20260918-01

> 依据 `design.md`。仅实现以下任务，不自行扩需求。

## 任务列表

### Milestone 1：Rust 配置持久化 + 路径解析改造

- [x] Task 1.1：新增 `web/src-tauri/src/installer/storage_path.rs`
  - `StorageConfig`（`#[serde(rename_all = "camelCase")]`，字段 `skill_storage_path: Option<String>`），写回合并式（保留未知字段）。
  - `configured_repo_root() -> Option<PathBuf>`、`write_skill_storage_path(&Path)`、`clear_skill_storage_path()`。
  - 配置文件路径：`dirs::config_dir().join("skillhub").join("desktop-config.json")`。
- [x] Task 1.2：`web/src-tauri/src/installer/agents.rs`
  - 新增 `default_repo_root()`（原 `~/.skillhub/skills` 实现）。
  - `repo_root()` 改为 `configured_repo_root().unwrap_or_else(default_repo_root)`。
  - 更新 `repo_root()` doc 注释。
- [x] Task 1.3：注册 `storage_path` 模块（`lib.rs`/`installer/mod.rs`）。
- [x] Task 1.4：单测（`storage_path` 读写合并式、清空、文件缺失；`repo_root` 默认回退）。

### Milestone 2：Rust 迁移逻辑 + 命令

- [x] Task 2.1：`storage_path.rs` 内新增纯函数 `migrate_repo_root(from, to, agent_roots) -> Result<StorageMigration, InstallError>`
  - 预检：`from` 为目录、`to` 绝对且 != from、`to` 不在任何 agent root 内、`to` 不存在或为空（非空 → 冲突错误）。
  - 枚举 `from` 下 `is_skill_entry` 的 `<slug>`。
  - **rename 前**用 `find_links_to_target_under` 预采集每条 `slug → [link_path]`。
  - `fs::rename(from, to)`；跨设备 `EXDEV` 上抛「仅支持同盘迁移」；`to` 已存在且空则先 `remove_dir(to)`。
  - rename 后逐条 `remove_file` 旧链接 + `create_link(link_path, to/<slug>)`；失败并入 warning。
  - `StorageMigration { new_path, moved_slugs, updated_links, warnings }`（camelCase）。
- [x] Task 2.2：`migrate_repo_root` 单测（temp tree）：整库搬移+链接重写、预检冲突（`to` 非空/嵌 agent 根/`to==from`/非绝对）中止、no-op、rename 失败上抛。
- [x] Task 2.3：`commands.rs` 新增 `get_skill_storage_path`、`set_skill_storage_path`、`reset_skill_storage_path`（`spawn_blocking`；迁移成功才写/清配置）。`lib.rs` invoke_handler 注册。
- [x] Task 2.4：`StorageMigration` 序列化字段 camelCase；错误消息中文友好。

### Milestone 3：前端设置项

- [x] Task 3.1：`web/src/features/skill/tauri-installer.ts` 新增 `getSkillStoragePath()`、`setSkillStoragePath(path)`、`resetSkillStoragePath()`。
- [x] Task 3.2：`web/src/shared/components/settings-dialog.tsx`
  - 分区一「技能安装方式」：`note` 从 `bg-muted/50` 盒子改为分区标题下小号淡色说明文字。
  - 分区二「技能存储路径」（新增 `SettingsSection` + 分隔线）：只读 `Input` + `FolderOpen` 图标按钮；`AlertDialog` 二次确认 → `open({ directory: true })` → `setSkillStoragePath`；「恢复默认路径」按钮（≠ 默认时显示）；加载/错误/警告呈现。
- [x] Task 3.3：i18n 资源（`settings.storagePath.*`、调整 `settings.installMode.note` 语义）。
- [x] Task 3.4：前端单测（存储路径区渲染、确认弹窗触发、命令调用、恢复默认；mock `invokeTauri` 与 dialog）。

### Milestone 4：验证与收尾

- [x] Task 4.1：`cd web/src-tauri && cargo build` / `cargo test` 通过；`cd web && pnpm typecheck && pnpm lint && pnpm test`。
- [x] Task 4.2：手动 `pnpm tauri:dev` 验证真实迁移 + 软链接更新 + 恢复默认。

## 验收检查点

- [x] 编译通过（Rust + TS）
- [x] 单元测试通过（Rust + 前端）
- [x] 集成/手动验证通过（真实迁移 + 链接更新）
- [x] Review 通过

## 下一步

执行 `/harness-verify skill-storage-path-20260918-01`（若有 API 测试）
或 `/harness-review skill-storage-path-20260918-01`
