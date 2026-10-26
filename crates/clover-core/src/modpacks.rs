//! Modpacks: Modrinth `.mrpack` files and CurseForge pack zips, from disk or straight
//! from either site. Importing makes a new instance with the pack's Minecraft version and
//! loader, downloads its files, and copies its overrides (configs, resource packs, ...).
//! References: https://support.modrinth.com/en/articles/8802351-modrinth-modpack-format-mrpack
//! and https://docs.curseforge.com/rest-api/

use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use serde::Serialize;
use serde_json::{Value, json as body};

use crate::download::{self, Task, Verify};
use crate::error::{Result, msg};
use crate::install::Reporter;
use crate::instances::{self, Instance};
use crate::mods::{self, CURSEFORGE, CURSEFORGE_MINECRAFT, InstalledMod, Kind, MODRINTH, PAGE_SIZE, SearchHit, SearchPage, Shelf, Sites, Source, json, text};
use crate::paths::Paths;

const CURSEFORGE_MODPACKS_CLASS: u32 = 4471;

/// A CurseForge file the author doesn't let other launchers download. The person has to
/// fetch it from the site and drop it into the folder named here.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockedFile {
    pub name: String,
    pub url: String,
    pub folder: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Imported {
    pub instance: Instance,
    pub blocked: Vec<BlockedFile>,
}

pub async fn search(sites: &Sites<'_>, source: Source, query: &str, offset: u32) -> Result<SearchPage> {
    let sort_by_popularity = query.trim().is_empty();
    match source {
        Source::Modrinth => {
            let res = json(sites.client.get(format!("{MODRINTH}/search")).query(&[
                ("query", query),
                ("facets", r#"[["project_type:modpack"]]"#),
                ("index", if sort_by_popularity { "downloads" } else { "relevance" }),
                ("limit", &PAGE_SIZE.to_string()),
                ("offset", &offset.to_string()),
            ]))
            .await?;
            let hits = res["hits"].as_array().cloned().unwrap_or_default();
            Ok(SearchPage {
                total: res["total_hits"].as_u64().unwrap_or(0),
                hits: hits
                    .iter()
                    .map(|h| SearchHit {
                        source,
                        id: text(h, "project_id"),
                        title: text(h, "title"),
                        author: text(h, "author"),
                        description: text(h, "description"),
                        icon_url: h["icon_url"].as_str().filter(|u| !u.is_empty()).map(String::from),
                        downloads: h["downloads"].as_u64().unwrap_or(0),
                        page_url: format!("https://modrinth.com/modpack/{}", text(h, "slug")),
                    })
                    .collect(),
            })
        }
        Source::Curseforge => {
            let res = json(sites.curseforge(format!("{CURSEFORGE}/mods/search"))?.query(&[
                ("gameId", CURSEFORGE_MINECRAFT.to_string()),
                ("classId", CURSEFORGE_MODPACKS_CLASS.to_string()),
                ("searchFilter", query.to_string()),
                ("sortField", if sort_by_popularity { "2" } else { "1" }.into()),
                ("sortOrder", "desc".into()),
                ("pageSize", PAGE_SIZE.to_string()),
                ("index", offset.to_string()),
            ]))
            .await?;
            let hits = res["data"].as_array().cloned().unwrap_or_default();
            Ok(SearchPage {
                total: res["pagination"]["totalCount"].as_u64().unwrap_or(0),
                hits: hits
                    .iter()
                    .map(|h| SearchHit {
                        source,
                        id: h["id"].as_u64().unwrap_or(0).to_string(),
                        title: text(h, "name"),
                        author: h["authors"][0]["name"].as_str().unwrap_or_default().to_string(),
                        description: text(h, "summary"),
                        icon_url: h["logo"]["thumbnailUrl"].as_str().map(String::from),
                        downloads: h["downloadCount"].as_f64().unwrap_or(0.0) as u64,
                        page_url: text(&h["links"], "websiteUrl"),
                    })
                    .collect(),
            })
        }
    }
}

/// Downloads a pack's newest release file into `packs/`, ready to import.
pub async fn download(sites: &Sites<'_>, paths: &Paths, source: Source, project: &str) -> Result<PathBuf> {
    let (url, name, sha1) = match source {
        Source::Modrinth => {
            let versions = json(sites.client.get(format!("{MODRINTH}/project/{project}/version"))).await?;
            let versions = versions.as_array().cloned().unwrap_or_default();
            let version = versions
                .iter()
                .find(|v| v["version_type"] == "release")
                .or(versions.first())
                .ok_or_else(|| msg("That modpack has no versions to install."))?;
            let files = version["files"].as_array().cloned().unwrap_or_default();
            let file = files.iter().find(|f| f["primary"] == true).or(files.first()).ok_or_else(|| msg("That modpack version has no file."))?;
            (text(file, "url"), text(file, "filename"), file["hashes"]["sha1"].as_str().map(String::from))
        }
        Source::Curseforge => {
            let files = json(sites.curseforge(format!("{CURSEFORGE}/mods/{project}/files"))?.query(&[("pageSize", "50")])).await?;
            let mut files = files["data"].as_array().cloned().unwrap_or_default();
            files.sort_by(|a, b| text(b, "fileDate").cmp(&text(a, "fileDate")));
            let file = files
                .iter()
                .find(|f| f["releaseType"] == 1 && f["downloadUrl"].is_string())
                .or(files.iter().find(|f| f["downloadUrl"].is_string()))
                .ok_or_else(|| msg("That modpack's author doesn't allow other launchers to download it."))?;
            let sha1 = file["hashes"].as_array().into_iter().flatten().find(|h| h["algo"] == 1).map(|h| text(h, "value"));
            (text(file, "downloadUrl"), text(file, "fileName"), sha1)
        }
    };
    let name = Path::new(&name).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "modpack.zip".into());
    let path = paths.root.join("packs").join(name);
    download::fetch(sites.client, Task { url, path: path.clone(), sha1 }, Verify::Full).await?;
    Ok(path)
}

