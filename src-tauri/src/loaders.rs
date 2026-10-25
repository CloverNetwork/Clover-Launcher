//! Mod loaders, as data rather than code.
//!
//! Three ways in, so loaders this file has never heard of still work:
//! - **Meta servers**: Fabric and every project that copies its meta API (Quilt, Legacy
//!   Fabric, Ornithe, Babric, ...). People can add their own server in Settings.
//! - **Installers**: Forge and NeoForge, run headlessly to write a profile.
//! - **Custom profiles**: any launcher profile JSON, imported from a file.
//!
//! Each path ends with a profile in `versions/<id>/<id>.json` that `profile::resolve` reads.

use std::cmp::Ordering;
use std::collections::VecDeque;
use std::path::Path;
use std::process::Stdio;

use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

use crate::download::{self, Task, Verify};
use crate::error::{Result, msg};
use crate::install::Reporter;
use crate::instances::Instance;
use crate::paths::Paths;
use crate::profile;
use crate::settings::Settings;

pub const VANILLA: &str = "vanilla";
pub const CUSTOM: &str = "custom";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoaderSource {
    pub id: String,
    pub name: String,
    #[serde(flatten)]
    pub kind: SourceKind,
    /// Added by the person in Settings rather than built in.
    #[serde(default)]
    pub custom: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum SourceKind {
    /// A Fabric-compatible meta server. `url` includes the API version, e.g. `.../v2`;
    /// `loader_path` is the segment after `/versions/`, usually "loader".
    Meta { url: String, loader_path: String },
    Forge,
    NeoForge,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoaderVersion {
    /// What the loader's API calls it; stored on the instance.
    pub version: String,
    /// What people see, e.g. "54.1.0" for Forge's "1.21.4-54.1.0".
    pub label: String,
    pub stable: bool,
}

fn meta(id: &str, name: &str, url: &str, loader_path: &str) -> LoaderSource {
    LoaderSource {
        id: id.into(),
        name: name.into(),
        kind: SourceKind::Meta { url: url.into(), loader_path: loader_path.into() },
        custom: false,
    }
}

pub fn builtin() -> Vec<LoaderSource> {
    vec![
        meta("fabric", "Fabric", "https://meta.fabricmc.net/v2", "loader"),
        meta("quilt", "Quilt", "https://meta.quiltmc.org/v3", "loader"),
        LoaderSource { id: "neoforge".into(), name: "NeoForge".into(), kind: SourceKind::NeoForge, custom: false },
        LoaderSource { id: "forge".into(), name: "Forge".into(), kind: SourceKind::Forge, custom: false },
        meta("legacy-fabric", "Legacy Fabric", "https://meta.legacyfabric.net/v2", "loader"),
        meta("ornithe", "Ornithe", "https://meta.ornithemc.net/v3", "fabric-loader"),
        meta("ornithe-quilt", "Ornithe (Quilt Loader)", "https://meta.ornithemc.net/v3", "quilt-loader"),
        meta("babric", "Babric", "https://meta.babric.glass-launcher.net/v2", "loader"),
    ]
}

pub fn all(settings: &Settings) -> Vec<LoaderSource> {
    let mut sources = builtin();
    sources.extend(settings.loader_sources.iter().cloned());
    sources
}

pub fn find(settings: &Settings, id: &str) -> Result<LoaderSource> {
    all(settings)
        .into_iter()
        .find(|s| s.id == id)
        .ok_or_else(|| msg(format!("The mod loader “{id}” isn't set up. Add it again in Settings.")))
}

/// Validates and names a meta server someone adds in Settings.
pub fn custom_source(name: &str, url: &str, loader_path: &str) -> Result<LoaderSource> {
    let name = name.trim();
    let url = url.trim().trim_end_matches('/');
    let loader_path = Some(loader_path.trim().trim_matches('/')).filter(|p| !p.is_empty()).unwrap_or("loader");
    if name.is_empty() {
        return Err(msg("Give the loader a name."));
    }
    if !url.starts_with("https://") {
        return Err(msg("The meta server address needs to start with https://."));
    }
    let slug: String = name.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' }).collect();
    Ok(LoaderSource {
        id: format!("custom-{}", slug.trim_matches('-')),
        name: name.into(),
        kind: SourceKind::Meta { url: url.into(), loader_path: loader_path.into() },
        custom: true,
    })
}

/// Loader versions available for a game version, newest first. Empty if unsupported.
pub async fn versions(client: &Client, source: &LoaderSource, game: &str) -> Result<Vec<LoaderVersion>> {
    let mut list = match &source.kind {
        SourceKind::Meta { url, loader_path } => meta_versions(client, url, loader_path, game).await?,
        SourceKind::Forge => {
            let prefix = format!("{game}-");
            maven_versions(client, "https://maven.minecraftforge.net/net/minecraftforge/forge/maven-metadata.xml")
                .await?
                .into_iter()
                .filter_map(|v| {
                    let label = v.strip_prefix(&prefix)?.to_string();
                    Some(LoaderVersion { stable: true, label, version: v })
                })
                .collect()
        }
        SourceKind::NeoForge => {
            let Some(prefix) = neoforge_prefix(game) else { return Ok(vec![]) };
            maven_versions(client, "https://maven.neoforged.net/releases/net/neoforged/neoforge/maven-metadata.xml")
                .await?
                .into_iter()
                .filter(|v| v.starts_with(&prefix))
                .map(|v| LoaderVersion { stable: !v.contains("beta"), label: v.clone(), version: v })
                .collect()
        }
    };
    list.sort_by(|a, b| natural_cmp(&b.version, &a.version));
    Ok(list)
}

#[derive(Deserialize)]
struct MetaEntry {
    loader: MetaLoader,
}

#[derive(Deserialize)]
struct MetaLoader {
    version: String,
    stable: Option<bool>,
}

async fn meta_versions(client: &Client, url: &str, loader_path: &str, game: &str) -> Result<Vec<LoaderVersion>> {
    let res = client.get(format!("{url}/versions/{loader_path}/{game}")).send().await?;
    // Meta servers answer 400 or 404 for game versions they don't support.
    if matches!(res.status(), StatusCode::NOT_FOUND | StatusCode::BAD_REQUEST) {
        return Ok(vec![]);
    }
    let entries: Vec<MetaEntry> = res.error_for_status()?.json().await?;
    Ok(entries
        .into_iter()
        .map(|e| LoaderVersion { stable: e.loader.stable.unwrap_or(true), label: e.loader.version.clone(), version: e.loader.version })
        .collect())
}

async fn maven_versions(client: &Client, url: &str) -> Result<Vec<String>> {
    let xml = client.get(url).send().await?.error_for_status()?.text().await?;
    Ok(xml
        .split("<version>")
        .skip(1)
        .filter_map(|s| s.split_once("</version>").map(|(v, _)| v.trim().to_string()))
        .collect())
}

/// NeoForge numbers releases after the game: 1.21.4 -> "21.4.", 1.21 -> "21.0.",
/// and for year-based game versions 26.1 -> "26.1.0.", 26.1.2 -> "26.1.2.".
fn neoforge_prefix(game: &str) -> Option<String> {
    let (parts, width): (Vec<&str>, usize) = match game.strip_prefix("1.") {
        Some(rest) => (rest.split('.').collect(), 2),
        None => (game.split('.').collect(), 3),
    };
    if parts.iter().any(|p| p.parse::<u32>().is_err()) || parts.len() > width {
        return None; // snapshots and pre-releases
    }
    let mut padded: Vec<&str> = parts;
    padded.resize(width, "0");
    Some(format!("{}.", padded.join(".")))
}

/// Compares version strings piece by piece, treating digit runs as numbers.
fn natural_cmp(a: &str, b: &str) -> Ordering {
    fn pieces(s: &str) -> Vec<(bool, &str)> {
        let mut out = Vec::new();
        let mut start = 0;
        let bytes = s.as_bytes();
        for i in 1..=bytes.len() {
            if i == bytes.len() || bytes[i].is_ascii_digit() != bytes[start].is_ascii_digit() {
                out.push((bytes[start].is_ascii_digit(), &s[start..i]));
                start = i;
            }
        }
        out
    }
    for (x, y) in pieces(a).into_iter().zip(pieces(b)) {
        let ord = match (x, y) {
            ((true, x), (true, y)) => x.parse::<u128>().unwrap_or(0).cmp(&y.parse::<u128>().unwrap_or(0)),
            ((_, x), (_, y)) => x.cmp(y),
        };
        if ord != Ordering::Equal {
            return ord;
        }
    }
    a.len().cmp(&b.len())
}

/// Makes sure the instance's profile exists on disk and returns its ID.
pub async fn ensure_profile(
    client: &Client,
    paths: &Paths,
    settings: &Settings,
    instance: &Instance,
    verify: Verify,
    reporter: &mut impl Reporter,
) -> Result<String> {
    // Reuse the profile from the last install unless the person asked to verify everything.
    if let Some(id) = &instance.profile_id {
        if verify == Verify::Existing && profile::json_path(paths, id).exists() {
            return Ok(id.clone());
        }
    }
    let loader_version = || instance.loader_version.clone().ok_or_else(|| msg("Choose a mod loader version for this instance."));
    match instance.loader.as_str() {
        VANILLA => Ok(instance.version.clone()),
        CUSTOM => loader_version(),
        id => {
            let source = find(settings, id)?;
            let version = loader_version()?;
            reporter.stage(&format!("Installing {}…", source.name));
            match &source.kind {
                SourceKind::Meta { url, loader_path } => {
                    meta_profile(client, paths, &format!("{url}/versions/{loader_path}/{}/{version}/profile/json", instance.version)).await
                }
                SourceKind::Forge => {
                    let url = format!("https://maven.minecraftforge.net/net/minecraftforge/forge/{version}/forge-{version}-installer.jar");
                    run_installer(client, paths, settings, instance, &url, verify, reporter).await
                }
                SourceKind::NeoForge => {
                    let url = format!("https://maven.neoforged.net/releases/net/neoforged/neoforge/{version}/neoforge-{version}-installer.jar");
                    run_installer(client, paths, settings, instance, &url, verify, reporter).await
                }
            }
        }
    }
}

async fn meta_profile(client: &Client, paths: &Paths, url: &str) -> Result<String> {
    let res = client.get(url).send().await?;
    if !res.status().is_success() {
        return Err(msg("The mod loader's server doesn't have this version any more. Pick another loader version."));
    }
    let bytes = res.bytes().await?;
    save_profile(paths, &bytes)
}

/// Writes a profile JSON to `versions/<id>/<id>.json` after checking it parses.
pub fn save_profile(paths: &Paths, bytes: &[u8]) -> Result<String> {
    let json: crate::manifest::VersionJson =
        serde_json::from_slice(bytes).map_err(|e| msg(format!("That isn't a launcher profile: {e}")))?;
    if json.inherits_from.is_none() && json.main_class.is_none() {
        return Err(msg("That profile has neither a parent version nor a main class."));
    }
    if json.id.is_empty() || json.id.contains(['/', '\\']) || json.id.starts_with('.') {
        return Err(msg("That profile has an unusable ID."));
    }
    let path = profile::json_path(paths, &json.id);
    std::fs::create_dir_all(path.parent().unwrap_or(Path::new(".")))?;
    std::fs::write(path, bytes)?;
    Ok(json.id)
}

/// Runs a Forge-style installer with `--installClient`, which writes a profile and
/// generated libraries into our directory (it uses the official launcher's layout).
async fn run_installer(
    client: &Client,
    paths: &Paths,
    settings: &Settings,
    instance: &Instance,
    url: &str,
    verify: Verify,
    reporter: &mut impl Reporter,
) -> Result<String> {
    let file_name = url.rsplit('/').next().unwrap_or("installer.jar");
    let installer = paths.root.join("installers").join(file_name);
    download::fetch(client, Task { url: url.into(), path: installer.clone(), sha1: None }, Verify::Existing).await?;

    let id = installer_profile_id(&installer)?;
    // Verifying reruns the installer, which regenerates its patched files.
    if verify == Verify::Existing && profile::json_path(paths, &id).exists() {
        return Ok(id);
    }

    // The installer refuses to run without the official launcher's profile list.
    let profiles = paths.root.join("launcher_profiles.json");
    if !profiles.exists() {
        std::fs::write(&profiles, r#"{"profiles":{}}"#)?;
    }

    // The installer runs on the Java of the Minecraft version it patches.
    let vanilla = profile::resolve(client, paths, &instance.version, verify).await?;
    let need = crate::java::requirement(vanilla.java.as_ref());
    let java = crate::java::find_or_install(client, paths, settings, &need, verify, reporter).await?;
    reporter.stage(&format!("Running the {} installer…", if url.contains("neoforge") { "NeoForge" } else { "Forge" }));
    let mut child = Command::new(&java)
        .arg("-jar")
        .arg(&installer)
        .arg("--installClient")
        .arg(&paths.root)
        .current_dir(&paths.root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(crate::game::java_start_error)?;

    let mut out = BufReader::new(child.stdout.take().expect("piped")).lines();
    let mut err = BufReader::new(child.stderr.take().expect("piped")).lines();
    let mut tail = VecDeque::new();
    let (mut out_open, mut err_open) = (true, true);
    while out_open || err_open {
        let line = tokio::select! {
            line = out.next_line(), if out_open => {
                let line = line?;
                out_open = line.is_some();
                line
            }
            line = err.next_line(), if err_open => {
                let line = line?;
                err_open = line.is_some();
                line
            }
        };
        if let Some(line) = line {
            if tail.len() == 4 {
                tail.pop_front();
            }
            tail.push_back(line.clone());
            reporter.log(line);
        }
    }
    let status = child.wait().await?;
    if !status.success() || !profile::json_path(paths, &id).exists() {
        let detail = Vec::from(tail).join(" ");
        return Err(msg(format!("The installer didn't finish. {detail}")));
    }
    Ok(id)
}

fn installer_profile_id(installer: &Path) -> Result<String> {
    let mut archive = zip::ZipArchive::new(std::fs::File::open(installer)?)?;
    let file = archive.by_name("install_profile.json").map_err(|_| msg("That installer has no install profile."))?;
    let json: serde_json::Value = serde_json::from_reader(file)?;
    json.get("version")
        .and_then(|v| v.as_str())
        .map(String::from)
        // Installers before 2018 use a format that can't install headlessly.
        .ok_or_else(|| msg("This loader version is too old to install automatically. Pick a newer one."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neoforge_prefixes() {
        assert_eq!(neoforge_prefix("1.21.4").as_deref(), Some("21.4."));
        assert_eq!(neoforge_prefix("1.21").as_deref(), Some("21.0."));
        assert_eq!(neoforge_prefix("26.1").as_deref(), Some("26.1.0."));
        assert_eq!(neoforge_prefix("26.1.2").as_deref(), Some("26.1.2."));
        assert_eq!(neoforge_prefix("25w14a"), None);
    }

    #[test]
    fn natural_ordering() {
        assert_eq!(natural_cmp("21.4.161", "21.4.99"), Ordering::Greater);
        assert_eq!(natural_cmp("0.20.0-beta.9", "0.20.0-beta.10"), Ordering::Less);
    }
}
