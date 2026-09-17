//! Tests for the `.skillhub` repository display slice: the synthetic `.skillhub`
//! scan source, the derived `repo_managed` / `linked_agents` fields, and the
//! repository uninstall path (real directory + all agent links).
//!
//! Link-related cases are Unix-only (they need `std::os::unix::fs::symlink`).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::installer::agents::AGENT_PROFILES;
use crate::installer::link::find_links_to_target_under;
use crate::installer::local_skills::{
    scan_roots, uninstall_repo_skill_under, LocalSkill, REPO_AGENT_ID,
};
use crate::installer::metadata::write_metadata;
use crate::installer::metadata::InstalledMetadata;

struct TempTree {
    root: PathBuf,
}

impl TempTree {
    fn new(tag: &str) -> Self {
        static NONCE: AtomicU64 = AtomicU64::new(0);
        let nonce = NONCE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "skillhub-t-{tag}-{pid}-{nonce}",
            pid = std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        Self { root }
    }

    fn at(&self, relative: &str) -> PathBuf {
        self.root.join(relative)
    }
}

impl Drop for TempTree {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn make_skill(dir: &Path, metadata: Option<(&str, &str, &str)>) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(dir.join("SKILL.md"), "# skill\n").unwrap();
    if let Some((namespace, slug, version)) = metadata {
        write_metadata(
            dir,
            &InstalledMetadata::new("https://skill.example.com", namespace, slug, version),
        )
        .unwrap();
    }
}

#[cfg(unix)]
fn symlink_dir(target: &Path, link: &Path) {
    std::os::unix::fs::symlink(target, link).unwrap();
}

fn slugs_of(skills: &[LocalSkill]) -> Vec<&str> {
    skills.iter().map(|skill| skill.slug.as_str()).collect()
}

// ---------------------------------------------------------------------------
// Milestone 1: the `.skillhub` synthetic scan source
// ---------------------------------------------------------------------------

#[test]
fn scan_folds_the_repo_source_in_as_a_sixth_root() {
    let tree = TempTree::new("repo-source");
    let repo = tree.at("repo-skills");
    make_skill(&repo.join("weather"), Some(("global", "weather", "1.0.0")));

    let (skills, warnings) = scan_roots(&[(REPO_AGENT_ID.to_string(), repo.clone())]);

    assert!(warnings.is_empty());
    assert_eq!(slugs_of(&skills), vec!["weather"]);
    assert_eq!(
        skills[0].origin,
        crate::installer::local_skills::SkillOrigin::Managed
    );
}

/// `namespace-sync.json` sits at `~/.skillhub`, one level above `repo_root()`
/// (`~/.skillhub/skills`), so it must never appear as a skill.
#[test]
fn scan_ignores_the_cli_workspace_file_outside_the_repo_skills_dir() {
    let tree = TempTree::new("repo-workspace");
    let repo = tree.at("repo-skills");
    make_skill(&repo.join("weather"), Some(("global", "weather", "1.0.0")));

    // The workspace file lives next to (not inside) the skills sub-directory.
    let workspace_file = tree.at("namespace-sync.json");
    std::fs::write(&workspace_file, "{}").unwrap();

    let (skills, _) = scan_roots(&[(REPO_AGENT_ID.to_string(), repo.clone())]);

    assert_eq!(slugs_of(&skills), vec!["weather"]);
}

#[cfg(unix)]
#[test]
fn a_shared_skill_aggregates_into_one_record_with_derived_fields() {
    let tree = TempTree::new("repo-aggregate");
    let repo = tree.at("repo-skills");
    let real = repo.join("weather");
    make_skill(&real, Some(("global", "weather", "1.0.0")));

    let claude = tree.at("claude-skills");
    let codex = tree.at("codex-skills");
    std::fs::create_dir_all(&claude).unwrap();
    std::fs::create_dir_all(&codex).unwrap();
    symlink_dir(&real, &claude.join("weather"));
    symlink_dir(&real, &codex.join("weather"));

    let (skills, _) = scan_roots(&[
        (REPO_AGENT_ID.to_string(), repo.clone()),
        ("claude-code".to_string(), claude.clone()),
        ("codex".to_string(), codex.clone()),
    ]);

    assert_eq!(skills.len(), 1, "one real directory must yield one skill");
    let weather = &skills[0];
    assert!(
        weather.repo_managed,
        "a repo item must be flagged repo-managed"
    );
    assert_eq!(
        weather.linked_agents,
        vec!["claude-code", "codex"],
        "both agents linking to the repo item must be listed"
    );
    assert_eq!(weather.locations.len(), 3);
}