/// Makes a new instance from a pack file. If anything fails, the half-made instance is
/// removed again.
pub async fn import(sites: &Sites<'_>, paths: &Paths, file: &Path, reporter: &mut impl Reporter) -> Result<Imported> {
    let mut archive = zip::ZipArchive::new(File::open(file).map_err(|e| msg(format!("Couldn't open that file ({e}).")))?)
        .map_err(|_| msg("That file isn't a modpack. Choose a .mrpack file or a CurseForge modpack zip."))?;
    let read = |archive: &mut zip::ZipArchive<File>, name: &str| -> Option<Value> {
        let mut entry = archive.by_name(name).ok()?;
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).ok()?;
        serde_json::from_slice(&bytes).ok()
    };
    if let Some(index) = read(&mut archive, "modrinth.index.json") {
        let pack = Pack::modrinth(&index)?;
        let instance = instances::create(paths, &pack.name, &pack.minecraft, &pack.loader, pack.loader_version.clone())?;
        let result = fill_modrinth(sites, paths, &instance, &index, &mut archive, reporter).await;
        finish(paths, instance, result)
    } else if let Some(manifest) = read(&mut archive, "manifest.json") {
        let pack = Pack::curseforge(&manifest)?;
        let instance = instances::create(paths, &pack.name, &pack.minecraft, &pack.loader, pack.loader_version.clone())?;
        let result = fill_curseforge(sites, paths, &instance, &manifest, &mut archive, reporter).await;
        finish(paths, instance, result)
    } else {
        Err(msg("That file isn't a modpack. Choose a .mrpack file or a CurseForge modpack zip."))
    }
}

fn finish(paths: &Paths, instance: Instance, result: Result<Vec<BlockedFile>>) -> Result<Imported> {
    match result {
        Ok(blocked) => Ok(Imported { instance, blocked }),
        Err(e) => {
            let _ = instances::delete(paths, &instance.id);
            Err(e)
        }
    }
}

