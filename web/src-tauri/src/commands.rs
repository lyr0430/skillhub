use serde::Serialize;
use std::path::Path;

use crate::installer::agents::{find_agent, resolve_agent_targets, skill_dir, AgentTarget};
use crate::installer::attach::{attach_skill_to_agent, AttachResult};
use crate::installer::homepage::validate_external_url;
use crate::installer::install::{
    download_skill_zip, install_skill, DownloadZipResult, InstallInput, InstallMode, InstallResult,
};
use crate::installer::link::LocationKind;
use crate::installer::local_skills::{
    scan_local_skills, uninstall_location, uninstall_repo_skill, LocalSkill, UninstallRepoResult,
};
use crate::installer::metadata::{detect_status, SkillStatus};

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

/// What `list_installed_skills` returns: the aggregated skills, plus any skills
/// root that could not be read (so the UI can say so instead of silently
/// showing a short list).
#[derive(Debug, Serialize)]
pub struct LocalSkillsPayload {
    pub skills: Vec<LocalSkill>,
    pub warnings: Vec<String>,
}

/// Tauri command: list every skill found across the known agents' skills roots.
///
/// This covers skills skillhub did not install (hand-copied, third-party, or
/// dangling links) as well as managed ones; each carries an `origin` so the UI
/// can offer the right actions.
#[tauri::command]
pub fn list_installed_skills() -> CommandResult<LocalSkillsPayload> {
    let (skills, warnings) = scan_local_skills();
    CommandResult::success(LocalSkillsPayload { skills, warnings })
}

/// Per-agent install status for a single skill.
#[derive(Debug, Serialize)]
pub struct AgentSkillStatus {
    pub agent: String,
    pub installed: bool,
    pub version: String,
    pub outdated: bool,
    /// True when a non-skillhub entry of the same slug exists (manual skill,
    /// or a link whose target is gone).
    pub unmanaged: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<LocationKind>,
}

