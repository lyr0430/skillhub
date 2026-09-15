use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::installer::agents::{profile_root, AGENT_PROFILES};
use crate::installer::error::InstallError;
use crate::installer::frontmatter::read_frontmatter_value;
use crate::installer::homepage::{resolve_homepage, HomepageSource};
use crate::installer::link::{
    classify, ensure_under_roots, remove_location, LocationKind, LocationRemoval,
};
use crate::installer::metadata::{has_metadata, read_metadata};

/// The frontmatter key skill authors use to point at their source repository.
const HOMEPAGE_KEY: &str = "homepage";

/// Our own bookkeeping, left as siblings in a skills root.
///
/// `backup_dir` renames an unmanaged directory to `<name>.skillhub-backup-<ts>`
/// and a failed install can leave `<name>.skillhub-tmp-<pid>-...` behind, both
/// directly inside the root. Without this they get scanned back as brand-new
/// unmanaged skills — offering the user an uninstall button for their own backup.
const BOOKKEEPING_MARKERS: [&str; 2] = [".skillhub-backup-", ".skillhub-tmp-"];

fn is_skillhub_bookkeeping(name: &str) -> bool {
    BOOKKEEPING_MARKERS
        .iter()
        .any(|marker| name.contains(marker))
}

/// Whether a local skill is one skillhub installed, and therefore one we can
/// update as well as remove.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SkillOrigin {
    /// Installed by skillhub: carries a readable `.skillhub/metadata.json`.
    Managed,
    /// Anything else — hand-copied, installed by another tool, or carrying
    /// metadata we could not parse. Visible and removable, never updated.
    Unmanaged,
}

/// One way a skill is reachable from an agent's skills root.
#[derive(Debug, Serialize)]
pub struct SkillLocation {
    pub agent: String,
    pub path: String,
    pub kind: LocationKind,
    /// The link target exactly as written on disk (a relative link stays
    /// relative). Display only — never used to resolve a write target.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
}

/// A locally installed skill, aggregated across every agent root it appears in.
///
/// Serialized as camelCase because this crosses into TypeScript; the enum
/// values (`origin`, `kind`, `homepageSource`) are kebab-case strings.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalSkill {
    /// Directory name; the identity shown when there is no metadata.
    pub slug: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub registry: Option<String>,
    /// Set only when metadata records a different slug than the directory name,
    /// so the UI can flag the mismatch.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata_slug: Option<String>,
    pub origin: SkillOrigin,
    /// Canonical directory the skill actually lives in; absent for broken and
    /// foreign links, which resolve to nothing usable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub real_path: Option<String>,
    pub locations: Vec<SkillLocation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub homepage_source: Option<HomepageSource>,
    /// The entry exists but could not be resolved (permissions, I/O error).
    pub unreadable: bool,
}

/// A skill entry found under one agent root, before it is aggregated.
struct Entry {
    agent: String,
    path: PathBuf,
    kind: LocationKind,
    target: Option<String>,
}

/// Scan every known agent's skills root and aggregate what is there.
///
/// Returns the skills plus human-readable warnings for roots that could not be
/// read. A single unreadable root never aborts the scan.
pub fn scan_local_skills() -> (Vec<LocalSkill>, Vec<String>) {
    let roots: Vec<(String, PathBuf)> = AGENT_PROFILES
        .iter()
        .map(|profile| (profile.id.to_string(), profile_root(profile)))
        .collect();
    scan_roots(&roots)
}

/// Scan an explicit set of `(agent_id, skills_root)` pairs.
///
/// Split out from [`scan_local_skills`] so tests can point it at a temp tree
/// instead of the real home directory.
pub fn scan_roots(roots: &[(String, PathBuf)]) -> (Vec<LocalSkill>, Vec<String>) {
    let mut entries: Vec<Entry> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    for (agent, root) in roots {
        let read_dir = match fs::read_dir(root) {
            Ok(iter) => iter,
            // A missing root just means that agent is not installed here.
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
            Err(err) => {
                warnings.push(format!("无法读取 {}: {err}", root.display()));
                continue;
            }
        };

        for dir_entry in read_dir.flatten() {
            let path = dir_entry.path();
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            // Hidden entries are bookkeeping (`.skillhub`, `.DS_Store`), not skills.
            if name.starts_with('.') {
                continue;
            }
            // So are our own backup and temp siblings (see BOOKKEEPING_MARKERS).
            if is_skillhub_bookkeeping(name) {
                continue;
            }
            // Skills roots also collect plain files (README, notes); skip them.
            if !is_skill_entry(&path) {
                continue;
            }

            let Ok(kind) = classify(&path) else {
                warnings.push(format!("无法读取条目 {}", path.display()));
                continue;
            };
            let target = if kind.is_link() {
                fs::read_link(&path)
                    .ok()
                    .map(|target| target.to_string_lossy().into_owned())
            } else {
                None
            };

            entries.push(Entry {
                agent: agent.clone(),
                path,
                kind,
                target,
            });
        }
    }

    (aggregate(entries), warnings)
}

/// True when an entry could be a skill at all: a directory or a symlink.
fn is_skill_entry(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|meta| {
            let file_type = meta.file_type();
            file_type.is_symlink() || file_type.is_dir()
        })
        .unwrap_or(false)
}

