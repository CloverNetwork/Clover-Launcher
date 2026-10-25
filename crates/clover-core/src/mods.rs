//! Mods from Modrinth and CurseForge, installed into an instance's `mods` folder.
//! References: https://docs.modrinth.com/api/ and https://docs.curseforge.com/rest-api/
//!
//! Each instance keeps `mods.json` beside its game folder with where each file came
//! from. Jars someone drops into the folder by hand still show up, just without a source.
//! Disabling a mod renames `name.jar` to `name.jar.disabled`, as other launchers do.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use reqwest::{Client, RequestBuilder};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json as body};
use sha1::{Digest, Sha1};

use crate::download::{self, Task, Verify};
use crate::error::{Result, msg};
use crate::instances::Instance;
use crate::paths::Paths;

pub(crate) const MODRINTH: &str = "https://api.modrinth.com/v2";
pub(crate) const CURSEFORGE: &str = "https://api.curseforge.com/v1";
pub(crate) const CURSEFORGE_MINECRAFT: u32 = 432;
const CURSEFORGE_MODS_CLASS: u32 = 6;
pub(crate) const PAGE_SIZE: u32 = 20;
/// Dependencies of dependencies are followed, but not forever.
const MAX_DEPENDENCY_DEPTH: usize = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Modrinth,
    Curseforge,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub source: Source,
    pub id: String,
    pub title: String,
    pub author: String,
    pub description: String,
    pub icon_url: Option<String>,
    pub downloads: u64,
    pub page_url: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchPage {
    pub hits: Vec<SearchHit>,
    pub total: u64,
}

/// A file in the instance's mods folder.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledMod {
    /// The jar's name, without ".disabled".
    pub file: String,
    pub title: String,
    pub source: Option<Source>,
    pub project_id: Option<String>,
    pub version: Option<String>,
    pub icon_url: Option<String>,
    /// Pulled in because another mod needs it, rather than picked.
    #[serde(default)]
    pub dependency: bool,
    #[serde(skip_deserializing)]
    pub enabled: bool,
}

/// What a mod has to match: the game version and the loader's names on each site.
pub struct Target {
    game: String,
    modrinth_loaders: Vec<&'static str>,
    curseforge_loader: Option<u32>,
}

impl Target {
    pub fn for_instance(instance: &Instance) -> Result<Self> {
        // Quilt loads Fabric mods, so Fabric mods count as compatible with it.
        let (modrinth, curseforge): (&[&'static str], Option<u32>) = match instance.loader.as_str() {
            "fabric" => (&["fabric"], Some(4)),
            "quilt" => (&["quilt", "fabric"], Some(5)),
            "neoforge" => (&["neoforge"], Some(6)),
            "forge" => (&["forge"], Some(1)),
            "legacy-fabric" => (&["legacy-fabric"], None),
            "ornithe" | "ornithe-quilt" => (&["ornithe"], None),
            "babric" => (&["babric"], None),
            // Meta servers added in Settings serve Fabric-compatible loaders.
            id if id.starts_with("custom-") => (&["fabric"], Some(4)),
            _ => return Err(msg("Mods need a mod loader. Create an instance with one, such as Fabric or NeoForge.")),
        };
        Ok(Self { game: instance.version.clone(), modrinth_loaders: modrinth.to_vec(), curseforge_loader: curseforge })
    }

    fn curseforge_loader(&self) -> Result<u32> {
        self.curseforge_loader.ok_or_else(|| msg("CurseForge doesn't list mods for this loader. Try Modrinth."))
    }
}

/// HTTP access for both sites. CurseForge only answers requests with an API key.
pub struct Sites<'a> {
    pub client: &'a Client,
    pub curseforge_key: Option<&'a str>,
}

impl Sites<'_> {
    pub(crate) fn curseforge(&self, url: String) -> Result<RequestBuilder> {
        Ok(self.client.get(url).header("x-api-key", self.curseforge_key()?))
    }

    pub(crate) fn curseforge_post(&self, url: String) -> Result<RequestBuilder> {
        Ok(self.client.post(url).header("x-api-key", self.curseforge_key()?))
    }

    fn curseforge_key(&self) -> Result<&str> {
        self.curseforge_key.ok_or_else(|| msg("CurseForge needs an API key. Add one in Settings."))
    }
}

