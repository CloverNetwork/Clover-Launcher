//! The menu bar. Every command in the window is also reachable here.

use tauri::menu::{AboutMetadata, Menu, MenuItem, SubmenuBuilder};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder, Wry};

pub fn build(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    let item = |id: &str, label: &str, accel: Option<&str>| MenuItem::with_id(app, id, label, true, accel);

    let app_menu = SubmenuBuilder::new(app, "Clover Launcher")
        .about(Some(AboutMetadata::default()))
        .separator()
        .item(&item("settings", "Settings…", Some("CmdOrCtrl+,"))?)
        .separator()
        .services()
        .separator()
        .hide()
        .hide_others()
        .show_all()
        .separator()
        .quit()
        .build()?;
    let file = SubmenuBuilder::new(app, "File")
        .item(&item("new_instance", "New Instance…", Some("CmdOrCtrl+N"))?)
        .separator()
        .item(&item("show_folder", "Show in Finder", Some("CmdOrCtrl+Shift+R"))?)
        .separator()
        .item(&item("delete_instance", "Delete Instance…", Some("CmdOrCtrl+Backspace"))?)
        .separator()
        .close_window()
        .build()?;
    let edit = SubmenuBuilder::new(app, "Edit").undo().redo().separator().cut().copy().paste().select_all().build()?;
    let view = SubmenuBuilder::new(app, "View")
        .item(&item("toggle_sidebar", "Toggle Sidebar", Some("CmdOrCtrl+Ctrl+S"))?)
        .item(&item("toggle_log", "Toggle Game Log", Some("CmdOrCtrl+L"))?)
        .separator()
        .fullscreen()
        .build()?;
    let game = SubmenuBuilder::new(app, "Game")
        .item(&item("play", "Play", Some("CmdOrCtrl+Enter"))?)
        .item(&item("stop", "Stop", Some("CmdOrCtrl+."))?)
        .separator()
        .item(&item("account", "Account…", None)?)
        .build()?;
    let window = SubmenuBuilder::new(app, "Window").minimize().maximize().build()?;

    Menu::with_items(app, &[&app_menu, &file, &edit, &view, &game, &window])
}

/// Settings opens its own window; everything else is forwarded to the main window.
pub fn handle(app: &AppHandle, id: &str) {
    match id {
        "settings" => open_settings(app),
        other => {
            let _ = app.emit_to("main", "menu-command", other);
        }
    }
}

pub fn open_settings(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.set_focus();
        return;
    }
    let _ = WebviewWindowBuilder::new(app, "settings", WebviewUrl::App("settings".into()))
        .title("Settings")
        .inner_size(540.0, 560.0)
        .resizable(false)
        .minimizable(false)
        .maximizable(false)
        .build();
}
