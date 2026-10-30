//! A replacement for the game's own client jar, per instance, like Prism Launcher's
//! "Replace Minecraft.jar". Old modding setups and patched clients ship a whole jar.
//!
//! The chosen file is kept as `jars/custom.jar` in the instance folder. At launch it's put on
//! the classpath under the same file name the original would have, in the instance's own
//! folder, because Forge and NeoForge recognize the game jar by that name. Mojang's jar in the
//! shared versions folder is never touched, so other instances and "Use Mojang's" keep working.

use std::fs::File;
use std::path::{Path, PathBuf};

use crate::error::{Result, msg};
use crate::instances::{self, Instance};
use crate::paths::Paths;
use crate::profile::Resolved;

fn dir(paths: &Paths, id: &str) -> PathBuf {
    paths.instance(id).join("jars")
}

fn stored(paths: &Paths, id: &str) -> PathBuf {
    dir(paths, id).join("custom.jar")
}

/// Checks that a file is a jar with classes in it, so a wrong pick fails here rather than
/// as a crash at launch.
fn check(file: &Path) -> Result<()> {
    let archive = File::open(file).map_err(|e| msg(format!("Couldn't open that file ({e}).")))?;
    let archive = zip::ZipArchive::new(archive).map_err(|_| msg("That file isn't a jar."))?;
    if !archive.file_names().any(|name| name.ends_with(".class")) {
        return Err(msg("That jar has no game classes in it."));
    }
    Ok(())
}

/// Uses `source` instead of Mojang's client jar for this instance.
pub fn set(paths: &Paths, id: &str, source: &Path) -> Result<Instance> {
    check(source)?;
    let mut instance = instances::get(paths, id)?;
    let target = stored(paths, id);
    std::fs::create_dir_all(dir(paths, id))?;
    let partial = target.with_extension("jar.part");
    std::fs::copy(source, &partial)?;
    std::fs::rename(&partial, &target)?;
    instance.custom_jar = Some(source.file_name().map_or("custom.jar".into(), |n| n.to_string_lossy().into_owned()));
    instances::save(paths, &instance)?;
    Ok(instance)
}

/// Goes back to Mojang's client jar.
pub fn clear(paths: &Paths, id: &str) -> Result<Instance> {
    let mut instance = instances::get(paths, id)?;
    let _ = std::fs::remove_dir_all(dir(paths, id));
    instance.custom_jar = None;
    instances::save(paths, &instance)?;
    Ok(instance)
}

/// The jar to put on the classpath in place of the client jar, if the instance has one.
pub fn for_launch(paths: &Paths, instance: &Instance, resolved: &Resolved) -> Result<Option<PathBuf>> {
    if instance.custom_jar.is_none() {
        return Ok(None);
    }
    let source = stored(paths, &instance.id);
    if !source.exists() {
        return Err(msg("The replacement game jar is missing. Choose it again in Edit Instance, or use Mojang's."));
    }
    let target = dir(paths, &instance.id).join(format!("{}.jar", resolved.id));
    let modified = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
    let len = |p: &Path| std::fs::metadata(p).map(|m| m.len()).ok();
    if len(&target) != len(&source) || modified(&target) < modified(&source) {
        std::fs::copy(&source, &target)?;
    }
    Ok(Some(target))
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    fn jar(path: &Path, entry: &str) {
        let mut zip = zip::ZipWriter::new(File::create(path).unwrap());
        zip.start_file(entry, zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(b"\xca\xfe\xba\xbe").unwrap();
        zip.finish().unwrap();
    }

    #[test]
    fn replaces_and_restores_the_game_jar() {
        let root = std::env::temp_dir().join(format!("clover-jar-{}", std::process::id()));
        let paths = Paths::at(root.clone());
        let instance = instances::create(&paths, "Patched", "1.8.9", "vanilla", None).unwrap();
        std::fs::create_dir_all(&root).unwrap();
        let good = root.join("Patched Client.jar");
        jar(&good, "net/minecraft/client/Main.class");
        let notes = root.join("notes.jar");
        jar(&notes, "readme.txt");

        assert!(set(&paths, &instance.id, &notes).unwrap_err().to_string().contains("no game classes"));
        let changed = set(&paths, &instance.id, &good).unwrap();
        assert_eq!(changed.custom_jar.as_deref(), Some("Patched Client.jar"));
        assert!(stored(&paths, &instance.id).exists());

        let cleared = clear(&paths, &instance.id).unwrap();
        assert_eq!(cleared.custom_jar, None);
        assert!(!dir(&paths, &instance.id).exists());
        std::fs::remove_dir_all(root).unwrap();
    }
}
