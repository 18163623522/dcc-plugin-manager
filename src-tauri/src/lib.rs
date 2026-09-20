pub mod commands;
pub mod compat;
pub mod detect;
pub mod install;
pub mod registry;
pub mod sources;
pub mod uninstall;
pub mod update;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            commands::detect_engines,
            commands::list_plugins,
            commands::add_local_source,
            commands::add_git_source,
            commands::list_git_refs,
            commands::check_updates,
            commands::install_local,
            commands::plan_uninstall,
            commands::do_uninstall,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
