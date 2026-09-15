//! Tests for the local skill model: link classification, cross-agent
//! aggregation, the "never follow a link for writes, never write through a
//! link path" contract, homepage resolution, and the uninstall path boundary.
//!
//! Link-specific cases are Unix-only (they need `std::os::unix::fs::symlink`
//! and on Windows creating one requires elevation); the rest run everywhere.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::installer::frontmatter::read_frontmatter_value;
use crate::installer::homepage::{resolve_homepage, validate_external_url, HomepageSource};
use crate::installer::install::{resolve_install_target, swap_into_place, Predecessor};
use crate::installer::link::{classify, ensure_under_roots, remove_location, LocationKind};
use crate::installer::local_skills::{scan_roots, uninstall_location_under, SkillOrigin};
use crate::installer::metadata::{metadata_path, read_metadata, write_metadata, InstalledMetadata};

/// A throwaway tree under the OS temp dir, removed when it goes out of scope.
///
/// Names carry a process-wide nonce because `cargo test` runs these in parallel
/// threads of one process, where the pid alone repeats.
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

/// Create a skill directory holding a `SKILL.md`, optionally with metadata.
fn make_skill(dir: &Path, skill_md: &str, metadata: Option<(&str, &str, &str)>) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(dir.join("SKILL.md"), skill_md).unwrap();
    if let Some((namespace, slug, version)) = metadata {
        write_metadata(
            dir,
            &InstalledMetadata::new("https://skill.example.com", namespace, slug, version),
        )
        .unwrap();
    }
}

fn managed_metadata() -> InstalledMetadata {
    InstalledMetadata::new("https://skill.example.com", "global", "my-skill", "1.0.0")
}

#[cfg(unix)]
fn symlink_dir(target: &Path, link: &Path) {
    std::os::unix::fs::symlink(target, link).unwrap();
}

fn slugs_of(skills: &[crate::installer::local_skills::LocalSkill]) -> Vec<&str> {
    skills.iter().map(|skill| skill.slug.as_str()).collect()
}

// ---------------------------------------------------------------------------
// Scanning: every entry shape is classified, nothing is mistaken for a skill
// ---------------------------------------------------------------------------

#[test]
fn scan_skips_hidden_entries_and_plain_files() {
    let tree = TempTree::new("scan-skip");
    let root = tree.at("skills");
    std::fs::create_dir_all(&root).unwrap();

    std::fs::write(root.join(".DS_Store"), "junk").unwrap();
    std::fs::write(root.join("notes.md"), "# notes").unwrap();
    std::fs::create_dir_all(root.join(".skillhub")).unwrap();
    make_skill(
        &root.join("real-skill"),
        "# real\n",
        Some(("global", "real-skill", "1.0.0")),
    );

    let (skills, warnings) = scan_roots(&[(String::from("claude-code"), root.clone())]);

    assert_eq!(slugs_of(&skills), vec!["real-skill"]);
    assert!(warnings.is_empty());
}

#[test]
fn scan_reports_unmanaged_directories_not_just_managed_ones() {
    let tree = TempTree::new("scan-unmanaged");
    let root = tree.at("skills");

    make_skill(
        &root.join("managed"),
        "# managed\n",
        Some(("global", "managed", "1.2.3")),
    );
    // No metadata: hand-copied or installed by another tool.
    make_skill(&root.join("manual"), "# manual\n", None);

    let (skills, _) = scan_roots(&[(String::from("claude-code"), root.clone())]);

    assert_eq!(slugs_of(&skills), vec!["managed", "manual"]);

    let managed = &skills[0];
    assert_eq!(managed.origin, SkillOrigin::Managed);
    assert_eq!(managed.version.as_deref(), Some("1.2.3"));

    let manual = &skills[1];
    assert_eq!(manual.origin, SkillOrigin::Unmanaged);
    assert!(manual.version.is_none());
    assert_eq!(manual.locations.len(), 1);
}

#[test]
fn scan_ignores_a_missing_root() {
    let tree = TempTree::new("scan-missing");

    let (skills, warnings) =
        scan_roots(&[(String::from("claude-code"), tree.at("does-not-exist"))]);

    assert!(skills.is_empty());
    assert!(warnings.is_empty(), "a missing agent root is not a warning");
}

