//! Tests for the install-mode split: the serde contract with the web view, the
//! shared-repository path, and — most importantly — [`create_link`], which is the
//! one new operation that writes to an agent's skills root.
//!
//! `create_link` cases are Unix-only: they need `std::os::unix::fs::symlink`, and
//! on Windows creating one requires elevation.

use std::path::PathBuf;

use crate::installer::agents::{home_dir, repo_root, repo_skill_dir};
use crate::installer::install::{should_share_to_repo, InstallInput, InstallMode};
use crate::installer::link::create_link;
use crate::installer::test_support::{make_skill_dir, TempTree};

#[cfg(test)]
mod tests {
    use super::*;

    // ---------------------------------------------------------------- serde

    #[test]
    fn install_mode_defaults_to_shared_when_the_field_is_absent() {
        // An older web view does not send `installMode`. Serde must fall back to
        // Shared rather than failing the whole install.
        let json = r#"{
            "namespace": "global",
            "slug": "demo",
            "version": "1.0.0",
            "agent": "claude-code"
        }"#;

        let input: InstallInput = serde_json::from_str(json).unwrap();

        assert_eq!(input.install_mode, InstallMode::Shared);
        assert!(!input.preserve_existing);
        assert!(input.dir.is_none());
    }

    #[test]
    fn install_mode_parses_both_wire_values() {
        let build = |mode: &str| {
            format!(
                r#"{{"namespace":"global","slug":"demo","version":"1.0.0",
                    "agent":"claude-code","installMode":"{mode}"}}"#
            )
        };

        let shared: InstallInput = serde_json::from_str(&build("shared")).unwrap();
        let copy: InstallInput = serde_json::from_str(&build("copy")).unwrap();

        assert_eq!(shared.install_mode, InstallMode::Shared);
        assert_eq!(copy.install_mode, InstallMode::Copy);
    }

    #[test]
    fn install_mode_rejects_an_unknown_wire_value() {
        // Guards the wire contract: a typo on the TS side must not silently
        // become `Copy` (the old behaviour) via a permissive fallback.
        let json = r#"{"namespace":"global","slug":"demo","version":"1.0.0",
                       "agent":"claude-code","installMode":"symlink"}"#;

        assert!(serde_json::from_str::<InstallInput>(json).is_err());
    }

    // ----------------------------------------------------------- repo paths

    #[test]
    fn repo_skill_dir_lives_under_the_shared_repository() {
        let dir = repo_skill_dir("my-skill");

        assert!(dir.is_absolute(), "repo path must be absolute: {dir:?}");
        assert!(
            dir.ends_with(PathBuf::from(".skillhub").join("skills").join("my-skill")),
            "unexpected repo path: {dir:?}"
        );
        assert_eq!(dir.parent().unwrap(), repo_root());
    }

    #[test]
    fn repo_paths_do_not_collide_with_the_cli_workspace_file() {
        // The CLI keeps its workspace at `~/.skillhub/namespace-sync.json`. The
        // shared repository is one level deeper, so writing skills cannot
        // overwrite it — and the file must not sit inside the repo root, or the
        // scan would later trip over it.
        let workspace_file = home_dir().join(".skillhub").join("namespace-sync.json");

        assert!(!workspace_file.starts_with(repo_root()));
        assert_eq!(repo_root().parent().unwrap(), home_dir().join(".skillhub"));
    }

    // -------------------------------------------------- share decision rule

    #[test]
    fn shared_applies_only_to_a_fresh_install() {
        assert!(should_share_to_repo(InstallMode::Shared, false));
    }

    #[test]
    fn an_explicit_dir_opts_out_of_sharing_even_in_shared_mode() {
        // An update must keep writing through the existing entry, so the shared
        // branch is skipped and the existing link contract is preserved.
        assert!(!should_share_to_repo(InstallMode::Shared, true));
    }

    #[test]
    fn copy_mode_never_shares() {
        assert!(!should_share_to_repo(InstallMode::Copy, false));
        assert!(!should_share_to_repo(InstallMode::Copy, true));
    }

    // ----------------------------------------------------------- create_link

    #[cfg(unix)]
    #[test]
    fn create_link_points_the_entry_at_the_real_directory() {
        let tree = TempTree::new("create");
        let source = tree.path("repo/demo");
        let target = tree.path("agent/skills/demo");
        make_skill_dir(&source);

        let outcome = create_link(&target, &source).unwrap();

        assert!(outcome.created);
        assert!(!outcome.already_linked);
        assert!(outcome.warning.is_none());
        // The entry is a link, and it resolves to the shared directory.
        assert!(std::fs::symlink_metadata(&target)
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(
            std::fs::canonicalize(&target).unwrap(),
            std::fs::canonicalize(&source).unwrap()
        );
    }

    #[cfg(unix)]
    #[test]
    fn create_link_makes_missing_parent_directories() {
        // The agent may never have been used on this machine, so its skills root
        // does not exist yet.
        let tree = TempTree::new("parents");
        let source = tree.path("repo/demo");
        let target = tree.path("never-used/.claude/skills/demo");
        make_skill_dir(&source);

        assert!(create_link(&target, &source).unwrap().created);
        assert!(std::fs::canonicalize(&target).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn create_link_is_idempotent_for_the_same_source() {
        let tree = TempTree::new("idempotent");
        let source = tree.path("repo/demo");
        let target = tree.path("agent/skills/demo");
        make_skill_dir(&source);

        assert!(create_link(&target, &source).unwrap().created);

        let second = create_link(&target, &source).unwrap();
        assert!(!second.created);
        assert!(second.already_linked);
        assert!(second.warning.is_none());
    }

    #[cfg(unix)]
    #[test]
    fn create_link_treats_a_differently_spelled_target_as_the_same_link() {
        // Canonicalization is what makes this work: the same directory reached
        // via a differently-spelled path (here, a symlinked parent) must still
        // count as "already linked", not as a foreign link.
        let tree = TempTree::new("respell");
        let source = tree.path("repo/demo");
        make_skill_dir(&source);
        let alias = tree.path("alias-to-repo");
        std::os::unix::fs::symlink(tree.path("repo"), &alias).unwrap();

        let target = tree.path("agent/skills/demo");
        assert!(create_link(&target, &source).unwrap().created);

        let second = create_link(&target, &alias.join("demo")).unwrap();
        assert!(second.already_linked, "expected the alias to compare equal");
        assert!(!second.created);
    }

    #[cfg(unix)]
    #[test]
    fn create_link_refuses_to_displace_a_real_directory() {
        let tree = TempTree::new("occupied-dir");
        let source = tree.path("repo/demo");
        let target = tree.path("agent/skills/demo");
        make_skill_dir(&source);
        make_skill_dir(&target);
        std::fs::write(target.join("SKILL.md"), "user content").unwrap();

        let outcome = create_link(&target, &source).unwrap();

        assert!(!outcome.created);
        assert!(!outcome.already_linked);
        assert!(outcome.warning.is_some());
        // The user's directory is untouched — not linked over, not deleted.
        assert!(!std::fs::symlink_metadata(&target)
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(
            std::fs::read_to_string(target.join("SKILL.md")).unwrap(),
            "user content"
        );
    }

    #[cfg(unix)]
    #[test]
    fn create_link_refuses_to_displace_a_foreign_link() {
        // A link pointing somewhere else (e.g. ~/.cc-switch/skills) is the user's
        // arrangement and must not be retargeted.
        let tree = TempTree::new("occupied-link");
        let source = tree.path("repo/demo");
        let elsewhere = tree.path("third-party/demo");
        make_skill_dir(&source);
        make_skill_dir(&elsewhere);
        let target = tree.path("agent/skills/demo");
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&elsewhere, &target).unwrap();

        let outcome = create_link(&target, &source).unwrap();

        assert!(!outcome.created);
        assert!(outcome.warning.is_some());
        assert_eq!(
            std::fs::canonicalize(&target).unwrap(),
            std::fs::canonicalize(&elsewhere).unwrap(),
            "the foreign link must still point where it did"
        );
    }

    #[cfg(unix)]
    #[test]
    fn create_link_rejects_a_source_that_is_not_a_directory() {
        let tree = TempTree::new("bad-source");
        let source = tree.path("repo/demo");
        std::fs::create_dir_all(source.parent().unwrap()).unwrap();
        std::fs::write(&source, "not a directory").unwrap();

        let err = create_link(&tree.path("agent/skills/demo"), &source).unwrap_err();
        assert_eq!(err.code, "not_a_directory");
    }

    #[cfg(unix)]
    #[test]
    fn create_link_rejects_a_source_that_is_itself_a_link() {
        // Chaining a link to a link would make the shared skill depend on the
        // intermediate link's lifetime.
        let tree = TempTree::new("link-source");
        let real = tree.path("repo/demo");
        make_skill_dir(&real);
        let link_source = tree.path("repo/demo-alias");
        std::os::unix::fs::symlink(&real, &link_source).unwrap();

        let err = create_link(&tree.path("agent/skills/demo"), &link_source).unwrap_err();
        assert_eq!(err.code, "not_a_directory");
    }
}
