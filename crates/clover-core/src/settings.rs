use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::loaders::LoaderSource;
use crate::paths::Paths;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// A `java` executable to use for every version. Empty means automatic: the Java each
    /// version needs, downloaded when the computer doesn't have it.
    pub java_path: String,
    pub memory_mb: u32,
    /// Fabric-compatible meta servers the person added.
    pub loader_sources: Vec<LoaderSource>,
    /// The person's own CurseForge API key, from console.curseforge.com.
    pub curseforge_api_key: String,
    /// On tiling compositors such as Hyprland, where windows have no title bar: show
    /// Clover's own Full Screen and Quit buttons anyway.
    pub show_window_buttons: bool,
    /// Show what's being played as the person's Discord status.
    pub discord_status: bool,
    /// The Discord application whose name and icon the status shows, for builds without one.
    pub discord_app_id: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self { java_path: String::new(), memory_mb: 4096, loader_sources: vec![], curseforge_api_key: String::new(), show_window_buttons: false, discord_status: true, discord_app_id: String::new() }
    }
}

impl Settings {
    /// The person's own Java, if they chose one. Older settings stored "java" for "the one on
    /// the PATH"; that now means automatic too.
    /// The key from Settings, or one built in with `CURSEFORGE_API_KEY` at compile time.
    /// Keys are kept out of the source code.
    pub fn curseforge_key(&self) -> Option<&str> {
        Some(self.curseforge_api_key.trim()).filter(|k| !k.is_empty()).or(option_env!("CURSEFORGE_API_KEY"))
    }

    /// The Discord application ID from Settings, or one built in with `DISCORD_APP_ID`.
    pub fn discord_app_id(&self) -> Option<&str> {
        Some(self.discord_app_id.trim()).filter(|k| !k.is_empty()).or(option_env!("DISCORD_APP_ID"))
    }

    pub fn custom_java(&self) -> Option<&str> {
        Some(self.java_path.trim()).filter(|p| !p.is_empty() && *p != "java")
    }
}

pub fn load(paths: &Paths) -> Settings {
    std::fs::read(paths.settings_file())
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

pub fn save(paths: &Paths, settings: &Settings) -> Result<()> {
    std::fs::write(paths.settings_file(), serde_json::to_vec_pretty(settings)?)?;
    Ok(())
}
