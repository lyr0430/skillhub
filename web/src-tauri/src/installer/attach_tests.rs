//! Tests for attaching an already-installed skill to another agent.
//!
//! The central claim under test is the validation boundary: a shared skill's real
//! directory lives at `~/.skillhub/skills/<slug>`, *outside* every agent root, so
//! the agent-root check that guards uninstall must NOT be applied to the source
//! here — it would reject exactly the skills this command exists to spread.

use crate::installer::attach::{
    attach_entry_name, attach_skill_to_agent, attach_under, copy_dir_recursive,
    resolve_attach_source,
};
use crate::installer::install::InstallMode;
use crate::installer::test_support::{make_skill_dir, TempTree};

#[cfg(test)]
mod tests {
    use super::*;

    // ------------------------------------------------------- source boundary

    #[test]
    fn accepts_a_source_outside_every_agent_root() {
        // Stands in for `~/.skillhub/skills/<slug>`: a real skill directory that
        // is deliberately not under any agent's skills root.
        let tree = TempTree::new("attach-outside");
        let source = tree.path("shared-repo/weather");
        make_skill_dir(&source);

        let resolved = resolve_attach_source(&source).unwrap();

        assert_eq!(resolved, std::fs::canonicalize(&source).unwrap());
    }

    #[test]
    fn rejects_a_source_without_a_skill_file() {
        let tree = TempTree::new("attach-noskill");
        let source = tree.path("shared-repo/not-a-skill");
        std::fs::create_dir_all(&source).unwrap();

        let err = resolve_attach_source(&source).unwrap_err();
        assert_eq!(err.code, "not_a_skill");
    }

    #[test]
    fn rejects_a_source_that_does_not_exist() {
        let tree = TempTree::new("attach-missing");

        assert!(resolve_attach_source(&tree.path("nope")).is_err());
    }

    #[test]
    fn rejects_a_source_that_is_a_file() {
        let tree = TempTree::new("attach-file");
        let source = tree.path("shared-repo/SKILL.md");
        std::fs::create_dir_all(source.parent().unwrap()).unwrap();
        std::fs::write(&source, "x").unwrap();

        assert!(resolve_attach_source(&source).is_err());
    }

    #[test]
    fn entry_name_comes_from_the_source_directory() {
        // A directory the user renamed must attach under its own name, not the
        // published slug from metadata.
        let tree = TempTree::new("attach-name");
        let source = tree.path("shared-repo/My Skill");
        make_skill_dir(&source);

        assert_eq!(attach_entry_name(&source).unwrap(), "My Skill");
    }

    // --------------------------------------------------------------- shared