/// What a pack needs: its name, Minecraft version and loader in Clover's terms.
#[derive(Debug, PartialEq)]
struct Pack {
    name: String,
    minecraft: String,
    loader: String,
    loader_version: Option<String>,
}

impl Pack {
    fn modrinth(index: &Value) -> Result<Self> {
        let deps = &index["dependencies"];
        let minecraft = deps["minecraft"].as_str().ok_or_else(|| msg("The modpack doesn't say which Minecraft version it's for."))?;
        let dep = |key: &str| deps[key].as_str().map(String::from);
        let (loader, version) = if let Some(v) = dep("fabric-loader") {
            ("fabric", Some(v))
        } else if let Some(v) = dep("quilt-loader") {
            ("quilt", Some(v))
        } else if let Some(v) = dep("neoforge") {
            ("neoforge", Some(v))
        } else if let Some(v) = dep("forge") {
            // Clover names Forge versions the way Forge's own files do: "1.20.1-47.2.0".
            ("forge", Some(format!("{minecraft}-{v}")))
        } else {
            ("vanilla", None)
        };
        Ok(Self { name: pack_name(index), minecraft: minecraft.into(), loader: loader.into(), loader_version: version })
    }

    fn curseforge(manifest: &Value) -> Result<Self> {
        let minecraft = manifest["minecraft"]["version"].as_str().ok_or_else(|| msg("The modpack doesn't say which Minecraft version it's for."))?;
        let loaders = manifest["minecraft"]["modLoaders"].as_array().cloned().unwrap_or_default();
        let id = loaders.iter().find(|l| l["primary"] == true).or(loaders.first()).map(|l| text(l, "id")).unwrap_or_default();
        let (loader, version) = match id.split_once('-') {
            Some(("forge", v)) => ("forge", Some(format!("{minecraft}-{v}"))),
            Some(("neoforge", v)) => ("neoforge", Some(v.to_string())),
            Some(("fabric", v)) => ("fabric", Some(v.to_string())),
            Some(("quilt", v)) => ("quilt", Some(v.to_string())),
            _ if id.is_empty() => ("vanilla", None),
            _ => return Err(msg(format!("Clover doesn't know the mod loader “{id}” this modpack uses."))),
        };
        Ok(Self { name: pack_name(manifest), minecraft: minecraft.into(), loader: loader.into(), loader_version: version })
    }
}

fn pack_name(json: &Value) -> String {
    let name = json["name"].as_str().unwrap_or("Modpack");
    match json["versionId"].as_str().or(json["version"].as_str()) {
        Some(version) if !version.is_empty() => format!("{name} {version}"),
        _ => name.to_string(),
    }
}

/// Joins a path from a pack onto the game folder, refusing anything that would land outside it.
fn inside(game_dir: &Path, relative: &str) -> Result<PathBuf> {
    let relative = Path::new(relative);
    let safe = relative.components().all(|c| matches!(c, Component::Normal(_)));
    if !safe || relative.as_os_str().is_empty() {
        return Err(msg(format!("The modpack lists an unsafe file path: {}", relative.display())));
    }
    Ok(game_dir.join(relative))
}

/// Copies the pack's override folders over the game folder, in order.
fn extract_overrides(archive: &mut zip::ZipArchive<File>, game_dir: &Path, folders: &[&str]) -> Result<()> {
    for folder in folders {
        let prefix = format!("{}/", folder.trim_end_matches('/'));
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i)?;
            let Some(name) = entry.enclosed_name() else { continue };
            let name = name.to_string_lossy().replace('\\', "/");
            let Some(relative) = name.strip_prefix(&prefix) else { continue };
            if relative.is_empty() || entry.is_dir() {
                continue;
            }
            let out = inside(game_dir, relative)?;
            std::fs::create_dir_all(out.parent().expect("has parent"))?;
            std::io::copy(&mut entry, &mut File::create(out)?)?;
        }
    }
    Ok(())
}