/// Tauri command: report for every agent whether the given skill is installed,
/// its local version, and whether it differs from the requested version.
#[tauri::command]
pub fn detect_skill_status(slug: String, version: String) -> CommandResult<Vec<AgentSkillStatus>> {
    let results = resolve_agent_targets()
        .into_iter()
        .map(|target| {
            let status = find_agent(&target.id)
                .map(|profile| {
                    let dir = skill_dir(profile, &slug);
                    detect_status(&dir, Some(&version))
                })
                // Unknown agent: report nothing rather than guessing.
                .unwrap_or(SkillStatus {
                    installed: false,
                    version: String::new(),
                    outdated: false,
                    unmanaged: false,
                    kind: None,
                });
            AgentSkillStatus {
                agent: target.id,
                installed: status.installed,
                version: status.version,
                outdated: status.outdated,
                unmanaged: status.unmanaged,
                kind: status.kind,
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

/// Tauri command: download a skill version zip to `path` (the location the user
/// picked in the save dialog).
///
/// Downloads via `reqwest`, following the redirect to pre-signed object storage,
/// so it works in `tauri dev`, in a packaged build, and on any client — unlike
/// the WebView's `<a download>`, which WKWebView does not reliably handle.
#[tauri::command]
pub async fn download_skill_zip_command(
    registry: String,
    namespace: String,
    slug: String,
    version: String,
    path: String,
) -> CommandResult<DownloadZipResult> {
    // Run the blocking download off the async executor.
    let result = tauri::async_runtime::spawn_blocking(move || {
        download_skill_zip(&registry, &namespace, &slug, &version, Path::new(&path))
    })
    .await;

    match result {
        Ok(Ok(downloaded)) => CommandResult::success(downloaded),
        Ok(Err(err)) => CommandResult::failure(err.message),
        Err(join_err) => CommandResult::failure(format!("下载任务执行失败: {join_err}")),
    }
}

/// Tauri command: make an already-installed skill available to another agent.
///
/// Purely local — nothing is downloaded and no registry is consulted, so this
/// works for skills that have no registry origin. `mode` mirrors the install
/// preference: shared links the agent entry to the real directory, copy
/// duplicates the contents.
#[tauri::command]
pub async fn attach_skill_to_agent_command(
    source_dir: String,
    agent: String,
    mode: InstallMode,
) -> CommandResult<AttachResult> {
    // Filesystem work (a recursive copy in the worst case) off the async executor.
    let result = tauri::async_runtime::spawn_blocking(move || {
        attach_skill_to_agent(Path::new(&source_dir), &agent, mode)
    })
    .await;

    match result {
        Ok(Ok(attached)) => CommandResult::success(attached),
        Ok(Err(err)) => CommandResult::failure(err.message),
        Err(join_err) => CommandResult::failure(format!("添加任务执行失败: {join_err}")),
    }
}

/// Result of a successful uninstall.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UninstallResult {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    /// The entry that was removed (the link itself, when it was a link).
    pub dir: String,
    /// What kind of entry it was, so the UI copy can be accurate.
    pub removed_kind: LocationKind,
    /// Set when only a link was removed: the real directory survives here.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub real_path_kept: Option<String>,
    /// Backup path when a non-skillhub dir was preserved instead of deleted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backup_dir: Option<String>,
}

/// Tauri command: remove one skill location.
///
/// Takes an explicit `dir` rather than an `(agent, slug)` pair, because a skill
/// reachable from several agents has several locations and the user may want to
/// detach just one of them. The path is validated against the known agent roots
/// inside `uninstall_location` before anything is touched.
#[tauri::command]
pub async fn uninstall_skill_command(
    dir: String,
    agent: Option<String>,
) -> CommandResult<UninstallResult> {
    let result = tauri::async_runtime::spawn_blocking(move || {
        let path = std::path::Path::new(&dir);
        let removal = uninstall_location(path)?;
        Ok::<_, crate::installer::error::InstallError>(UninstallResult {
            ok: true,
            agent,
            dir,
            removed_kind: removal.removed_kind,
            real_path_kept: removal.real_path_kept,
            backup_dir: removal.backup_dir,
        })
    })
    .await;

    match result {
        Ok(Ok(res)) => CommandResult::success(res),
        Ok(Err(err)) => CommandResult::failure(err.message),
        Err(join_err) => CommandResult::failure(format!("卸载任务执行失败: {join_err}")),
    }
}

/// Tauri command: remove a skill from the shared repository.
///
/// Deletes the real directory at `~/.skillhub/skills/<slug>` plus every agent
/// link pointing at it. Unlike [`uninstall_skill_command`], this removes the
/// repository item itself, so all linked agents lose the skill at once. The
/// web view confirms before calling, listing the affected agents.
#[tauri::command]
pub async fn uninstall_repo_skill_command(slug: String) -> CommandResult<UninstallRepoResult> {
    let result = tauri::async_runtime::spawn_blocking(move || uninstall_repo_skill(&slug)).await;

    match result {
        Ok(Ok(res)) => CommandResult::success(res),
        Ok(Err(err)) => CommandResult::failure(err.message),
        Err(join_err) => CommandResult::failure(format!("仓库卸载任务执行失败: {join_err}")),
    }
}

/// Tauri command: open a skill's source URL in the operating system's browser.
///
/// The URL can come from a `SKILL.md` this app did not author, so it is
/// validated as http/https before it reaches the OS, and it is always handed to
/// the default browser rather than navigated to inside the webview.
#[tauri::command]
pub fn open_external_url(url: String) -> CommandResult<()> {
    // Validate and execute the same string: `validate_external_url` trims, so
    // opening the untrimmed value would act on something never checked.
    let url = url.trim();
    if let Err(err) = validate_external_url(url) {
        return CommandResult::failure(err.message);
    }

    // Windows uses `explorer.exe` rather than `cmd /C start`: explorer takes the
    // URL as a single argument with no shell in between, so URL characters that
    // mean something to cmd (`&`, `^`, `|`) cannot be reinterpreted.
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open").arg(url).spawn();
    #[cfg(target_os = "windows")]
    let result = std::process::Command::new("explorer").arg(url).spawn();
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let result = std::process::Command::new("xdg-open").arg(url).spawn();

    match result {
        Ok(_) => CommandResult::success(()),
        Err(err) => CommandResult::failure(format!("打开浏览器失败: {err}")),
    }
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

/// Tauri command: reveal a file (or directory) in the OS file manager, with the
/// entry itself selected — Finder `open -R`, Explorer `/select,`.
///
/// Unlike [`open_directory`], which opens a directory, this selects the named
/// file so the user sees exactly what was downloaded.
#[tauri::command]
pub fn reveal_path(path: String) -> CommandResult<()> {
    let p = std::path::Path::new(&path);
    if !p.exists() {
        return CommandResult::failure(format!("路径不存在: {path}"));
    }

    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open")
        .args(["-R", &path])
        .spawn();
    #[cfg(target_os = "windows")]
    let result = std::process::Command::new("explorer")
        .args(["/select,", &path])
        .spawn();
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let result = std::process::Command::new("xdg-open").arg(&path).spawn();

    match result {
        Ok(_) => CommandResult::success(()),
        Err(err) => CommandResult::failure(format!("打开文件管理器失败: {err}")),
    }
}