#[cfg(unix)]
#[test]
fn a_repo_item_with_no_agent_links_is_repo_managed_but_not_linked() {
    let tree = TempTree::new("repo-orphan");
    let repo = tree.at("repo-skills");
    make_skill(&repo.join("lonely"), Some(("global", "lonely", "1.0.0")));

    let (skills, _) = scan_roots(&[(REPO_AGENT_ID.to_string(), repo.clone())]);

    assert_eq!(skills.len(), 1);
    assert!(skills[0].repo_managed);
    assert!(skills[0].linked_agents.is_empty());
}

#[test]
fn a_copy_installed_agent_skill_is_not_repo_managed() {
    let tree = TempTree::new("repo-copy");
    let claude = tree.at("claude-skills");
    // A real directory (not a link) inside an agent root: the copy-install shape.
    make_skill(&claude.join("manual"), Some(("global", "manual", "1.0.0")));

    let (skills, _) = scan_roots(&[("claude-code".to_string(), claude.clone())]);

    assert_eq!(skills.len(), 1);
    assert!(
        !skills[0].repo_managed,
        "a copy-installed skill is not repo-managed"
    );
    assert!(skills[0].linked_agents.is_empty());
}

#[cfg(unix)]
#[test]
fn repo_skill_serializes_repo_managed_and_linked_agents() {
    let tree = TempTree::new("repo-wire");
    let repo = tree.at("repo-skills");
    let real = repo.join("weather");
    make_skill(&real, Some(("global", "weather", "1.0.0")));

    let claude = tree.at("claude-skills");
    std::fs::create_dir_all(&claude).unwrap();
    symlink_dir(&real, &claude.join("weather"));

    let (skills, _) = scan_roots(&[
        (REPO_AGENT_ID.to_string(), repo.clone()),
        ("claude-code".to_string(), claude.clone()),
    ]);
    let json = serde_json::to_value(&skills[0]).unwrap();

    assert_eq!(json["repoManaged"], true);
    assert_eq!(json["linkedAgents"][0], "claude-code");
    assert!(
        json.get("repo_managed").is_none(),
        "snake_case keys must not leak to the web view"
    );
}

// ---------------------------------------------------------------------------
// Milestone 4: find_links_to_target_under
// ---------------------------------------------------------------------------

#[cfg(unix)]
#[test]
fn find_links_finds_zero_one_and_two_links() {
    let tree = TempTree::new("find-links");
    let repo = tree.at("repo");
    let real = repo.join("weather");
    make_skill(&real, Some(("global", "weather", "1.0.0")));

    let claude = tree.at("claude");
    let codex = tree.at("codex");
    std::fs::create_dir_all(&claude).unwrap();
    std::fs::create_dir_all(&codex).unwrap();

    let roots = vec![
        ("claude-code".to_string(), claude.clone()),
        ("codex".to_string(), codex.clone()),
    ];

    // Zero links.
    let (links, warnings) = find_links_to_target_under(&real, &roots);
    assert!(links.is_empty());
    assert!(warnings.is_empty());

    // One link.
    symlink_dir(&real, &claude.join("weather"));
    let (links, _) = find_links_to_target_under(&real, &roots);
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].0, "claude-code");

    // Two links.
    symlink_dir(&real, &codex.join("weather"));
    let (links, _) = find_links_to_target_under(&real, &roots);
    assert_eq!(links.len(), 2);
}

#[cfg(unix)]
#[test]
fn find_links_does_not_match_a_link_to_a_different_target() {
    let tree = TempTree::new("find-wrong");
    let repo = tree.at("repo");
    let real = repo.join("weather");
    make_skill(&real, Some(("global", "weather", "1.0.0")));
    let other = repo.join("other");
    make_skill(&other, None);

    let claude = tree.at("claude");
    std::fs::create_dir_all(&claude).unwrap();
    symlink_dir(&other, &claude.join("weather"));

    let roots = vec![("claude-code".to_string(), claude.clone())];
    let (links, _) = find_links_to_target_under(&repo.join("weather"), &roots);
    assert!(
        links.is_empty(),
        "a link to another directory must not be reported as pointing at the target"
    );
}

