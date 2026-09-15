use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::installer::error::{io_error, InstallError};
use crate::installer::link::LocationKind;

/// The on-disk sub-directory (relative to a skill install dir) where skillhub
/// records install metadata. Mirrors the CLI's `.skillhub/metadata.json` so the
/// desktop client and CLI share the same local bookkeeping.
const METADATA_DIR: &str = ".skillhub";
const METADATA_FILE: &str = "metadata.json";

/// Metadata recorded at install time, compatible with the CLI's
/// `InstalledSkillMetadata` (schema version 1, source "skillhub").
///
/// Field names are intentionally camelCase to match the CLI's on-disk JSON
/// contract (see `cli/src/services/installed-skill-metadata.ts`). The CLI also
/// writes `versionId` / `fingerprint` / `files`, which this struct does not
/// model — [`write_metadata`] preserves those keys verbatim rather than
/// dropping them on rewrite.
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
    /// Optional out-of-band homepage for skills whose `SKILL.md` carries none.
    /// Read-only for now — nothing writes it yet, but the field is reserved so
    /// an installer that does record one stays forward-compatible.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
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
            homepage: None,
        }
    }
}

/// Absolute path to the `.skillhub/metadata.json` for a skill install dir.
pub fn metadata_path(install_dir: &Path) -> PathBuf {
    install_dir.join(METADATA_DIR).join(METADATA_FILE)
}

/// Write metadata into `install_dir/.skillhub/metadata.json`.
///
/// Fields already present in the file that this struct does not model (the
/// CLI's `versionId`, `fingerprint`, `files`, `agent`) are read back and
/// re-emitted. Both clients share this file, so a plain overwrite here would
/// silently strip the CLI's bookkeeping every time the desktop client updates a
/// skill.
pub fn write_metadata(
    install_dir: &Path,
    metadata: &InstalledMetadata,
) -> Result<(), InstallError> {
    let dir = install_dir.join(METADATA_DIR);
    fs::create_dir_all(&dir).map_err(|e| io_error("创建 .skillhub 目录", &e))?;

    let mut merged = read_raw_metadata(install_dir).unwrap_or_default();
    let known = serde_json::to_value(metadata)
        .map_err(|e| InstallError::new("metadata_serialize", format!("序列化元数据失败: {e}")))?;
    let serde_json::Value::Object(known) = known else {
        return Err(InstallError::new(
            "metadata_serialize",
            "序列化元数据失败: 结果不是对象",
        ));
    };
    for (key, value) in known {
        merged.insert(key, value);
    }

    let content = serde_json::to_vec_pretty(&serde_json::Value::Object(merged))
        .map_err(|e| InstallError::new("metadata_serialize", format!("序列化元数据失败: {e}")))?;
    fs::write(metadata_path(install_dir), content).map_err(|e| io_error("写入元数据", &e))?;
    Ok(())
}

/// Read the raw metadata object, unparsed, so unknown keys survive a rewrite.
fn read_raw_metadata(install_dir: &Path) -> Option<serde_json::Map<String, serde_json::Value>> {
    let content = fs::read_to_string(metadata_path(install_dir)).ok()?;
    match serde_json::from_str::<serde_json::Value>(&content).ok()? {
        serde_json::Value::Object(map) => Some(map),
        _ => None,
    }
}

/// Read metadata from an install dir, if present and valid.
///
/// Returns `Ok(None)` when the skill is not installed by skillhub (no metadata).
pub fn read_metadata(install_dir: &Path) -> Result<Option<InstalledMetadata>, InstallError> {
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
    /// True when the skill entry exists but carries no usable skillhub
    /// metadata (a manually-placed or third-party skill). The client should ask
    /// the user before replacing or deleting it.
    pub unmanaged: bool,
    /// How the entry exists on disk, when it does at all. Distinguishes a real
    /// directory from a symlink, and a symlink whose target has gone away.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<LocationKind>,
}

/// Determine the status of a skill at `install_dir` against an optional
/// `requested_version`.
///
/// Existence is judged with `symlink_metadata` rather than `Path::exists`: the
/// latter follows links, so a dangling symlink would be reported as "not
/// installed" and leave the user with no clue that a broken entry is sitting in
/// their skills directory.
pub fn detect_status(
    install_dir: &Path,
    requested_version: Option<&str>,
) -> Result<SkillStatus, InstallError> {
    let kind = crate::installer::link::classify(install_dir).ok();

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
                kind,
            })
        }
        None => Ok(SkillStatus {
            installed: false,
            version: String::new(),
            outdated: false,
            unmanaged: kind.is_some(),
            kind,
        }),
    }
}

/// True when `install_dir` carries skillhub install metadata we can actually
/// read, i.e. the skill was installed by (and is manageable by) skillhub rather
/// than placed manually or by another tool.
///
/// Deliberately not `metadata_path(..).exists()`. A truncated, hand-edited, or
/// half-written `metadata.json` means we cannot tell what this directory is, and
/// the only safe answer is "not ours". Existence-checking instead of
/// parsing-checking is what would let a corrupt file turn a directory the UI
/// promises to back up into one that gets deleted outright — and it is the only
/// definition of "managed" in the crate, so the scan, the uninstall path, and
/// the install-over-existing path cannot drift apart.
pub fn has_metadata(install_dir: &Path) -> bool {
    matches!(read_metadata(install_dir), Ok(Some(_)))
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
