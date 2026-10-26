use std::path::PathBuf;

use reqwest::Client;

use crate::download::{self, Task, Verify};
use crate::error::{Result, msg};
use crate::instances::Instance;
use crate::{java, loaders};
use crate::manifest::{AssetIndex, RESOURCES_URL};
use crate::paths::Paths;
use crate::profile::{self, Resolved};
use crate::settings::Settings;

/// Where install progress goes: the app's windows, or stdout in tests.
pub trait Reporter: Send {
    fn stage(&mut self, text: &str);
    fn files(&mut self, done: usize, total: usize);
    fn log(&mut self, line: String);
}

/// The client jar on the classpath, named after the profile as the official launcher does.
/// Loaders rely on the name: NeoForge and Forge skip `${version_name}.jar` when they
/// build their module layer, so a jar named after the vanilla version would clash.
pub fn client_jar_path(paths: &Paths, resolved: &Resolved) -> PathBuf {
    profile::jar_path(paths, &resolved.id)
}

/// Copies the vanilla client jar to the profile's own name when they differ.
/// `force` recopies it, for when Verify Game Files may have replaced the original.
pub fn link_client_jar(paths: &Paths, resolved: &Resolved, force: bool) -> Result<()> {
    let target = client_jar_path(paths, resolved);
    let source = profile::jar_path(paths, &resolved.jar_id);
    let same_size = |a: &PathBuf, b: &PathBuf| Some(std::fs::metadata(a).ok()?.len() == std::fs::metadata(b).ok()?.len());
    if target != source && (force || same_size(&target, &source) != Some(true)) {
        std::fs::create_dir_all(target.parent().expect("jar has a parent"))?;
        std::fs::copy(source, target)?;
    }
    Ok(())
}

/// Prepares the instance's loader, then downloads whatever its resolved profile needs and
/// isn't on disk yet: client jar, libraries, legacy natives, assets and log config.
pub struct Installed {
    pub resolved: Resolved,
    /// The `java` to launch it with.
    pub java: PathBuf,
    /// How many files were downloaded, Java's included.
    pub downloaded: usize,
}

pub async fn install(
    client: &Client,
    paths: &Paths,
    settings: &Settings,
    instance: &Instance,
    verify: Verify,
    reporter: &mut impl Reporter,
) -> Result<Installed> {
    let profile_id = loaders::ensure_profile(client, paths, settings, instance, verify, reporter).await?;
    reporter.stage(if verify == Verify::Full { "Verifying game files…" } else { "Checking game files…" });
    let resolved = profile::resolve(client, paths, &profile_id, verify).await?;

    let index_path = paths.assets().join("indexes").join(format!("{}.json", resolved.asset_index.id));
    let index_task =
        Task { url: resolved.asset_index.url.clone(), path: index_path.clone(), sha1: Some(resolved.asset_index.sha1.clone()) };
    let mut downloaded = usize::from(download::fetch(client, index_task, verify).await?);
    let index: AssetIndex = serde_json::from_slice(&std::fs::read(index_path)?)?;

    let mut tasks = vec![Task {
        url: resolved.client.url.clone(),
        path: profile::jar_path(paths, &resolved.jar_id),
        sha1: Some(resolved.client.sha1.clone()).filter(|s| !s.is_empty()),
    }];

    for lib in &resolved.libraries {
        for file in lib.artifact().into_iter().chain(lib.native_artifact()) {
            let path = paths.libraries().join(&file.path);
            match file.url {
                Some(url) => tasks.push(Task { url, path, sha1: file.sha1 }),
                // Installer-generated files have no URL; they must already exist.
                None if path.exists() => {}
                None => {
                    return Err(msg(format!(
                        "{} is missing and can't be downloaded. Reinstall the mod loader by creating the instance again.",
                        lib.name
                    )));
                }
            }
        }
    }

    for object in index.objects.values() {
        let prefix = &object.hash[..2];
        tasks.push(Task {
            url: format!("{RESOURCES_URL}/{prefix}/{}", object.hash),
            path: paths.assets().join("objects").join(prefix).join(&object.hash),
            sha1: Some(object.hash.clone()),
        });
    }

    if let Some(logging) = &resolved.logging {
        tasks.push(Task {
            url: logging.file.url.clone(),
            path: paths.assets().join("log_configs").join(&logging.file.id),
            sha1: Some(logging.file.sha1.clone()),
        });
    }

    downloaded += download::run(client, tasks, verify, |done, total| reporter.files(done, total)).await?;
    link_client_jar(paths, &resolved, verify == Verify::Full)?;
    // An instance's own Java beats automatic Java and the one in Settings.
    let java = match instance.options.java_path.as_deref().filter(|p| !p.trim().is_empty()) {
        Some(path) => PathBuf::from(path),
        None => {
            let need = java::requirement(resolved.java.as_ref());
            java::find_or_install(client, paths, settings, &need, verify, reporter).await?
        }
    };
    Ok(Installed { resolved, java, downloaded })
}
