//! Attach an already-installed skill to another agent.
//!
//! Distinct from [`crate::installer::install`]: nothing is downloaded and the
//! registry is never consulted, so this works for skills that have no registry
//! origin at all (hand-copied, or installed by another tool). The content already
//! exists on disk; the only question is how the other agent gets at it.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::installer::agents::{find_agent, profile_root};
use crate::installer::error::{io_error, not_found, InstallError};
use crate::installer::frontmatter::SKILL_FILE;
use crate::installer::install::{is_skill_package, InstallMode};
use crate::installer::link::create_link;

/// Outcome of attaching one skill to one agent.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachResult {
    pub ok: bool,
    pub agent: String,
    /// The agent-side entry that now exposes the skill.
    pub dir: String,
    /// The real directory the content actually lives in.
    pub real_dir: String,
    /// Set when nothing was attached, or to explain what happened.
    pub warnings: Vec<String>,
}

/// Resolve and validate the directory a skill's real content lives in.
///
/// Deliberately **not** `ensure_under_agent_root`. A shared install keeps its real
/// directory at `~/.skillhub/skills/<slug>`, which sits outside every agent skills
/// root — an agent-root boundary here would reject exactly the skills that sharing
/// produces, which is the main thing this command exists to spread around.
///
/// The boundary that does apply is the content check: the directory must
/// canonicalize and must contain a `SKILL.md`, using the same predicate
/// (`is_skill_package`) that gates writing through a link. That is what stops this
/// from being pointed at an arbitrary directory.
pub fn resolve_attach_source(source_dir: &Path) -> Result<PathBuf, InstallError> {
    let source = fs::canonicalize(source_dir).map_err(|err| io_error("解析技能目录", &err))?;

    if !source.is_dir() {
        return Err(InstallError::new(
            "not_a_directory",
            format!("源路径不是目录: {}", source_dir.display()),
        ));
    }
    if !is_skill_package(&source) {
        return Err(InstallError::new(
            "not_a_skill",
            format!(
                "源目录不是技能目录（缺少 {SKILL_FILE}）: {}",
                source.display()
            ),
        ));
    }

    Ok(source)
}

/// The agent-side entry name for a skill, taken from its source directory.
///
/// The directory name — not the metadata slug — because that is the name the user
/// sees on the local-skills page, and a skill they renamed should not reappear
/// under its published name in the other agent.
///
/// Not run through `validate_slug`: that constrains what may be *created* as a new
/// slug, whereas this name already exists on disk. A directory legitimately named
/// `My Skill` must still be attachable. It cannot traverse, being a single path
/// component of an already-canonicalized path.
pub fn attach_entry_name(source: &Path) -> Result<String, InstallError> {
    source
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .map(|name| name.to_string())
        .ok_or_else(|| not_found("源目录名无法解析"))
}

/// Make an existing skill available to another agent.
///
/// `mode` mirrors the install preference: `Shared` links the agent entry to the
/// real directory, `Copy` duplicates the contents into the agent's own directory.
/// Neither overwrites an occupied entry — see [`create_link`] and
/// [`copy_dir_recursive`] — because replacing something is a destructive act the
/// user should pick explicitly via uninstall.
pub fn attach_skill_to_agent(
    source_dir: &Path,
    agent_id: &str,
    mode: InstallMode,
) -> Result<AttachResult, InstallError> {
    let profile = find_agent(agent_id)
        .ok_or_else(|| not_found(&format!("不支持的 agent: {agent_id}")))?;

    attach_under(source_dir, &profile_root(profile), agent_id, mode)
}

/// [`attach_skill_to_agent`] against an explicit agent skills root.
///
/// Split out for the same reason as `uninstall_location_under` and `scan_roots`:
/// the real version resolves roots from the static profiles, i.e. the user's home
/// directory, and a test that runs against it would create and delete entries in
/// the developer's own `~/.claude/skills`. Tests point this at a temp tree.
pub fn attach_under(
    source_dir: &Path,
    agent_root: &Path,
    agent_id: &str,
    mode: InstallMode,
) -> Result<AttachResult, InstallError> {
    let source = resolve_attach_source(source_dir)?;
    let slug = attach_entry_name(&source)?;

    // Safe by construction: an agent skills root joined with a single path
    // component taken from an already-canonicalized path. No user-supplied
    // separator or `..` can appear here, so no further boundary check is needed.
    let target = agent_root.join(&slug);

    let mut warnings: Vec<String> = Vec::new();

    match mode {
        InstallMode::Shared => {
            let link = create_link(&target, &source)?;
            if link.created {
                warnings.push(format!("已链接到共享目录：{}", source.display()));
            } else if link.already_linked {
                warnings.push(format!("{} 已链接到同一共享目录，未做改动", target.display()));
            }
            if let Some(warning) = link.warning {
                warnings.push(warning);
            }
        }
        InstallMode::Copy => {
            if fs::symlink_metadata(&target).is_ok() {
                warnings.push(format!(
                    "{} 已被占用，未复制；如需替换请先卸载该条目",
                    target.display()
                ));
            } else {
                copy_dir_recursive(&source, &target)?;
                warnings.push(format!("已复制到：{}", target.display()));
            }
        }
    }

    Ok(AttachResult {
        ok: true,
        agent: agent_id.to_string(),
        dir: target.to_string_lossy().into_owned(),
        real_dir: source.to_string_lossy().into_owned(),
        warnings,
    })
}

/// Recursively copy a directory tree.
///
/// std has no recursive copy. Symlinks *inside* the source are recreated as links
/// rather than followed, so a copy cannot be used to pull in content from outside
/// the skill — the same reasoning as the zip-slip guard on extraction.
pub fn copy_dir_recursive(source: &Path, target: &Path) -> Result<(), InstallError> {
    fs::create_dir_all(target).map_err(|err| io_error("创建目标目录", &err))?;

    let entries = fs::read_dir(source).map_err(|err| io_error("读取源目录", &err))?;

    for entry in entries {
        let entry = entry.map_err(|err| io_error("读取源目录项", &err))?;
        let from = entry.path();
        let to = target.join(entry.file_name());
        let file_type = entry
            .file_type()
            .map_err(|err| io_error("读取条目类型", &err))?;

        if file_type.is_symlink() {
            copy_symlink(&from, &to)?;
        } else if file_type.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else {
            fs::copy(&from, &to).map_err(|err| io_error("复制文件", &err))?;
        }
    }

    Ok(())
}

/// Recreate a symlink at `dst` pointing where `src` points.
#[cfg(unix)]
fn copy_symlink(src: &Path, dst: &Path) -> Result<(), InstallError> {
    let target = fs::read_link(src).map_err(|err| io_error("读取链接目标", &err))?;
    std::os::unix::fs::symlink(target, dst).map_err(|err| io_error("复制符号链接", &err))
}

/// Recreate a symlink at `dst` pointing where `src` points.
///
/// Windows needs to be told whether the link is to a file or a directory, so the
/// original is inspected; a dangling link is assumed to be a file link.
#[cfg(windows)]
fn copy_symlink(src: &Path, dst: &Path) -> Result<(), InstallError> {
    let target = fs::read_link(src).map_err(|err| io_error("读取链接目标", &err))?;
    let is_dir = fs::metadata(src).map(|meta| meta.is_dir()).unwrap_or(false);

    let result = if is_dir {
        std::os::windows::fs::symlink_dir(target, dst)
    } else {
        std::os::windows::fs::symlink_file(target, dst)
    };
    result.map_err(|err| io_error("复制符号链接", &err))
}
