//! The user-configurable skill repository root.
//!
//! The repository root (`~/.skillhub/skills` by default) is not a hard constant:
//! a shared install keeps the real files there and each agent holds a symlink, so
//! a user who wants the library on another disk or directory must be able to
//! relocate it. This module holds the override and the relocation logic.
//!
//! The override is stored in the desktop app's **own** config directory, never
//! inside `~/.skillhub`. That directory has three unrelated meanings (the shared
//! `skills/` root, the CLI workspace `namespace-sync.json`, and a skill's own
//! `.skillhub/metadata.json`), and the desktop client is only allowed to write
//! inside `skills/` — so writing the override there would collide with the CLI
//! workspace layer. `dirs::config_dir()` is a pure function (no Tauri handle), so
//! [`repo_root`] can read it from anywhere and it is unit-testable.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::installer::error::{io_error, InstallError};
use crate::installer::link::{create_link, find_links_to_target_under};
use crate::installer::metadata::is_skillhub_sibling;

/// The config file that records the storage-path override.
fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("skillhub")
        .join("desktop-config.json")
}

/// Read the config at `path` as a JSON object, or an empty one when unset or
/// unreadable.
///
/// Read as a `Map` rather than a typed struct so the write path can be
/// **merge-preserving**: an unknown field the CLI or a future version added is
/// carried through instead of being silently dropped on the next write.
fn read_config_map_at(path: &Path) -> serde_json::Map<String, serde_json::Value> {
    match fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
        Err(_) => serde_json::Map::new(),
    }
}

fn write_config_map_at(
    path: &Path,
    map: &serde_json::Map<String, serde_json::Value>,
) -> Result<(), InstallError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| io_error("创建配置目录", &err))?;
    }
    let json = serde_json::to_string_pretty(map)
        .map_err(|err| InstallError::new("config_error", format!("序列化配置失败: {err}")))?;
    fs::write(path, json).map_err(|err| io_error("写入配置", &err))
}

/// The configured repository root, or `None` when unset, unreadable or invalid.
pub fn configured_repo_root() -> Option<PathBuf> {
    configured_repo_root_at(&config_path())
}

/// [`configured_repo_root`] against an explicit config path, for tests.
pub fn configured_repo_root_at(path: &Path) -> Option<PathBuf> {
    read_config_map_at(path)
        .get("skillStoragePath")
        .and_then(|value| value.as_str())
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
}

/// Persist a new repository root.
pub fn write_skill_storage_path(path: &Path) -> Result<(), InstallError> {
    write_skill_storage_path_at(&config_path(), path)
}

/// [`write_skill_storage_path`] against an explicit config path, for tests.
pub fn write_skill_storage_path_at(config: &Path, path: &Path) -> Result<(), InstallError> {
    let mut map = read_config_map_at(config);
    map.insert(
        "skillStoragePath".to_string(),
        serde_json::Value::String(path.to_string_lossy().into_owned()),
    );
    write_config_map_at(config, &map)
}

/// Clear the override so [`crate::installer::agents::repo_root`] falls back to
/// the default `~/.skillhub/skills`.
pub fn clear_skill_storage_path() -> Result<(), InstallError> {
    clear_skill_storage_path_at(&config_path())
}

/// [`clear_skill_storage_path`] against an explicit config path, for tests.
pub fn clear_skill_storage_path_at(config: &Path) -> Result<(), InstallError> {
    let mut map = read_config_map_at(config);
    map.remove("skillStoragePath");
    write_config_map_at(config, &map)
}

/// What [`migrate_repo_root`] did, so the UI can report it truthfully.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageMigration {
    /// The new repository root (absolute).
    pub new_path: String,
    /// The skill directory names that were moved, one per repository entry.
    pub moved_slugs: Vec<String>,
    /// The agent symlinks that were re-pointed to the new root.
    pub updated_links: Vec<String>,
    /// Read failures and links that could not be re-pointed.
    pub warnings: Vec<String>,
}

