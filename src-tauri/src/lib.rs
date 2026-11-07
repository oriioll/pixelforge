//! Tauri shell: turns UI requests into [`pixelforge_core`] jobs and progress events.

mod commands;
mod history;
mod paths;
mod state;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(state::AppState::default())
        .invoke_handler(tauri::generate_handler![
            commands::environment,
            commands::inspect_image,
            commands::enhance,
            commands::cancel_job,
            commands::history_list,
            commands::history_remove,
            commands::history_clear,
        ])
        .run(tauri::generate_context!())
        .expect("error while running pixelforge");
}
