//! The menu bar. Every command in the window is also reachable here.

use tauri::menu::{AboutMetadata, CheckMenuItem, Menu, MenuItem, SubmenuBuilder};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder, Wry};

use crate::auth::Accounts;

/// Account item IDs are `account:<uuid>`; the window switches to that account.
pub const ACCOUNT_PREFIX: &str = "account:";

/// Rebuilt whenever accounts change, so Game › Account always lists them.
pub fn build(app: &AppHandle, accounts: &Accounts) -> tauri::Result<Menu<Wry>> {
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
        .item(&item("view_sidebar", "as Sidebar", Some("CmdOrCtrl+1"))?)
        .item(&item("view_grid", "as Grid", Some("CmdOrCtrl+2"))?)
        .separator()
        .item(&item("toggle_sidebar", "Toggle Sidebar", Some("CmdOrCtrl+Ctrl+S"))?)
        .item(&item("toggle_log", "Toggle Game Log", Some("CmdOrCtrl+L"))?)
        .separator()
        .fullscreen()
        .build()?;
    let mut account = SubmenuBuilder::new(app, "Account");
    for profile in &accounts.accounts {
        let label = if profile.offline { format!("{} (Offline)", profile.name) } else { profile.name.clone() };
        let active = accounts.active.as_ref() == Some(&profile.uuid);
        let id = format!("{ACCOUNT_PREFIX}{}", profile.uuid);
        account = account.item(&CheckMenuItem::with_id(app, id, label, true, active, None::<&str>)?);
    }
    if !accounts.accounts.is_empty() {
        account = account.separator();
    }
    let sign_out_label = match accounts.current() {
        Some(profile) => format!("Sign Out {}", profile.name),
        None => "Sign Out".into(),
    };
    let account = account
        .item(&MenuItem::with_id(app, "skins", "Skins & Capes…", accounts.current().is_some(), Some("CmdOrCtrl+Shift+K"))?)
        .separator()
        .item(&item("add_account", "Add Account…", None)?)
        .item(&MenuItem::with_id(app, "sign_out", sign_out_label, accounts.current().is_some(), None::<&str>)?)
        .build()?;

    let game = SubmenuBuilder::new(app, "Game")
        .item(&item("play", "Play", Some("CmdOrCtrl+Enter"))?)
        .item(&item("stop", "Stop", Some("CmdOrCtrl+."))?)
        .separator()
        .item(&item("verify_files", "Verify Game Files", None)?)
        .separator()
        .item(&account)
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