/// A migration result for a repository root that cannot be relocated because it
/// is no longer on disk (lost disk, user-deleted directory), or for a
/// no-op reset. There is nothing to move or re-point; the caller may still want
/// to move the config override.
pub fn empty_migration(new_path: &Path) -> StorageMigration {
    StorageMigration {
        new_path: new_path.to_string_lossy().into_owned(),
        moved_slugs: Vec::new(),
        updated_links: Vec::new(),
        warnings: Vec::new(),
    }
}

/// True for a directory or a symlink, i.e. something that could be a skill entry.
fn is_dir_or_symlink(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|meta| {
            let file_type = meta.file_type();
            file_type.is_dir() || file_type.is_symlink()
        })
        .unwrap_or(false)
}

/// Resolve `path` enough to compare it against a canonicalized root.
///
/// `path` may not exist yet (the user-picked destination, before any write), so
/// the final component cannot be canonicalized. Its parent is, then the name is
/// re-appended — the same trick [`ensure_under_roots`] uses. Without this, a
/// symlinked temp dir (`/var/folders` → `/private/var/folders` on macOS) would
/// compare a raw path against a resolved root and silently miss the overlap.
fn resolved(path: &Path) -> PathBuf {
    if let Ok(real) = fs::canonicalize(path) {
        return real;
    }
    if let (Some(parent), Some(name)) = (path.parent(), path.file_name()) {
        if let Ok(real_parent) = fs::canonicalize(parent) {
            return real_parent.join(name);
        }
    }
    path.to_path_buf()
}

/// Whether `path` is `root` itself or inside it.
///
/// `root` must exist to be meaningful; if the agent is not installed its root is
/// not on disk and cannot contain the destination.
fn within_root(path: &Path, root: &Path) -> bool {
    let Ok(root) = fs::canonicalize(root) else {
        return false; // Agent not installed; cannot contain the destination.
    };
    resolved(path).starts_with(root)
}

/// Validate a migration destination without requiring the source to exist.
///
/// Used by the recovery path, where the configured source is gone (lost disk,
/// user-deleted directory) but the user still wants to point the repository at a
/// new location. The checks below do not depend on `from` being on disk, so they
/// can run even when nothing is left to move. A `from` that does not exist is
/// never itself an error here — [`migrate_repo_root`] rejects it first.
pub fn validate_migration_destination(
    from: &Path,
    to: &Path,
    agent_roots: &[(String, PathBuf)],
) -> Result<(), InstallError> {
    if !to.is_absolute() {
        return Err(InstallError::new("invalid_path", "目标路径必须为绝对路径"));
    }
    if to == from {
        return Err(InstallError::new(
            "same_path",
            "新路径与当前技能仓库路径相同",
        ));
    }
    for (_, root) in agent_roots {
        if within_root(to, root) {
            return Err(InstallError::new(
                "inside_agent_root",
                format!("目标路径不能位于 agent 技能目录内: {}", root.display()),
            ));
        }
    }
    if to.starts_with(from) {
        return Err(InstallError::new(
            "inside_source",
            "目标路径不能在当前技能仓库目录内",
        ));
    }
    if to.exists() {
        let empty = fs::read_dir(to)
            .map(|mut entries| entries.next().is_none())
            .unwrap_or(false);
        if !empty {
            return Err(InstallError::new(
                "target_not_empty",
                format!("目标目录非空，无法迁移: {}", to.display()),
            ));
        }
    }
    Ok(())
}

