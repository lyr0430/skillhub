use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::installer::agents::{profile_root, AGENT_PROFILES};
use crate::installer::error::{io_error, InstallError};
use crate::installer::metadata::backup_dir;

/// How a skill entry exists on disk inside an agent's skills root.
///
/// The distinction matters because the same logical skill is uninstalled and
/// updated very differently depending on which variant it is: a link must never
/// be followed for writes, and an unmanaged directory must never be deleted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum LocationKind {
    /// A plain directory holding the skill files.
    Dir,
    /// A symlink resolving to an existing directory.
    Symlink,
    /// A symlink whose target no longer exists.
    BrokenSymlink,
    /// A symlink we refuse to treat as a skill: it points at a file, or
    /// `canonicalize` failed for a reason other than a missing target (a
    /// symlink loop, a permission error).
    ForeignSymlink,
}

impl LocationKind {
    /// True for every variant that is a symlink on disk, i.e. where removing
    /// the entry must not touch whatever it points at.
    pub fn is_link(self) -> bool {
        matches!(
            self,
            LocationKind::Symlink | LocationKind::BrokenSymlink | LocationKind::ForeignSymlink
        )
    }
}

/// Classify a skill entry without following links.
///
/// Uses `symlink_metadata` on purpose: `Path::is_dir()` follows links and would
/// report a symlink as a directory, which is exactly the confusion that lets a
/// write escape onto the link target.
pub fn classify(path: &Path) -> Result<LocationKind, InstallError> {
    let meta = fs::symlink_metadata(path).map_err(|err| match err.kind() {
        ErrorKind::NotFound => InstallError::new("not_installed", "该技能未在本机安装，无法操作"),
        _ => io_error("读取条目状态", &err),
    })?;

    if !meta.file_type().is_symlink() {
        return Ok(if meta.file_type().is_dir() {
            LocationKind::Dir
        } else {
            LocationKind::ForeignSymlink
        });
    }

    match fs::canonicalize(path) {
        Ok(target) if target.is_dir() => Ok(LocationKind::Symlink),
        Ok(_) => Ok(LocationKind::ForeignSymlink),
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(LocationKind::BrokenSymlink),
        Err(_) => Ok(LocationKind::ForeignSymlink),
    }
}

/// Resolve the real directory a skill location writes to.
///
/// This is the single entry point every mutating path must go through. It
/// dereferences the link so callers operate on the real directory; nobody may
/// write through the link path itself, or a plain `rename` would replace the
/// link with a real directory and silently detach it from its target.
pub fn resolve_real_dir(path: &Path) -> Result<PathBuf, InstallError> {
    let real = fs::canonicalize(path).map_err(|err| io_error("解析真实目录", &err))?;
    if !real.is_dir() {
        return Err(InstallError::new(
            "not_a_directory",
            format!("目标不是目录: {}", path.display()),
        ));
    }
    Ok(real)
}

/// What a successful location removal actually did, so the UI can tell the user
/// the truth about what was and was not deleted.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocationRemoval {
    pub removed_kind: LocationKind,
    /// Set when only a link was removed: the real directory is still on disk.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub real_path_kept: Option<String>,
    /// Set when an unmanaged directory was renamed aside instead of deleted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backup_dir: Option<String>,
}

/// Remove one skill location.
///
/// The rule this function exists to enforce: **a link is only ever unlinked**.
/// It is tempting to call `fs::remove_dir_all` everywhere and rely on std's
/// current Unix behaviour of unlinking a top-level symlink, but that behaviour
/// is undocumented, differs on Windows, and has followed links historically.
/// So the symlink case is branched on explicitly and never reaches
/// `remove_dir_all`.
pub fn remove_location(path: &Path, managed: bool) -> Result<LocationRemoval, InstallError> {
    let kind = classify(path)?;

    if kind.is_link() {
        let real_path_kept = if kind == LocationKind::Symlink {
            fs::canonicalize(path)
                .ok()
                .map(|real| real.to_string_lossy().into_owned())
        } else {
            None
        };
        fs::remove_file(path).map_err(|err| io_error("移除符号链接", &err))?;
        return Ok(LocationRemoval {
            removed_kind: kind,
            real_path_kept,
            backup_dir: None,
        });
    }

    if managed {
        fs::remove_dir_all(path).map_err(|err| io_error("卸载失败", &err))?;
        Ok(LocationRemoval {
            removed_kind: kind,
            real_path_kept: None,
            backup_dir: None,
        })
    } else {
        // Not ours to delete: rename it aside so the user's content survives.
        let backup = backup_dir(path)?;
        Ok(LocationRemoval {
            removed_kind: kind,
            real_path_kept: None,
            backup_dir: Some(backup.to_string_lossy().into_owned()),
        })
    }
}

