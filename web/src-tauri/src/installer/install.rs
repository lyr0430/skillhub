use std::fs;
use std::io::{Cursor, Read, Seek};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::installer::agents::{display_path, find_agent, skill_dir, validate_slug};
use crate::installer::error::{io_error, network_error, not_found, zip_error, InstallError};
use crate::installer::homepage::validate_external_url;
use crate::installer::link::{ensure_under_agent_root, resolve_real_dir};
use crate::installer::metadata::{
    backup_dir, has_metadata, write_metadata, InstalledMetadata, SIBLING_TAGS,
};

/// Result of a single install, returned to the web view.
#[derive(Debug, serde::Serialize)]
pub struct InstallResult {
    pub ok: bool,
    /// Directory the skill files were written to. For a symlinked skill this is
    /// the link's target, not the link — the link itself is left alone.
    pub dir: String,
    pub agent: String,
    pub warnings: Vec<String>,
}

/// Targets accepted from the web view for `install_skill`.
///
/// Serialized as camelCase to match the TypeScript caller. Without this the
/// field below would be `preserve_existing` on the wire, the caller's
/// `preserveExisting` would be silently dropped by serde, and `#[serde(default)]`
/// would supply `false` — which means "delete the existing directory" rather
/// than "back it up", exactly inverting the user's choice.
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallInput {
    pub namespace: String,
    pub slug: String,
    pub version: String,
    pub agent: String,
    /// Optional override for the entry to install into. Still validated: the
    /// desktop client only ever installs into an entry directly inside a known
    /// agent skills root, so anything else is refused.
    pub dir: Option<String>,
    /// When a same-named non-skillhub directory already exists, `true` backs it
    /// up before installing, `false` overwrites it in place. Defaults to false.
    #[serde(default)]
    pub preserve_existing: bool,
}

/// What sits at the install path, and therefore what has to happen before a real
/// directory can take its place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Predecessor {
    /// Nothing there yet.
    Absent,
    /// A real directory — ours or not; [`swap_into_place`] decides which.
    Directory,
    /// A symlink resolving to nothing usable.
    StaleLink,
    /// A plain file. Never silently overwritten: a file at a skill path is not
    /// something this code can classify, so it gets the same treatment as any
    /// other unmanaged entry.
    File,
}

/// Build the absolute download URL for a skill version zip.
///
/// Mirrors the CLI's `/api/cli/v1/skills/{namespace}/{slug}/versions/{version}/download`.
pub fn build_download_url(registry: &str, namespace: &str, slug: &str, version: &str) -> String {
    format!(
        "{registry}/api/cli/v1/skills/{namespace}/{slug}/versions/{version}/download",
        registry = registry.trim_end_matches('/'),
    )
}

/// Extract a zip stream into `dest`. Entries are sanitized so no entry can
/// escape `dest` (zip-slip protection).
pub fn extract_zip<R: Read + Seek>(reader: R, dest: &Path) -> Result<(), InstallError> {
    let mut archive = zip::ZipArchive::new(reader).map_err(zip_error)?;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(zip_error)?;

        // Resolve the entry path against dest and confirm it stays inside dest.
        let relative = entry.enclosed_name().ok_or_else(|| {
            zip_error(zip::result::ZipError::InvalidArchive(
                "skill archive contains an unsafe path",
            ))
        })?;

        let out_path = dest.join(relative);

        if entry.is_dir() {
            fs::create_dir_all(&out_path).map_err(|e| io_error("创建目录", &e))?;
        } else {
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent).map_err(|e| io_error("创建目录", &e))?;
            }
            let mut buf = Vec::new();
            entry
                .read_to_end(&mut buf)
                .map_err(|e| zip_error(zip::result::ZipError::Io(e)))?;
            fs::write(&out_path, &buf).map_err(|e| io_error("解压写入", &e))?;
        }
    }
    Ok(())
}

