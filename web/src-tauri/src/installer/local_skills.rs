use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::installer::agents::{profile_root, repo_root, validate_slug, AGENT_PROFILES};
use crate::installer::error::{io_error, InstallError};
use crate::installer::frontmatter::read_frontmatter_value;
use crate::installer::homepage::{resolve_homepage, HomepageSource};
use crate::installer::link::{
    classify, ensure_under_roots, find_links_to_target_under, remove_location, LocationKind,
    LocationRemoval,
};
use crate::installer::metadata::{
    backup_dir, has_metadata, is_skillhub_sibling, read_metadata_lenient,
};

/// The frontmatter key skill authors use to point at their source repository.
const HOMEPAGE_KEY: &str = "homepage";

/// Synthetic agent id for the shared skill repository (`~/.skillhub/skills/`).
///
/// This is a display-level classification only: it is not a real agent, never
/// enters [`AGENT_PROFILES`], and so can never be picked as an install target.
/// It exists so the scan can fold the repository in as a sixth root and the web
/// view can offer a `.skillhub` category alongside the per-agent ones.
pub const REPO_AGENT_ID: &str = ".skillhub";

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
    /// Whether the skill lives in the shared repository (`~/.skillhub/skills/`).
    ///
    /// Derived from `locations`: true when one of them carries the synthetic
    /// [`REPO_AGENT_ID`]. A skill installed as an independent copy into an agent
    /// (a real directory, not a link) is not repo-managed.
    pub repo_managed: bool,
    /// The agents (excluding the synthetic `REPO_AGENT_ID` itself) that link to
    /// this repository skill. Only meaningful when `repo_managed`; empty for a
    /// skill that is not in the repository.
    pub linked_agents: Vec<String>,
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
    let mut roots: Vec<(String, PathBuf)> = AGENT_PROFILES
        .iter()
        .map(|profile| (profile.id.to_string(), profile_root(profile)))
        .collect();
    // Synthetic source: the shared repository root. Its sub-directories are real
    // skill directories (not dot-named), so the existing dot / sibling filtering
    // leaves them alone, and `aggregate()` folds them into the same `LocalSkill`
    // as the agent links that point at them.
    roots.push((REPO_AGENT_ID.to_string(), repo_root()));
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
            // So are our own backup / stash / temp siblings. They sit directly
            // inside the root and look exactly like a skill, so without this the
            // scan offers the user an uninstall button for their own backup.
            if is_skillhub_sibling(name) {
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

        // Derive the repo classification from the aggregated locations, in the
        // same loop that already sorts them: zero extra traversal.
        skill.repo_managed = skill
            .locations
            .iter()
            .any(|location| location.agent == REPO_AGENT_ID);
        // `linked_agents` is only meaningful for a repo item (it is a display
        // field for "who shares this repository skill"); a copy-installed or
        // agent-only skill has no repository entry, so it stays empty.
        skill.linked_agents = if skill.repo_managed {
            let mut agents: Vec<String> = skill
                .locations
                .iter()
                .filter(|location| location.agent != REPO_AGENT_ID)
                .map(|location| location.agent.clone())
                .collect();
            agents.dedup();
            agents
        } else {
            Vec::new()
        };
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
            repo_managed: false,
            linked_agents: Vec::new(),
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
            repo_managed: false,
            linked_agents: Vec::new(),
        };
    };

    // Metadata we cannot parse counts as unmanaged on purpose: reading it wrong
    // would mean treating someone else's content as ours to delete.
    let metadata = read_metadata_lenient(dir);
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
        repo_managed: false,
        linked_agents: Vec::new(),
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

/// What removing a skill from the shared repository actually did.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UninstallRepoResult {
    pub ok: bool,
    /// The repository directory that was removed.
    pub removed_dir: String,
    /// The agent links that were removed first, one per agent id.
    pub removed_links: Vec<String>,
    /// Set when a real directory was not installed by skillhub and was renamed
    /// aside (backed up) rather than deleted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backup_dir: Option<String>,
    /// Set when a link could not be removed (e.g. permission); the real directory
    /// is left in place so nothing is half-removed.
    pub warnings: Vec<String>,
}

