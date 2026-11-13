mod apps;
mod db;
mod http;
mod idle_window;
mod island_window;
mod settings;
mod shortcuts;
mod system;
mod taskbar_clock;
mod time;
mod tray;
mod webview_runtime;

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
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(shortcuts::plugin())
        .on_menu_event(|app, event| tray::handle_menu_event(app, event.id().as_ref()))
        .setup(|app| {
            let dir = app.path().data_dir()?.join("Duri");
            std::fs::create_dir_all(&dir)?;
            let db = Arc::new(db::Db::open(&dir.join("duri.db"))?);
            app.manage(db.clone());
            app.manage(island_window::IslandState::default());
            app.manage(shortcuts::Shortcuts::default());

            island_window::set_hide_in_fullscreen(settings::load(&db).hide_in_fullscreen);
            apps::init(app.handle(), db.clone(), &dir);
            system::init(app.handle());
            taskbar_clock::init(app.handle(), db, &dir);
            island_window::create(app.handle())?;
            island_window::spawn_hit_test(app.handle().clone());
            island_window::spawn_fullscreen_watch(app.handle().clone());
            tray::create(app.handle())?;
            shortcuts::apply(app.handle(), &settings::load(&app.state::<Arc<db::Db>>()));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            island_window::island_set_hit_rects,
            island_window::island_set_focus,
            island_window::island_pointer,
            island_window::island_set_hud,
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
            apps::bluetooth::bluetooth_devices,
            apps::power::power_get,
            apps::power::power_set_brightness,
            apps::privacy::privacy_get,
            settings::settings_get,
            system::system_stats,
            system::system_clock_24h,
            taskbar_clock::taskbar_clock_status,
            taskbar_clock::taskbar_clock_preview,
            taskbar_clock::taskbar_clock_reference,
            taskbar_clock::taskbar_clock_search_city,
            settings::settings_set,
            shortcuts::shortcuts_failed,
            tray::open_settings_window,
            tray::tray_set_labels,
            tray::island_context_menu,
            apps::notifications::notifications_get,
            apps::notifications::notification_dismiss,
            apps::notifications::notifications_clear,
            apps::music::music_get,
            apps::music::music_control,
            apps::music::music_seek,
            apps::music::music_select_source,
            apps::lyrics::lyrics_get,
            apps::meter::audio_meter,
            idle_window::music_idle_open,
            idle_window::music_idle_close,
            apps::sessions::sessions_get,
            apps::sessions::session_dismiss,
            apps::sessions::session_permission,
            apps::sessions::claude_permissions_set,
            apps::sessions::claude_hooks_status,
            apps::sessions::claude_hooks_install,
            apps::sessions::claude_hooks_uninstall,
            apps::keys::keys_get,
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