#[cfg(unix)]
#[test]
fn scan_classifies_links_without_following_them() {
    let tree = TempTree::new("scan-links");
    let root = tree.at("skills");
    std::fs::create_dir_all(&root).unwrap();

    // A real skill a link points at.
    let store = tree.at("store");
    make_skill(
        &store.join("linked"),
        "# linked\n",
        Some(("global", "linked", "1.0.0")),
    );
    symlink_dir(&store.join("linked"), &root.join("linked"));

    // A dangling link.
    symlink_dir(&tree.at("nowhere"), &root.join("dangling"));

    // A link to a plain file: not a skill.
    std::fs::write(tree.at("a-file.txt"), "x").unwrap();
    symlink_dir(&tree.at("a-file.txt"), &root.join("file-link"));

    let (skills, _) = scan_roots(&[(String::from("claude-code"), root.clone())]);

    let by_slug = |slug: &str| {
        skills
            .iter()
            .find(|skill| skill.slug == slug)
            .unwrap_or_else(|| panic!("missing {slug} in {:?}", slugs_of(&skills)))
    };

    let linked = by_slug("linked");
    assert_eq!(linked.locations[0].kind, LocationKind::Symlink);
    assert_eq!(linked.origin, SkillOrigin::Managed);
    assert!(linked.real_path.as_deref().unwrap().ends_with("linked"));

    let dangling = by_slug("dangling");
    assert_eq!(dangling.locations[0].kind, LocationKind::BrokenSymlink);
    assert_eq!(dangling.origin, SkillOrigin::Unmanaged);
    assert!(dangling.real_path.is_none());

    let file_link = by_slug("file-link");
    assert_eq!(file_link.locations[0].kind, LocationKind::ForeignSymlink);
}

#[cfg(unix)]
#[test]
fn scan_aggregates_a_link_with_the_directory_it_points_at() {
    let tree = TempTree::new("scan-merge");

    // The same real directory, reachable from two agent roots: once directly,
    // once through a link — exactly the shape of ~/.agents/skills/x plus
    // ~/.claude/skills/x -> ../../.agents/skills/x.
    let store = tree.at("agents-skills");
    make_skill(
        &store.join("shared"),
        "# shared\n",
        Some(("global", "shared", "1.0.0")),
    );

    let claude = tree.at("claude-skills");
    std::fs::create_dir_all(&claude).unwrap();
    symlink_dir(&store.join("shared"), &claude.join("shared"));

    let (skills, _) = scan_roots(&[
        (String::from("generic"), store.clone()),
        (String::from("claude-code"), claude.clone()),
    ]);

    assert_eq!(skills.len(), 1, "one real directory must yield one skill");
    let shared = &skills[0];
    assert_eq!(
        shared.locations.len(),
        2,
        "both reachable locations must be reported"
    );
    assert_eq!(
        shared
            .locations
            .iter()
            .map(|location| location.agent.as_str())
            .collect::<Vec<_>>(),
        vec!["claude-code", "generic"]
    );

    let claude_location = shared
        .locations
        .iter()
        .find(|location| location.agent == "claude-code")
        .unwrap();
    assert_eq!(claude_location.kind, LocationKind::Symlink);
    // The link target is kept verbatim, not resolved.
    assert!(claude_location.target.is_some());
}

#[cfg(unix)]
#[test]
fn scan_keeps_two_independent_directories_apart() {
    let tree = TempTree::new("scan-independent");

    let first = tree.at("root-a");
    let second = tree.at("root-b");
    make_skill(
        &first.join("same-name"),
        "# a\n",
        Some(("global", "same-name", "1.0.0")),
    );
    make_skill(
        &second.join("same-name"),
        "# b\n",
        Some(("global", "same-name", "2.0.0")),
    );

    let (skills, _) = scan_roots(&[
        (String::from("generic"), first.clone()),
        (String::from("codex"), second.clone()),
    ]);

    assert_eq!(
        skills.len(),
        2,
        "two separate real directories are two skills, not one"
    );
    assert!(skills.iter().all(|skill| skill.locations.len() == 1));
}

// ---------------------------------------------------------------------------
// The contract: a link is only ever unlinked
// ---------------------------------------------------------------------------

#[cfg(unix)]
#[test]
fn removing_a_link_leaves_the_real_directory_untouched() {
    let tree = TempTree::new("unlink");
    let root = tree.at("skills");
    std::fs::create_dir_all(&root).unwrap();

    let store = tree.at("store");
    let real = store.join("kept");
    make_skill(&real, "# kept\n", Some(("global", "kept", "1.0.0")));
    std::fs::write(real.join("data.txt"), "precious").unwrap();

    let link = root.join("kept");
    symlink_dir(&real, &link);

    let before = std::fs::metadata(real.join("data.txt"))
        .unwrap()
        .modified()
        .unwrap();

    let outcome = remove_location(&link, true).unwrap();

    assert_eq!(outcome.removed_kind, LocationKind::Symlink);
    assert!(
        std::fs::symlink_metadata(&link).is_err(),
        "the link itself must be gone"
    );
    assert!(real.is_dir(), "the real directory must survive");
    assert_eq!(
        std::fs::read_to_string(real.join("data.txt")).unwrap(),
        "precious"
    );
    let after = std::fs::metadata(real.join("data.txt"))
        .unwrap()
        .modified()
        .unwrap();
    assert_eq!(
        before, after,
        "the real directory's files must be untouched"
    );
    assert!(
        outcome.real_path_kept.is_some(),
        "the user must be told where the surviving directory is"
    );
}