/// Fold per-agent entries into one record per real directory.
///
/// Aggregating on the canonical path is what keeps `~/.claude/skills/x`
/// (a link) and `~/.agents/skills/x` (the real directory) from showing up as
/// two unrelated skills with very different uninstall consequences.
fn aggregate(entries: Vec<Entry>) -> Vec<LocalSkill> {
    let mut skills: Vec<LocalSkill> = Vec::new();
    let mut index_by_key: HashMap<String, usize> = HashMap::new();

    for entry in entries {
        // Broken and foreign links have nothing to resolve; they stand alone.
        let real = match entry.kind {
            LocationKind::Dir | LocationKind::Symlink => fs::canonicalize(&entry.path).ok(),
            LocationKind::BrokenSymlink | LocationKind::ForeignSymlink => None,
        };

        let key = match &real {
            Some(path) => path.to_string_lossy().into_owned(),
            None => format!("link:{}", entry.path.to_string_lossy()),
        };

        let location = SkillLocation {
            agent: entry.agent.clone(),
            path: entry.path.to_string_lossy().into_owned(),
            kind: entry.kind,
            target: entry.target.clone(),
        };

        if let Some(&existing) = index_by_key.get(&key) {
            skills[existing].locations.push(location);
            continue;
        }

        index_by_key.insert(key, skills.len());
        skills.push(build_skill(&entry, real.as_deref(), location));
    }

    for skill in &mut skills {
        skill
            .locations
            .sort_by(|a, b| a.agent.cmp(&b.agent).then_with(|| a.path.cmp(&b.path)));
    }
    skills.sort_by(|a, b| a.slug.cmp(&b.slug));
    skills
}

/// Build the aggregated record for a skill's first-seen location.
fn build_skill(entry: &Entry, real: Option<&Path>, location: SkillLocation) -> LocalSkill {
    let slug = entry
        .path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_string();

    // A broken or foreign link has no readable content, and we deliberately do
    // not read through it: it is unmanaged by definition and only removable as
    // a link.
    if entry.kind.is_link() && real.is_none() {
        return LocalSkill {
            slug,
            namespace: None,
            version: None,
            registry: None,
            metadata_slug: None,
            origin: SkillOrigin::Unmanaged,
            real_path: None,
            locations: vec![location],
            homepage: None,
            homepage_source: None,
            unreadable: false,
        };
    }

    let Some(dir) = real else {
        return LocalSkill {
            slug,
            namespace: None,
            version: None,
            registry: None,
            metadata_slug: None,
            origin: SkillOrigin::Unmanaged,
            real_path: None,
            locations: vec![location],
            homepage: None,
            homepage_source: None,
            unreadable: true,
        };
    };

    // Metadata we cannot parse counts as unmanaged on purpose: reading it wrong
    // would mean treating someone else's content as ours to delete.
    let metadata = read_metadata(dir).ok().flatten();
    let origin = if metadata.is_some() {
        SkillOrigin::Managed
    } else {
        SkillOrigin::Unmanaged
    };

    let author_homepage = read_frontmatter_value(dir, HOMEPAGE_KEY);
    let (homepage, homepage_source) = resolve_homepage(author_homepage, metadata.as_ref());

    // Surfaced only when it disagrees with the directory name, so the UI can
    // flag a skill the user has renamed.
    let metadata_slug = metadata
        .as_ref()
        .filter(|meta| meta.slug != slug)
        .map(|meta| meta.slug.clone());

    LocalSkill {
        slug,
        namespace: metadata.as_ref().map(|meta| meta.namespace.clone()),
        version: metadata.as_ref().map(|meta| meta.version.clone()),
        registry: metadata.as_ref().map(|meta| meta.registry.clone()),
        metadata_slug,
        origin,
        real_path: Some(dir.to_string_lossy().into_owned()),
        locations: vec![location],
        homepage,
        homepage_source,
        unreadable: false,
    }
}

/// Remove one location of a skill.
///
/// The path comes from the web view, so it is validated against the agent roots
/// first; classification then decides between unlinking, deleting, and backing
/// up (see [`remove_location`]).
pub fn uninstall_location(dir: &Path) -> Result<LocationRemoval, InstallError> {
    let roots: Vec<PathBuf> = AGENT_PROFILES.iter().map(profile_root).collect();
    uninstall_location_under(dir, &roots)
}

/// [`uninstall_location`] against an explicit set of roots.
///
/// Split out so the delete-versus-back-up decision — the only thing standing
/// between a user's directory and `remove_dir_all` — can be tested directly
/// instead of only through its helper.
pub fn uninstall_location_under(
    dir: &Path,
    roots: &[PathBuf],
) -> Result<LocationRemoval, InstallError> {
    let path = ensure_under_roots(dir, roots)?;
    let kind = classify(&path)?;
    // `has_metadata` parses, so a corrupt metadata.json lands on the backup
    // side — the same verdict the scan reaches, and the same one the confirm
    // dialog promised the user.
    let managed = kind == LocationKind::Dir && has_metadata(&path);
    remove_location(&path, managed)
}
