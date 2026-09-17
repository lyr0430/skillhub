mod commands;
mod installer;

/// Entry point invoked from `main.rs`.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let result = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // Apply the bundled app icon to the main window so the Dock/taskbar
            // shows the SkillHub logo (Tauri only embeds the bundle icon for
            // packaged builds by default).
            use tauri::Manager;
            if let Some(window) = app.get_webview_window("main") {
                if let Some(icon) = app.default_window_icon() {
                    let _ = window.set_icon(icon.clone());
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::detect_agents,
            commands::detect_skill_status,
            commands::list_installed_skills,
            commands::install_skill_command,
            commands::download_skill_zip_command,
            commands::attach_skill_to_agent_command,
            commands::uninstall_skill_command,
            commands::uninstall_repo_skill_command,
            commands::open_directory,
            commands::reveal_path,
            commands::open_external_url
        ])
        .run(tauri::generate_context!());

    if let Err(error) = result {
        report_startup_failure(&error);
        std::process::exit(1);
    }
}

/// Persist a startup failure somewhere the user can actually find it.
///
/// On Windows release builds `main.rs` sets `windows_subsystem = "windows"`, so
/// there is no console for `eprintln!` to reach: a failure to build the event
/// loop (WebView2 missing, a bad bundle, an antivirus block) otherwise looks
/// exactly like the app never having been launched. Writing a log file is the
/// dependency-free half of the fix; a native error dialog would need a Win32
/// binding crate, which this crate deliberately does not carry.
///
/// Exiting non-zero matters too: without it the process would report success
/// even though no window was ever shown.
fn report_startup_failure(error: &impl std::fmt::Display) {
    let log_dir = dirs::data_local_dir().unwrap_or_else(std::env::temp_dir);
    let _ = std::fs::create_dir_all(&log_dir);
    let _ = std::fs::write(
        log_dir.join("skillhub-desktop-startup-error.log"),
        format!("SkillHub failed to start:\n{error}\n"),
    );
    eprintln!("SkillHub failed to start: {error}");
}
