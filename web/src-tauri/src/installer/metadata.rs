use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::installer::error::{io_error, InstallError};

/// The on-disk sub-directory (relative to a skill install dir) where skillhub
/// records install metadata. Mirrors the CLI's `.skillhub/metadata.json` so the
/// desktop client and CLI share the same local bookkeeping.
const METADATA_DIR: &str = ".skillhub";
const METADATA_FILE: &str = "metadata.json";

/// Metadata recorded at install time, compatible with the CLI's
/// `InstalledSkillMetadata` (schema version 1, source "skillhub").
///
/// Field names are intentionally camelCase to match the CLI's on-disk JSON
/// contract (see `cli/src/services/installed-skill-metadata.ts`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(non_snake_case)]
pub struct InstalledMetadata {
    pub schemaVersion: u32,
    pub registry: String,
    pub namespace: String,
    pub slug: String,
    pub version: String,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub installedAt: Option<String>,
}

impl InstalledMetadata {
    pub fn new(registry: &str, namespace: &str, slug: &str, version: &str) -> Self {
        Self {
            schemaVersion: 1,
            registry: registry.to_string(),
            namespace: namespace.to_string(),
            slug: slug.to_string(),
            version: version.to_string(),
            source: "skillhub".to_string(),
            installedAt: None,
        }
    }
}

/// Absolute path to the `.skillhub/metadata.json` for a skill install dir.
pub fn metadata_path(install_dir: &Path) -> PathBuf {
    install_dir.join(METADATA_DIR).join(METADATA_FILE)
}

/// Write metadata into `install_dir/.skillhub/metadata.json`.
pub fn write_metadata(
    install_dir: &Path,
    metadata: &InstalledMetadata,
) -> Result<(), InstallError> {
    let dir = install_dir.join(METADATA_DIR);
    fs::create_dir_all(&dir).map_err(|e| io_error("创建 .skillhub 目录", &e))?;
    let content = serde_json::to_vec_pretty(metadata)
        .map_err(|e| InstallError::new("metadata_serialize", format!("序列化元数据失败: {e}")))?;
    fs::write(metadata_path(install_dir), content).map_err(|e| io_error("写入元数据", &e))?;
    Ok(())
}

/// Read metadata from an install dir, if present and valid.
///
/// Returns `Ok(None)` when the skill is not installed by skillhub (no metadata).
fn read_metadata(install_dir: &Path) -> Result<Option<InstalledMetadata>, InstallError> {
    let path = metadata_path(install_dir);
    if !path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(&path).map_err(|e| io_error("读取元数据", &e))?;
    let value: InstalledMetadata = serde_json::from_str(&content)
        .map_err(|e| InstallError::new("metadata_parse", format!("解析元数据失败: {e}")))?;
    Ok(Some(value))
}

/// Status of a single skill on a single agent, exposed to the web view.
#[derive(Debug, Serialize)]
pub struct SkillStatus {
    pub installed: bool,
    /// Installed version when skillhub metadata exists, else empty.
    pub version: String,
    /// Whether the installed version differs from the requested one.
    pub outdated: bool,
    /// True when the skill dir exists but carries no skillhub metadata (a
    /// manually-placed skill). The client should ask the user before replacing
    /// or deleting such a directory.
    pub unmanaged: bool,
}

/// Determine the status of a skill at `install_dir` against an optional
/// `requested_version`. When metadata is absent but the directory exists, the
/// skill is reported as `installed: false` with `unmanaged: true` (a directory
/// that exists from a manual copy, which we cannot manage but should not
/// silently delete).
pub fn detect_status(
    install_dir: &Path,
    agent_id: &str,
    slug: &str,
    requested_version: Option<&str>,
) -> Result<SkillStatus, InstallError> {
    let _ = (agent_id, slug);
    match read_metadata(install_dir)? {
        Some(meta) => {
            let outdated = requested_version
                .map(|requested| requested != meta.version)
                .unwrap_or(false);
            Ok(SkillStatus {
                installed: true,
                version: meta.version,
                outdated,
                unmanaged: false,
            })
        }
        None => Ok(SkillStatus {
            installed: false,
            version: String::new(),
            outdated: false,
            unmanaged: install_dir.exists(),
        }),
    }
}

