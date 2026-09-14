mod commands;
mod installer;

/// Entry point invoked from `main.rs`.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            // Apply the bundled app icon to the main window in development mode
            // so the Dock/taskbar shows the SkillHub logo (Tauri only embeds the
            // bundle icon for packaged builds by default).
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
            commands::uninstall_skill_command,
            commands::open_directory
        ])
        .run(tauri::generate_context!())
        .expect("error while running the SkillHub desktop application");
}
