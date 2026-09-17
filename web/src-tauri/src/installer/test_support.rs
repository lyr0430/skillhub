//! Shared fixtures for the installer test modules.
//!
//! Only for modules written alongside it; `local_skill_tests` predates this and
//! carries its own richer helper set.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// A throwaway tree under the OS temp dir, removed when it goes out of scope.
///
/// Names carry a process-wide nonce because `cargo test` runs these in parallel
/// threads of one process, where the pid alone repeats.
pub struct TempTree {
    root: PathBuf,
}

impl TempTree {
    pub fn new(tag: &str) -> Self {
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

    /// A path inside the tree. Parent directories are not created.
    pub fn path(&self, rel: &str) -> PathBuf {
        self.root.join(rel)
    }
}

impl Drop for TempTree {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// Make a directory that looks like a skill package (`SKILL.md` inside).
pub fn make_skill_dir(path: &Path) {
    std::fs::create_dir_all(path).unwrap();
    std::fs::write(path.join("SKILL.md"), "---\nname: x\n---\n").unwrap();
}
