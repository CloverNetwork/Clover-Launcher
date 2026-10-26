//! Exports an instance as a Modrinth `.mrpack`. Files Modrinth hosts are listed by their
//! download link; everything else (mods from elsewhere, configs) is packed in as overrides.
//! Reference: https://support.modrinth.com/en/articles/8802351-modrinth-modpack-format-mrpack

use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::path::Path;

use serde::Serialize;
use serde_json::{Value, json as body};
use sha1::{Digest, Sha1};

use crate::error::{Result, msg};
use crate::instances::Instance;
use crate::mods::{self, Kind, MODRINTH, Shelf, Sites, json, text};
use crate::paths::Paths;
use crate::worlds::add_dir;

/// Folders of settings that make a pack play the way it was set up.
const CONFIG_FOLDERS: [&str; 4] = ["config", "defaultconfigs", "kubejs", "scripts"];

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Exported {
    /// Files listed by their Modrinth download.
    pub linked: usize,
    /// Files packed in whole.
    pub included: usize,
}

/// The pack's `dependencies`: Minecraft and the loader, in Modrinth's names.
fn dependencies(instance: &Instance) -> Result<Value> {
    let mut deps = body!({ "minecraft": instance.version });
    let version = instance.loader_version.clone().unwrap_or_default();
    let key = match instance.loader.as_str() {
        "vanilla" => return Ok(deps),
        "fabric" => "fabric-loader",
        "quilt" => "quilt-loader",
        "neoforge" => "neoforge",
        "forge" => "forge",
        _ => return Err(msg("Modrinth packs can only use Fabric, Quilt, Forge or NeoForge.")),
    };
    // Clover names Forge versions like Forge's files, "1.20.1-47.2.0"; packs only want "47.2.0".
    let version = match version.strip_prefix(&format!("{}-", instance.version)) {
        Some(short) if key == "forge" => short.to_string(),
        _ => version,
    };
    deps[key] = Value::String(version);
    Ok(deps)
}

