use std::path::PathBuf;

use crate::installer::agents::home_dir;
use crate::installer::agents::{
    find_agent, is_installed, profile_root, resolve_agent_targets, validate_slug, AGENT_PROFILES,
};
use crate::installer::install::{build_download_url, extract_zip, InstallInput};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_agent_profiles_and_targets() {
        // Known agents are present.
        assert!(find_agent("claude-code").is_some());
        assert!(find_agent("generic").is_some());
        assert!(find_agent("opencode").is_some());

        // Unknown agent is rejected.
        assert!(find_agent("nope").is_none());

        // resolve_agent_targets yields one target per profile.
        let targets = resolve_agent_targets();
        assert_eq!(targets.len(), AGENT_PROFILES.len());
        assert!(targets.iter().any(|t| t.id == "claude-code"));
    }

    #[test]
    fn profile_root_is_absolute_and_under_home() {
        let profile = find_agent("claude-code").unwrap();
        let root = profile_root(profile);
        assert!(root.is_absolute(), "root must be absolute: {root:?}");

        // Expected sub-path on both macOS and Windows home layouts.
        let expected_suffix = PathBuf::from(".claude").join("skills");
        assert!(
            root.ends_with(&expected_suffix),
            "unexpected root: {root:?}"
        );
    }

    #[test]
    fn is_installed_reflects_directory_existence() {
        // Current home generally does NOT have `.claude/skills` for a fresh env,
        // but this is environment-dependent. Just confirm it returns a bool and
        // matches directory existence.
        let profile = find_agent("claude-code").unwrap();
        let expected = profile_root(profile).exists();
        assert_eq!(is_installed(profile), expected);
    }

    #[test]
    fn slug_validation_blocks_traversal() {
        assert!(validate_slug("my-skill"));
        assert!(validate_slug("meeting-minutes-generator"));
        assert!(!validate_slug(""));
        assert!(!validate_slug("../escape"));
        assert!(!validate_slug(".hidden"));
        assert!(!validate_slug("a/../../b"));
        assert!(!validate_slug("has space"));
        // Long slugs rejected.
        assert!(!validate_slug(&"a".repeat(300)));
    }

    #[test]
    fn download_url_uses_cli_v1_shape() {
        let url = build_download_url(
            "https://skill.example.com",
            "team-alpha",
            "my-skill",
            "1.2.3",
        );
        assert_eq!(
            url,
            "https://skill.example.com/api/cli/v1/skills/team-alpha/my-skill/versions/1.2.3/download"
        );
        // Trailing slash on registry is normalised.
        let url2 = build_download_url("https://skill.example.com/", "global", "s", "1.0.0");
        assert_eq!(
            url2,
            "https://skill.example.com/api/cli/v1/skills/global/s/versions/1.0.0/download"
        );
    }

    #[test]
    fn install_input_deserializes_from_web_payload() {
        let json =
            r#"{"namespace":"global","slug":"my-skill","version":"1.0.0","agent":"claude-code"}"#;
        let input: InstallInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.namespace, "global");
        assert_eq!(input.slug, "my-skill");
        assert_eq!(input.agent, "claude-code");
        assert!(input.dir.is_none());
    }

    #[test]
    fn extract_zip_unzips_entries() {
        // Build a tiny zip in memory with one file and one nested dir.
        let mut buf = Vec::new();
        {
            let mut writer = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            let opts = zip::write::SimpleFileOptions::default();
            writer.start_file("SKILL.md", opts).unwrap();
            use std::io::Write;
            writer.write_all(b"# hello skill\n").unwrap();
            writer.add_directory("references/", opts).unwrap();
            writer.finish().unwrap();
        }

        let dest = std::env::temp_dir().join(format!("skillhub-zip-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dest);
        std::fs::create_dir_all(&dest).unwrap();

        extract_zip(std::io::Cursor::new(buf), &dest).expect("extract should succeed");

        assert!(dest.join("SKILL.md").exists());
        assert!(dest.join("references").is_dir());
        let content = std::fs::read_to_string(dest.join("SKILL.md")).unwrap();
        assert!(content.contains("hello skill"));

        let _ = std::fs::remove_dir_all(&dest);
    }

    #[test]
    fn extract_zip_rejects_zip_slip() {
        // A zip entry via `../` must not escape destination.
        // (Construct via a raw string entry path; the enclosed_name() guard.)
        let mut buf = Vec::new();
        {
            let mut writer = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            // Use a direct raw entry to emulate a malicious path.
            let opts = zip::write::SimpleFileOptions::default();
            writer.add_directory("../escape", opts).unwrap();
            writer.finish().unwrap();
        }
        let dest = std::env::temp_dir().join(format!("skillhub-zip-slip-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dest);
        std::fs::create_dir_all(&dest).unwrap();
        assert!(extract_zip(std::io::Cursor::new(buf), &dest).is_err());
        let _ = std::fs::remove_dir_all(&dest);
    }

    #[test]
    fn home_dir_returns_real_dir() {
        let home = home_dir();
        assert!(!home.as_os_str().is_empty());
    }

    #[test]
    fn metadata_write_and_read_roundtrip() {
        let dir = std::env::temp_dir().join(format!("skillhub-meta-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let meta = crate::installer::metadata::InstalledMetadata::new(
            "https://skill.example.com",
            "global",
            "my-skill",
            "1.2.3",
        );
        crate::installer::metadata::write_metadata(&dir, &meta).unwrap();
        assert!(crate::installer::metadata::metadata_path(&dir).exists());

        let status = crate::installer::metadata::detect_status(
            &dir,
            "claude-code",
            "my-skill",
            Some("1.2.3"),
        )
        .unwrap();
        assert!(status.installed);
        assert_eq!(status.version, "1.2.3");
        assert!(!status.outdated);

        // A different requested version marks it outdated.
        let status2 = crate::installer::metadata::detect_status(
            &dir,
            "claude-code",
            "my-skill",
            Some("2.0.0"),
        )
        .unwrap();
        assert!(status2.installed);
        assert!(status2.outdated);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn detect_status_reports_not_installed_without_metadata() {
        let dir = std::env::temp_dir().join(format!("skillhub-meta-none-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let status =
            crate::installer::metadata::detect_status(&dir, "codex", "s", Some("1.0.0")).unwrap();
        assert!(!status.installed);
        assert!(status.version.is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn uninstall_dir_backs_up_a_manual_skill() {
        let dir =
            std::env::temp_dir().join(format!("skillhub-uninstall-manual-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("references")).unwrap();
        std::fs::write(dir.join("SKILL.md"), "# manual skill").unwrap();

        // A dir without metadata (manual skill) is backed up, not deleted.
        let outcome = crate::installer::metadata::uninstall_dir(&dir).unwrap();
        let backup = PathBuf::from(outcome.backup_dir.unwrap());
        assert!(!dir.exists(), "original dir should be renamed away");
        assert!(backup.exists());
        assert!(backup.join("SKILL.md").exists());

        let _ = std::fs::remove_dir_all(&backup);
    }

    #[test]
    fn uninstall_dir_removes_a_skillhub_skill() {
        let dir =
            std::env::temp_dir().join(format!("skillhub-uninstall-managed-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        crate::installer::metadata::write_metadata(
            &dir,
            &crate::installer::metadata::InstalledMetadata::new(
                "https://skill.example.com",
                "global",
                "my-skill",
                "1.0.0",
            ),
        )
        .unwrap();

        // A skillhub-installed dir (has metadata) is removed directly.
        let outcome = crate::installer::metadata::uninstall_dir(&dir).unwrap();
        assert!(outcome.backup_dir.is_none());
        assert!(!dir.exists());
    }

    #[test]
    fn scan_agent_skills_lists_installed_and_skips_plain_dirs() {
        let root = std::env::temp_dir().join(format!("skillhub-scan-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);

        // A skillhub-installed skill (has metadata).
        let skill_dir = root.join("my-skill");
        std::fs::create_dir_all(&skill_dir).unwrap();
        crate::installer::metadata::write_metadata(
            &skill_dir,
            &crate::installer::metadata::InstalledMetadata::new(
                "https://skill.example.com",
                "global",
                "my-skill",
                "1.2.3",
            ),
        )
        .unwrap();

        // A plain directory without metadata must be skipped.
        let plain = root.join("not-a-skill");
        std::fs::create_dir_all(&plain).unwrap();

        let skills = crate::installer::metadata::scan_agent_skills("claude-code", &root);
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].slug, "my-skill");
        assert_eq!(skills[0].version, "1.2.3");
        assert_eq!(skills[0].agent, "claude-code");
        assert!(skills[0].dir.ends_with("my-skill"));

        let _ = std::fs::remove_dir_all(&root);
    }
}
