use std::path::PathBuf;

use tauri::{AppHandle, Manager};

use crate::error::Result;

/// On-disk layout, rooted at the app data directory:
///
/// ```text
/// libraries/                 shared Maven-style jars
/// assets/indexes, objects/   shared hash-addressed assets
/// versions/<id>/             client jar + version JSON
/// instances/<id>/            game directory + instance.json
/// ```
#[derive(Clone)]
pub struct Paths {
    pub root: PathBuf,
}

impl Paths {
    pub fn new(app: &AppHandle) -> Result<Self> {
        let root = app.path().app_data_dir()?;
        std::fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    #[cfg(test)]
    pub fn at(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn libraries(&self) -> PathBuf {
        self.root.join("libraries")
    }

    pub fn assets(&self) -> PathBuf {
        self.root.join("assets")
    }

    pub fn version_dir(&self, id: &str) -> PathBuf {
        self.root.join("versions").join(id)
    }

    pub fn instances(&self) -> PathBuf {
        self.root.join("instances")
    }

    pub fn instance(&self, id: &str) -> PathBuf {
        self.instances().join(id)
    }

    pub fn settings_file(&self) -> PathBuf {
        self.root.join("settings.json")
    }

    pub fn account_file(&self) -> PathBuf {
        self.root.join("account.json")
    }
}