pub(crate) async fn json(request: RequestBuilder) -> Result<Value> {
    let res = request.send().await?;
    match res.status().as_u16() {
        // The hint about approved third-party keys sits next to wherever this shows.
        403 if res.url().host_str() == Some("api.curseforge.com") => Err(msg("CurseForge rejected the API key.")),
        429 => Err(msg("Too many requests for now. Wait a moment and try again.")),
        _ => Ok(res.error_for_status()?.json().await?),
    }
}

pub(crate) fn text(value: &Value, key: &str) -> String {
    value[key].as_str().unwrap_or_default().to_string()
}

/// Whether CurseForge accepts a key, checked with its cheapest request.
pub async fn check_curseforge_key(client: &Client, key: &str) -> Result<()> {
    let key = key.trim();
    if key.is_empty() {
        return Err(msg("Enter a key first."));
    }
    let sites = Sites { client, curseforge_key: Some(key) };
    json(sites.curseforge(format!("{CURSEFORGE}/games/{CURSEFORGE_MINECRAFT}"))?).await.map(|_| ())
}

pub async fn search(sites: &Sites<'_>, target: &Target, source: Source, query: &str, offset: u32) -> Result<SearchPage> {
    match source {
        Source::Modrinth => {
            let loaders: Vec<String> = target.modrinth_loaders.iter().map(|l| format!("categories:{l}")).collect();
            let facets = serde_json::json!([["project_type:mod"], [format!("versions:{}", target.game)], loaders]);
            let index = if query.trim().is_empty() { "downloads" } else { "relevance" };
            let res = json(sites.client.get(format!("{MODRINTH}/search")).query(&[
                ("query", query),
                ("facets", &facets.to_string()),
                ("index", index),
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
                        page_url: format!("https://modrinth.com/mod/{}", text(h, "slug")),
                    })
                    .collect(),
            })
        }
        Source::Curseforge => {
            let loader = target.curseforge_loader()?;
            // Sort field 2 is popularity; it suits an empty search, relevance suits a typed one.
            let sort = if query.trim().is_empty() { "2" } else { "1" };
            let res = json(sites.curseforge(format!("{CURSEFORGE}/mods/search"))?.query(&[
                ("gameId", CURSEFORGE_MINECRAFT.to_string()),
                ("classId", CURSEFORGE_MODS_CLASS.to_string()),
                ("searchFilter", query.to_string()),
                ("gameVersion", target.game.clone()),
                ("modLoaderType", loader.to_string()),
                ("sortField", sort.into()),
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

/// A specific file to put in the mods folder, and the projects it needs.
struct Resolved {
    record: InstalledMod,
    url: String,
    sha1: Option<String>,
    requires: Vec<String>,
}

async fn resolve_modrinth(sites: &Sites<'_>, target: &Target, project: &str) -> Result<Resolved> {
    let loaders = serde_json::to_string(&target.modrinth_loaders)?;
    let games = serde_json::to_string(&[&target.game])?;
    let info = json(sites.client.get(format!("{MODRINTH}/project/{project}"))).await?;
    let versions = json(
        sites.client.get(format!("{MODRINTH}/project/{project}/version")).query(&[("loaders", loaders), ("game_versions", games)]),
    )
    .await?;
    let versions = versions.as_array().cloned().unwrap_or_default();
    // Newest release first; fall back to the newest beta or alpha.
    let version = versions
        .iter()
        .find(|v| v["version_type"] == "release")
        .or(versions.first())
        .ok_or_else(|| msg(format!("{} has no version for Minecraft {} with this loader.", text(&info, "title"), target.game)))?;
    let files = version["files"].as_array().cloned().unwrap_or_default();
    let file = files.iter().find(|f| f["primary"] == true).or(files.first()).ok_or_else(|| msg("That version has no file."))?;
    let requires = version["dependencies"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|d| d["dependency_type"] == "required")
        .filter_map(|d| d["project_id"].as_str().map(String::from))
        .collect();
    Ok(Resolved {
        record: InstalledMod {
            file: text(file, "filename"),
            title: text(&info, "title"),
            source: Some(Source::Modrinth),
            project_id: Some(text(&info, "id")),
            version: Some(text(version, "version_number")),
            icon_url: info["icon_url"].as_str().filter(|u| !u.is_empty()).map(String::from),
            dependency: false,
            enabled: true,
        },
        url: text(file, "url"),
        sha1: file["hashes"]["sha1"].as_str().map(String::from),
        requires,
    })
}

async fn resolve_curseforge(sites: &Sites<'_>, target: &Target, project: &str) -> Result<Resolved> {
    let loader = target.curseforge_loader()?;
    let info = json(sites.curseforge(format!("{CURSEFORGE}/mods/{project}"))?).await?["data"].clone();
    let files = json(sites.curseforge(format!("{CURSEFORGE}/mods/{project}/files"))?.query(&[
        ("gameVersion", target.game.clone()),
        ("modLoaderType", loader.to_string()),
        ("pageSize", "50".into()),
    ]))
    .await?;
    let mut files = files["data"].as_array().cloned().unwrap_or_default();
    files.sort_by(|a, b| text(b, "fileDate").cmp(&text(a, "fileDate")));
    // Release type 1 is a release; 2 and 3 are beta and alpha.
    let file = files
        .iter()
        .find(|f| f["releaseType"] == 1)
        .or(files.first())
        .ok_or_else(|| msg(format!("{} has no file for Minecraft {} with this loader.", text(&info, "name"), target.game)))?;
    let url = file["downloadUrl"].as_str().map(String::from).ok_or_else(|| {
        msg(format!(
            "{}'s author doesn't allow other launchers to download it. Get it from {}.",
            text(&info, "name"),
            text(&info["links"], "websiteUrl")
        ))
    })?;
    let sha1 = file["hashes"].as_array().into_iter().flatten().find(|h| h["algo"] == 1).map(|h| text(h, "value"));
    // Relation type 3 is a required dependency.
    let requires = file["dependencies"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|d| d["relationType"] == 3)
        .filter_map(|d| d["modId"].as_u64().map(|id| id.to_string()))
        .collect();
    Ok(Resolved {
        record: InstalledMod {
            file: text(file, "fileName"),
            title: text(&info, "name"),
            source: Some(Source::Curseforge),
            project_id: Some(project.to_string()),
            version: Some(text(file, "displayName")),
            icon_url: info["logo"]["thumbnailUrl"].as_str().map(String::from),
            dependency: false,
            enabled: true,
        },
        url,
        sha1,
        requires,
    })
}

fn mods_dir(paths: &Paths, instance: &Instance) -> PathBuf {
    paths.instance(&instance.id).join("minecraft").join("mods")
}

fn records_file(paths: &Paths, instance: &Instance) -> PathBuf {
    paths.instance(&instance.id).join("mods.json")
}

fn load_records(paths: &Paths, instance: &Instance) -> Vec<InstalledMod> {
    std::fs::read(records_file(paths, instance)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

pub(crate) fn save_records(paths: &Paths, instance: &Instance, records: &[InstalledMod]) -> Result<()> {
    std::fs::write(records_file(paths, instance), serde_json::to_vec_pretty(records)?)?;
    Ok(())
}

/// The mods folder as it is now, with what's known about each file.
pub fn installed(paths: &Paths, instance: &Instance) -> Result<Vec<InstalledMod>> {
    let records = load_records(paths, instance);
    let dir = mods_dir(paths, instance);
    let Ok(entries) = std::fs::read_dir(&dir) else { return Ok(vec![]) };
    let mut mods: Vec<InstalledMod> = entries
        .filter_map(|entry| {
            let name = entry.ok()?.file_name().to_string_lossy().into_owned();
            let (file, enabled) = match name.strip_suffix(".disabled") {
                Some(file) => (file.to_string(), false),
                None => (name, true),
            };
            if !file.ends_with(".jar") {
                return None;
            }
            let mut record = records.iter().find(|r| r.file == file).cloned().unwrap_or_else(|| InstalledMod {
                title: file.trim_end_matches(".jar").to_string(),
                file: file.clone(),
                source: None,
                project_id: None,
                version: None,
                icon_url: None,
                dependency: false,
                enabled,
            });
            record.enabled = enabled;
            Some(record)
        })
        .collect();
    mods.sort_by_key(|m| m.title.to_lowercase());
    Ok(mods)
}

/// Installs a project's best matching file and everything it requires.
/// Returns the titles of what was added, the picked mod first.
pub async fn install(sites: &Sites<'_>, paths: &Paths, instance: &Instance, source: Source, project: &str) -> Result<Vec<String>> {
    install_queue(sites, paths, instance, source, vec![(project.to_string(), 0)]).await
}

/// Installs projects and what they require. Depth 0 is a picked mod (updated if present);
/// deeper entries are dependencies, left alone when already installed.
async fn install_queue(
    sites: &Sites<'_>,
    paths: &Paths,
    instance: &Instance,
    source: Source,
    mut queue: Vec<(String, usize)>,
) -> Result<Vec<String>> {
    let target = Target::for_instance(instance)?;
    let dir = mods_dir(paths, instance);
    std::fs::create_dir_all(&dir)?;
    let mut records = load_records(paths, instance);
    let present: HashSet<String> = installed(paths, instance)?.into_iter().map(|m| m.file).collect();
    let mut added = Vec::new();
    let mut seen = HashSet::new();

    while let Some((project, depth)) = queue.pop() {
        if !seen.insert(project.clone()) || depth > MAX_DEPENDENCY_DEPTH {
            continue;
        }
        let already = records.iter().any(|r| r.source == Some(source) && r.project_id.as_deref() == Some(project.as_str()));
        // A dependency that's already installed is fine as it is; a picked mod is updated.
        if depth > 0 && already {
            continue;
        }
        let resolved = match source {
            Source::Modrinth => resolve_modrinth(sites, &target, &project).await?,
            Source::Curseforge => resolve_curseforge(sites, &target, &project).await?,
        };
        let id = resolved.record.project_id.clone();
        // Replace an older file of the same project.
        for old in records.iter().filter(|r| r.source == Some(source) && r.project_id == id && r.file != resolved.record.file) {
            let _ = std::fs::remove_file(dir.join(&old.file));
            let _ = std::fs::remove_file(dir.join(format!("{}.disabled", old.file)));
        }
        records.retain(|r| !(r.source == Some(source) && r.project_id == id));
        if !present.contains(&resolved.record.file) || depth == 0 {
            let task = Task { url: resolved.url, path: dir.join(&resolved.record.file), sha1: resolved.sha1 };
            download::fetch(sites.client, task, Verify::Full).await?;
        }
        added.push(resolved.record.title.clone());
        records.push(InstalledMod { dependency: depth > 0, ..resolved.record });
        queue.extend(resolved.requires.into_iter().map(|p| (p, depth + 1)));
    }

    save_records(paths, instance, &records)?;
    Ok(added)
}

/// A newer file for an installed mod. `update` installs exactly this file, so what the
/// person saw is what they get.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModUpdate {
    /// The installed file it replaces.
    pub file: String,
    pub title: String,
    pub source: Source,
    pub project_id: String,
    pub current_version: Option<String>,
    pub new_version: String,
    pub new_file: String,
    pub url: String,
    pub sha1: Option<String>,
    pub icon_url: Option<String>,
    /// Projects the new version requires.
    #[serde(default)]
    pub requires: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheck {
    pub updates: Vec<ModUpdate>,
    /// Why some mods weren't checked, such as CurseForge without a key.
    pub skipped: Option<String>,
}

fn file_on_disk(dir: &Path, file: &str) -> PathBuf {
    let enabled = dir.join(file);
    if enabled.exists() { enabled } else { dir.join(format!("{file}.disabled")) }
}

/// Looks for newer versions of every mod. Modrinth is asked about every jar by its hash,
/// so it also finds updates for mods added by hand; CurseForge mods are checked one by one.
pub async fn check_updates(sites: &Sites<'_>, paths: &Paths, instance: &Instance) -> Result<UpdateCheck> {
    let target = Target::for_instance(instance)?;
    let dir = mods_dir(paths, instance);
    let mods = installed(paths, instance)?;
    let mut updates = Vec::new();
    let mut skipped = None;

    let by_hash: HashMap<String, &InstalledMod> = mods
        .iter()
        .filter(|m| m.source != Some(Source::Curseforge))
        .filter_map(|m| Some((hex::encode(Sha1::digest(std::fs::read(file_on_disk(&dir, &m.file)).ok()?)), m)))
        .collect();
    if !by_hash.is_empty() {
        let hashes: Vec<&String> = by_hash.keys().collect();
        let current = json(sites.client.post(format!("{MODRINTH}/version_files")).json(&body!({ "hashes": hashes, "algorithm": "sha1" }))).await?;
        let latest = json(sites.client.post(format!("{MODRINTH}/version_files/update")).json(&body!({
            "hashes": hashes,
            "algorithm": "sha1",
            "loaders": target.modrinth_loaders,
            "game_versions": [target.game],
        })))
        .await?;
        let mut found = Vec::new();
        for (hash, new) in latest.as_object().into_iter().flatten() {
            let (Some(installed), now) = (by_hash.get(hash), &current[hash]) else { continue };
            if now["id"] == new["id"] {
                continue;
            }
            // Someone on a stable release isn't offered a beta.
            if now["version_type"] == "release" && new["version_type"] != "release" {
                continue;
            }
            let files = new["files"].as_array().cloned().unwrap_or_default();
            let Some(file) = files.iter().find(|f| f["primary"] == true).or(files.first()) else { continue };
            found.push(ModUpdate {
                file: installed.file.clone(),
                title: installed.title.clone(),
                source: Source::Modrinth,
                project_id: text(new, "project_id"),
                current_version: now["version_number"].as_str().map(String::from).or(installed.version.clone()),
                new_version: text(new, "version_number"),
                new_file: text(file, "filename"),
                url: text(file, "url"),
                sha1: file["hashes"]["sha1"].as_str().map(String::from),
                icon_url: installed.icon_url.clone(),
                requires: new["dependencies"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|d| d["dependency_type"] == "required")
                    .filter_map(|d| d["project_id"].as_str().map(String::from))
                    .collect(),
            });
        }
        // Mods added by hand only have a file name; use the project's real name and icon.
        let ids: Vec<&String> = found.iter().map(|u| &u.project_id).collect();
        if !ids.is_empty() {
            let projects = json(sites.client.get(format!("{MODRINTH}/projects")).query(&[("ids", serde_json::to_string(&ids)?)])).await?;
            for project in projects.as_array().into_iter().flatten() {
                for update in found.iter_mut().filter(|u| u.project_id == text(project, "id")) {
                    update.title = text(project, "title");
                    update.icon_url = project["icon_url"].as_str().filter(|u| !u.is_empty()).map(String::from);
                }
            }
        }
        updates.extend(found);
    }

    let from_curseforge: Vec<&InstalledMod> = mods.iter().filter(|m| m.source == Some(Source::Curseforge)).collect();
    if !from_curseforge.is_empty() {
        if sites.curseforge_key.is_none() {
            skipped = Some("CurseForge mods weren't checked because there's no CurseForge API key.".into());
        } else {
            for installed in from_curseforge {
                let Some(project) = &installed.project_id else { continue };
                // A mod whose newest file can't be downloaded here is skipped, not an error.
                let Ok(newest) = resolve_curseforge(sites, &target, project).await else { continue };
                if newest.record.file == installed.file {
                    continue;
                }
                updates.push(ModUpdate {
                    file: installed.file.clone(),
                    title: installed.title.clone(),
                    source: Source::Curseforge,
                    project_id: project.clone(),
                    current_version: installed.version.clone(),
                    new_version: newest.record.version.unwrap_or_default(),
                    new_file: newest.record.file,
                    url: newest.url,
                    sha1: newest.sha1,
                    icon_url: installed.icon_url.clone(),
                    requires: newest.requires,
                });
            }
        }
    }

    updates.sort_by_key(|u| u.title.to_lowercase());
    Ok(UpdateCheck { updates, skipped })
}

/// Swaps an installed mod for the newer file from `check_updates`, keeping it on or off
/// as it was, then installs anything the new version newly requires.
pub async fn update(sites: &Sites<'_>, paths: &Paths, instance: &Instance, update: &ModUpdate) -> Result<Vec<String>> {
    check_file_name(&update.file)?;
    check_file_name(&update.new_file)?;
    let trusted = ["https://cdn.modrinth.com/", "https://edge.forgecdn.net/", "https://mediafilez.forgecdn.net/"];
    if !trusted.iter().any(|host| update.url.starts_with(host)) {
        return Err(msg("That update doesn't come from Modrinth or CurseForge."));
    }
    let dir = mods_dir(paths, instance);
    let was_enabled = !dir.join(format!("{}.disabled", update.file)).exists();
    let new_path = dir.join(&update.new_file);
    download::fetch(sites.client, Task { url: update.url.clone(), path: new_path.clone(), sha1: update.sha1.clone() }, Verify::Full)
        .await?;
    if update.new_file != update.file {
        let _ = std::fs::remove_file(dir.join(&update.file));
        let _ = std::fs::remove_file(dir.join(format!("{}.disabled", update.file)));
    }
    if !was_enabled {
        std::fs::rename(&new_path, dir.join(format!("{}.disabled", update.new_file)))?;
    }

    let mut records = load_records(paths, instance);
    let dependency = records.iter().find(|r| r.file == update.file).is_some_and(|r| r.dependency);
    records.retain(|r| r.file != update.file && !(r.source == Some(update.source) && r.project_id.as_deref() == Some(update.project_id.as_str())));
    records.push(InstalledMod {
        file: update.new_file.clone(),
        title: update.title.clone(),
        source: Some(update.source),
        project_id: Some(update.project_id.clone()),
        version: Some(update.new_version.clone()),
        icon_url: update.icon_url.clone(),
        dependency,
        enabled: was_enabled,
    });
    save_records(paths, instance, &records)?;

    let mut added = vec![update.title.clone()];
    let needed = update.requires.iter().map(|p| (p.clone(), 1)).collect();
    added.extend(install_queue(sites, paths, instance, update.source, needed).await?);
    Ok(added)
}

/// Applies updates one after another. Returns the titles of everything updated or added.
pub async fn update_all(sites: &Sites<'_>, paths: &Paths, instance: &Instance, updates: &[ModUpdate]) -> Result<Vec<String>> {
    let mut changed = Vec::new();
    for item in updates {
        changed.extend(update(sites, paths, instance, item).await?);
    }
    Ok(changed)
}

pub fn set_enabled(paths: &Paths, instance: &Instance, file: &str, enabled: bool) -> Result<()> {
    check_file_name(file)?;
    let dir = mods_dir(paths, instance);
    let (on, off) = (dir.join(file), dir.join(format!("{file}.disabled")));
    let (from, to) = if enabled { (off, on) } else { (on, off) };
    if from.exists() {
        std::fs::rename(from, to)?;
    }
    Ok(())
}

pub fn remove(paths: &Paths, instance: &Instance, file: &str) -> Result<()> {
    check_file_name(file)?;
    let dir = mods_dir(paths, instance);
    let _ = std::fs::remove_file(dir.join(file));
    let _ = std::fs::remove_file(dir.join(format!("{file}.disabled")));
    let mut records = load_records(paths, instance);
    records.retain(|r| r.file != file);
    save_records(paths, instance, &records)
}

/// Mod files are named by the sites; never let one reach outside the mods folder.
fn check_file_name(file: &str) -> Result<()> {
    if file.is_empty() || file.contains(['/', '\\']) || file.starts_with('.') || !file.ends_with(".jar") {
        return Err(msg("That isn't a mod file."));
    }
    Ok(())
}

pub fn folder(paths: &Paths, instance: &Instance) -> Result<PathBuf> {
    let dir = mods_dir(paths, instance);
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn instance(loader: &str) -> Instance {
        Instance {
            id: format!("mods-test-{loader}"),
            name: "Test".into(),
            version: "1.21.4".into(),
            loader: loader.into(),
            loader_version: Some("0.20.1".into()),
            profile_id: None,
            created_at: 0,
            last_played: None,
            installed: false,
        }
    }

    #[test]
    fn rejects_unsafe_file_names() {
        assert!(check_file_name("sodium.jar").is_ok());
        assert!(check_file_name("../escape.jar").is_err());
        assert!(check_file_name("notes.txt").is_err());
    }

    #[test]
    fn vanilla_has_no_mod_target() {
        assert!(Target::for_instance(&instance("vanilla")).is_err());
        assert_eq!(Target::for_instance(&instance("quilt")).unwrap().modrinth_loaders, ["quilt", "fabric"]);
    }

    /// CurseForge answers an unapproved key with a bare 403; the check should explain it.
    #[tokio::test]
    #[ignore]
    async fn rejects_an_unapproved_curseforge_key() {
        let error = check_curseforge_key(&Client::new(), "$2a$10$not-a-real-key").await.unwrap_err().to_string();
        assert!(error.contains("rejected"), "{error}");
    }

    /// Puts an old Sodium in the mods folder by hand, finds its update by hash, applies it,
    /// and checks nothing is left to update. Run with `cargo test updates_an_old -- --ignored`.
    #[tokio::test]
    #[ignore]
    async fn updates_an_old_hand_added_mod() {
        let dir = std::env::temp_dir().join(format!("clover-updates-{}", std::process::id()));
        let paths = Paths::at(dir.clone());
        let instance = instance("fabric");
        let client = Client::builder().user_agent("CloverNetwork/Clover-Launcher/test").build().unwrap();
        let sites = Sites { client: &client, curseforge_key: None };

        let versions = json(client.get(format!("{MODRINTH}/project/sodium/version")).query(&[
            ("loaders", r#"["fabric"]"#),
            ("game_versions", r#"["1.21.4"]"#),
        ]))
        .await
        .unwrap();
        let oldest = versions.as_array().unwrap().last().unwrap();
        let old_file = text(&oldest["files"][0], "filename");
        let mods = mods_dir(&paths, &instance);
        std::fs::create_dir_all(&mods).unwrap();
        let bytes = client.get(text(&oldest["files"][0], "url")).send().await.unwrap().bytes().await.unwrap();
        std::fs::write(mods.join(&old_file), bytes).unwrap();
        set_enabled(&paths, &instance, &old_file, false).unwrap();

        let check = check_updates(&sites, &paths, &instance).await.unwrap();
        let sodium = check.updates.iter().find(|u| u.title == "Sodium").expect("Sodium update found by hash");
        println!("{} → {}", sodium.current_version.as_deref().unwrap_or("?"), sodium.new_version);
        update(&sites, &paths, &instance, sodium).await.unwrap();

        let after = installed(&paths, &instance).unwrap();
        assert_eq!(after.len(), 1);
        assert_eq!(after[0].file, sodium.new_file);
        assert_eq!(after[0].source, Some(Source::Modrinth));
        assert!(!after[0].enabled, "a disabled mod stays disabled");
        assert!(check_updates(&sites, &paths, &instance).await.unwrap().updates.is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// Installs Iris from Modrinth, which requires Sodium, then disables and removes it.
    /// Run with `cargo test modrinth_install -- --ignored`.
    #[tokio::test]
    #[ignore]
    async fn modrinth_install_with_dependencies() {
        let dir = std::env::temp_dir().join(format!("clover-mods-{}", std::process::id()));
        let paths = Paths::at(dir.clone());
        let instance = instance("fabric");
        let client = Client::builder().user_agent("CloverNetwork/Clover-Launcher/test").build().unwrap();
        let sites = Sites { client: &client, curseforge_key: None };

        let page = search(&sites, &Target::for_instance(&instance).unwrap(), Source::Modrinth, "iris", 0).await.unwrap();
        let iris = page.hits.iter().find(|h| h.title.starts_with("Iris")).expect("Iris in results");
        let added = install(&sites, &paths, &instance, Source::Modrinth, &iris.id).await.unwrap();
        println!("added: {added:?}");
        assert!(added.iter().any(|t| t.starts_with("Sodium")), "Sodium comes along as a dependency");

        let mods = installed(&paths, &instance).unwrap();
        let sodium = mods.iter().find(|m| m.title.starts_with("Sodium")).unwrap();
        assert!(sodium.dependency && sodium.enabled);
        set_enabled(&paths, &instance, &sodium.file, false).unwrap();
        assert!(!installed(&paths, &instance).unwrap().iter().find(|m| m.file == sodium.file).unwrap().enabled);
        remove(&paths, &instance, &sodium.file).unwrap();
        assert_eq!(installed(&paths, &instance).unwrap().len(), mods.len() - 1);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
