mod commands;
mod installer;

/// Entry point invoked from `main.rs`.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            commands::detect_agents,
            commands::install_skill_command
        ])
        .run(tauri::generate_context!())
        .expect("error while running the SkillHub desktop application");
}
