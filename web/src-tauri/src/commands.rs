use serde::Serialize;

use crate::installer::agents::{resolve_agent_targets, AgentTarget};
use crate::installer::install::{install_skill, InstallInput, InstallResult};

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
