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
    /// Anything else that is not a directory: a symlink we refuse to treat as a
    /// skill (it points at a file, or `canonicalize` failed for a reason other
    /// than a missing target — a loop, a permission error), **or a plain file**.
    /// `is_link()` is therefore a statement about the bucket, not about the
    /// filesystem; anything destructive must re-check `symlink_metadata`.
    ForeignSymlink,
}

impl LocationKind {
    /// True for the variants that are *usually* a symlink on disk.
    ///
    /// Not a substitute for asking the filesystem: `ForeignSymlink` also holds
    /// plain files, so a destructive caller must re-check `symlink_metadata`
    /// before unlinking (see [`remove_location`]).
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

    // Ask the filesystem directly rather than trusting `kind.is_link()`. A plain
    // file is neither a directory nor a usable link, so `classify` files it under
    // `ForeignSymlink` too — and unlinking whatever lands in that bucket would
    // delete a file this crate never actually classified. This is the one
    // decision in the module that must not rest on a bucket label.
    let is_link = fs::symlink_metadata(path)
        .map(|meta| meta.file_type().is_symlink())
        .unwrap_or(false);

    if is_link {
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

    if managed && kind == LocationKind::Dir {
        fs::remove_dir_all(path).map_err(|err| io_error("卸载失败", &err))?;
        Ok(LocationRemoval {
            removed_kind: kind,
            real_path_kept: None,
            backup_dir: None,
        })
    } else {
        // Not ours to delete: rename it aside so the user's content survives.
        // Covers both an unmanaged directory and a plain file at a skill path.
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

/// What [`create_link`] did, so the caller can report it truthfully.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkCreation {
    /// The link was created by this call.
    pub created: bool,
    /// A link to the same real directory was already in place; nothing changed.
    pub already_linked: bool,
    /// Set when nothing was created because the path is occupied by something
    /// else. Carries the message to show the user.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
}

/// Point the agent-side entry `target` at the real directory `source`.
///
/// `source` must itself be a real directory, never a link: chaining a link to
/// another link would make the shared skill depend on the intermediate link's
/// lifetime, which is the fragility sharing exists to remove.
///
/// **Refuses to displace anything.** Only an absent path is linked. An existing
/// entry — a directory, a foreign link, or a dangling link — is reported through
/// `warning` and left untouched, because replacing it is a destructive act the
/// user should choose explicitly (the local-skills page already offers uninstall
/// for exactly that). The single exception is a link that already resolves to
/// `source`, which is a no-op so that re-installing is idempotent.
///
/// The link is written with `source` *as given*, not canonicalized, so the target
/// shown to the user and stored on disk stays the readable `~/.skillhub/skills/x`
/// rather than macOS's `/System/Volumes/Data/...` expansion.
pub fn create_link(target: &Path, source: &Path) -> Result<LinkCreation, InstallError> {
    // `symlink_metadata` on purpose: `is_dir()` follows links, and a link here
    // would violate the "source is a real directory" contract this relies on.
    let source_meta = fs::symlink_metadata(source)
        .map_err(|err| io_error("读取共享目录状态", &err))?;
    if !source_meta.file_type().is_dir() {
        return Err(InstallError::new(
            "not_a_directory",
            format!("链接目标必须是真实目录: {}", source.display()),
        ));
    }

    match fs::symlink_metadata(target) {
        Err(err) if err.kind() == ErrorKind::NotFound => {}
        Err(err) => return Err(io_error("读取条目状态", &err)),
        Ok(_) => {
            // Canonicalize *both* sides for the comparison: the link may have
            // been written with a different but equivalent spelling of the same
            // directory (`~` vs absolute, or macOS's `/System/Volumes/Data`
            // prefix), and a string compare would then call it a foreign link.
            let is_same_link = classify(target)
                .map(|kind| kind == LocationKind::Symlink)
                .unwrap_or(false)
                && match (fs::canonicalize(target), fs::canonicalize(source)) {
                    (Ok(existing), Ok(wanted)) => existing == wanted,
                    _ => false,
                };

            if is_same_link {
                return Ok(LinkCreation {
                    created: false,
                    already_linked: true,
                    warning: None,
                });
            }

            return Ok(LinkCreation {
                created: false,
                already_linked: false,
                warning: Some(format!(
                    "{} 已被占用，未创建链接；如需改为共享安装，请先卸载该条目",
                    target.display()
                )),
            });
        }
    }

    // The agent's skills root may not exist yet on a machine where the agent was
    // never used — the repo side got `create_dir_all`, this side has not.
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|err| io_error("创建 agent 目录", &err))?;
    }

    symlink_dir(target, source).map_err(|err| io_error("创建符号链接", &err))?;

    Ok(LinkCreation {
        created: true,
        already_linked: false,
        warning: None,
    })
}

/// Create a directory symlink at `target` pointing to `source`.
///
/// Split per platform because the two have no common std API: Windows must be
/// told it is a directory link, and creating one may require Developer Mode or
/// elevation — hence a real error rather than a panic.
#[cfg(unix)]
fn symlink_dir(target: &Path, source: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(source, target)
}

#[cfg(windows)]
fn symlink_dir(target: &Path, source: &Path) -> std::io::Result<()> {
    std::os::windows::fs::symlink_dir(source, target)
}
