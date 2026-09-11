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
}