#[cfg(unix)]
#[test]
fn removing_a_dangling_link_only_unlinks_it() {
    let tree = TempTree::new("unlink-dangling");
    let root = tree.at("skills");
    std::fs::create_dir_all(&root).unwrap();

    let link = root.join("stale");
    symlink_dir(&tree.at("nowhere"), &link);

    let outcome = remove_location(&link, false).unwrap();

    assert_eq!(outcome.removed_kind, LocationKind::BrokenSymlink);
    assert!(std::fs::symlink_metadata(&link).is_err());
}

#[cfg(unix)]
#[test]
fn updating_through_a_link_keeps_the_link_and_updates_the_target() {
    let tree = TempTree::new("update-link");
    let root = tree.at("skills");
    std::fs::create_dir_all(&root).unwrap();

    let store = tree.at("store");
    let real = store.join("linked");
    make_skill(
        &real,
        "# version one\n",
        Some(("global", "linked", "1.0.0")),
    );

    let link = root.join("linked");
    symlink_dir(&real, &link);
    let target_before = std::fs::read_link(&link).unwrap();

    // Stand in for the freshly extracted package: a sibling temp dir holding
    // the new content, which is where install_skill would have put it.
    let staging = store.join("linked.skillhub-tmp-test");
    std::fs::create_dir_all(&staging).unwrap();
    std::fs::write(staging.join("SKILL.md"), "# version two\n").unwrap();

    let (resolved, predecessor, mut warnings) = resolve_install_target(&link).unwrap();
    assert_eq!(
        resolved,
        std::fs::canonicalize(&real).unwrap(),
        "writes must land on the link's target"
    );
    assert_eq!(predecessor, Predecessor::Directory);
    assert!(
        std::fs::symlink_metadata(&link).is_ok(),
        "resolving must not touch the link — it runs before the download"
    );

    swap_into_place(&resolved, &staging, &predecessor, false, &mut warnings).unwrap();
    write_metadata(
        &resolved,
        &InstalledMetadata::new("https://skill.example.com", "global", "linked", "2.0.0"),
    )
    .unwrap();

    let link_meta = std::fs::symlink_metadata(&link).unwrap();
    assert!(
        link_meta.file_type().is_symlink(),
        "the link must still be a link, not a real directory"
    );
    assert_eq!(
        std::fs::read_link(&link).unwrap(),
        target_before,
        "the link target must not have been rewritten"
    );
    assert_eq!(
        std::fs::read_to_string(real.join("SKILL.md")).unwrap(),
        "# version two\n"
    );
    assert_eq!(read_metadata(&real).unwrap().unwrap().version, "2.0.0");
}

#[cfg(unix)]
#[test]
fn installing_over_a_dangling_link_replaces_the_link() {
    let tree = TempTree::new("install-dangling");
    let link = tree.at("skills").join("stale");
    std::fs::create_dir_all(link.parent().unwrap()).unwrap();
    symlink_dir(&tree.at("nowhere"), &link);

    let (resolved, predecessor, mut warnings) = resolve_install_target(&link).unwrap();

    assert_eq!(resolved, link, "there is no target, so we install in place");
    assert_eq!(predecessor, Predecessor::StaleLink);
    // Resolution is side-effect free: clearing happens only after the download
    // succeeds, so a failed download cannot cost the user this entry.
    assert!(
        std::fs::symlink_metadata(&link).is_ok(),
        "resolving must not clear anything"
    );

    let staging = tree.at("staging");
    std::fs::create_dir_all(&staging).unwrap();
    std::fs::write(staging.join("SKILL.md"), "# fresh\n").unwrap();

    swap_into_place(&resolved, &staging, &predecessor, false, &mut warnings).unwrap();

    assert!(
        std::fs::symlink_metadata(&link).is_ok(),
        "a real directory now sits where the dangling link was"
    );
    assert!(!std::fs::symlink_metadata(&link)
        .unwrap()
        .file_type()
        .is_symlink());
    assert!(link.join("SKILL.md").is_file());
    assert!(
        warnings.iter().any(|warning| warning.contains("失效")),
        "the user should be told the link was dropped: {warnings:?}"
    );
}

#[cfg(unix)]
#[test]
fn installing_over_a_plain_file_backs_it_up_instead_of_deleting_it() {
    let tree = TempTree::new("install-file");
    let target = tree.at("skills").join("weather");
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    std::fs::write(&target, "notes the user wrote").unwrap();

    let (_, predecessor, mut warnings) = resolve_install_target(&target).unwrap();
    assert_eq!(
        predecessor,
        Predecessor::File,
        "a plain file is not a link and must not take the unlink path"
    );

    let staging = tree.at("staging");
    std::fs::create_dir_all(&staging).unwrap();
    std::fs::write(staging.join("SKILL.md"), "# skill\n").unwrap();

    swap_into_place(&target, &staging, &predecessor, false, &mut warnings).unwrap();

    assert!(target.is_dir(), "the skill is installed as a directory");
    assert!(
        warnings.iter().any(|warning| warning.contains("文件")),
        "the user should be told their file was moved aside: {warnings:?}"
    );

    // The file survived, renamed to a sibling backup.
    let backup = std::fs::read_dir(target.parent().unwrap())
        .unwrap()
        .flatten()
        .map(|entry| entry.path())
        .find(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.contains(".skillhub-backup-"))
        })
        .expect("the original file must still be on disk");
    assert_eq!(
        std::fs::read_to_string(backup).unwrap(),
        "notes the user wrote"
    );
}