/// Reject a registry that is not a plain http(s) origin.
///
/// Interpolated straight into the download URL, and it arrives from the web
/// view — or, for an update, from the on-disk `.skillhub/metadata.json` of a
/// skill that may have been installed from anywhere. Neither is trusted.
fn validate_registry(registry: &str) -> Result<(), InstallError> {
    validate_external_url(registry).map_err(|_| {
        InstallError::new(
            "invalid_registry",
            format!("registry 地址无效（仅支持 http/https）: {registry}"),
        )
    })
}

/// Decide where an install writes, without touching anything.
///
/// Deliberately side-effect free. Clearing a stale link or file is the caller's
/// job *after* the download succeeds — doing it here would let a download that
/// fails, or a registry that rejects us, destroy an entry the user already had.
pub fn resolve_install_target(
    target_path: &Path,
) -> Result<(PathBuf, Predecessor, Vec<String>), InstallError> {
    let mut warnings: Vec<String> = Vec::new();

    let meta = match fs::symlink_metadata(target_path) {
        Ok(meta) => meta,
        // Nothing in the way (missing, or an unreadable parent — either way
        // there is nothing to preserve).
        Err(_) => return Ok((target_path.to_path_buf(), Predecessor::Absent, warnings)),
    };

    let file_type = meta.file_type();

    if file_type.is_symlink() {
        // `resolve_real_dir` is the crate's single resolution entry point: it
        // canonicalizes and refuses anything that is not a directory, which here
        // is exactly the line between "update the target" and "this link is
        // stale".
        return match resolve_real_dir(target_path) {
            // A link to a real directory: the bytes go to the target and the
            // link is left exactly as it was.
            Ok(real) => {
                // Following a link is how one shared skill serves several
                // agents, but it is also the only path where the write target is
                // chosen by the filesystem instead of by us. Everything below —
                // backup, rename-aside, delete — acts on `real`, so a link
                // pointing anywhere at all would hand that directory to
                // `swap_into_place`: `~/.claude/skills/home -> ~` would move the
                // user's home aside and delete it. Require the target to be a
                // skill package first, so only a directory that announces itself
                // as a skill is ever written through.
                if !is_skill_package(&real) {
                    return Err(InstallError::new(
                        "unsafe_link_target",
                        format!(
                            "软链接指向的不是技能目录（缺少 {}），已拒绝写入：{}",
                            crate::installer::frontmatter::SKILL_FILE,
                            real.display()
                        ),
                    ));
                }
                warnings.push(format!("已更新该软链接指向的真实目录：{}", real.display()));
                Ok((real, Predecessor::Directory, warnings))
            }
            // A link to a file, a loop, or a missing target: nothing to update.
            Err(_) => Ok((target_path.to_path_buf(), Predecessor::StaleLink, warnings)),
        };
    }

    if file_type.is_dir() {
        return Ok((target_path.to_path_buf(), Predecessor::Directory, warnings));
    }

    Ok((target_path.to_path_buf(), Predecessor::File, warnings))
}

/// Clear whatever is blocking `target_path`, now that the replacement content is
/// already extracted and safe.
fn clear_predecessor(
    target_path: &Path,
    predecessor: &Predecessor,
    warnings: &mut Vec<String>,
) -> Result<(), InstallError> {
    match predecessor {
        Predecessor::StaleLink => {
            fs::remove_file(target_path).map_err(|e| io_error("移除失效链接", &e))?;
            warnings.push("原位置是一个失效的软链接，已移除并安装为普通目录".to_string());
        }
        Predecessor::File => {
            // A plain file is not a symlink, so it must not be fed to the
            // "remove the link" branch, and it is not ours to delete either.
            let backup = backup_dir(target_path)?;
            warnings.push(format!(
                "该位置原本是一个文件，已备份到：{}",
                backup.display()
            ));
        }
        Predecessor::Absent | Predecessor::Directory => {}
    }
    Ok(())
}

