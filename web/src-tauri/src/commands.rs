use serde::Serialize;

use crate::installer::agents::{find_agent, resolve_agent_targets, skill_dir, AgentTarget};
use crate::installer::install::{install_skill, InstallInput, InstallResult};
use crate::installer::metadata::{
    detect_status, scan_agent_skills, uninstall_dir, InstalledSkill, SkillStatus,
};
/// Result wrapper returned to the web view by commands.
#[derive(Debug, Serialize)]
pub struct CommandResult<T> {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl<T> CommandResult<T> {
    fn success(data: T) -> Self {
        Self {
            ok: true,
            data: Some(data),
            error: None,
        }
    }

    fn failure(message: String) -> Self {
        Self {
            ok: false,
            data: None,
            error: Some(message),
        }
    }
}

/// Tauri command: enumerate known agents and whether each is installed locally.
#[tauri::command]
pub fn detect_agents() -> CommandResult<Vec<AgentTarget>> {
    CommandResult::success(resolve_agent_targets())
}

/// Tauri command: list all skillhub-installed skills across every known agent,
/// discovered by scanning each agent's skill root for `.skillhub/metadata.json`.
#[tauri::command]
pub fn list_installed_skills() -> CommandResult<Vec<InstalledSkill>> {
    let mut all = Vec::new();
    for target in resolve_agent_targets() {
        let root = std::path::Path::new(&target.dir);
        all.extend(scan_agent_skills(&target.id, root));
    }
    CommandResult::success(all)
}

/// Per-agent install status for a single skill.
#[derive(Debug, Serialize)]
pub struct AgentSkillStatus {
    pub agent: String,
    pub installed: bool,
    pub version: String,
    pub outdated: bool,
    /// True when a non-skillhub directory of the same slug exists (manual skill).
    pub unmanaged: bool,
}

/// Tauri command: report for every agent whether the given skill is installed,
/// its local version, and whether it differs from the requested version.
#[tauri::command]
pub fn detect_skill_status(slug: String, version: String) -> CommandResult<Vec<AgentSkillStatus>> {
    let results = resolve_agent_targets()
        .into_iter()
        .map(|target| {
            let profile = find_agent(&target.id);
            let status = profile
                .and_then(|p| {
                    let dir = skill_dir(p, &slug);
                    detect_status(&dir, &target.id, &slug, Some(&version)).ok()
                })
                .unwrap_or(SkillStatus {
                    installed: false,
                    version: String::new(),
                    outdated: false,
                    unmanaged: false,
                });
            AgentSkillStatus {
                agent: target.id,
                installed: status.installed,
                version: status.version,
                outdated: status.outdated,
                unmanaged: status.unmanaged,
            }
        })
        .collect();
    CommandResult::success(results)
}

/// Tauri command: download and install a skill zip to a target agent directory.
///
/// `registry` is passed per call so the desktop app can target any SkillHub
/// registry; the binary itself does not bake in a registry URL.
#[tauri::command]
pub async fn install_skill_command(
    input: InstallInput,
    registry: String,
) -> CommandResult<InstallResult> {
    // Run the blocking download/extract off the async executor.
    let result =
        tauri::async_runtime::spawn_blocking(move || install_skill(&input, &registry)).await;

    match result {
        Ok(Ok(installed)) => CommandResult::success(installed),
        Ok(Err(err)) => CommandResult::failure(err.message),
        Err(join_err) => CommandResult::failure(format!("安装任务执行失败: {join_err}")),
    }
}

/// Tauri command: uninstall a skill from a target agent directory.
#[tauri::command]
pub async fn uninstall_skill_command(
    agent: String,
    slug: String,
) -> CommandResult<UninstallResult> {
    let result = tauri::async_runtime::spawn_blocking(move || {
        let profile = find_agent(&agent).ok_or_else(|| {
            crate::installer::error::not_found(&format!("不支持的 agent: {agent}"))
        })?;
        let dir = skill_dir(profile, &slug);
        let outcome = uninstall_dir(&dir)?;
        Ok::<_, crate::installer::error::InstallError>(UninstallResult {
            ok: true,
            agent,
            dir: dir.to_string_lossy().into_owned(),
            backup_dir: outcome.backup_dir,
        })
    })
    .await;

    match result {
        Ok(Ok(res)) => CommandResult::success(res),
        Ok(Err(err)) => CommandResult::failure(err.message),
        Err(join_err) => CommandResult::failure(format!("卸载任务执行失败: {join_err}")),
    }
}

/// Result of a successful uninstall.
#[derive(Debug, Serialize)]
pub struct UninstallResult {
    pub ok: bool,
    pub agent: String,
    pub dir: String,
    /// Backup path when a non-skillhub dir was preserved instead of deleted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backup_dir: Option<String>,
}

/// Tauri command: reveal a directory in the operating system's file manager
/// (Finder on macOS, Explorer on Windows, xdg-open elsewhere).
#[tauri::command]
pub fn open_directory(dir: String) -> CommandResult<()> {
    let path = std::path::Path::new(&dir);
    if !path.exists() {
        return CommandResult::failure(format!("目录不存在: {dir}"));
    }

    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open").arg(&dir).spawn();
    #[cfg(target_os = "windows")]
    let result = std::process::Command::new("explorer").arg(&dir).spawn();
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let result = std::process::Command::new("xdg-open").arg(&dir).spawn();

    match result {
        Ok(_) => CommandResult::success(()),
        Err(err) => CommandResult::failure(format!("打开文件管理器失败: {err}")),
    }
}
