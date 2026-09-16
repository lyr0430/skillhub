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
/// Two deliberate choices, both about the same failure:
///
/// * Existence is judged with `symlink_metadata` (via `classify`) rather than
///   `Path::exists`: the latter follows links, so a dangling symlink would be
///   reported as "not installed" and leave the user with no clue that a broken
///   entry is sitting in their skills directory.
/// * **Infallible.** A directory whose `metadata.json` exists but does not parse
///   must be reported as `unmanaged`, which is what makes the client ask the
///   user before touching it. Returning a `Result` here is what let a caller
///   `.ok()` the parse error into `unmanaged: false` — "this slot is empty, go
///   ahead" — and delete the directory with no backup. There is no error left to
///   swallow, so that mistake cannot be made again.
pub fn detect_status(install_dir: &Path, requested_version: Option<&str>) -> SkillStatus {
    let kind = crate::installer::link::classify(install_dir).ok();

    match read_metadata_lenient(install_dir) {
        Some(meta) => {
            let outdated = requested_version
                .map(|requested| requested != meta.version)
                .unwrap_or(false);
            SkillStatus {
                installed: true,
                version: meta.version,
                outdated,
                unmanaged: false,
                kind,
            }
        }
        None => SkillStatus {
            installed: false,
            version: String::new(),
            outdated: false,
            // The entry exists, so it is *unmanaged*, not absent — the client
            // will ask before replacing or deleting it.
            unmanaged: kind.is_some(),
            kind,
        },
    }
}

/// Read metadata, treating "absent" and "present but unreadable" alike.
///
/// This is the single verdict the whole crate uses for "is this directory
/// ours?": the scan, the uninstall path, the install-over-existing path, and the
/// per-agent status all bottom out here, so they cannot disagree about a
/// directory whose metadata is corrupt.
///
/// A corrupt file must read as "not ours". The alternative — treating it as
/// absent — is what lets another tool's directory be deleted when the UI
/// promised a backup, and it is also what stops a hostile skill from making
/// itself look unmanaged to avoid being deleted.
pub fn read_metadata_lenient(install_dir: &Path) -> Option<InstalledMetadata> {
    read_metadata(install_dir).ok().flatten()
}

/// True when `install_dir` carries skillhub metadata we can actually read, i.e.
/// the skill was installed by (and is manageable by) skillhub rather than placed
/// manually or by another tool.
///
/// Deliberately not `metadata_path(..).exists()` — see
/// [`read_metadata_lenient`] for why existence is the wrong question.
pub fn has_metadata(install_dir: &Path) -> bool {
    read_metadata_lenient(install_dir).is_some()
}

/// Tags this crate uses for the sibling entries it creates next to a skill.
///
/// Every generator and the scan's filter must agree on this list: a tag that is
/// generated but not recognised comes back as a phantom skill with its own
/// uninstall button, and the user is offered to delete their own backup.
pub(crate) const SIBLING_TAGS: [&str; 3] = ["backup", "tmp", "old"];

/// True when `name` looks like a sibling this crate generated:
/// `<stem>.skillhub-<tag>-<digits>[-<digits>…]`.
///
/// Matched by shape rather than by substring on purpose. A substring test would
/// also hide a user's own directory named `my.skillhub-tmp-notes` — a real skill
/// the page should still list and let them manage.
pub fn is_skillhub_sibling(name: &str) -> bool {
    let Some((_, rest)) = name.rsplit_once(".skillhub-") else {
        return false;
    };

    let mut parts = rest.split('-');
    let Some(tag) = parts.next() else {
        return false;
    };
    if !SIBLING_TAGS.contains(&tag) {
        return false;
    }

    // `backup` is `<tag>-<seconds>[-<index>]`; the others are
    // `<tag>-<pid>-<nanos>-<nonce>`. All digits either way.
    let numbers: Vec<&str> = parts.collect();
    !numbers.is_empty()
        && numbers.len() <= 4
        && numbers
            .iter()
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
}

/// Rename a non-skillhub directory (or file) to a sibling backup so its contents
/// are not lost when the skill is replaced or uninstalled. Returns the backup
/// path.
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
