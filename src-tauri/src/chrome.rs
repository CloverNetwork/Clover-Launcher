//! Window title bars. Hyprland and similar tiling compositors manage windows with the
//! keyboard, so on Hyprland Clover's windows open without a title bar. People who want
//! buttons can turn on Clover's own Full Screen and Quit buttons in Settings. Minimize is
//! never offered there: Hyprland has no minimized windows.

use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowChrome {
    pub hyprland: bool,
    /// Show Clover's own Full Screen and Quit (or Close) buttons.
    pub buttons: bool,
}

/// Hyprland sets `HYPRLAND_INSTANCE_SIGNATURE` for everything it starts.
pub fn is_hyprland() -> bool {
    detect(|key| std::env::var(key).ok())
}

fn detect(var: impl Fn(&str) -> Option<String>) -> bool {
    var("HYPRLAND_INSTANCE_SIGNATURE").is_some_and(|v| !v.is_empty())
        || var("XDG_CURRENT_DESKTOP").is_some_and(|v| v.to_ascii_lowercase().contains("hyprland"))
}

/// Whether windows get the system title bar.
pub fn decorated() -> bool {
    !is_hyprland()
}

pub fn current(settings: &clover_core::settings::Settings) -> WindowChrome {
    let hyprland = is_hyprland();
    WindowChrome { hyprland, buttons: hyprland && settings.show_window_buttons }
}

#[cfg(test)]
mod tests {
    use super::detect;

    #[test]
    fn detects_hyprland() {
        assert!(detect(|k| (k == "HYPRLAND_INSTANCE_SIGNATURE").then(|| "abc_123".into())));
        assert!(detect(|k| (k == "XDG_CURRENT_DESKTOP").then(|| "Hyprland".into())));
        assert!(!detect(|k| (k == "XDG_CURRENT_DESKTOP").then(|| "GNOME".into())));
        assert!(!detect(|_| None));
    }
}