/// Swap an already-extracted skill package into `real_dir`.
///
/// `real_dir` must be a *resolved* directory — never a symlink path. `tmp_dir`
/// is the extraction scratch directory (a sibling of `real_dir`, so the final
/// `rename` stays on one filesystem); it is removed once the swap succeeds.
pub fn swap_into_place(
    real_dir: &Path,
    tmp_dir: &Path,
    predecessor: &Predecessor,
    preserve_existing: bool,
    warnings: &mut Vec<String>,
) -> Result<(), InstallError> {
    clear_predecessor(real_dir, predecessor, warnings)?;

    // The zip may contain a single top-level folder (the skill package); if so,
    // flatten it so `<slug>/SKILL.md` lands directly under the target dir.
    let effective = flatten_single_root(tmp_dir);

    if real_dir.exists() {
        if has_metadata(real_dir) {
            // A skillhub-managed install is replaced in place.
            replace_directory(real_dir, &effective)?;
            warnings.push("已覆盖该目录下已有的同名 skill".to_string());
        } else if preserve_existing {
            // Back up the non-skillhub (likely user-authored) directory, then
            // install, so its contents are not lost.
            let backup = backup_dir(real_dir)?;
            warnings.push(format!(
                "该目录下已有同名但非本仓库安装的 skill，原目录已备份到：{}",
                backup.display()
            ));
            fs::rename(&effective, real_dir).map_err(|e| io_error("移动安装目录", &e))?;
        } else {
            // User chose to overwrite the non-skillhub directory in place.
            replace_directory(real_dir, &effective)?;
            warnings.push("已覆盖该目录下同名但非本仓库安装的 skill".to_string());
        }
    } else {
        fs::rename(&effective, real_dir).map_err(|e| io_error("移动安装目录", &e))?;
    }

    // Clean up the temp extraction dir if it still exists (non-flattened case).
    if tmp_dir.exists() {
        let _ = fs::remove_dir_all(tmp_dir);
    }

    Ok(())
}

/// Replace `dir` with `source`, keeping the old content recoverable throughout.
///
/// `remove_dir_all` followed by `rename` leaves a window in which neither
/// exists: a crash, or a rename that fails on a read-only parent, loses the
/// previous version *and* strands the new one in the temp directory. Renaming
/// the old directory aside first means every intermediate state has the content
/// somewhere on disk, and the aside copy is only dropped once the new one is in
/// place — and if the rename fails, the original goes back.
fn replace_directory(dir: &Path, source: &Path) -> Result<(), InstallError> {
    let aside = unique_sibling(dir, "old");
    fs::rename(dir, &aside).map_err(|e| io_error("暂存原目录", &e))?;

    match fs::rename(source, dir) {
        Ok(()) => {
            let _ = fs::remove_dir_all(&aside);
            Ok(())
        }
        Err(err) => {
            // Put the original back rather than leaving the skill missing.
            let _ = fs::rename(&aside, dir);
            Err(io_error("移动安装目录", &err))
        }
    }
}

/// True when `dir` is a skill package: a directory holding a `SKILL.md`.
///
/// Strict about `<dir>/SKILL.md` rather than also accepting a nested package
/// root, because the only job this predicate has is to gate a destructive write
/// (see [`resolve_install_target`]). The permissive form would accept any
/// directory that happens to contain one anywhere below it, which is most of a
/// file system — including the ones this guard exists to keep out.
fn is_skill_package(dir: &Path) -> bool {
    dir.join(crate::installer::frontmatter::SKILL_FILE)
        .is_file()
}