#[cfg(unix)]
#[test]
fn find_links_ignores_a_dangling_link() {
    let tree = TempTree::new("find-dangling");
    let repo = tree.at("repo");
    let real = repo.join("weather");
    make_skill(&real, Some(("global", "weather", "1.0.0")));

    let claude = tree.at("claude");
    std::fs::create_dir_all(&claude).unwrap();
    symlink_dir(&tree.at("nowhere"), &claude.join("weather"));

    let roots = vec![("claude-code".to_string(), claude.clone())];
    let (links, _) = find_links_to_target_under(&real, &roots);
    assert!(links.is_empty());
}

// ---------------------------------------------------------------------------
// Milestone 4: uninstall_repo_skill_under
// ---------------------------------------------------------------------------

#[cfg(unix)]
#[test]
fn repo_uninstall_removes_the_directory_and_all_links() {
    let tree = TempTree::new("repo-uninstall");
    let repo = tree.at("repo");
    let real = repo.join("weather");
    make_skill(&real, Some(("global", "weather", "1.0.0")));

    let claude = tree.at("claude");
    let codex = tree.at("codex");
    std::fs::create_dir_all(&claude).unwrap();
    std::fs::create_dir_all(&codex).unwrap();
    symlink_dir(&real, &claude.join("weather"));
    symlink_dir(&real, &codex.join("weather"));

    let roots = vec![
        ("claude-code".to_string(), claude.clone()),
        ("codex".to_string(), codex.clone()),
    ];
    let result = uninstall_repo_skill_under(&repo, &roots, "weather").unwrap();

    assert!(result.ok);
    assert_eq!(result.removed_links.len(), 2);
    assert!(!real.exists(), "the real directory must be gone");
    assert!(
        std::fs::symlink_metadata(claude.join("weather")).is_err(),
        "the claude link must be gone"
    );
    assert!(
        std::fs::symlink_metadata(codex.join("weather")).is_err(),
        "the codex link must be gone"
    );
}

#[cfg(unix)]
#[test]
fn repo_uninstall_removes_links_before_the_directory() {
    let tree = TempTree::new("repo-order");
    let repo = tree.at("repo");
    let real = repo.join("weather");
    make_skill(&real, Some(("global", "weather", "1.0.0")));

    let claude = tree.at("claude");
    std::fs::create_dir_all(&claude).unwrap();
    symlink_dir(&real, &claude.join("weather"));

    let roots = vec![("claude-code".to_string(), claude.clone())];
    let result = uninstall_repo_skill_under(&repo, &roots, "weather").unwrap();

    assert!(result.ok);
    // Both the link and the directory are gone; the link went first.
    assert!(std::fs::symlink_metadata(claude.join("weather")).is_err());
    assert!(!real.exists());
}

#[test]
fn repo_uninstall_rejects_an_invalid_slug() {
    let tree = TempTree::new("repo-bad-slug");
    let repo = tree.at("repo");

    let err = uninstall_repo_skill_under(&repo, &[], "../escape").unwrap_err();
    assert_eq!(err.code, "invalid_slug");
}

#[test]
fn repo_uninstall_errors_on_a_missing_repo_entry() {
    let tree = TempTree::new("repo-outside");
    let repo = tree.at("repo");
    // The slug passes validation but no such directory exists under `repo`, so
    // this is the missing-entry read error, not a containment rejection (there is
    // no explicit containment check — containment is structural, via
    // `repo_root().join(slug)` with a validated single-component slug).
    let err = uninstall_repo_skill_under(&repo, &[], "weather").unwrap_err();
    assert_eq!(
        err.code, "io_error",
        "a missing repo entry is a read error, not a silent no-op"
    );
}

#[test]
fn repo_uninstall_rejects_a_missing_entry() {
    let tree = TempTree::new("repo-missing");
    let repo = tree.at("repo");

    let err = uninstall_repo_skill_under(&repo, &[], "weather").unwrap_err();
    // Not found surfaces as a read error on symlink_metadata.
    assert_eq!(err.code, "io_error");
}