    #[cfg(unix)]
    #[test]
    fn shared_attach_links_the_agent_entry_to_the_real_directory() {
        let tree = TempTree::new("attach-shared");
        let source = tree.path("shared-repo/demo");
        make_skill_dir(&source);
        let agent_root = tree.path("agent/.claude/skills");

        let result = attach_under(&source, &agent_root, "claude-code", InstallMode::Shared).unwrap();

        let target = agent_root.join("demo");
        assert!(result.ok);
        assert_eq!(result.agent, "claude-code");
        assert_eq!(result.dir, target.to_string_lossy());
        assert_eq!(
            result.real_dir,
            std::fs::canonicalize(&source).unwrap().to_string_lossy()
        );
        // The entry is a link resolving to the shared directory.
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
    fn shared_attach_is_idempotent_and_reports_it() {
        let tree = TempTree::new("attach-idem");
        let source = tree.path("shared-repo/demo");
        make_skill_dir(&source);
        let agent_root = tree.path("agent/.claude/skills");

        let first = attach_under(&source, &agent_root, "claude-code", InstallMode::Shared).unwrap();
        let second = attach_under(&source, &agent_root, "claude-code", InstallMode::Shared).unwrap();

        assert!(first.ok && second.ok);
        assert!(first.warnings.iter().any(|w| w.contains("已链接")));
        // Second run is a no-op, not an error.
        assert!(second.warnings.iter().any(|w| w.contains("未做改动")));
    }

    #[cfg(unix)]
    #[test]
    fn shared_attach_refuses_an_occupied_entry() {
        let tree = TempTree::new("attach-occupied");
        let source = tree.path("shared-repo/demo");
        make_skill_dir(&source);
        let agent_root = tree.path("agent/.claude/skills");
        // The agent already has its own real copy of the skill.
        make_skill_dir(&agent_root.join("demo"));
        std::fs::write(agent_root.join("demo/SKILL.md"), "user local content").unwrap();

        let result = attach_under(&source, &agent_root, "claude-code", InstallMode::Shared).unwrap();

        assert!(result.ok);
        assert!(result.warnings.iter().any(|w| w.contains("已被占用")));
        // Untouched: still a real directory holding the user's content.
        let target = agent_root.join("demo");
        assert!(!std::fs::symlink_metadata(&target)
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(
            std::fs::read_to_string(target.join("SKILL.md")).unwrap(),
            "user local content"
        );
    }

    #[test]
    fn copy_attach_duplicates_the_skill_into_the_agent_directory() {
        let tree = TempTree::new("attach-copy");
        let source = tree.path("shared-repo/demo");
        make_skill_dir(&source);
        std::fs::write(source.join("extra.md"), "body").unwrap();
        let agent_root = tree.path("agent/.claude/skills");

        let result = attach_under(&source, &agent_root, "claude-code", InstallMode::Copy).unwrap();

        let target = agent_root.join("demo");
        assert!(result.ok);
        // A real directory, not a link, and the source is untouched.
        assert!(!std::fs::symlink_metadata(&target)
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(std::fs::read_to_string(target.join("extra.md")).unwrap(), "body");
        assert!(source.join("extra.md").is_file());
    }

    #[test]
    fn copy_attach_refuses_an_occupied_entry() {
        let tree = TempTree::new("attach-copy-occupied");
        let source = tree.path("shared-repo/demo");
        make_skill_dir(&source);
        let agent_root = tree.path("agent/.claude/skills");
        make_skill_dir(&agent_root.join("demo"));
        std::fs::write(agent_root.join("demo/SKILL.md"), "keep me").unwrap();

        let result = attach_under(&source, &agent_root, "claude-code", InstallMode::Copy).unwrap();

        assert!(result.warnings.iter().any(|w| w.contains("已被占用")));
        assert_eq!(
            std::fs::read_to_string(agent_root.join("demo/SKILL.md")).unwrap(),
            "keep me"
        );
    }

    #[test]
    fn rejects_an_unknown_agent() {
        let tree = TempTree::new("attach-agent");
        let source = tree.path("shared-repo/demo");
        make_skill_dir(&source);

        let err = attach_skill_to_agent(&source, "not-an-agent", InstallMode::Shared).unwrap_err();
        assert_eq!(err.code, "not_found");
    }

    // ------------------------------------------------------- copy primitives

    #[test]
    fn copy_dir_recursive_copies_the_whole_tree() {
        let tree = TempTree::new("copy-tree");
        let source = tree.path("src");
        make_skill_dir(&source);
        std::fs::create_dir_all(source.join("nested/deep")).unwrap();
        std::fs::write(source.join("nested/a.txt"), "a").unwrap();
        std::fs::write(source.join("nested/deep/b.txt"), "b").unwrap();

        let target = tree.path("dst");
        copy_dir_recursive(&source, &target).unwrap();

        assert_eq!(std::fs::read_to_string(target.join("SKILL.md")).unwrap(), "---\nname: x\n---\n");
        assert_eq!(std::fs::read_to_string(target.join("nested/a.txt")).unwrap(), "a");
        assert_eq!(std::fs::read_to_string(target.join("nested/deep/b.txt")).unwrap(), "b");
    }

    #[cfg(unix)]
    #[test]
    fn copy_dir_recursive_recreates_symlinks_instead_of_following_them() {
        // Following a link would let a copy pull in content from outside the
        // skill — the same hazard the zip-slip guard exists to prevent.
        let tree = TempTree::new("copy-link");
        let outside = tree.path("outside");
        make_skill_dir(&outside);
        std::fs::write(outside.join("secret.txt"), "outside content").unwrap();

        let source = tree.path("src");
        make_skill_dir(&source);
        std::os::unix::fs::symlink(&outside, source.join("linked")).unwrap();

        let target = tree.path("dst");
        copy_dir_recursive(&source, &target).unwrap();

        let copied = target.join("linked");
        assert!(
            std::fs::symlink_metadata(&copied).unwrap().file_type().is_symlink(),
            "the link must be recreated, not dereferenced"
        );
        // No copied *files* from the outside directory exist under the target.
        assert!(!target.join("secret.txt").exists());
    }

    #[test]
    fn copy_dir_recursive_helper_is_not_confused_by_a_real_directory() {
        // Regression guard for the obvious inverse mistake: a plain directory of
        // the same name must be copied, not skipped.
        let tree = TempTree::new("copy-plain");
        let source = tree.path("src");
        make_skill_dir(&source);
        let target = tree.path("dst");

        copy_dir_recursive(&source, &target).unwrap();

        assert!(target.join("SKILL.md").is_file());
    }
}
