use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::error::{Result, msg};
use crate::paths::Paths;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Instance {
    pub id: String,
    pub name: String,
    /// The Minecraft version.
    pub version: String,
    /// "vanilla", "custom", or a loader source ID such as "fabric".
    pub loader: String,
    /// The loader's version, or the profile ID for a custom profile.
    #[serde(default)]
    pub loader_version: Option<String>,
    /// The launcher profile the last install resolved to, so launches can skip the network.
    #[serde(default)]
    pub profile_id: Option<String>,
    pub created_at: u64,
    pub last_played: Option<u64>,
    pub installed: bool,
}

pub fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

pub fn list(paths: &Paths) -> Result<Vec<Instance>> {
    let dir = paths.instances();
    if !dir.exists() {
        return Ok(vec![]);
    }
    let mut instances: Vec<Instance> = std::fs::read_dir(dir)?
        .filter_map(|entry| {
            let file = entry.ok()?.path().join("instance.json");
            serde_json::from_slice(&std::fs::read(file).ok()?).ok()
        })
        .collect();
    instances.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(instances)
}

pub fn get(paths: &Paths, id: &str) -> Result<Instance> {
    let file = paths.instance(id).join("instance.json");
    let bytes = std::fs::read(file).map_err(|_| msg("That instance no longer exists."))?;
    Ok(serde_json::from_slice(&bytes)?)
}

pub fn save(paths: &Paths, instance: &Instance) -> Result<()> {
    let dir = paths.instance(&instance.id);
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join("instance.json"), serde_json::to_vec_pretty(instance)?)?;
    Ok(())
}

pub fn create(paths: &Paths, name: &str, version: &str, loader: &str, loader_version: Option<String>) -> Result<Instance> {
    let name = name.trim();
    if name.is_empty() {
        return Err(msg("Give the instance a name."));
    }
    if loader != "vanilla" && loader_version.is_none() {
        return Err(msg("Choose a mod loader version."));
    }
    let slug: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .collect();
    let suffix = &uuid::Uuid::new_v4().simple().to_string()[..6];
    let instance = Instance {
        id: format!("{}-{suffix}", slug.trim_matches('-')),
        name: name.to_string(),
        version: version.to_string(),
        loader: loader.to_string(),
        loader_version,
        profile_id: None,
        created_at: now(),
        last_played: None,
        installed: false,
    };
    save(paths, &instance)?;
    Ok(instance)
}

pub fn delete(paths: &Paths, id: &str) -> Result<()> {
    std::fs::remove_dir_all(paths.instance(id))?;
    Ok(())
}
