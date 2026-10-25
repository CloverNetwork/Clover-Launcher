use std::path::PathBuf;

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
    /// Clover's data folder. The app passes the platform's app data directory.
    pub fn new(root: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    /// For tests: a folder that may not exist yet.
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

    pub fn accounts_file(&self) -> PathBuf {
        self.root.join("accounts.json")
    }

    /// Before account switching there was a single account in this file.
    pub fn legacy_account_file(&self) -> PathBuf {
        self.root.join("account.json")
    }
}
