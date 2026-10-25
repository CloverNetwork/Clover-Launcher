//! Resolves a launcher profile and everything it `inheritsFrom` into one launchable version.

use std::collections::HashSet;
use std::path::PathBuf;

use reqwest::Client;

use crate::download::{self, Task};
use crate::error::{Result, msg};
use crate::manifest::{
    self, Argument, Artifact, AssetIndexRef, Library, LoggingClient, VersionJson, VersionManifest, rules_allow,
};
use crate::paths::Paths;

const MAX_DEPTH: usize = 8;

/// A version ready to install and launch.
pub struct Resolved {
    /// The profile's own ID, e.g. "fabric-loader-0.20.1-1.21.4". Passed to the game as its version name.
    pub id: String,
    /// The vanilla version whose client jar is used.
    pub jar_id: String,
    pub kind: String,
    pub main_class: String,
    pub assets: String,
    pub asset_index: AssetIndexRef,
    pub client: Artifact,
    /// Only libraries allowed on this OS, loader copies first.
    pub libraries: Vec<Library>,
    pub game_args: Vec<Argument>,
    pub jvm_args: Vec<Argument>,
    pub legacy_game_args: Option<String>,
    pub java_major: Option<u32>,
    pub logging: Option<LoggingClient>,
}

pub fn json_path(paths: &Paths, id: &str) -> PathBuf {
    paths.version_dir(id).join(format!("{id}.json"))
}

pub fn jar_path(paths: &Paths, id: &str) -> PathBuf {
    paths.version_dir(id).join(format!("{id}.jar"))
}

pub fn load(paths: &Paths, id: &str) -> Result<VersionJson> {
    let bytes = std::fs::read(json_path(paths, id))?;
    serde_json::from_slice(&bytes).map_err(|e| msg(format!("The profile “{id}” isn't valid: {e}")))
}

/// Loads `id` and its parents, downloading vanilla versions from Mojang as needed.
pub async fn resolve(client: &Client, paths: &Paths, id: &str) -> Result<Resolved> {
    let mut manifest = None;
    let mut chain = Vec::new();
    let mut next = Some(id.to_string());
    while let Some(id) = next.take() {
        if chain.len() == MAX_DEPTH {
            return Err(msg("This profile inherits from too many other profiles."));
        }
        let json = load_or_fetch(client, paths, &mut manifest, &id).await?;
        next = json.inherits_from.clone();
        chain.push(json);
    }
    merge(chain)
}

async fn load_or_fetch(
    client: &Client,
    paths: &Paths,
    manifest: &mut Option<VersionManifest>,
    id: &str,
) -> Result<VersionJson> {
    // Some loaders (Ornithe) name their parent "<version>-vanilla", following
    // launchers that keep a renamed copy of the vanilla profile.
    let candidates = [Some(id), id.strip_suffix("-vanilla")];
    for candidate in candidates.into_iter().flatten() {
        if manifest.is_none() {
            *manifest = Some(manifest::fetch_manifest(client).await?);
        }
        let mojang = manifest.as_ref().and_then(|m| m.versions.iter().find(|v| v.id == candidate));
        if let Some(entry) = mojang {
            let task = Task { url: entry.url.clone(), path: json_path(paths, candidate), sha1: Some(entry.sha1.clone()) };
            download::fetch(client, task).await?;
            return load(paths, candidate);
        }
        if json_path(paths, candidate).exists() {
            return load(paths, candidate);
        }
    }
    Err(msg(format!("Couldn't find the Minecraft version “{id}”.")))
}

/// Child-first merge, following the official launcher: scalar fields come from the
/// nearest profile that sets them, arguments concatenate parent-first, and a loader's
/// library replaces the inherited library with the same group and artifact.
fn merge(chain: Vec<VersionJson>) -> Result<Resolved> {
    let id = chain[0].id.clone();
    let missing = |field: &str| msg(format!("The profile “{id}” has no {field}."));

    let mut libraries = Vec::new();
    let mut seen = HashSet::new();
    for json in &chain {
        let level: Vec<&Library> = json.libraries.iter().filter(|l| rules_allow(&l.rules)).collect();
        let keys: Vec<String> = level.iter().map(|l| l.key()).collect();
        for (lib, key) in level.into_iter().zip(&keys) {
            if !seen.contains(key) {
                libraries.push(lib.clone());
            }
        }
        seen.extend(keys);
    }

    let mut game_args = Vec::new();
    let mut jvm_args = Vec::new();
    for json in chain.iter().rev() {
        if let Some(arguments) = &json.arguments {
            game_args.extend(arguments.game.iter().cloned());
            jvm_args.extend(arguments.jvm.iter().cloned());
        }
    }

    let root = chain.iter().rev().find(|j| j.downloads.is_some()).ok_or_else(|| missing("client download"))?;
    Ok(Resolved {
        jar_id: root.id.clone(),
        kind: chain.iter().find_map(|j| j.kind.clone()).unwrap_or_else(|| "release".into()),
        main_class: chain.iter().find_map(|j| j.main_class.clone()).ok_or_else(|| missing("main class"))?,
        assets: chain.iter().find_map(|j| j.assets.clone()).ok_or_else(|| missing("assets"))?,
        asset_index: chain.iter().find_map(|j| j.asset_index.clone()).ok_or_else(|| missing("asset index"))?,
        client: root.downloads.as_ref().map(|d| d.client.clone()).ok_or_else(|| missing("client download"))?,
        legacy_game_args: chain.iter().find_map(|j| j.minecraft_arguments.clone()),
        java_major: chain.iter().find_map(|j| j.java_version.as_ref().map(|v| v.major_version)),
        logging: chain.iter().find_map(|j| j.logging.as_ref().and_then(|l| l.client.clone())),
        libraries,
        game_args,
        jvm_args,
        id,
    })
}