async fn fill_modrinth(
    sites: &Sites<'_>,
    paths: &Paths,
    instance: &Instance,
    index: &Value,
    archive: &mut zip::ZipArchive<File>,
    reporter: &mut impl Reporter,
) -> Result<Vec<BlockedFile>> {
    let game_dir = paths.instance(&instance.id).join("minecraft");
    let mut tasks = Vec::new();
    for file in index["files"].as_array().into_iter().flatten() {
        // Server-only files stay out of a client.
        if file["env"]["client"] == "unsupported" {
            continue;
        }
        let url = file["downloads"].as_array().and_then(|d| d.iter().find_map(|u| u.as_str().filter(|u| u.starts_with("https://"))));
        let Some(url) = url else { return Err(msg(format!("The modpack has no download for {}.", text(file, "path")))) };
        tasks.push(Task {
            url: url.to_string(),
            path: inside(&game_dir, &text(file, "path"))?,
            sha1: file["hashes"]["sha1"].as_str().map(String::from),
        });
    }
    reporter.stage(&format!("Downloading {} files…", tasks.len()));
    download::run(sites.client, tasks, Verify::Existing, |done, total| reporter.files(done, total)).await?;
    reporter.stage("Copying the modpack's settings…");
    extract_overrides(archive, &game_dir, &["overrides", "client-overrides"])?;
    Ok(vec![])
}