/// Download a skill zip and install it into the target agent directory.
///
/// The download is requested anonymously (public skills). A per-call `registry`
/// must be passed from the web view; it is not baked into the binary so the
/// desktop app can target any SkillHub registry.
///
/// **Symlink contract:** when the target entry is a symlink, the bytes are
/// written to the link's *target* and the link is left exactly as it was. Doing
/// it the other way — removing the entry and renaming a directory into its place
/// — would silently convert the link into a real directory, stranding the
/// original target on an old version and detaching every other agent that shared
/// it. See [`resolve_install_target`].
pub fn install_skill(input: &InstallInput, registry: &str) -> Result<InstallResult, InstallError> {
    // Validate the slug so a malformed skill cannot escape the target dir.
    if !validate_slug(&input.slug) {
        return Err(InstallError::new("invalid_slug", "技能 slug 无效"));
    }
    validate_registry(registry)?;

    // Resolve the install directory. An explicit `dir` still gets the same
    // boundary check as uninstall, because it arrives from the web view: an
    // entry sitting directly inside one of the known agent skills roots.
    // Unvalidated, a crafted value would turn this into an arbitrary-path
    // delete, since the swap below removes a directory that already exists.
    let target_path = if let Some(dir) = &input.dir {
        ensure_under_agent_root(Path::new(dir))?
    } else {
        let profile = find_agent(&input.agent)
            .ok_or_else(|| not_found(&format!("不支持的 agent: {}", input.agent)))?;
        skill_dir(profile, &input.slug)
    };

    let (real_dir, predecessor, mut warnings) = resolve_install_target(&target_path)?;

    // Download the zip.
    let url = build_download_url(registry, &input.namespace, &input.slug, &input.version);
    let response = reqwest::blocking::get(&url).map_err(network_error)?;
    if !response.status().is_success() {
        return Err(InstallError::new(
            "download_failed",
            format!("下载失败 (HTTP {}) — {}", response.status(), url),
        ));
    }
    let bytes = response.bytes().map_err(network_error)?;

    // Extract to a temporary sibling of the real directory, so the swap stays on
    // one filesystem and the final rename is atomic.
    let parent = real_dir
        .parent()
        .ok_or_else(|| not_found("目标目录无法解析"))?;
    fs::create_dir_all(parent).map_err(|e| io_error("创建目录", &e))?;

    let tmp_dir = unique_sibling(&real_dir, "tmp");
    if tmp_dir.exists() {
        fs::remove_dir_all(&tmp_dir).map_err(|e| io_error("清理临时目录", &e))?;
    }
    extract_zip(Cursor::new(bytes), &tmp_dir)?;

    swap_into_place(
        &real_dir,
        &tmp_dir,
        &predecessor,
        input.preserve_existing,
        &mut warnings,
    )?;

    // Record install metadata so the desktop client (and CLI) can detect the
    // installed version and support uninstall/upgrade later.
    write_metadata(
        &real_dir,
        &InstalledMetadata::new(registry, &input.namespace, &input.slug, &input.version),
    )?;

    Ok(InstallResult {
        ok: true,
        dir: display_path(&real_dir),
        agent: input.agent.clone(),
        warnings,
    })
}

/// A unique sibling of `path`, named `<name>.skillhub-<tag>-<pid>-<nanos>-<n>`.
///
/// Derived from the directory name plus a nonce rather than
/// `with_extension(..)`, which stripped the extension off slugs containing a dot
/// (`foo.bar` → `foo.tmp-install`) so two different skills could collide.
///
/// `tag` must come from [`SIBLING_TAGS`]: the scan filters these names out by
/// shape, so a tag it does not know comes back as a phantom skill.
fn unique_sibling(path: &Path, tag: &str) -> PathBuf {
    static NONCE: AtomicU64 = AtomicU64::new(0);

    debug_assert!(
        SIBLING_TAGS.contains(&tag),
        "tag {tag:?} is not in SIBLING_TAGS, so the scan would surface it as a skill"
    );

    let nonce = NONCE.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.subsec_nanos())
        .unwrap_or(0);

    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("skill");

    path.with_file_name(format!(
        "{name}.skillhub-{tag}-{pid}-{nanos}-{nonce}",
        pid = std::process::id()
    ))
}

/// If the extracted temp dir contains exactly one top-level directory, treat it
/// as the skill package root and return it (so we install `<slug>/SKILL.md`).
/// Otherwise return the temp dir itself.
fn flatten_single_root(tmp_dir: &Path) -> PathBuf {
    let mut entries = match fs::read_dir(tmp_dir) {
        Ok(iter) => iter.filter_map(|e| e.ok()).collect::<Vec<_>>(),
        Err(_) => return tmp_dir.to_path_buf(),
    };

    if entries.len() == 1 {
        let only = entries.remove(0);
        if only.path().is_dir() {
            return only.path();
        }
    }

    tmp_dir.to_path_buf()
}
