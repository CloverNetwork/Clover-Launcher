use std::path::PathBuf;

use reqwest::Client;
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::download::{self, Task};
use crate::error::{Result, msg};
use crate::manifest::{self, AssetIndex, RESOURCES_URL, VersionJson, rules_allow};
use crate::paths::Paths;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub instance_id: String,
    pub done: usize,
    pub total: usize,
}

pub fn version_json_path(paths: &Paths, version: &str) -> PathBuf {
    paths.version_dir(version).join(format!("{version}.json"))
}

pub fn client_jar_path(paths: &Paths, version: &str) -> PathBuf {
    paths.version_dir(version).join(format!("{version}.jar"))
}

pub fn load_version(paths: &Paths, version: &str) -> Result<VersionJson> {
    Ok(serde_json::from_slice(&std::fs::read(version_json_path(paths, version))?)?)
}

/// Downloads the client, libraries, natives, assets and log config for a version.
pub async fn install(app: &AppHandle, client: &Client, paths: &Paths, instance_id: &str, version: &str) -> Result<VersionJson> {
    let manifest = manifest::fetch_manifest(client).await?;
    let entry = manifest
        .versions
        .iter()
        .find(|v| v.id == version)
        .ok_or_else(|| msg(format!("Minecraft {version} isn't in Mojang's version list.")))?;
    download::fetch(
        client,
        Task { url: entry.url.clone(), path: version_json_path(paths, version), sha1: Some(entry.sha1.clone()) },
    )
    .await?;
    let json = load_version(paths, version)?;

    let index_path = paths.assets().join("indexes").join(format!("{}.json", json.asset_index.id));
    download::fetch(
        client,
        Task { url: json.asset_index.url.clone(), path: index_path.clone(), sha1: Some(json.asset_index.sha1.clone()) },
    )
    .await?;
    let index: AssetIndex = serde_json::from_slice(&std::fs::read(index_path)?)?;

    let mut tasks = vec![Task {
        url: json.downloads.client.url.clone(),
        path: client_jar_path(paths, version),
        sha1: Some(json.downloads.client.sha1.clone()),
    }];

    for lib in json.libraries.iter().filter(|l| rules_allow(&l.rules)) {
        let artifacts = lib.downloads.as_ref().and_then(|d| d.artifact.as_ref()).into_iter().chain(lib.native_artifact());
        for artifact in artifacts {
            let Some(path) = &artifact.path else { continue };
            tasks.push(Task {
                url: artifact.url.clone(),
                path: paths.libraries().join(path),
                sha1: Some(artifact.sha1.clone()),
            });
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

    if let Some(logging) = json.logging.as_ref().and_then(|l| l.client.as_ref()) {
        tasks.push(Task {
            url: logging.file.url.clone(),
            path: paths.assets().join("log_configs").join(&logging.file.id),
            sha1: Some(logging.file.sha1.clone()),
        });
    }

    // Emit roughly every 1% so the UI stays smooth without flooding IPC.
    let mut last_emitted = 0;
    download::run(client, tasks, |done, total| {
        if done == total || done - last_emitted >= (total / 100).max(1) {
            last_emitted = done;
            let _ = app.emit("install-progress", Progress { instance_id: instance_id.to_string(), done, total });
        }
    })
    .await?;

    Ok(json)
}