/// Remove a skill from the shared repository: its real directory plus every
/// agent link pointing at it.
///
/// This is the **opposite** of [`uninstall_location`]: there, detaching one
/// agent's link leaves the real directory behind; here, deleting the repository
/// item must also detach every agent that linked to it, or those links would go
/// dangling. Order matters — links are removed first, then the real directory —
/// so a halfway failure never leaves a dangling link pointing at a deleted
/// directory longer than necessary.
pub fn uninstall_repo_skill(slug: &str) -> Result<UninstallRepoResult, InstallError> {
    let agent_roots: Vec<(String, PathBuf)> = AGENT_PROFILES
        .iter()
        .map(|profile| (profile.id.to_string(), profile_root(profile)))
        .collect();
    uninstall_repo_skill_under(&repo_root(), &agent_roots, slug)
}

/// [`uninstall_repo_skill`] against an explicit repository root and agent roots,
/// split out so tests can drive it against a temp tree.
pub fn uninstall_repo_skill_under(
    repo_root: &Path,
    agent_roots: &[(String, PathBuf)],
    slug: &str,
) -> Result<UninstallRepoResult, InstallError> {
    if !validate_slug(slug) {
        return Err(InstallError::new(
            "invalid_slug",
            format!("技能目录名无效: {slug}"),
        ));
    }

    let real_dir = repo_root.join(slug);
    let (links, discovery_warnings) = find_links_to_target_under(&real_dir, agent_roots);
    remove_repo_item(&real_dir, links, discovery_warnings)
}

/// Validate that `real_dir` is a real directory (never a link) and then remove
/// every listed agent link before removing the directory itself.
///
/// A link must never reach `remove_dir_all`: unlinking a link on the top-level
/// would follow it and delete the link's target. Each link is removed as a link
/// only, and if any cannot be removed the real directory is left in place so no
/// content is lost while a dangling link still points at it.
///
/// The directory is deleted only when it is a skillhub-managed repo item
/// (readable `.skillhub/metadata.json`); a hand-placed directory with no
/// metadata is renamed aside, mirroring the per-agent uninstall path — the same
/// single definition of "ours" the scan and the agent path use.
fn remove_repo_item(
    real_dir: &Path,
    links: Vec<(String, PathBuf)>,
    discovery_warnings: Vec<String>,
) -> Result<UninstallRepoResult, InstallError> {
    let meta =
        std::fs::symlink_metadata(real_dir).map_err(|err| io_error("读取仓库技能状态", &err))?;
    if !meta.file_type().is_dir() {
        return Err(InstallError::new(
            "not_a_directory",
            format!("仓库技能不是真实目录: {}", real_dir.display()),
        ));
    }

    let mut warnings: Vec<String> = discovery_warnings;
    let mut removed_links: Vec<String> = Vec::new();
    for (agent, link_path) in &links {
        match std::fs::remove_file(link_path) {
            Ok(()) => removed_links.push(agent.clone()),
            Err(err) => warnings.push(format!("移除 {agent} 链接失败: {err}")),
        }
    }

    // If any link could not be removed — or a root was unreadable, so we cannot
    // be sure we found every link — leave the real directory in place: the user
    // would otherwise lose the content while a dangling link still points at the
    // now-missing directory.
    if !warnings.is_empty() {
        return Ok(UninstallRepoResult {
            ok: false,
            removed_dir: String::new(),
            removed_links,
            backup_dir: None,
            warnings,
        });
    }

    // Same single definition of "ours" the scan and the per-agent uninstall use:
    // a readable metadata.json. A hand-placed directory that is not skillhub's
    // is renamed aside rather than deleted, so the user's content survives.
    if has_metadata(real_dir) {
        std::fs::remove_dir_all(real_dir).map_err(|err| io_error("删除仓库技能目录", &err))?;
        Ok(UninstallRepoResult {
            ok: true,
            removed_dir: real_dir.to_string_lossy().into_owned(),
            removed_links,
            backup_dir: None,
            warnings,
        })
    } else {
        let backup = backup_dir(real_dir)?;
        Ok(UninstallRepoResult {
            ok: true,
            removed_dir: String::new(),
            removed_links,
            backup_dir: Some(backup.to_string_lossy().into_owned()),
            warnings,
        })
    }
}
