//! The menu bar. Every command in the window is also reachable here.

use tauri::menu::{AboutMetadata, CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, SubmenuBuilder};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder, Wry};

use clover_core::auth::Accounts;

use crate::i18n::tr;

/// Account item IDs are `account:<uuid>`; the window switches to that account.
pub const ACCOUNT_PREFIX: &str = "account:";

/// Where an instance shortcut goes, in each OS's words.
const SHORTCUT_LABEL: &str = if cfg!(target_os = "macos") {
    "Add to Applications"
} else if cfg!(windows) {
    "Create Desktop Shortcut"
} else {
    "Add to App Launcher"
};

/// Rebuilt whenever accounts change, so Game › Account always lists them.
pub fn build(app: &AppHandle, accounts: &Accounts) -> tauri::Result<Menu<Wry>> {
    let item = |id: &str, label: &str, accel: Option<&str>| MenuItem::with_id(app, id, tr(label), true, accel);
    // The system's own items, with their titles in the app's language.
    let system = |make: fn(&AppHandle, Option<&str>) -> tauri::Result<PredefinedMenuItem<Wry>>, label: &str| make(app, Some(&tr(label)));

    let app_menu = SubmenuBuilder::new(app, "Clover Launcher")
        .item(&PredefinedMenuItem::about(app, Some(&tr("About Clover Launcher")), Some(AboutMetadata::default()))?)
        .separator()
        .item(&item("check_updates", "Check for Updates…", None)?)
        .item(&item("settings", "Settings…", Some("CmdOrCtrl+,"))?)
        .separator()
        .item(&system(PredefinedMenuItem::services, "Services")?)
        .separator()
        .item(&system(PredefinedMenuItem::hide, "Hide Clover Launcher")?)
        .item(&system(PredefinedMenuItem::hide_others, "Hide Others")?)
        .item(&system(PredefinedMenuItem::show_all, "Show All")?)
        .separator()
        .item(&system(PredefinedMenuItem::quit, "Quit Clover Launcher")?)
        .build()?;
    let file = SubmenuBuilder::new(app, tr("File"))
        .item(&item("new_instance", "New Instance…", Some("CmdOrCtrl+N"))?)
        .item(&item("import_modpack", "Import Modpack…", Some("CmdOrCtrl+Shift+N"))?)
        .separator()
        .item(&item("edit_instance", "Edit Instance…", Some("CmdOrCtrl+I"))?)
        .item(&item("duplicate_instance", "Duplicate", Some("CmdOrCtrl+D"))?)
        .item(&item("export_instance", "Export as Modpack…", None)?)
        .separator()
        .item(&item("show_folder", "Show in Finder", Some("CmdOrCtrl+Shift+R"))?)
        .item(&item("create_shortcut", SHORTCUT_LABEL, None)?)
        .separator()
        // No ⌘⌫: the menu would catch it before text fields, where it deletes to the line start.
        .item(&item("delete_instance", "Move to Trash", None)?)
        .separator()
        .item(&system(PredefinedMenuItem::close_window, "Close Window")?)
        .build()?;
    let edit = SubmenuBuilder::new(app, tr("Edit"))
        .item(&system(PredefinedMenuItem::undo, "Undo")?)
        .item(&system(PredefinedMenuItem::redo, "Redo")?)
        .separator()
        .item(&system(PredefinedMenuItem::cut, "Cut")?)
        .item(&system(PredefinedMenuItem::copy, "Copy")?)
        .item(&system(PredefinedMenuItem::paste, "Paste")?)
        .item(&system(PredefinedMenuItem::select_all, "Select All")?)
        .build()?;
    let view = SubmenuBuilder::new(app, tr("View"))
        .item(&item("view_sidebar", "as Sidebar", Some("CmdOrCtrl+1"))?)
        .item(&item("view_grid", "as Grid", Some("CmdOrCtrl+2"))?)
        .separator()
        .item(&item("toggle_sidebar", "Toggle Sidebar", Some("CmdOrCtrl+Ctrl+S"))?)
        .item(&item("toggle_log", "Toggle Game Log", Some("CmdOrCtrl+L"))?)
        .separator()
        .item(&system(PredefinedMenuItem::fullscreen, "Enter Full Screen")?)
        .build()?;
    let mut account = SubmenuBuilder::new(app, tr("Account"));
    for profile in &accounts.accounts {
        let label = if profile.offline { format!("{} ({})", profile.name, if crate::i18n::german() { "offline" } else { "Offline" }) } else { profile.name.clone() };
        let active = accounts.active.as_ref() == Some(&profile.uuid);
        let id = format!("{ACCOUNT_PREFIX}{}", profile.uuid);
        account = account.item(&CheckMenuItem::with_id(app, id, label, true, active, None::<&str>)?);
    }
    if !accounts.accounts.is_empty() {
        account = account.separator();
    }
    let sign_out_label = match accounts.current() {
        Some(profile) if crate::i18n::german() => format!("{} abmelden", profile.name),
        Some(profile) => format!("Sign Out {}", profile.name),
        None => tr("Sign Out"),
    };
    let account = account
        .item(&MenuItem::with_id(app, "skins", tr("Skins & Capes…"), accounts.current().is_some(), Some("CmdOrCtrl+Shift+K"))?)
        .separator()
        .item(&item("add_account", "Add Account…", None)?)
        .item(&MenuItem::with_id(app, "sign_out", sign_out_label, accounts.current().is_some(), None::<&str>)?)
        .build()?;

    let game = SubmenuBuilder::new(app, tr("Game"))
        .item(&item("play", "Play", Some("CmdOrCtrl+Enter"))?)
        .item(&item("stop", "Stop", Some("CmdOrCtrl+."))?)
        .separator()
        .item(&item("verify_files", "Verify Game Files", None)?)
        .separator()
        .item(&account)
        .build()?;
    let window = SubmenuBuilder::new(app, tr("Window"))
        .item(&system(PredefinedMenuItem::minimize, "Minimize")?)
        .item(&system(PredefinedMenuItem::maximize, "Zoom")?)
        .build()?;

    Menu::with_items(app, &[&app_menu, &file, &edit, &view, &game, &window])
}

/// Settings opens its own window; everything else is forwarded to the main window.
pub fn handle(app: &AppHandle, id: &str) {
    match id {
        "settings" => open_settings(app),
        other => {
            // A plain emit: the window's `listen` doesn't receive `emit_to` events. Only the
            // main window listens for menu commands.
            let _ = app.emit("menu-command", other);
        }
    }
}

pub fn open_settings(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.set_focus();
        return;
    }
    let _ = WebviewWindowBuilder::new(app, "settings", WebviewUrl::App("settings".into()))
        // GNOME calls it Preferences.
        .title(tr(if cfg!(target_os = "linux") { "Preferences" } else { "Settings" }))
        .inner_size(540.0, 560.0)
        .resizable(false)
        .minimizable(false)
        .maximizable(false)
        .decorations(crate::chrome::decorated())
        // Elsewhere the page shows the window once drawn, so there's no white flash. macOS
        // opens it right away: a hidden window there doesn't draw its page at all.
        .visible(cfg!(target_os = "macos"))
        .build();
}
