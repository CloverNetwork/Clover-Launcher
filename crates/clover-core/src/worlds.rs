//! An instance's worlds and screenshots: listing them, backing worlds up, and moving either
//! to the Trash. World details come from each world's `level.dat`, a gzipped NBT file.

use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use base64::Engine;
use serde::{Deserialize, Serialize};

use crate::error::{Result, msg};
use crate::instances::{Instance, move_to_trash};
use crate::paths::Paths;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct World {
    /// The folder in `saves`, which is how the game and Clover refer to it.
    pub folder: String,
    pub name: String,
    /// Seconds since 1970.
    pub last_played: Option<u64>,
    pub mode: &'static str,
    /// The Minecraft version it was last played in.
    pub version: Option<String>,
    /// `icon.png` as a data URL, when the world has one.
    pub icon: Option<String>,
}

#[derive(Deserialize)]
struct LevelFile {
    #[serde(rename = "Data")]
    data: Level,
}

#[derive(Deserialize)]
struct Level {
    #[serde(rename = "LevelName")]
    name: Option<String>,
    #[serde(rename = "LastPlayed")]
    last_played: Option<i64>,
    #[serde(rename = "GameType")]
    game_type: Option<i32>,
    hardcore: Option<i8>,
    #[serde(rename = "Version")]
    version: Option<LevelVersion>,
}

#[derive(Deserialize)]
struct LevelVersion {
    #[serde(rename = "Name")]
    name: Option<String>,
}

fn saves(paths: &Paths, instance: &Instance) -> PathBuf {
    paths.instance(&instance.id).join("minecraft").join("saves")
}

/// A world's folder, checked to be a real world inside `saves`.
pub fn world_dir(paths: &Paths, instance: &Instance, folder: &str) -> Result<PathBuf> {
    let single = Path::new(folder).components().count() == 1 && !folder.starts_with('.') && !folder.contains(['/', '\\']);
    let dir = saves(paths, instance).join(folder);
    if !single || !dir.join("level.dat").is_file() {
        return Err(msg("That world no longer exists."));
    }
    Ok(dir)
}

fn read_level(path: &Path) -> Option<Level> {
    let mut bytes = Vec::new();
    flate2::read::GzDecoder::new(File::open(path).ok()?).read_to_end(&mut bytes).ok()?;
    fastnbt::from_bytes::<LevelFile>(&bytes).ok().map(|f| f.data)
}

/// The worlds in `saves`, most recently played first.
pub fn list(paths: &Paths, instance: &Instance) -> Result<Vec<World>> {
    let Ok(entries) = std::fs::read_dir(saves(paths, instance)) else { return Ok(vec![]) };
    let mut worlds: Vec<World> = entries
        .filter_map(|entry| {
            let dir = entry.ok()?.path();
            let folder = dir.file_name()?.to_string_lossy().into_owned();
            if !dir.join("level.dat").is_file() {
                return None;
            }
            let level = read_level(&dir.join("level.dat"));
            let modified = || std::fs::metadata(dir.join("level.dat")).ok()?.modified().ok()?.duration_since(UNIX_EPOCH).ok().map(|d| d.as_secs());
            let mode = match level.as_ref() {
                Some(l) if l.hardcore.unwrap_or(0) != 0 => "Hardcore",
                Some(l) => match l.game_type {
                    Some(1) => "Creative",
                    Some(2) => "Adventure",
                    Some(3) => "Spectator",
                    _ => "Survival",
                },
                None => "Survival",
            };
            let icon = std::fs::read(dir.join("icon.png"))
                .ok()
                .map(|png| format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(png)));
            Some(World {
                name: level.as_ref().and_then(|l| l.name.clone()).filter(|n| !n.trim().is_empty()).unwrap_or_else(|| folder.clone()),
                last_played: level.as_ref().and_then(|l| l.last_played).map(|ms| (ms / 1000) as u64).or_else(modified),
                mode,
                version: level.and_then(|l| l.version?.name),
                icon,
                folder,
            })
        })
        .collect();
    worlds.sort_by_key(|w| std::cmp::Reverse(w.last_played));
    Ok(worlds)
}

/// Zips a world into the instance's `backups` folder. `label` tells backups apart, such
/// as the local date and time; the file is "<world> <label>.zip".
pub fn backup(paths: &Paths, instance: &Instance, folder: &str, label: &str) -> Result<PathBuf> {
    let dir = world_dir(paths, instance, folder)?;
    let label: String = label.chars().map(|c| if c.is_alphanumeric() || " -_.".contains(c) { c } else { '-' }).collect();
    let backups = paths.instance(&instance.id).join("backups");
    std::fs::create_dir_all(&backups)?;
    let out = backups.join(format!("{folder} {}.zip", label.trim()));
    let partial = out.with_extension("zip.part");
    let result = (|| -> Result<()> {
        let mut zip = zip::ZipWriter::new(File::create(&partial)?);
        let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        add_dir(&mut zip, &dir, folder, options)?;
        zip.finish()?;
        Ok(())
    })();
    match result {
        Ok(()) => {
            std::fs::rename(&partial, &out)?;
            Ok(out)
        }
        Err(e) => {
            let _ = std::fs::remove_file(&partial);
            Err(e)
        }
    }
}

