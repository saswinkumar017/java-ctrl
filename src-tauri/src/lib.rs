mod app_state;
mod commands;
mod core;

use tauri::Manager;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let state = app_state::AppState::new(app.handle())?;
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_app_info,
            commands::scan_jdks,
            commands::list_processes,
            commands::get_process,
            commands::terminate_process,
            commands::monitor_sample,
            commands::get_samples,
            commands::get_gc_events,
            commands::get_threads,
            commands::get_class_loading,
            commands::get_jit,
            commands::get_runtime_details,
            commands::check_jfr,
            commands::start_jfr,
            commands::dump_jfr,
            commands::stop_jfr,
            commands::capture_heap_dump,
            commands::analyze_heap,
            commands::list_dumps,
            commands::delete_dump,
            commands::open_path,
            commands::probe_spring,
            commands::discover_spring,
            commands::list_spring_connections,
            commands::save_spring_connection,
            commands::delete_spring_connection,
            commands::check_spring_connection,
            commands::get_settings,
            commands::save_settings,
            commands::prune_storage,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Java Control Centre");
}
