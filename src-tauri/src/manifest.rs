//! Mojang's version manifest and the launcher profile ("version JSON") format.
//! Reference: https://minecraft.wiki/w/Client.json
//!
//! Mod loaders ship partial profiles that `inheritsFrom` a vanilla version, so every
//! field except `id` is optional here. `profile::resolve` merges a chain into one.

use std::collections::HashMap;

use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::error::Result;

const MANIFEST_URL: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
const MOJANG_LIBRARIES: &str = "https://libraries.minecraft.net/";
pub const RESOURCES_URL: &str = "https://resources.download.minecraft.net";

#[derive(Debug, Deserialize)]
pub struct VersionManifest {
    pub versions: Vec<VersionEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionEntry {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub url: String,
    pub sha1: String,
    pub release_time: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionJson {
    pub id: String,
    pub inherits_from: Option<String>,
    pub main_class: Option<String>,
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub assets: Option<String>,
    pub asset_index: Option<AssetIndexRef>,
    pub downloads: Option<VersionDownloads>,
    #[serde(default)]
    pub libraries: Vec<Library>,
    /// 1.13+
    pub arguments: Option<Arguments>,
    /// Before 1.13
    pub minecraft_arguments: Option<String>,
    pub java_version: Option<JavaVersion>,
    pub logging: Option<Logging>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AssetIndexRef {
    pub id: String,
    pub url: String,
    pub sha1: String,
}

#[derive(Debug, Deserialize)]
pub struct VersionDownloads {
    pub client: Artifact,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Artifact {
    pub path: Option<String>,
    /// Empty for files a loader installer generates locally.
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub sha1: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Library {
    pub name: String,
    pub downloads: Option<LibraryDownloads>,
    /// Maven repository for libraries listed only by coordinate (Fabric-style profiles).
    pub url: Option<String>,
    pub sha1: Option<String>,
    #[serde(default)]
    pub rules: Vec<Rule>,
    /// Legacy natives: OS name -> classifier key, e.g. "natives-osx".
    pub natives: Option<HashMap<String, String>>,
    pub extract: Option<Extract>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LibraryDownloads {
    pub artifact: Option<Artifact>,
    #[serde(default)]
    pub classifiers: HashMap<String, Artifact>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Extract {
    #[serde(default)]
    pub exclude: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Rule {
    pub action: String,
    pub os: Option<OsRule>,
    pub features: Option<HashMap<String, bool>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OsRule {
    pub name: Option<String>,
    pub arch: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Arguments {
    #[serde(default)]
    pub game: Vec<Argument>,
    #[serde(default)]
    pub jvm: Vec<Argument>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum Argument {
    Plain(String),
    Conditional { rules: Vec<Rule>, value: ArgValue },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum ArgValue {
    One(String),
    Many(Vec<String>),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JavaVersion {
    pub major_version: u32,
}

#[derive(Debug, Deserialize)]
pub struct Logging {
    pub client: Option<LoggingClient>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LoggingClient {
    pub argument: String,
    pub file: LogFile,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LogFile {
    pub id: String,
    pub url: String,
    pub sha1: String,
}

#[derive(Debug, Deserialize)]
pub struct AssetIndex {
    pub objects: HashMap<String, AssetObject>,
}

#[derive(Debug, Deserialize)]
pub struct AssetObject {
    pub hash: String,
}

/// A library file on disk, and where to get it if it's missing.
pub struct LibraryFile {
    pub path: String,
    pub url: Option<String>,
    pub sha1: Option<String>,
}

pub fn os_name() -> &'static str {
    if cfg!(target_os = "macos") {
        "osx"
    } else if cfg!(windows) {
        "windows"
    } else {
        "linux"
    }
}

fn os_arch() -> &'static str {
    if cfg!(target_arch = "x86") {
        "x86"
    } else if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        "x86_64"
    }
}

/// Mojang's rule semantics: the last matching rule wins; no rules means allowed.
/// Feature-gated rules (demo user, custom resolution, quick play) never match.
pub fn rules_allow(rules: &[Rule]) -> bool {
    if rules.is_empty() {
        return true;
    }
    let mut allowed = false;
    for rule in rules {
        let os_matches = rule.os.as_ref().is_none_or(|os| {
            os.name.as_deref().is_none_or(|n| n == os_name())
                && os.arch.as_deref().is_none_or(|a| a == os_arch())
        });
        if os_matches && rule.features.is_none() {
            allowed = rule.action == "allow";
        }
    }
    allowed
}

/// `group:artifact:version[:classifier][@ext]` -> `group/path/artifact/version/artifact-version[-classifier].ext`
pub fn maven_path(coordinate: &str) -> Option<String> {
    let (coordinate, ext) = coordinate.split_once('@').unwrap_or((coordinate, "jar"));
    let mut parts = coordinate.split(':');
    let (group, artifact, version) = (parts.next()?, parts.next()?, parts.next()?);
    let classifier = parts.next().map(|c| format!("-{c}")).unwrap_or_default();
    Some(format!("{}/{artifact}/{version}/{artifact}-{version}{classifier}.{ext}", group.replace('.', "/")))
}

impl Library {
    /// The jar that goes on the classpath, if this library has one.
    pub fn artifact(&self) -> Option<LibraryFile> {
        match &self.downloads {
            // Mojang and installer-style entries say exactly where the file is.
            Some(downloads) => downloads.artifact.as_ref().and_then(|a| self.file(a)),
            // Coordinate-only entries with a natives map hold nothing but natives.
            None if self.natives.is_some() => None,
            // Fabric-style entries give a coordinate and a repository.
            None => self.from_repository(&self.name, self.sha1.clone()),
        }
    }

    /// The legacy native classifier for this OS, if the library has one.
    pub fn native_artifact(&self) -> Option<LibraryFile> {
        let classifier = self.natives.as_ref()?.get(os_name())?.replace("${arch}", "64");
        match &self.downloads {
            Some(downloads) => self.file(downloads.classifiers.get(&classifier)?),
            None => self.from_repository(&format!("{}:{classifier}", self.name), None),
        }
    }

    fn from_repository(&self, coordinate: &str, sha1: Option<String>) -> Option<LibraryFile> {
        let path = maven_path(coordinate)?;
        let repo = self.url.as_deref().unwrap_or(MOJANG_LIBRARIES);
        Some(LibraryFile { url: Some(format!("{}/{path}", repo.trim_end_matches('/'))), path, sha1 })
    }

    /// Identity for overriding: a loader's copy of a library replaces the vanilla one
    /// regardless of version, but different classifiers are different libraries.
    pub fn key(&self) -> String {
        let name = self.name.split('@').next().unwrap_or_default();
        let parts: Vec<&str> = name.split(':').collect();
        match parts.as_slice() {
            // Old versions ship ASM as one "asm-all" jar; loaders bring the split modules.
            // Treating them as the same library stops both landing on the classpath.
            ["org.ow2.asm", "asm-all", ..] => "org.ow2.asm:asm".into(),
            [group, artifact, _version, classifier, ..] => format!("{group}:{artifact}:{classifier}"),
            [group, artifact, ..] => format!("{group}:{artifact}"),
            _ => name.to_string(),
        }
    }

    fn file(&self, artifact: &Artifact) -> Option<LibraryFile> {
        Some(LibraryFile {
            path: artifact.path.clone().or_else(|| maven_path(&self.name))?,
            url: Some(artifact.url.clone()).filter(|u| !u.is_empty()),
            sha1: Some(artifact.sha1.clone()).filter(|s| !s.is_empty()),
        })
    }
}

pub async fn fetch_manifest(client: &Client) -> Result<VersionManifest> {
    Ok(client.get(MANIFEST_URL).send().await?.error_for_status()?.json().await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maven_paths() {
        assert_eq!(
            maven_path("net.fabricmc:fabric-loader:0.20.1").unwrap(),
            "net/fabricmc/fabric-loader/0.20.1/fabric-loader-0.20.1.jar"
        );
        assert_eq!(
            maven_path("org.lwjgl:lwjgl:3.3.3:natives-macos@zip").unwrap(),
            "org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3-natives-macos.zip"
        );
    }

    /// Parses the live latest-release JSON. Run with `cargo test -- --ignored`.
    #[tokio::test]
    #[ignore]
    async fn parses_latest_release() {
        let client = Client::new();
        let manifest = fetch_manifest(&client).await.unwrap();
        let entry = manifest.versions.iter().find(|v| v.kind == "release").unwrap();
        let json: VersionJson = client.get(&entry.url).send().await.unwrap().json().await.unwrap();
        assert_eq!(json.id, entry.id);
        assert!(json.libraries.iter().any(|l| rules_allow(&l.rules)));
    }
}