/// Relocate the repository `from` to `to`, re-pointing every agent symlink that
/// waited on the old location.
///
/// Operates on the **whole** repository root as one `rename`, so it is only
/// supported when `to` is on the same filesystem as `from`. The caller supplies
/// explicit paths and agent roots so a test can point this at a temp tree.
///
/// Semantics, in order:
/// 1. **Pre-flight**: reject a non-existent source. The destination checks
///    (relative, same path, inside an agent skills root, nested in `from`,
///    non-empty) live in [`validate_migration_destination`] and abort with
///    nothing written.
/// 2. **Discover** repo skill entries and, for each, the agent symlinks that
///    resolve to it. This must happen **before** the rename: afterwards the old
///    real directories no longer exist and cannot be canonicalized.
/// 3. **Rename** the whole root; a cross-device move surfaces as a clear error.
/// 4. **Re-point** each discovered link to the new location. A link that cannot
///    be rewritten is reported as a warning rather than failing the whole move.
pub fn migrate_repo_root(
    from: &Path,
    to: &Path,
    agent_roots: &[(String, PathBuf)],
) -> Result<StorageMigration, InstallError> {
    if !from.is_dir() {
        return Err(InstallError::new(
            "not_found",
            format!("当前技能仓库不存在: {}", from.display()),
        ));
    }
    validate_migration_destination(from, to, agent_roots)?;

    // Enumerate the repo's skill entries (hidden and backup siblings are not
    // skills) so both the move result and the per-entry link discovery have a
    // stable set to work from.
    let mut slugs: Vec<String> = Vec::new();
    let read_dir = fs::read_dir(from).map_err(|err| io_error("读取技能仓库", &err))?;
    for entry in read_dir.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if name.starts_with('.') || is_skillhub_sibling(name) {
            continue;
        }
        if !is_dir_or_symlink(&path) {
            continue;
        }
        slugs.push(name.to_string());
    }

    // Discover links before the rename; afterwards the old targets are gone and
    // cannot be canonicalized back to a match.
    let mut warnings: Vec<String> = Vec::new();
    let mut links_by_slug: Vec<(String, Vec<PathBuf>)> = Vec::new();
    for slug in &slugs {
        let (found, discovery_warnings) = find_links_to_target_under(&from.join(slug), agent_roots);
        warnings.extend(discovery_warnings);
        links_by_slug.push((
            slug.clone(),
            found.into_iter().map(|(_, link_path)| link_path).collect(),
        ));
    }

    // The rename is the only step that can fail the whole operation. An existing
    // but empty `to` is removed first so `rename` lands cleanly on it.
    let rename = if to.exists() {
        fs::remove_dir(to).and_then(|_| fs::rename(from, to))
    } else {
        if let Some(parent) = to.parent() {
            fs::create_dir_all(parent).map_err(|err| io_error("创建目标目录", &err))?;
        }
        fs::rename(from, to)
    };
    if let Err(err) = rename {
        if err.kind() == ErrorKind::CrossesDevices {
            return Err(InstallError::new(
                "cross_device",
                "目标磁盘与当前技能仓库不同，仅支持同盘迁移",
            ));
        }
        return Err(io_error("迁移技能仓库", &err));
    }

    // Re-point each discovered link. `create_link` refuses to displace an entry,
    // so the old link is removed first; it verifies the new source is a real
    // directory, which it is — the rename moved it.
    //
    // A repo entry that is itself a symlink (not a real directory) is the one
    // exception: it points at a target **outside** the repository, the rename
    // left that target where it was, and the agent links to it still resolve.
    // Deleting and re-creating them against `to/<slug>` would fail (create_link
    // refuses a symlink source) and strand the links the user already had, so
    // for such an entry we leave the existing agent links alone.
    let mut updated_links: Vec<String> = Vec::new();
    for (slug, links) in &links_by_slug {
        let new_dir = to.join(slug);
        let source_is_symlink = fs::symlink_metadata(&new_dir)
            .map(|meta| meta.file_type().is_symlink())
            .unwrap_or(false);
        for link_path in links {
            if source_is_symlink {
                // The link already resolves to the real target, which did not
                // move. Nothing to rewrite; keep it as-is.
                continue;
            }
            if let Err(err) = fs::remove_file(link_path) {
                warnings.push(format!("移除旧链接失败: {}: {err}", link_path.display()));
                continue;
            }
            match create_link(link_path, &new_dir) {
                Ok(_) => updated_links.push(link_path.to_string_lossy().into_owned()),
                Err(err) => warnings.push(format!(
                    "更新链接失败: {}: {}",
                    link_path.display(),
                    err.message
                )),
            }
        }
    }

    Ok(StorageMigration {
        new_path: to.to_string_lossy().into_owned(),
        moved_slugs: slugs,
        updated_links,
        warnings,
    })
}
