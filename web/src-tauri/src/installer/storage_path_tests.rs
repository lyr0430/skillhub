//! Tests for the configurable skill repository root ([`super::storage_path`]).
//!
//! Config read/write and the pre-flight rejections are cross-platform; the
//! happy-path relocation and symlink re-pointing need real symlinks, so they are
//! gated to `#[cfg(unix)]` (the same caveat as the rest of the installer's
//! symlink handling).

use std::fs;
use std::path::{Path, PathBuf};

use super::storage_path::{
    clear_skill_storage_path_at, configured_repo_root_at, migrate_repo_root,
    write_skill_storage_path_at,
};
use super::test_support::{make_skill_dir, TempTree};

// ---------------------------------------------------------------------------
// Config read/write
// ---------------------------------------------------------------------------

#[test]
fn configured_repo_root_is_none_when_unset() {
    let temp = TempTree::new("cfg-unset");
    let config = temp.path("config.json");

    assert_eq!(configured_repo_root_at(&config), None);
}

#[test]
fn configured_repo_root_roundtrips_through_write() {
    let temp = TempTree::new("cfg-roundtrip");
    let config = temp.path("config.json");
    let wanted = temp.path("/nowhere/not-yet"); // absolute, need not exist

    write_skill_storage_path_at(&config, &wanted).unwrap();
    assert_eq!(configured_repo_root_at(&config), Some(wanted));
}