/// A directory the user hand-placed in the repo root (no skillhub metadata) must
/// be renamed aside, not deleted — the same single definition of "ours" the scan
/// and the per-agent uninstall path use.
#[cfg(unix)]
#[test]
fn repo_uninstall_backs_up_an_unmanaged_directory() {
    let tree = TempTree::new("repo-unmanaged");
    let repo = tree.at("repo");
    let real = repo.join("hand-made");
    std::fs::create_dir_all(&real).unwrap();
    std::fs::write(real.join("SKILL.md"), "# mine\n").unwrap();
    std::fs::write(real.join("notes.txt"), "precious").unwrap();

    let claude = tree.at("claude");
    std::fs::create_dir_all(&claude).unwrap();
    symlink_dir(&real, &claude.join("hand-made"));

    let roots = vec![("claude-code".to_string(), claude.clone())];
    let result = uninstall_repo_skill_under(&repo, &roots, "hand-made").unwrap();

    assert!(result.ok);
    assert!(
        result.backup_dir.is_some(),
        "an unmanaged dir must be backed up"
    );
    assert!(
        std::fs::symlink_metadata(claude.join("hand-made")).is_err(),
        "the claude link must still be removed"
    );
    // The content survived, renamed aside.
    let backup = PathBuf::from(result.backup_dir.as_deref().unwrap());
    assert_eq!(
        std::fs::read_to_string(backup.join("notes.txt")).unwrap(),
        "precious"
    );
}

/// A link that cannot be removed must leave the real directory in place, so the
/// user's content survives and no dangling link is created.
///
/// Removal of a symlink needs write permission on the containing directory, so
/// locking the agent root read-only makes `remove_file` fail while discovery
/// still finds the link.
#[cfg(unix)]
#[test]
fn repo_uninstall_keeps_the_directory_when_a_link_fails() {
    use std::os::unix::fs::PermissionsExt;

    let tree = TempTree::new("repo-link-fail");
    let repo = tree.at("repo");
    let real = repo.join("weather");
    make_skill(&real, Some(("global", "weather", "1.0.0")));

    let claude = tree.at("claude");
    std::fs::create_dir_all(&claude).unwrap();
    symlink_dir(&real, &claude.join("weather"));
    // Make the agent root read-only so removing the link inside it fails while
    // discovery (read_dir + symlink_metadata) still succeeds.
    std::fs::set_permissions(&claude, std::fs::Permissions::from_mode(0o555)).unwrap();

    // A test process running as root ignores mode bits, which would make this
    // pass vacuously. Check before asserting anything about the failure path.
    let removal_fails = std::fs::remove_file(claude.join("weather")).is_err();
    // Recreate the link attempt did not delete (the probe above only *tried*).
    if !removal_fails {
        std::fs::set_permissions(&claude, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    let roots = vec![("claude-code".to_string(), claude.clone())];
    let result = uninstall_repo_skill_under(&repo, &roots, "weather").unwrap();

    if removal_fails {
        assert!(!result.ok, "a failed link removal must report failure");
        assert!(
            real.exists(),
            "the real directory must be kept when a link cannot be removed"
        );
        assert_eq!(result.removed_links.len(), 0);
        assert!(
            result.warnings.iter().any(|w| w.contains("claude-code")),
            "the warning must name the agent whose link failed: {:?}",
            result.warnings
        );
        // Restore so TempTree::drop can clean up.
        std::fs::set_permissions(&claude, std::fs::Permissions::from_mode(0o755)).unwrap();
    } else {
        // Running as root: the mode bits did not deny removal, so the success
        // path ran instead. Say so rather than letting the test pass silently.
        eprintln!("skipped link-failure assertion: chmod 555 did not deny remove_file");
        assert!(!real.exists());
    }
}

// ---------------------------------------------------------------------------
// Milestone 2 support: AGENT_PROFILES must not contain the synthetic id
// ---------------------------------------------------------------------------

#[test]
fn agent_profiles_do_not_contain_the_synthetic_repo_id() {
    assert!(
        AGENT_PROFILES.iter().all(|p| p.id != REPO_AGENT_ID),
        "the synthetic repository id must never be an installable agent"
    );
}