/// Normalize `path` and assert it is a removable entry sitting directly inside
/// one of the known agent skill roots.
///
/// This is a security boundary, not a convenience. The directory arrives from
/// the web view, so without this check a crafted value would turn uninstall into
/// an arbitrary-path delete.
pub fn ensure_under_agent_root(path: &Path) -> Result<PathBuf, InstallError> {
    let roots: Vec<PathBuf> = AGENT_PROFILES.iter().map(profile_root).collect();
    ensure_under_roots(path, &roots)
}

/// True when a name could be a skill entry sitting inside a root.
///
/// Looser than [`validate_slug`] on purpose. That one constrains what may be
/// *created* as a slug; here we are only deciding whether a name is safe to act
/// on. A directory the user named `My Skill` is a real entry and must stay
/// removable — and it cannot traverse anywhere, because this is a single path
/// component: separators are impossible by construction, and `.`, `..`, and
/// leading-dot names are rejected outright.
fn is_plain_entry_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= 255 && name != "." && name != ".." && !name.starts_with('.')
}

/// [`ensure_under_agent_root`] against an explicit set of roots.
///
/// Split out so the boundary itself can be tested against a temp tree rather
/// than the real home directory.
pub fn ensure_under_roots(path: &Path, roots: &[PathBuf]) -> Result<PathBuf, InstallError> {
    // `Path` erases a trailing separator: `file_name()` is "b" for "/a/b/",
    // "/a/b/.", and "/a/b" alike. The kernel does not erase it — for the first
    // two forms it dereferences a final symlink — so a request to detach a link
    // would silently retarget onto the directory that link points at, deleting
    // the real skill and leaving the link behind. The raw text is the only place
    // that difference survives, so it is checked here and not on the parsed path.
    let raw = path.to_string_lossy();
    if raw.ends_with('/') || raw.ends_with('\\') || raw.ends_with("/.") || raw.ends_with("\\.") {
        return Err(InstallError::new(
            "invalid_path",
            format!("路径不能以分隔符或 . 结尾: {raw}"),
        ));
    }

    let name = path
        .file_name()
        .filter(|name| !name.is_empty())
        .ok_or_else(|| InstallError::new("invalid_path", "路径缺少名称"))?;
    let name_text = name
        .to_str()
        .ok_or_else(|| InstallError::new("invalid_path", "路径名称不是有效 UTF-8"))?;
    if !is_plain_entry_name(name_text) {
        return Err(InstallError::new(
            "invalid_slug",
            format!("技能目录名无效: {name_text}"),
        ));
    }

    // Only the parent is canonicalized; the final component is kept exactly as
    // written so a link stays a link. Canonicalizing the whole path would
    // dereference it — the opposite of what every caller here needs — and would
    // also fail on a dangling link, which is precisely one of the things we must
    // still be able to clean up.
    let parent = path
        .parent()
        .ok_or_else(|| InstallError::new("invalid_path", "路径无法解析"))?;
    let parent = fs::canonicalize(parent).map_err(|err| io_error("解析父目录", &err))?;
    let normalized = parent.join(name);

    for root in roots {
        let Ok(root) = fs::canonicalize(root) else {
            continue; // Agent not present on this machine.
        };
        // Compare components rather than string prefixes: `/x/.claude/skills-evil`
        // must not pass as a child of `/x/.claude/skills`.
        if normalized.parent() == Some(root.as_path()) {
            return Ok(normalized);
        }
    }

    Err(InstallError::new(
        "outside_agent_root",
        format!("拒绝操作 agent 目录之外的路径: {}", path.display()),
    ))
}