#[test]
fn write_is_merge_preserving_for_unknown_fields() {
    let temp = TempTree::new("cfg-merge");
    let config = temp.path("config.json");
    fs::create_dir_all(config.parent().unwrap()).unwrap();
    fs::write(&config, r#"{ "otherField": 42 }"#).unwrap();

    write_skill_storage_path_at(&config, &temp.path("repo")).unwrap();

    let raw = fs::read_to_string(&config).unwrap();
    assert!(
        raw.contains("\"otherField\": 42"),
        "an unknown field must survive the round-trip: {raw}"
    );
    assert!(raw.contains("\"skillStoragePath\""));
}

#[test]
fn clear_returns_to_none() {
    let temp = TempTree::new("cfg-clear");
    let config = temp.path("config.json");

    write_skill_storage_path_at(&config, &temp.path("repo")).unwrap();
    assert!(configured_repo_root_at(&config).is_some());

    clear_skill_storage_path_at(&config).unwrap();
    assert_eq!(configured_repo_root_at(&config), None);
}

#[test]
fn configured_repo_root_ignores_an_empty_value() {
    let temp = TempTree::new("cfg-empty");
    let config = temp.path("config.json");
    write_skill_storage_path_at(&config, &Path::new("")).unwrap();

    assert_eq!(configured_repo_root_at(&config), None);
}

// ---------------------------------------------------------------------------
// Pre-flight rejections (cross-platform, no symlinks needed)
// ---------------------------------------------------------------------------

#[test]
fn rejects_a_missing_source() {
    let temp = TempTree::new("mig-missing");
    let from = temp.path("repo");
    let to = temp.path("dest");

    let err = migrate_repo_root(&from, &to, &[]).unwrap_err();
    assert_eq!(err.code, "not_found");
}

#[test]
fn rejects_a_relative_destination() {
    let temp = TempTree::new("mig-relative");
    let from = temp.path("repo");
    make_skill_dir(&from);
    let to = PathBuf::from("relative/dest");

    let err = migrate_repo_root(&from, &to, &[]).unwrap_err();
    assert_eq!(err.code, "invalid_path");
}

#[test]
fn rejects_same_source_and_destination() {
    let temp = TempTree::new("mig-same");
    let from = temp.path("repo");
    make_skill_dir(&from);

    let err = migrate_repo_root(&from, &from, &[]).unwrap_err();
    assert_eq!(err.code, "same_path");
}

#[test]
fn rejects_a_destination_inside_an_agent_root() {
    let temp = TempTree::new("mig-agent");
    let from = temp.path("repo");
    make_skill_dir(&from);
    let agent_root = temp.path("agent");
    make_skill_dir(&agent_root);
    let to = agent_root.join("nested");
    let roots = vec![("claude-code".to_string(), agent_root)];

    let err = migrate_repo_root(&from, &to, &roots).unwrap_err();
    assert_eq!(err.code, "inside_agent_root");
}

#[test]
fn rejects_a_destination_nested_inside_the_source() {
    let temp = TempTree::new("mig-nested");
    let from = temp.path("repo");
    make_skill_dir(&from);
    let to = from.join("nested");

    let err = migrate_repo_root(&from, &to, &[]).unwrap_err();
    assert_eq!(err.code, "inside_source");
}

#[test]
fn rejects_a_non_empty_destination() {
    let temp = TempTree::new("mig-nonempty");
    let from = temp.path("repo");
    make_skill_dir(&from);
    let to = temp.path("dest");
    make_skill_dir(&to);

    let err = migrate_repo_root(&from, &to, &[]).unwrap_err();
    assert_eq!(err.code, "target_not_empty");
}

// ---------------------------------------------------------------------------
// Relocation + symlink re-pointing (needs real symlinks)
// ---------------------------------------------------------------------------

#[cfg(unix)]
#[test]
fn moves_the_whole_root_and_repoints_every_link() {
    use std::os::unix::fs::symlink;

    let temp = TempTree::new("mig-ok");
    let from = temp.path("repo");
    let to = temp.path("dest");
    let agent_root = temp.path("agent");
    fs::create_dir_all(&agent_root).unwrap();
    make_skill_dir(&from.join("alpha"));
    make_skill_dir(&from.join("beta"));
    symlink(&from.join("alpha"), &agent_root.join("alpha")).unwrap();
    symlink(&from.join("beta"), &agent_root.join("beta")).unwrap();

    let roots = vec![("claude-code".to_string(), agent_root.clone())];
    let result = migrate_repo_root(&from, &to, &roots).unwrap();

    // The whole root moved: both skills live at the new root now.
    assert!(to.join("alpha").is_dir());
    assert!(to.join("beta").is_dir());
    assert!(!from.exists());
    assert_eq!(result.moved_slugs.len(), 2);
    assert_eq!(result.updated_links.len(), 2);
    assert!(result.warnings.is_empty());

    // The agent links now resolve to the moved directories.
    let alpha_real = fs::canonicalize(agent_root.join("alpha")).unwrap();
    assert_eq!(alpha_real, fs::canonicalize(to.join("alpha")).unwrap());
    let beta_real = fs::canonicalize(agent_root.join("beta")).unwrap();
    assert_eq!(beta_real, fs::canonicalize(to.join("beta")).unwrap());
}

#[cfg(unix)]
#[test]
fn reports_a_warning_when_a_link_cannot_be_rewritten() {
    use std::os::unix::fs::symlink;

    let temp = TempTree::new("mig-link-fail");
    let from = temp.path("repo");
    let to = temp.path("dest");
    let agent_root = temp.path("agent");
    fs::create_dir_all(&agent_root).unwrap();
    make_skill_dir(&from.join("alpha"));
    // A symlink to the skill, whose agent root we will remove anyway — but here
    // we make the *re-point* fail by leaving the link target open is not needed;
    // instead simulate an unreadable root by making the discovery return nothing.
    symlink(&from.join("alpha"), &agent_root.join("alpha")).unwrap();

    // An unreadable agent root must surface a warning rather than silently drop.
    fs::set_permissions(&agent_root, fs::Permissions::from_mode(0o000)).unwrap();
    let roots = vec![("claude-code".to_string(), agent_root.clone())];
    let result = migrate_repo_root(&from, &to, &roots).unwrap();

    // Discovery of the link failed (root unreadable); the repo still moves.
    assert!(to.join("alpha").is_dir());
    assert!(
        result.warnings.iter().any(|w| w.contains("无法读取")),
        "an unreadable agent root must be reported: {:?}",
        result.warnings
    );
    fs::set_permissions(&agent_root, fs::Permissions::from_mode(0o755)).unwrap();
}

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