// ---------------------------------------------------------------------------
// Backups and temp dirs must not come back as skills
// ---------------------------------------------------------------------------

#[test]
fn scan_ignores_the_backups_and_temp_dirs_we_create() {
    let tree = TempTree::new("scan-bookkeeping");
    let root = tree.at("skills");

    make_skill(
        &root.join("real"),
        "# real\n",
        Some(("global", "real", "1.0.0")),
    );
    // Left behind by an uninstall of an unmanaged directory…
    make_skill(
        &root.join("hand-made.skillhub-backup-1757930000"),
        "# backup\n",
        None,
    );
    // …and by an install that never completed.
    make_skill(
        &root.join("half-installed.skillhub-tmp-123-456-0"),
        "# tmp\n",
        None,
    );

    let (skills, _) = scan_roots(&[(String::from("claude-code"), root.clone())]);

    assert_eq!(
        slugs_of(&skills),
        vec!["real"],
        "our own bookkeeping must not appear as skills the user can uninstall"
    );
}

// ---------------------------------------------------------------------------
// The uninstall path the command actually runs
// ---------------------------------------------------------------------------

/// Drive `uninstall_location_under` — not its helpers — because the
/// delete-versus-back-up decision lives there, and it is the only thing between
/// a user's directory and `remove_dir_all`.
#[test]
fn uninstall_backs_up_a_directory_whose_metadata_is_unreadable() {
    let tree = TempTree::new("uninstall-corrupt");
    let root = tree.at("skills");
    let dir = root.join("acme");
    std::fs::create_dir_all(dir.join(".skillhub")).unwrap();
    std::fs::write(dir.join("SKILL.md"), "# acme\n").unwrap();
    // Truncated: the scan reads this as unmanaged and the dialog promises a
    // backup, so the uninstall must not delete it.
    std::fs::write(metadata_path(&dir), r#"{"slug":"acme"}"#).unwrap();

    let roots = vec![root.clone()];

    // The scan and the uninstall agree on what "managed" means.
    let (skills, _) = scan_roots(&[(String::from("claude-code"), root.clone())]);
    assert_eq!(skills[0].origin, SkillOrigin::Unmanaged);

    let outcome = uninstall_location_under(&dir, &roots).unwrap();

    let backup = PathBuf::from(
        outcome
            .backup_dir
            .expect("an unreadable metadata.json must lead to a backup, not a delete"),
    );
    assert!(!dir.exists());
    assert!(backup.join("SKILL.md").is_file(), "the content survived");

    let _ = std::fs::remove_dir_all(&backup);
}

#[test]
fn uninstall_deletes_a_directory_whose_metadata_is_valid() {
    let tree = TempTree::new("uninstall-managed");
    let root = tree.at("skills");
    let dir = root.join("acme");
    make_skill(&dir, "# acme\n", Some(("global", "acme", "1.0.0")));

    let outcome = uninstall_location_under(&dir, &[root]).unwrap();

    assert!(outcome.backup_dir.is_none());
    assert!(!dir.exists());
}

#[cfg(unix)]
#[test]
fn uninstall_detaches_a_link_rather_than_its_target() {
    let tree = TempTree::new("uninstall-link");
    let root = tree.at("skills");
    std::fs::create_dir_all(&root).unwrap();

    let store = tree.at("store");
    let real = store.join("shared");
    make_skill(&real, "# shared\n", Some(("global", "shared", "1.0.0")));

    let link = root.join("shared");
    symlink_dir(&real, &link);

    let outcome = uninstall_location_under(&link, &[root]).unwrap();

    assert_eq!(outcome.removed_kind, LocationKind::Symlink);
    assert!(std::fs::symlink_metadata(&link).is_err());
    assert!(real.join("SKILL.md").is_file(), "the target must survive");
    // Reported canonicalized, which on macOS also rewrites the temp dir's
    // `/var` → `/private/var` alias — so compare against the canonical form.
    assert_eq!(
        outcome.real_path_kept.as_deref(),
        Some(
            std::fs::canonicalize(&real)
                .unwrap()
                .to_string_lossy()
                .to_string()
                .as_str()
        )
    );
}

#[test]
fn uninstall_refuses_a_path_with_a_trailing_separator() {
    let tree = TempTree::new("uninstall-trailing");
    let root = tree.at("skills");
    make_skill(&root.join("good"), "# good\n", None);
    let roots = vec![root.clone()];

    // `Path` erases the trailing separator, but the kernel does not: for these
    // forms it dereferences a final symlink, which would retarget a request to
    // detach a link onto the directory that link points at.
    for suffix in ["/", "/."] {
        let raw = format!("{}{}", root.join("good").display(), suffix);
        let error = uninstall_location_under(Path::new(&raw), &roots).unwrap_err();
        assert_eq!(
            error.code, "invalid_path",
            "{raw} must be refused, not resolved"
        );
    }

    assert!(root.join("good").is_dir(), "nothing may have been touched");
}

#[test]
fn uninstall_accepts_a_directory_name_that_is_not_a_valid_slug() {
    let tree = TempTree::new("uninstall-nonascii");
    let root = tree.at("skills");
    // The scan shows this entry, so uninstall has to be able to act on it; only
    // traversal-safety matters here, not slug syntax.
    make_skill(&root.join("我的 技能"), "# mine\n", None);

    let outcome = uninstall_location_under(&root.join("我的 技能"), &[root]).unwrap();

    assert!(outcome.backup_dir.is_some());
}

// ---------------------------------------------------------------------------
// The uninstall boundary
// ---------------------------------------------------------------------------

#[test]
fn ensure_under_roots_rejects_anything_outside_the_agent_roots() {
    let tree = TempTree::new("boundary");
    let root = tree.at("skills");
    make_skill(&root.join("good"), "# good\n", None);

    let roots = vec![root.clone()];

    // Inside: accepted, and normalized to an absolute path.
    let accepted = ensure_under_roots(&root.join("good"), &roots).unwrap();
    assert_eq!(accepted, std::fs::canonicalize(root.join("good")).unwrap());

    // A sibling that merely shares a name prefix.
    let sibling = tree.at("skills-evil");
    std::fs::create_dir_all(&sibling).unwrap();
    assert!(ensure_under_roots(&sibling, &roots).is_err());

    // The root itself is not a removable entry.
    assert!(ensure_under_roots(&root, &roots).is_err());

    // Traversal is normalized away rather than blocked outright: what matters
    // is where the path actually lands, and `root/../skills/good` is `root/good`.
    assert_eq!(
        ensure_under_roots(&root.join("..").join("skills").join("good"), &roots).unwrap(),
        std::fs::canonicalize(root.join("good")).unwrap()
    );

    // A parent of the root, and paths under an unrelated parent, are refused.
    assert!(ensure_under_roots(&root.join(".."), &roots).is_err());
    assert!(ensure_under_roots(&tree.at("elsewhere"), &roots).is_err());
    assert!(ensure_under_roots(&tree.at("elsewhere").join("x"), &roots).is_err());
}

#[cfg(unix)]
#[test]
fn ensure_under_roots_accepts_a_dangling_link_inside_a_root() {
    let tree = TempTree::new("boundary-link");
    let root = tree.at("skills");
    std::fs::create_dir_all(&root).unwrap();

    let link = root.join("stale");
    symlink_dir(&tree.at("nowhere"), &link);

    // `canonicalize` fails on a dangling link, so this path must be normalized
    // through its parent or a broken link could never be cleaned up.
    let accepted = ensure_under_roots(&link, std::slice::from_ref(&root)).unwrap();
    assert_eq!(
        accepted,
        std::fs::canonicalize(&root).unwrap().join("stale")
    );
}

// ---------------------------------------------------------------------------
// SKILL.md frontmatter
// ---------------------------------------------------------------------------

#[test]
fn frontmatter_reads_a_plain_homepage() {
    let tree = TempTree::new("fm-plain");
    let dir = tree.at("skill");
    make_skill(
        &dir,
        "---\nname: my-skill\nhomepage: https://github.com/team/repo\n---\n\n# My Skill\n",
        None,
    );

    assert_eq!(
        read_frontmatter_value(&dir, "homepage").as_deref(),
        Some("https://github.com/team/repo")
    );
}

#[test]
fn frontmatter_handles_quotes_comments_and_crlf() {
    let tree = TempTree::new("fm-quotes");

    let quoted = tree.at("quoted");
    make_skill(
        &quoted,
        "---\nhomepage: \"https://example.com/a\"\n---\n",
        None,
    );
    assert_eq!(
        read_frontmatter_value(&quoted, "homepage").as_deref(),
        Some("https://example.com/a")
    );

    let single = tree.at("single");
    make_skill(
        &single,
        "---\nhomepage: 'https://example.com/b'\n---\n",
        None,
    );
    assert_eq!(
        read_frontmatter_value(&single, "homepage").as_deref(),
        Some("https://example.com/b")
    );

    let commented = tree.at("commented");
    make_skill(
        &commented,
        "---\nhomepage: https://example.com/c # 源仓库\n---\n",
        None,
    );
    assert_eq!(
        read_frontmatter_value(&commented, "homepage").as_deref(),
        Some("https://example.com/c")
    );

    // A `#` that is not preceded by whitespace is part of the URL, not a comment.
    let fragment = tree.at("fragment");
    make_skill(
        &fragment,
        "---\nhomepage: https://example.com/d#readme\n---\n",
        None,
    );
    assert_eq!(
        read_frontmatter_value(&fragment, "homepage").as_deref(),
        Some("https://example.com/d#readme")
    );

    let crlf = tree.at("crlf");
    make_skill(
        &crlf,
        "---\r\nhomepage: https://example.com/e\r\n---\r\n",
        None,
    );
    assert_eq!(
        read_frontmatter_value(&crlf, "homepage").as_deref(),
        Some("https://example.com/e")
    );
}

#[test]
fn frontmatter_returns_none_when_there_is_nothing_to_read() {
    let tree = TempTree::new("fm-none");

    // No frontmatter block at all.
    let plain = tree.at("plain");
    make_skill(
        &plain,
        "# Just a title\n\nhomepage: https://example.com/x\n",
        None,
    );
    assert!(read_frontmatter_value(&plain, "homepage").is_none());

    // Key absent from an otherwise valid block.
    let absent = tree.at("absent");
    make_skill(&absent, "---\nname: x\n---\n", None);
    assert!(read_frontmatter_value(&absent, "homepage").is_none());

    // Empty value.
    let empty = tree.at("empty");
    make_skill(&empty, "---\nhomepage:\n---\n", None);
    assert!(read_frontmatter_value(&empty, "homepage").is_none());

    // The key only appears after the block closes.
    let after = tree.at("after");
    make_skill(
        &after,
        "---\nname: x\n---\nhomepage: https://example.com/y\n",
        None,
    );
    assert!(read_frontmatter_value(&after, "homepage").is_none());

    // A link target that merely starts with the key name must not match.
    let similar = tree.at("similar");
    make_skill(
        &similar,
        "---\nhomepage_alt: https://example.com/z\n---\n",
        None,
    );
    assert!(read_frontmatter_value(&similar, "homepage").is_none());

    // Missing SKILL.md entirely.
    let missing = tree.at("missing");
    std::fs::create_dir_all(&missing).unwrap();
    assert!(read_frontmatter_value(&missing, "homepage").is_none());
}

#[test]
fn frontmatter_ignores_a_nested_key() {
    let tree = TempTree::new("fm-nested");
    let dir = tree.at("skill");
    make_skill(
        &dir,
        "---\nmetadata:\n  homepage: https://example.com/nested\n---\n",
        None,
    );

    assert!(read_frontmatter_value(&dir, "homepage").is_none());
}

#[test]
fn frontmatter_reads_only_the_head_of_a_large_file() {
    let tree = TempTree::new("fm-large");
    let dir = tree.at("skill");
    std::fs::create_dir_all(&dir).unwrap();

    // Frontmatter present, followed by far more than the bounded read window.
    let mut content = String::from("---\nhomepage: https://example.com/big\n---\n\n");
    content.push_str(&"x".repeat(64 * 1024));
    std::fs::write(dir.join("SKILL.md"), content).unwrap();

    assert_eq!(
        read_frontmatter_value(&dir, "homepage").as_deref(),
        Some("https://example.com/big")
    );
}

#[test]
fn frontmatter_decodes_invalid_utf8_lossily_rather_than_failing() {
    let tree = TempTree::new("fm-utf8");

    // Frontmatter is clean; the invalid bytes sit further down a file we only
    // read the head of. Decoding lossily keeps the parse working — returning
    // `None` here would hide a link that is genuinely declared.
    let trailing = tree.at("trailing");
    std::fs::create_dir_all(&trailing).unwrap();
    let mut bytes = b"---\nhomepage: https://example.com/utf8\n---\n\n".to_vec();
    bytes.extend_from_slice(&[0xff, 0xfe, 0x80, 0x00, 0xc3]);
    std::fs::write(trailing.join("SKILL.md"), &bytes).unwrap();

    assert_eq!(
        read_frontmatter_value(&trailing, "homepage").as_deref(),
        Some("https://example.com/utf8")
    );

    // Invalid bytes in the first line mean there is no frontmatter block at all.
    let leading = tree.at("leading");
    std::fs::create_dir_all(&leading).unwrap();
    std::fs::write(leading.join("SKILL.md"), [0xff, 0xfe, 0x00, 0x01]).unwrap();

    assert!(read_frontmatter_value(&leading, "homepage").is_none());
}

#[cfg(unix)]
#[test]
fn scan_reports_a_root_it_cannot_read_without_aborting() {
    use std::os::unix::fs::PermissionsExt;

    let tree = TempTree::new("scan-unreadable");
    let locked = tree.at("locked");
    make_skill(&locked.join("inside"), "# inside\n", None);

    let good = tree.at("good");
    make_skill(
        &good.join("visible"),
        "# visible\n",
        Some(("global", "visible", "1.0.0")),
    );

    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();

    // A test process running as root ignores the mode bits, which would make
    // this pass vacuously — check before asserting anything about the warning.
    let unreadable = std::fs::read_dir(&locked).is_err();

    let (skills, warnings) = scan_roots(&[
        (String::from("locked-agent"), locked.clone()),
        (String::from("good-agent"), good.clone()),
    ]);

    // Whatever the permissions allowed, the readable root must still be scanned:
    // one bad root never aborts the whole scan.
    assert_eq!(slugs_of(&skills), vec!["visible"]);

    if unreadable {
        assert_eq!(warnings.len(), 1, "warnings: {warnings:?}");
        assert!(
            warnings[0].contains("locked"),
            "the warning must name the root that failed: {warnings:?}"
        );
    } else {
        // Mode bits are not enforced for this user (running as root), so the
        // branch under test is unreachable here. Say so rather than letting the
        // test pass as a silent no-op.
        eprintln!("skipped warning assertion: chmod 000 did not deny read_dir");
    }

    // Restore so TempTree::drop can clean up.
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
}

#[test]
fn scan_prefers_the_authors_homepage_over_derived_ones() {
    let tree = TempTree::new("fm-scan");
    let root = tree.at("skills");
    make_skill(
        &root.join("my-skill"),
        "---\nhomepage: https://github.com/team/repo\n---\n",
        Some(("global", "my-skill", "1.0.0")),
    );

    let (skills, _) = scan_roots(&[(String::from("claude-code"), root.clone())]);

    assert_eq!(
        skills[0].homepage.as_deref(),
        Some("https://github.com/team/repo")
    );
    assert_eq!(skills[0].homepage_source, Some(HomepageSource::Frontmatter));
}

// ---------------------------------------------------------------------------
// Homepage resolution and the URL boundary
// ---------------------------------------------------------------------------

#[test]
fn homepage_falls_back_from_frontmatter_to_metadata_to_registry() {
    let metadata = managed_metadata();

    // 1. The author's own homepage wins.
    let (url, source) = resolve_homepage(
        Some("https://github.com/team/repo".to_string()),
        Some(&metadata),
    );
    assert_eq!(url.as_deref(), Some("https://github.com/team/repo"));
    assert_eq!(source, Some(HomepageSource::Frontmatter));

    // 2. An address recorded in metadata is next.
    let mut recorded = managed_metadata();
    recorded.homepage = Some("https://example.com/recorded".to_string());
    let (url, source) = resolve_homepage(None, Some(&recorded));
    assert_eq!(url.as_deref(), Some("https://example.com/recorded"));
    assert_eq!(source, Some(HomepageSource::Metadata));

    // 3. Otherwise the skill's own page on the registry.
    let (url, source) = resolve_homepage(None, Some(&metadata));
    assert_eq!(
        url.as_deref(),
        Some("https://skill.example.com/space/global/my-skill")
    );
    assert_eq!(source, Some(HomepageSource::Registry));

    // A frontmatter value that is not a usable http(s) URL is skipped rather
    // than trusted, so it falls through to the safe fallback.
    let (url, source) = resolve_homepage(Some("javascript:alert(1)".to_string()), Some(&metadata));
    assert_eq!(
        url.as_deref(),
        Some("https://skill.example.com/space/global/my-skill")
    );
    assert_eq!(source, Some(HomepageSource::Registry));

    // Nothing to go on at all.
    let (url, source) = resolve_homepage(None, None);
    assert!(url.is_none());
    assert!(source.is_none());
}

#[test]
fn validate_external_url_allows_http_and_https_only() {
    assert!(validate_external_url("https://example.com/a/b").is_ok());
    assert!(validate_external_url("http://example.com").is_ok());
    assert!(validate_external_url("HTTPS://EXAMPLE.COM").is_ok());
    assert!(validate_external_url("https://example.com/path?a=1&b=2").is_ok());

    assert!(validate_external_url("javascript:alert(1)").is_err());
    assert!(validate_external_url("data:text/html;base64,PHNjcmlwdD4=").is_err());
    assert!(validate_external_url("file:///etc/passwd").is_err());
    assert!(validate_external_url("vbscript:msgbox(1)").is_err());
    assert!(validate_external_url("/etc/passwd").is_err());
    assert!(validate_external_url("").is_err());
    assert!(validate_external_url("   ").is_err());

    // Control characters and whitespace would confuse the shell that opens it.
    assert!(validate_external_url("https://example.com/\nrm -rf /").is_err());
    assert!(validate_external_url("https://example.com/\0").is_err());
    assert!(validate_external_url("https://exa mple.com").is_err());

    // A bare scheme with no host cannot be opened.
    assert!(validate_external_url("https://").is_err());
    assert!(validate_external_url("https:///path").is_err());

    // Unreasonably long values are refused.
    let long = format!("https://example.com/{}", "a".repeat(4096));
    assert!(validate_external_url(&long).is_err());
}

// ---------------------------------------------------------------------------
// Metadata shared with the CLI
// ---------------------------------------------------------------------------

#[test]
fn scan_treats_unparseable_metadata_as_unmanaged() {
    let tree = TempTree::new("meta-broken");
    let root = tree.at("skills");
    let dir = root.join("broken");
    std::fs::create_dir_all(dir.join(".skillhub")).unwrap();
    std::fs::write(dir.join("SKILL.md"), "# broken\n").unwrap();
    std::fs::write(metadata_path(&dir), "{ this is not json").unwrap();

    let (skills, _) = scan_roots(&[(String::from("claude-code"), root.clone())]);

    assert_eq!(skills.len(), 1);
    assert_eq!(
        skills[0].origin,
        SkillOrigin::Unmanaged,
        "metadata we cannot read must never mean 'ours to delete'"
    );
}

#[test]
fn scan_flags_a_slug_that_disagrees_with_the_directory_name() {
    let tree = TempTree::new("meta-slug");
    let root = tree.at("skills");
    make_skill(
        &root.join("renamed-by-user"),
        "# renamed\n",
        Some(("global", "original-name", "1.0.0")),
    );

    let (skills, _) = scan_roots(&[(String::from("claude-code"), root.clone())]);

    assert_eq!(skills[0].slug, "renamed-by-user");
    assert_eq!(skills[0].metadata_slug.as_deref(), Some("original-name"));
}

#[test]
fn classify_distinguishes_a_directory_from_a_link() {
    let tree = TempTree::new("classify");
    let dir = tree.at("a-dir");
    std::fs::create_dir_all(&dir).unwrap();
    assert_eq!(classify(&dir).unwrap(), LocationKind::Dir);

    // A path that does not exist at all is an error, not a classification.
    assert!(classify(&tree.at("nope")).is_err());
}

// ---------------------------------------------------------------------------
// Manual check against the developer's real skills roots
// ---------------------------------------------------------------------------

/// Scan the actual home directory and print what was found.
///
/// Not part of the default suite — it reads and never writes, but it depends on
/// whatever the developer happens to have installed. Run it by hand to confirm
/// the scan against real-world links (absolute, relative, and pointing outside
/// the agent roots), which is the one thing a temp tree cannot reproduce:
///
/// ```text
/// cargo test -- --ignored --nocapture scan_real_home_roots
/// ```
#[test]
#[ignore = "reads the real home directory; run manually"]
fn scan_real_home_roots() {
    let (skills, warnings) = crate::installer::local_skills::scan_local_skills();

    for warning in &warnings {
        println!("warning: {warning}");
    }
    println!("{} skills", skills.len());

    let mut link_count = 0;
    for skill in &skills {
        for location in &skill.locations {
            if location.kind.is_link() {
                link_count += 1;
                println!(
                    "  link  {:?} {} -> {}",
                    location.kind,
                    location.path,
                    location.target.as_deref().unwrap_or("<none>")
                );
            }
        }
        println!(
            "  {:?} {} v{} locations={} homepage={}",
            skill.origin,
            skill.slug,
            skill.version.as_deref().unwrap_or("-"),
            skill.locations.len(),
            skill.homepage.as_deref().unwrap_or("-"),
        );
    }

    // Aggregation means shared directories outnumber the cards that show them.
    let shared = skills.iter().filter(|s| s.locations.len() > 1).count();
    println!("{link_count} link locations across {shared} shared skills");

    // Every aggregated skill must agree with its own locations.
    for skill in &skills {
        assert!(
            !skill.locations.is_empty(),
            "{} has no locations",
            skill.slug
        );
        if let Some(real) = &skill.real_path {
            assert!(
                Path::new(real).is_dir(),
                "{} reports a real_path that is not a directory: {real}",
                skill.slug
            );
        }
    }
}

// ---------------------------------------------------------------------------
// The JSON shape the web view consumes
// ---------------------------------------------------------------------------

#[test]
fn local_skill_serializes_to_the_web_contract() {
    let tree = TempTree::new("wire");
    let root = tree.at("skills");
    make_skill(
        &root.join("renamed"),
        "---\nhomepage: https://github.com/team/repo\n---\n",
        Some(("global", "original", "1.0.0")),
    );

    let (skills, _) = scan_roots(&[(String::from("claude-code"), root.clone())]);
    let json = serde_json::to_value(&skills[0]).unwrap();

    // Struct fields cross into TypeScript as camelCase, enum values stay
    // kebab-case strings. Changing either side silently breaks the page.
    assert_eq!(json["metadataSlug"], "original");
    assert_eq!(json["homepageSource"], "frontmatter");
    assert_eq!(json["origin"], "managed");
    assert_eq!(json["locations"][0]["kind"], "dir");
    assert!(json["realPath"].is_string());
    assert!(
        json.get("metadata_slug").is_none(),
        "snake_case keys must not leak to the web view"
    );
}
