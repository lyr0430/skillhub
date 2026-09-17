use serde::Serialize;
use std::path::{Path, PathBuf};

/// A single install target (an agent) exposed to the web view.
#[derive(Debug, Clone, Serialize)]
pub struct AgentTarget {
    pub id: String,
    pub name: String,
    pub dir: String,
    pub installed: bool,
}

/// Static agent definition with a user-level skills sub-directory relative to
/// the user home directory. Relative directories mirror the profile mappings in
/// [`cli/src/agents/profiles`](../../../../../../cli/src/agents/profiles) so the
/// desktop client and the CLI install to the same places.
pub struct AgentProfile {
    pub id: &'static str,
    pub name: &'static str,
    /// Sub-directory under the home dir, e.g. `.claude/skills`.
    pub user_skills: &'static str,
}

/// The set of agents supported for one-click install.
///
/// Virtual `generic` is the "default global skill location" (`.agents/skills`).
pub const AGENT_PROFILES: &[AgentProfile] = &[
    AgentProfile {
        id: "generic",
        name: "默认全局 Skill 位置",
        user_skills: ".agents/skills",
    },
    AgentProfile {
        id: "claude-code",
        name: "Claude Code",
        user_skills: ".claude/skills",
    },
    AgentProfile {
        id: "codex",
        name: "Codex",
        user_skills: ".codex/skills",
    },
    AgentProfile {
        id: "opencode",
        name: "OpenCode",
        user_skills: ".opencode/skills",
    },
    AgentProfile {
        id: "openclaw",
        name: "OpenClaw",
        user_skills: ".openclaw/skills",
    },
];

/// Resolve the user home directory.
///
/// Uses `dirs::home_dir()` so the path is correct on both macOS (`/Users/xxx`)
/// and Windows (`%USERPROFILE%`). Falls back to the current directory only as a
/// last resort so the desktop app can still start; callers generally expect a
/// real home to exist.
pub fn home_dir() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

/// Build the absolute user-level skill root for an agent profile.
pub fn profile_root(profile: &AgentProfile) -> PathBuf {
    home_dir().join(profile.user_skills)
}

/// Compute the target directory where a skill of `slug` would be installed for
/// the given agent profile, e.g. `~/.claude/skills/<slug>`.
pub fn skill_dir(profile: &AgentProfile, slug: &str) -> PathBuf {
    profile_root(profile).join(slug)
}

/// The shared skill repository root: `~/.skillhub/skills`.
///
/// This is where a *shared* install keeps the real files; each agent then holds a
/// symlink into it. Deliberately one level below `~/.skillhub`, which the CLI
/// already uses for its `namespace-sync.json` workspace file — the desktop client
/// writes only inside `skills/` and never touches that file.
pub fn repo_root() -> PathBuf {
    home_dir().join(".skillhub").join("skills")
}

/// The shared repository directory for one skill: `~/.skillhub/skills/<slug>`.
pub fn repo_skill_dir(slug: &str) -> PathBuf {
    repo_root().join(slug)
}

/// True if the agent's skills root directory already exists on disk, meaning the
/// agent "appears installed".
pub fn is_installed(profile: &AgentProfile) -> bool {
    profile_root(profile).exists()
}

/// Resolve every known agent target with its absolute dir and installed flag.
pub fn resolve_agent_targets() -> Vec<AgentTarget> {
    AGENT_PROFILES
        .iter()
        .map(|profile| {
            let dir = profile_root(profile);
            AgentTarget {
                id: profile.id.to_string(),
                name: profile.name.to_string(),
                dir: dir.to_string_lossy().into_owned(),
                installed: is_installed(profile),
            }
        })
        .collect()
}

/// Look up an agent profile by id (as sent from the web view).
pub fn find_agent(id: &str) -> Option<&'static AgentProfile> {
    AGENT_PROFILES.iter().find(|p| p.id == id)
}

/// Validate a slug so path traversal / oddly named skills cannot escape the
/// intended install directory. Mirrors the registry's coordinate rules.
pub fn validate_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 200
        && slug
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
        && !slug.starts_with('.')
}

/// Sanitize a display path for the log / error surface without revealing the
/// full home path verbatim when useful.
pub fn display_path(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}