pub async fn export(sites: &Sites<'_>, paths: &Paths, instance: &Instance, out: &Path) -> Result<Exported> {
    let deps = dependencies(instance)?;
    let game_dir = paths.instance(&instance.id).join("minecraft");

    // Everything that's turned on in the mods, resource packs and shaders folders.
    let mut candidates = Vec::new();
    for kind in [Kind::Mod, Kind::Resourcepack, Kind::Shader] {
        let shelf = Shelf::new(paths, instance, kind, None)?;
        for item in mods::installed(&shelf)?.into_iter().filter(|m| m.enabled) {
            candidates.push((format!("{}/{}", kind.folder(), item.file), shelf.dir().join(&item.file)));
        }
    }

    let mut hashes = HashMap::new();
    for (relative, path) in &candidates {
        if path.is_file() {
            hashes.insert(hex::encode(Sha1::digest(std::fs::read(path)?)), relative.clone());
        }
    }
    let mut files = Vec::new();
    if !hashes.is_empty() {
        let keys: Vec<&String> = hashes.keys().collect();
        let found = json(sites.client.post(format!("{MODRINTH}/version_files")).json(&body!({ "hashes": keys, "algorithm": "sha1" }))).await?;
        for (hash, version) in found.as_object().into_iter().flatten() {
            let Some(relative) = hashes.get(hash) else { continue };
            let Some(file) = version["files"].as_array().into_iter().flatten().find(|f| f["hashes"]["sha1"] == hash.as_str()) else { continue };
            files.push(body!({
                "path": relative,
                "hashes": { "sha1": hash, "sha512": text(&file["hashes"], "sha512") },
                "downloads": [text(file, "url")],
                "fileSize": file["size"],
            }));
        }
    }
    files.sort_by_key(|f| text(f, "path"));
    let linked: Vec<String> = files.iter().map(|f| text(f, "path")).collect();

    let index = body!({
        "formatVersion": 1,
        "game": "minecraft",
        "versionId": "1.0.0",
        "name": instance.name,
        "summary": "Exported from Clover Launcher",
        "files": files,
        "dependencies": deps,
    });

    let partial = out.with_extension("mrpack.part");
    let included = (|| -> Result<usize> {
        let mut zip = zip::ZipWriter::new(File::create(&partial)?);
        let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        zip.start_file("modrinth.index.json", options)?;
        zip.write_all(&serde_json::to_vec_pretty(&index)?)?;
        let mut included = 0;
        for (relative, path) in &candidates {
            if linked.contains(relative) {
                continue;
            }
            if path.is_dir() {
                add_dir(&mut zip, path, &format!("overrides/{relative}"), options)?;
            } else {
                zip.start_file(format!("overrides/{relative}"), options)?;
                zip.write_all(&std::fs::read(path)?)?;
            }
            included += 1;
        }
        for folder in CONFIG_FOLDERS {
            let dir = game_dir.join(folder);
            if dir.is_dir() {
                add_dir(&mut zip, &dir, &format!("overrides/{folder}"), options)?;
            }
        }
        zip.finish()?;
        Ok(included)
    })();
    match included {
        Ok(included) => {
            std::fs::rename(&partial, out)?;
            Ok(Exported { linked: linked.len(), included })
        }
        Err(e) => {
            let _ = std::fs::remove_file(&partial);
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_loaders_the_modrinth_way() {
        let mut instance = crate::instances::create(
            &Paths::at(std::env::temp_dir().join(format!("clover-export-deps-{}", std::process::id()))),
            "Pack",
            "1.20.1",
            "forge",
            Some("1.20.1-47.2.0".into()),
        )
        .unwrap();
        assert_eq!(dependencies(&instance).unwrap(), body!({ "minecraft": "1.20.1", "forge": "47.2.0" }));
        instance.loader = "legacy-fabric".into();
        assert!(dependencies(&instance).is_err());
    }

    /// Installs Sodium, adds a hand-made mod and a config, exports, then imports the pack
    /// as a new instance. Run with `cargo test exports_and_reimports -- --ignored`.
    #[tokio::test]
    #[ignore]
    async fn exports_and_reimports() {
        let dir = std::env::temp_dir().join(format!("clover-export-{}", std::process::id()));
        let paths = Paths::at(dir.clone());
        let client = reqwest::Client::builder().user_agent("CloverNetwork/Clover-Launcher/test").build().unwrap();
        let sites = Sites { client: &client, curseforge_key: None };
        let instance = crate::instances::create(&paths, "Export Me", "1.21.4", "fabric", Some("0.16.10".into())).unwrap();
        let shelf = Shelf::mods(&paths, &instance);
        mods::install(&sites, &shelf, &mods::Target::for_instance(&instance).unwrap(), mods::Source::Modrinth, "sodium").await.unwrap();
        std::fs::write(shelf.dir().join("handmade.jar"), b"not on modrinth").unwrap();
        let config = paths.instance(&instance.id).join("minecraft/config");
        std::fs::create_dir_all(&config).unwrap();
        std::fs::write(config.join("sodium-options.json"), b"{}").unwrap();

        let out = dir.join("Export Me.mrpack");
        let exported = export(&sites, &paths, &instance, &out).await.unwrap();
        assert_eq!((exported.linked, exported.included), (1, 1));

        struct Quiet;
        impl crate::install::Reporter for Quiet {
            fn stage(&mut self, _: &str) {}
            fn files(&mut self, _: usize, _: usize) {}
            fn log(&mut self, _: String) {}
        }
        let imported = crate::modpacks::import(&sites, &paths, &out, &mut Quiet).await.unwrap();
        assert_eq!((imported.instance.loader.as_str(), imported.instance.loader_version.as_deref()), ("fabric", Some("0.16.10")));
        let game = paths.instance(&imported.instance.id).join("minecraft");
        assert!(game.join("mods/handmade.jar").exists());
        assert!(game.join("config/sodium-options.json").exists());
        assert_eq!(std::fs::read_dir(game.join("mods")).unwrap().count(), 2);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