/// True when `install_dir` carries skillhub install metadata, i.e. the skill was
/// installed by (and is manageable by) skillhub rather than placed manually.
pub fn has_metadata(install_dir: &Path) -> bool {
    metadata_path(install_dir).exists()
}

/// Rename a non-skillhub directory to a sibling backup so its contents are not
/// lost when the skill is replaced or uninstalled. Returns the backup path.
pub(crate) fn backup_dir(install_dir: &Path) -> Result<PathBuf, InstallError> {
    let name = install_dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("skill");
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let mut candidate = install_dir.with_file_name(format!("{name}.skillhub-backup-{ts}"));
    let mut idx = 0;
    while candidate.exists() {
        idx += 1;
        candidate = install_dir.with_file_name(format!("{name}.skillhub-backup-{ts}-{idx}"));
    }

    fs::rename(install_dir, &candidate).map_err(|e| io_error("备份原 skill 目录", &e))?;
    Ok(candidate)
}

/// Outcome of removing a skill install dir.
///
/// `backup_dir` is `Some(path)` when a non-skillhub directory (a manually-placed
/// skill) was preserved by renaming it, `None` when the dir was deleted in place
/// (a skillhub-installed skill).
#[derive(Debug)]
pub struct UninstallOutcome {
    pub backup_dir: Option<String>,
}

/// Remove a skill install dir (and its `.skillhub` metadata).
///
/// If the directory exists but was NOT installed by skillhub (no metadata, e.g.
/// a manually-placed skill), it is backed up (renamed) rather than deleted so
/// the user's content is not lost.
pub fn uninstall_dir(install_dir: &Path) -> Result<UninstallOutcome, InstallError> {
    if !install_dir.exists() {
        return Err(InstallError::new(
            "not_installed",
            "该 skill 未安装，无法卸载",
        ));
    }

    if has_metadata(install_dir) {
        fs::remove_dir_all(install_dir).map_err(|e| io_error("卸载失败", &e))?;
        Ok(UninstallOutcome { backup_dir: None })
    } else {
        let backup = backup_dir(install_dir)?;
        Ok(UninstallOutcome {
            backup_dir: Some(backup.to_string_lossy().into_owned()),
        })
    }
}

/// A locally installed skill discovered by scanning an agent's skill root.
#[derive(Debug, Serialize)]
pub struct InstalledSkill {
    pub registry: String,
    pub namespace: String,
    pub slug: String,
    pub version: String,
    pub agent: String,
    pub dir: String,
}

/// Scan an agent's skills root for skillhub-installed skills (those carrying
/// `.skillhub/metadata.json`). Returns the installed skill records.
///
/// We scan the immediate sub-directories of `skills_root` rather than reading a
/// central inventory, so the desktop client stays consistent with whatever the
/// CLI/agents actually have on disk without a separate bookkeeping file.
pub fn scan_agent_skills(agent_id: &str, skills_root: &Path) -> Vec<InstalledSkill> {
    let mut skills = Vec::new();
    let Ok(entries) = fs::read_dir(skills_root) else {
        return skills;
    };

    for entry in entries.flatten() {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let slug = match entry.file_name().into_string() {
            Ok(name) => name,
            Err(_) => continue,
        };
        if let Ok(Some(meta)) = read_metadata(&dir) {
            skills.push(InstalledSkill {
                registry: meta.registry,
                namespace: meta.namespace,
                slug,
                version: meta.version,
                agent: agent_id.to_string(),
                dir: dir.to_string_lossy().into_owned(),
            });
        }
    }

    skills
}