pub(crate) fn add_dir(zip: &mut zip::ZipWriter<File>, dir: &Path, prefix: &str, options: zip::write::SimpleFileOptions) -> Result<()> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)?.filter_map(|e| e.ok()).collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let name = entry.file_name().to_string_lossy().into_owned();
        // The game holds this open while the world is loaded, and it's useless in a copy.
        if name == "session.lock" {
            continue;
        }
        let path = entry.path();
        let inner = format!("{prefix}/{name}");
        if entry.file_type()?.is_dir() {
            add_dir(zip, &path, &inner, options)?;
        } else {
            zip.start_file(inner, options)?;
            zip.write_all(&std::fs::read(&path)?)?;
        }
    }
    Ok(())
}

pub fn trash(paths: &Paths, instance: &Instance, folder: &str) -> Result<()> {
    move_to_trash(&world_dir(paths, instance, folder)?)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Screenshot {
    pub file: String,
    pub path: PathBuf,
    /// Seconds since 1970.
    pub taken: u64,
}

fn screenshots_dir(paths: &Paths, instance: &Instance) -> PathBuf {
    paths.instance(&instance.id).join("minecraft").join("screenshots")
}

/// Screenshots, newest first.
pub fn screenshots(paths: &Paths, instance: &Instance) -> Result<Vec<Screenshot>> {
    let Ok(entries) = std::fs::read_dir(screenshots_dir(paths, instance)) else { return Ok(vec![]) };
    let mut shots: Vec<Screenshot> = entries
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let file = entry.file_name().to_string_lossy().into_owned();
            if !file.to_lowercase().ends_with(".png") || file.starts_with('.') {
                return None;
            }
            let taken = entry.metadata().ok()?.modified().ok()?.duration_since(UNIX_EPOCH).ok()?.as_secs();
            Some(Screenshot { path: entry.path(), file, taken })
        })
        .collect();
    shots.sort_by(|a, b| b.taken.cmp(&a.taken).then_with(|| b.file.cmp(&a.file)));
    Ok(shots)
}

pub fn screenshot_path(paths: &Paths, instance: &Instance, file: &str) -> Result<PathBuf> {
    let path = screenshots_dir(paths, instance).join(file);
    if file.contains(['/', '\\']) || file.starts_with('.') || !path.is_file() {
        return Err(msg("That screenshot no longer exists."));
    }
    Ok(path)
}

pub fn trash_screenshot(paths: &Paths, instance: &Instance, file: &str) -> Result<()> {
    move_to_trash(&screenshot_path(paths, instance, file)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Writes a small level.dat the way the game does and reads it back.
    #[test]
    fn lists_and_backs_up_worlds() {
        #[derive(Serialize)]
        struct Out {
            #[serde(rename = "Data")]
            data: OutLevel,
        }
        #[derive(Serialize)]
        struct OutLevel {
            #[serde(rename = "LevelName")]
            name: String,
            #[serde(rename = "LastPlayed")]
            last_played: i64,
            #[serde(rename = "GameType")]
            game_type: i32,
            hardcore: i8,
        }
        let root = std::env::temp_dir().join(format!("clover-worlds-{}", std::process::id()));
        let paths = Paths::at(root.clone());
        let instance = crate::instances::create(&paths, "Worlds", "1.21.4", "vanilla", None).unwrap();
        let world = saves(&paths, &instance).join("New World");
        std::fs::create_dir_all(world.join("region")).unwrap();
        let nbt = fastnbt::to_bytes(&Out { data: OutLevel { name: "My Base".into(), last_played: 1_700_000_000_000, game_type: 1, hardcore: 0 } }).unwrap();
        let mut gz = flate2::write::GzEncoder::new(File::create(world.join("level.dat")).unwrap(), flate2::Compression::default());
        gz.write_all(&nbt).unwrap();
        gz.finish().unwrap();
        std::fs::write(world.join("region/r.0.0.mca"), b"chunks").unwrap();
        std::fs::write(world.join("session.lock"), b"").unwrap();

        let worlds = list(&paths, &instance).unwrap();
        assert_eq!(worlds.len(), 1);
        assert_eq!((worlds[0].name.as_str(), worlds[0].mode, worlds[0].last_played), ("My Base", "Creative", Some(1_700_000_000)));

        let zip = backup(&paths, &instance, "New World", "2026-10-26 14.03").unwrap();
        assert!(zip.ends_with("backups/New World 2026-10-26 14.03.zip"));
        let mut archive = zip::ZipArchive::new(File::open(zip).unwrap()).unwrap();
        assert!(archive.by_name("New World/region/r.0.0.mca").is_ok());
        assert!(archive.by_name("New World/session.lock").is_err());

        assert!(world_dir(&paths, &instance, "../Worlds").is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
