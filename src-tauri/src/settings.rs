use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::loaders::LoaderSource;
use crate::paths::Paths;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// Path to a `java` executable. "java" resolves from PATH.
    pub java_path: String,
    pub memory_mb: u32,
    /// Fabric-compatible meta servers the person added.
    pub loader_sources: Vec<LoaderSource>,
}

impl Default for Settings {
    fn default() -> Self {
        Self { java_path: "java".into(), memory_mb: 4096, loader_sources: vec![] }
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
