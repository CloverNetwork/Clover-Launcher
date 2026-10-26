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
    /// Time spent in the game, in seconds.
    #[serde(default)]
    pub play_seconds: u64,
    /// What the person changed in Edit Instance; empty means "use the defaults".
    #[serde(default)]
    pub options: InstanceOptions,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct InstanceOptions {
    /// Sidebar group, such as "Modded". Instances without one are listed first.
    pub group: Option<String>,
    /// Overrides the memory from Settings.
    pub memory_mb: Option<u32>,
    /// Overrides automatic Java and the Java from Settings.
    pub java_path: Option<String>,
    /// Extra JVM arguments, split like a shell would.
    pub jvm_args: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub fullscreen: bool,
    /// Shell commands run before launch (a failure stops the launch) and after the game exits.
    pub pre_launch: String,
    pub post_exit: String,
}

/// What Edit Instance can change.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Changes {
    pub name: String,
    pub version: String,
    pub loader: String,
    pub loader_version: Option<String>,
    pub options: InstanceOptions,
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
        play_seconds: 0,
        options: Default::default(),
    };
    save(paths, &instance)?;
    Ok(instance)
}

/// Moves an instance to the Trash (the Recycle Bin on Windows), so it can be put back.
/// If the system has no Trash, nothing is deleted: worlds are too valuable to lose for good.
/// Applies Edit Instance. Changing the game or loader version means installing again on
/// the next launch; the mods stay, and the Mods section then offers matching updates.
pub fn update(paths: &Paths, id: &str, changes: Changes) -> Result<Instance> {
    let mut instance = get(paths, id)?;
    let name = changes.name.trim();
    if name.is_empty() {
        return Err(msg("Give the instance a name."));
    }
    if changes.loader != "vanilla" && changes.loader != "custom" && changes.loader_version.is_none() {
        return Err(msg("Choose a mod loader version."));
    }
    let reinstall = instance.version != changes.version
        || instance.loader != changes.loader
        || instance.loader_version != changes.loader_version;
    if reinstall {
        instance.profile_id = None;
        instance.installed = false;
    }
    let mut options = changes.options;
    options.group = options.group.map(|g| g.trim().to_string()).filter(|g| !g.is_empty());
    options.java_path = options.java_path.map(|j| j.trim().to_string()).filter(|j| !j.is_empty());
    instance.name = name.to_string();
    instance.version = changes.version;
    instance.loader = changes.loader;
    instance.loader_version = changes.loader_version;
    instance.options = options;
    save(paths, &instance)?;
    Ok(instance)
}

/// Copies an instance, worlds and mods included, under a new name.
pub fn duplicate(paths: &Paths, id: &str) -> Result<Instance> {
    let original = get(paths, id)?;
    let mut copy = create(paths, &format!("{} Copy", original.name), &original.version, &original.loader, original.loader_version.clone())?;
    copy_dir(&paths.instance(&original.id), &paths.instance(&copy.id), &["instance.json"])?;
    copy.profile_id = original.profile_id.clone();
    copy.installed = original.installed;
    copy.options = original.options.clone();
    save(paths, &copy)?;
    Ok(copy)
}

fn copy_dir(from: &std::path::Path, to: &std::path::Path, skip: &[&str]) -> Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let name = entry.file_name();
        if skip.iter().any(|s| name == *s) {
            continue;
        }
        let kind = entry.file_type()?;
        if kind.is_dir() {
            copy_dir(&entry.path(), &to.join(&name), &[])?;
        } else if kind.is_file() {
            std::fs::copy(entry.path(), to.join(&name))?;
        }
    }
    Ok(())
}

/// Adds a finished session's length to the instance's play time.
pub fn add_play_time(paths: &Paths, id: &str, seconds: u64) -> Result<Instance> {
    let mut instance = get(paths, id)?;
    instance.play_seconds += seconds;
    save(paths, &instance)?;
    Ok(instance)
}

pub fn delete(paths: &Paths, id: &str) -> Result<()> {
    let dir = paths.instance(id);
    if !dir.exists() {
        return Err(msg("That instance no longer exists."));
    }
    move_to_trash(&dir)
}

/// Moves a file or folder to the system's Trash or Recycle Bin, so it can be put back.
pub fn move_to_trash(path: &std::path::Path) -> Result<()> {
    #[allow(unused_mut)]
    let mut trash = trash::TrashContext::default();
    // On macOS, the system's own call rather than asking Finder, which needs a permission prompt.
    #[cfg(target_os = "macos")]
    trash::macos::TrashContextExtMacos::set_delete_method(&mut trash, trash::macos::DeleteMethod::NsFileManager);
    trash.delete(path).map_err(|e| msg(format!("Couldn't move it to the Trash: {e}")))
}

#[cfg(test)]
mod edit_tests {
    use super::*;

    #[test]
    fn edits_and_duplicates() {
        let root = std::env::temp_dir().join(format!("clover-edit-{}", std::process::id()));
        let paths = Paths::at(root.clone());
        let mut original = create(&paths, "Survival", "1.21.4", "vanilla", None).unwrap();
        original.installed = true;
        original.profile_id = Some("1.21.4".into());
        save(&paths, &original).unwrap();
        let saves = paths.instance(&original.id).join("minecraft/saves/World");
        std::fs::create_dir_all(&saves).unwrap();
        std::fs::write(saves.join("level.dat"), "data").unwrap();

        let copy = duplicate(&paths, &original.id).unwrap();
        assert_eq!(copy.name, "Survival Copy");
        assert_ne!(copy.id, original.id);
        assert_eq!(std::fs::read_to_string(paths.instance(&copy.id).join("minecraft/saves/World/level.dat")).unwrap(), "data");

        let options = InstanceOptions { group: Some("  Modded ".into()), memory_mb: Some(8192), ..Default::default() };
        let changes = Changes { name: "Fabric World".into(), version: "1.21.4".into(), loader: "fabric".into(), loader_version: Some("0.20.1".into()), options };
        let edited = update(&paths, &original.id, changes).unwrap();
        assert_eq!(edited.name, "Fabric World");
        assert_eq!(edited.options.group.as_deref(), Some("Modded"));
        assert!(!edited.installed && edited.profile_id.is_none(), "a new loader means installing again");

        let bad = Changes { name: "x".into(), version: "1.21.4".into(), loader: "fabric".into(), loader_version: None, options: Default::default() };
        assert!(update(&paths, &original.id, bad).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    /// Moves a throwaway instance to the real Trash, checks it arrived, then removes it there.
    #[test]
    fn moves_to_the_trash() {
        let root = std::env::temp_dir().join(format!("clover-trash-{}", std::process::id()));
        let paths = crate::paths::Paths::at(root.clone());
        let instance = super::create(&paths, "Trash Test", "1.21.4", "vanilla", None).unwrap();
        std::fs::write(paths.instance(&instance.id).join("world.txt"), "keep me").unwrap();
        super::delete(&paths, &instance.id).unwrap();
        assert!(!paths.instance(&instance.id).exists());
        let trashed = std::path::PathBuf::from(std::env::var("HOME").unwrap()).join(".Trash").join(&instance.id);
        assert_eq!(std::fs::read_to_string(trashed.join("world.txt")).unwrap(), "keep me");
        std::fs::remove_dir_all(trashed).unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }
}
