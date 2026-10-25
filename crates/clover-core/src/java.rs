//! Finds the Java each Minecraft version needs, downloading Mojang's runtime when the
//! computer doesn't have one.
//!
//! Version profiles name a runtime component ("java-runtime-delta" is Java 21, "jre-legacy"
//! is Java 8). The order of preference:
//! 1. A Java path set in Settings, used for everything.
//! 2. A runtime Clover downloaded before, in `runtimes/<component>`.
//! 3. A Java on the computer whose major version matches exactly.
//! 4. Mojang's runtime for the component, downloaded now.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use reqwest::Client;
use serde::Deserialize;
use std::collections::HashMap;

use crate::download::{self, Task, Verify};
use crate::error::{Result, msg};
use crate::install::Reporter;
use crate::manifest::JavaVersion;
use crate::paths::Paths;
use crate::settings::Settings;

const RUNTIME_INDEX: &str =
    "https://piston-meta.mojang.com/v1/products/java-runtime/2ec0cc96c44e5a76b9c8b7c39df7210883d12871/all.json";
/// Written once every file of a runtime is in place.
const COMPLETE_MARKER: &str = ".clover-complete";

/// What a version asks for. Profiles from before Mojang added `javaVersion` get Java 8.
pub fn requirement(java: Option<&JavaVersion>) -> JavaVersion {
    match java {
        Some(java) if !java.component.is_empty() => java.clone(),
        _ => JavaVersion { component: "jre-legacy".into(), major_version: java.map_or(8, |j| j.major_version) },
    }
}

/// Returns the `java` executable to run a version with.
pub async fn find_or_install(
    client: &Client,
    paths: &Paths,
    settings: &Settings,
    need: &JavaVersion,
    verify: Verify,
    reporter: &mut impl Reporter,
) -> Result<PathBuf> {
    if let Some(custom) = settings.custom_java() {
        return Ok(PathBuf::from(custom));
    }
    let dir = paths.root.join("runtimes").join(&need.component);
    if verify == Verify::Existing {
        if dir.join(COMPLETE_MARKER).exists() {
            return Ok(executable(&dir));
        }
        if let Some(system) = system_java(need.major_version).await {
            return Ok(system);
        }
    }
    reporter.stage(&format!("Downloading Java {}…", need.major_version));
    install(client, &dir, &need.component, verify, reporter).await?;
    Ok(executable(&dir))
}

fn executable(dir: &Path) -> PathBuf {
    if cfg!(target_os = "macos") {
        dir.join("jre.bundle/Contents/Home/bin/java")
    } else if cfg!(windows) {
        dir.join("bin/javaw.exe")
    } else {
        dir.join("bin/java")
    }
}

/// Mojang's platform names, best first. Apple silicon falls back to Intel builds (run by
/// Rosetta) for runtimes Mojang only ships for Intel, such as Java 8, which old versions'
/// Intel-only native libraries need anyway.
fn platforms() -> &'static [&'static str] {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => &["mac-os-arm64", "mac-os"],
        ("macos", _) => &["mac-os"],
        ("windows", "aarch64") => &["windows-arm64", "windows-x64"],
        ("windows", "x86") => &["windows-x86"],
        ("windows", _) => &["windows-x64"],
        ("linux", "x86") => &["linux-i386"],
        _ => &["linux"],
    }
}

#[derive(Deserialize)]
struct RuntimeEntry {
    manifest: ManifestRef,
    version: RuntimeVersion,
}

#[derive(Deserialize)]
struct ManifestRef {
    sha1: String,
    url: String,
}

#[derive(Deserialize)]
struct RuntimeVersion {
    name: String,
}

#[derive(Deserialize)]
struct RuntimeManifest {
    files: HashMap<String, RuntimeFile>,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum RuntimeFile {
    Directory,
    File {
        #[serde(default)]
        executable: bool,
        downloads: FileDownloads,
    },
    Link {
        target: String,
    },
}

#[derive(Deserialize)]
struct FileDownloads {
    raw: RawDownload,
}

#[derive(Deserialize)]
struct RawDownload {
    sha1: String,
    url: String,
}

/// Mojang serves some of these files with a byte-order mark, which JSON parsers reject.
async fn get_json<T: serde::de::DeserializeOwned>(client: &Client, url: &str) -> Result<T> {
    let bytes = client.get(url).send().await?.error_for_status()?.bytes().await?;
    let body = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&bytes);
    Ok(serde_json::from_slice(body)?)
}

