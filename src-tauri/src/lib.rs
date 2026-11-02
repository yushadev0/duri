mod apps;
mod db;
mod island_window;
mod settings;
mod system;
mod time;
mod tray;

use std::sync::Arc;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            tray::open_settings(app);
        }))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .on_menu_event(|app, event| tray::handle_menu_event(app, event.id().as_ref()))
        .setup(|app| {
            let dir = app.path().data_dir()?.join("Duri");
            std::fs::create_dir_all(&dir)?;
            let db = Arc::new(db::Db::open(&dir.join("duri.db"))?);
            app.manage(db.clone());
            app.manage(island_window::IslandState::default());

            apps::init(app.handle(), db, &dir);
            system::init(app.handle());
            island_window::create(app.handle())?;
            island_window::spawn_hit_test(app.handle().clone());
            tray::create(app.handle())?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            island_window::island_set_hit_rects,
            island_window::island_set_focus,
            apps::clipboard::clips_list,
            apps::clipboard::clip_copy,
            apps::clipboard::clip_paste,
            apps::clipboard::clip_set_pinned,
            apps::clipboard::clip_delete,
            apps::clipboard::clips_clear,
            apps::notes::notes_list,
            apps::notes::note_create,
            apps::notes::note_update,
            apps::notes::note_delete,
            apps::notes::note_set_pinned,
            settings::settings_get,
            system::system_stats,
            settings::settings_set,
            tray::open_settings_window,
            tray::tray_set_labels,
            tray::island_context_menu,
            apps::music::music_get,
            apps::music::music_control,
            apps::music::music_seek,
            apps::music::music_select_source,
            apps::sessions::sessions_get,
            apps::sessions::session_dismiss,
            apps::sessions::claude_hooks_status,
            apps::sessions::claude_hooks_install,
            apps::sessions::claude_hooks_uninstall,
            apps::volume::volume_get,
            apps::volume::volume_set,
            apps::volume::volume_toggle_mute,
            apps::timers::timers_get,
            apps::timers::timer_start,
            apps::timers::timer_pause,
            apps::timers::timer_resume,
            apps::timers::timer_restart,
            apps::timers::timer_remove,
            apps::timers::stopwatch_start,
            apps::timers::stopwatch_pause,
            apps::timers::stopwatch_lap,
            apps::timers::stopwatch_reset,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Duri");
}