async fn fill_curseforge(
    sites: &Sites<'_>,
    paths: &Paths,
    instance: &Instance,
    manifest: &Value,
    archive: &mut zip::ZipArchive<File>,
    reporter: &mut impl Reporter,
) -> Result<Vec<BlockedFile>> {
    let game_dir = paths.instance(&instance.id).join("minecraft");
    let entries: Vec<&Value> = manifest["files"].as_array().into_iter().flatten().filter(|f| f["required"] != false).collect();
    let file_ids: Vec<u64> = entries.iter().filter_map(|f| f["fileID"].as_u64()).collect();
    let mod_ids: Vec<u64> = entries.iter().filter_map(|f| f["projectID"].as_u64()).collect();

    let mut blocked = Vec::new();
    if !file_ids.is_empty() {
        reporter.stage("Looking up the modpack's files…");
        let files = json(sites.curseforge_post(format!("{CURSEFORGE}/mods/files"))?.json(&body!({ "fileIds": file_ids }))).await?;
        let projects = json(sites.curseforge_post(format!("{CURSEFORGE}/mods"))?.json(&body!({ "modIds": mod_ids }))).await?;
        let projects: HashMap<u64, &Value> =
            projects["data"].as_array().into_iter().flatten().filter_map(|p| Some((p["id"].as_u64()?, p))).collect();

        let mut tasks = Vec::new();
        let mut records: HashMap<Kind, Vec<InstalledMod>> = HashMap::new();
        for file in files["data"].as_array().into_iter().flatten() {
            let project = file["modId"].as_u64().and_then(|id| projects.get(&id).copied());
            // Data packs in a pack are usually loaded by a mod from a folder of its own,
            // so anything that isn't a resource or shader pack goes in with the mods.
            let kind = match Kind::from_curseforge_class(project.and_then(|p| p["classId"].as_u64())) {
                Kind::Datapack => Kind::Mod,
                kind => kind,
            };
            let folder = kind.folder();
            let name = text(file, "fileName");
            let path = inside(&game_dir, &format!("{folder}/{name}"))?;
            let Some(url) = file["downloadUrl"].as_str() else {
                let page = project.map(|p| text(&p["links"], "websiteUrl")).unwrap_or_default();
                blocked.push(BlockedFile { url: format!("{page}/files/{}", file["id"]), name, folder: folder.into() });
                continue;
            };
            let sha1 = file["hashes"].as_array().into_iter().flatten().find(|h| h["algo"] == 1).map(|h| text(h, "value"));
            tasks.push(Task { url: url.to_string(), path, sha1 });
            records.entry(kind).or_default().push(InstalledMod {
                file: name,
                title: project.map(|p| text(p, "name")).unwrap_or_else(|| text(file, "displayName")),
                source: Some(Source::Curseforge),
                project_id: file["modId"].as_u64().map(|id| id.to_string()),
                version: Some(text(file, "displayName")),
                icon_url: project.and_then(|p| p["logo"]["thumbnailUrl"].as_str()).map(String::from),
                dependency: false,
                enabled: true,
            });
        }
        reporter.stage(&format!("Downloading {} files…", tasks.len()));
        download::run(sites.client, tasks, Verify::Existing, |done, total| reporter.files(done, total)).await?;
        // Remember where each file came from, so updates work for the pack's files.
        for (kind, records) in records {
            mods::save_records(&Shelf::new(paths, instance, kind, None)?, &records)?;
        }
    }

    reporter.stage("Copying the modpack's settings…");
    let overrides = manifest["overrides"].as_str().unwrap_or("overrides").to_string();
    extract_overrides(archive, &game_dir, &[&overrides])?;
    Ok(blocked)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_pack_loaders() {
        let mrpack = body!({ "name": "Pack", "versionId": "1.0", "dependencies": { "minecraft": "1.20.1", "forge": "47.2.0" } });
        assert_eq!(
            Pack::modrinth(&mrpack).unwrap(),
            Pack { name: "Pack 1.0".into(), minecraft: "1.20.1".into(), loader: "forge".into(), loader_version: Some("1.20.1-47.2.0".into()) }
        );
        let manifest = body!({ "name": "CF", "version": "", "minecraft": { "version": "1.21.1", "modLoaders": [{ "id": "neoforge-21.1.77", "primary": true }] } });
        let pack = Pack::curseforge(&manifest).unwrap();
        assert_eq!((pack.name.as_str(), pack.loader.as_str(), pack.loader_version.as_deref()), ("CF", "neoforge", Some("21.1.77")));
    }

    #[test]
    fn refuses_paths_outside_the_game_folder() {
        let game = Path::new("/tmp/game");
        assert!(inside(game, "mods/a.jar").is_ok());
        assert!(inside(game, "../escape.jar").is_err());
        assert!(inside(game, "/etc/passwd").is_err());
    }

    struct Quiet;
    impl Reporter for Quiet {
        fn stage(&mut self, _: &str) {}
        fn files(&mut self, _: usize, _: usize) {}
        fn log(&mut self, _: String) {}
    }

    /// Downloads Fabulously Optimized from Modrinth and imports it.
    /// Run with `cargo test imports_a_modrinth_pack -- --ignored`.
    #[tokio::test]
    #[ignore]
    async fn imports_a_modrinth_pack() {
        let dir = std::env::temp_dir().join(format!("clover-packs-{}", std::process::id()));
        let paths = Paths::at(dir.clone());
        let client = reqwest::Client::builder().user_agent("CloverNetwork/Clover-Launcher/test").build().unwrap();
        let sites = Sites { client: &client, curseforge_key: None };
        let page = search(&sites, Source::Modrinth, "fabulously optimized", 0).await.unwrap();
        let pack = page.hits.iter().find(|h| h.title == "Fabulously Optimized").expect("found the pack");
        let file = download(&sites, &paths, Source::Modrinth, &pack.id).await.unwrap();
        let imported = import(&sites, &paths, &file, &mut Quiet).await.unwrap();
        println!("{} → {} {:?}", imported.instance.name, imported.instance.loader, imported.instance.loader_version);
        assert_eq!(imported.instance.loader, "fabric");
        let mods = mods::installed(&Shelf::mods(&paths, &imported.instance)).unwrap();
        assert!(mods.len() > 10, "only {} mods", mods.len());
        assert!(paths.instance(&imported.instance.id).join("minecraft/config").exists(), "overrides copied");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