async fn install(client: &Client, dir: &Path, component: &str, verify: Verify, reporter: &mut impl Reporter) -> Result<()> {
    let index: HashMap<String, HashMap<String, Vec<RuntimeEntry>>> = get_json(client, RUNTIME_INDEX).await?;
    let entry = platforms()
        .iter()
        .find_map(|platform| index.get(*platform)?.get(component)?.first())
        .ok_or_else(|| msg(format!("Mojang doesn't offer the Java this version needs ({component}) for this computer.")))?;
    let manifest_bytes = client.get(&entry.manifest.url).send().await?.error_for_status()?.bytes().await?;
    if hex::encode(<sha1::Sha1 as sha1::Digest>::digest(&manifest_bytes)) != entry.manifest.sha1 {
        return Err(msg("Java's file list didn't match its checksum. Try again."));
    }
    let manifest: RuntimeManifest = serde_json::from_slice(&manifest_bytes)?;

    let mut tasks = Vec::new();
    let mut executables = Vec::new();
    let mut links = Vec::new();
    for (name, file) in &manifest.files {
        // Every path comes from Mojang's manifest; still, never write outside the runtime folder.
        if name.split('/').any(|part| part == "..") || name.starts_with('/') {
            return Err(msg("Java's file list contains an unsafe path."));
        }
        let path = dir.join(name);
        match file {
            RuntimeFile::Directory => std::fs::create_dir_all(&path)?,
            RuntimeFile::File { executable, downloads } => {
                if *executable {
                    executables.push(path.clone());
                }
                tasks.push(Task { url: downloads.raw.url.clone(), path, sha1: Some(downloads.raw.sha1.clone()) });
            }
            RuntimeFile::Link { target } => links.push((path, target.clone())),
        }
    }

    download::run(client, tasks, verify, |done, total| reporter.files(done, total)).await?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for path in executables {
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))?;
        }
        for (path, target) in links {
            let _ = std::fs::remove_file(&path);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::os::unix::fs::symlink(target, &path)?;
        }
    }
    #[cfg(not(unix))]
    let _ = (executables, links); // Windows runtimes ship no links, and files need no mode bits.

    std::fs::write(dir.join(COMPLETE_MARKER), &entry.version.name)?;
    Ok(())
}

/// A Java already on the computer with exactly this major version.
async fn system_java(major: u32) -> Option<PathBuf> {
    // On Apple silicon a local Java 8 is usually an arm64 build, which can't load the
    // Intel-only native libraries of the versions that need Java 8.
    if cfg!(all(target_os = "macos", target_arch = "aarch64")) && major <= 8 {
        return None;
    }
    let mut candidates = Vec::new();
    if cfg!(target_os = "macos") {
        let out = tokio::process::Command::new("/usr/libexec/java_home").arg("-v").arg(major.to_string()).output().await.ok()?;
        if out.status.success() {
            candidates.push(PathBuf::from(String::from_utf8_lossy(&out.stdout).trim()).join("bin/java"));
        }
    }
    if let Some(home) = std::env::var_os("JAVA_HOME") {
        candidates.push(PathBuf::from(home).join(if cfg!(windows) { "bin/javaw.exe" } else { "bin/java" }));
    }
    candidates.push(PathBuf::from("java"));
    for candidate in candidates {
        if java_major(&candidate).await == Some(major) {
            return Some(candidate);
        }
    }
    None
}

/// Reads the major version from `java -version`: "1.8.0_402" is 8, "21.0.2" is 21.
async fn java_major(java: &Path) -> Option<u32> {
    let out = tokio::process::Command::new(java).arg("-version").stdout(Stdio::null()).output().await.ok()?;
    parse_major(&String::from_utf8_lossy(&out.stderr))
}

fn parse_major(version_output: &str) -> Option<u32> {
    let quoted = version_output.split('"').nth(1)?;
    let mut parts = quoted.split(['.', '_', '-', '+']);
    match parts.next()?.parse().ok()? {
        1 => parts.next()?.parse().ok(),
        major => Some(major),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_java_versions() {
        assert_eq!(parse_major(r#"openjdk version "21.0.2" 2024-01-16"#), Some(21));
        assert_eq!(parse_major(r#"java version "1.8.0_402""#), Some(8));
        assert_eq!(parse_major(r#"openjdk version "26" 2026-03-17"#), Some(26));
        assert_eq!(parse_major("no version here"), None);
    }
}
