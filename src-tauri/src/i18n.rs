//! The menu bar's and window titles' words in the app's language. The window decides the
//! language (it may differ from the system's if someone picked one in Settings) and tells
//! Rust through `set_language`; until then, the system's language is used.

use std::sync::atomic::{AtomicBool, Ordering};

static GERMAN: AtomicBool = AtomicBool::new(false);

pub fn init() {
    let system = sys_locale::get_locale().unwrap_or_default().to_lowercase();
    GERMAN.store(system.starts_with("de"), Ordering::Relaxed);
}

/// Returns whether the language changed.
pub fn set(language: &str) -> bool {
    let german = language == "de";
    GERMAN.swap(german, Ordering::Relaxed) != german
}

pub fn german() -> bool {
    GERMAN.load(Ordering::Relaxed)
}

/// Translates a menu label or title; unknown text stays as it is.
pub fn tr(text: &str) -> String {
    if !german() {
        return text.to_string();
    }
    let german = match text {
        "Check for Updates…" => "Nach Updates suchen …",
        "Settings…" => "Einstellungen …",
        "Settings" => "Einstellungen",
        "Preferences" => "Einstellungen",
        "Sign In with Microsoft" => "Mit Microsoft anmelden",
        "File" => "Ablage",
        "New Instance…" => "Neue Instanz …",
        "Import Modpack…" => "Modpack importieren …",
        "Edit Instance…" => "Instanz bearbeiten …",
        "Duplicate" => "Duplizieren",
        "Export as Modpack…" => "Als Modpack exportieren …",
        "Show in Finder" => "Im Finder zeigen",
        "Add to Applications" => "Zu Programmen hinzufügen",
        "Create Desktop Shortcut" => "Desktopverknüpfung erstellen",
        "Add to App Launcher" => "Zum App-Starter hinzufügen",
        "Move to Trash" => "In den Papierkorb legen",
        "Remove from Applications" => "Aus Programmen entfernen",
        "Delete Desktop Shortcut" => "Desktopverknüpfung löschen",
        "Remove from App Launcher" => "Aus dem App-Starter entfernen",
        "Close Window" => "Fenster schließen",
        "Edit" => "Bearbeiten",
        "Undo" => "Widerrufen",
        "Redo" => "Wiederholen",
        "Cut" => "Ausschneiden",
        "Copy" => "Kopieren",
        "Paste" => "Einsetzen",
        "Select All" => "Alles auswählen",
        "View" => "Darstellung",
        "as Sidebar" => "als Seitenleiste",
        "as Grid" => "als Raster",
        "Toggle Sidebar" => "Seitenleiste ein/aus",
        "Toggle Game Log" => "Spielprotokoll ein/aus",
        "Enter Full Screen" => "Vollbildmodus aktivieren",
        "Account" => "Account",
        "Sign Out" => "Abmelden",
        "Skins & Capes…" => "Skins & Umhänge …",
        "Add Account…" => "Account hinzufügen …",
        "Game" => "Spiel",
        "Play" => "Spielen",
        "Stop" => "Stoppen",
        "Verify Game Files" => "Spieldateien prüfen",
        "Window" => "Fenster",
        "Minimize" => "Im Dock ablegen",
        "Zoom" => "Zoomen",
        "Services" => "Dienste",
        "Hide Clover Launcher" => "Clover Launcher ausblenden",
        "Hide Others" => "Andere ausblenden",
        "Show All" => "Alle einblenden",
        "Quit Clover Launcher" => "Clover Launcher beenden",
        "About Clover Launcher" => "Über Clover Launcher",
        "Game Log" => "Spielprotokoll",
        other => return other.to_string(),
    };
    german.to_string()
}
