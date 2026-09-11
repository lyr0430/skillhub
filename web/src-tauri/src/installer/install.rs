use std::fs;
use std::io::{Cursor, Read, Seek};
use std::path::{Path, PathBuf};

use crate::installer::agents::{display_path, find_agent, skill_dir, validate_slug};
use crate::installer::error::{io_error, network_error, not_found, zip_error, InstallError};

/// Result of a single install, returned to the web view.
#[derive(Debug, serde::Serialize)]
pub struct InstallResult {
    pub ok: bool,
    pub dir: String,
    pub agent: String,
    pub warnings: Vec<String>,
}

/// Targets accepted from the web view for `install_skill`.
#[derive(Debug, serde::Deserialize)]
pub struct InstallInput {
    pub namespace: String,
    pub slug: String,
    pub version: String,
    pub agent: String,
    /// Optional absolute override directory; when present it wins over the
    /// agent-derived directory (mirrors `--dir` in the CLI).
    pub dir: Option<String>,
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

/// Download a skill zip and install it into the target agent directory.
///
/// The download is requested anonymously (public skills). A per-call `registry`
/// must be passed from the web view; it is not baked into the binary so the
/// desktop app can target any SkillHub registry.
pub fn install_skill(input: &InstallInput, registry: &str) -> Result<InstallResult, InstallError> {
    let mut warnings: Vec<String> = Vec::new();

    // Validate the slug so a malformed skill cannot escape the target dir.
    if !validate_slug(&input.slug) {
        return Err(InstallError::new("invalid_slug", "技能 slug 无效"));
    }

    // Resolve the install directory: explicit override, else agent-derived.
    let target_dir = if let Some(dir) = &input.dir {
        Path::new(dir).to_path_buf()
    } else {
        let profile = find_agent(&input.agent)
            .ok_or_else(|| not_found(&format!("不支持的 agent: {}", input.agent)))?;
        skill_dir(profile, &input.slug)
    };

    // Download the zip.
    let url = build_download_url(registry, &input.namespace, &input.slug, &input.version);
    let response = reqwest::blocking::get(&url).map_err(network_error)?;
    if !response.status().is_success() {
        return Err(InstallError::new(
            "download_failed",
            format!("下载失败 (HTTP {})", response.status()),
        ));
    }
    let bytes = response.bytes().map_err(network_error)?;

    // Pre-clean any partial destination so a failed install does not leave a
    // half-written directory, then extract into a temp dir and move into place.
    let parent = target_dir
        .parent()
        .ok_or_else(|| not_found("目标目录无法解析"))?;
    fs::create_dir_all(parent).map_err(|e| io_error("创建目录", &e))?;

    // Extract to a temporary sibling, then swap in atomically.
    let tmp_dir = target_dir.with_extension("tmp-install");
    if tmp_dir.exists() {
        fs::remove_dir_all(&tmp_dir).map_err(|e| io_error("清理临时目录", &e))?;
    }
    extract_zip(Cursor::new(bytes), &tmp_dir)?;

    // The zip may contain a single top-level folder (the skill package); if so,
    // flatten it so `<slug>/SKILL.md` lands directly under the target dir.
    let effective = flatten_single_root(&tmp_dir);

    if target_dir.exists() {
        // Replace any previous install of this slug for the target agent.
        fs::remove_dir_all(&target_dir).map_err(|e| io_error("替换已有安装", &e))?;
        warnings.push("已覆盖该目录下已有的同名 skill".to_string());
    }
    fs::rename(&effective, &target_dir).map_err(|e| io_error("移动安装目录", &e))?;

    // Clean up the temp extraction dir if it still exists (non-flattened case).
    if tmp_dir.exists() {
        let _ = fs::remove_dir_all(&tmp_dir);
    }

    Ok(InstallResult {
        ok: true,
        dir: display_path(&target_dir),
        agent: input.agent.clone(),
        warnings,
    })
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
