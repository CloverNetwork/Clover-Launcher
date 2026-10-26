//! Mods, resource packs, shader packs and data packs from Modrinth and CurseForge.
//! References: https://docs.modrinth.com/api/ and https://docs.curseforge.com/rest-api/
//!
//! Each kind has its folder in the game folder (data packs live inside a world) and a
//! record beside it of where each file came from. Files someone drops into a folder by hand
//! still show up, just without a source. Turning one off renames `name.jar` to
//! `name.jar.disabled`, as other launchers do.

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

/// What a file is, which decides its folder and which projects fit.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    #[default]
    Mod,
    Resourcepack,
    Shader,
    Datapack,
}

impl Kind {
    fn modrinth_type(self) -> &'static str {
        match self {
            Kind::Mod => "mod",
            Kind::Resourcepack => "resourcepack",
            Kind::Shader => "shader",
            Kind::Datapack => "datapack",
        }
    }

    /// CurseForge's class IDs for Minecraft.
    pub(crate) fn curseforge_class(self) -> u32 {
        match self {
            Kind::Mod => 6,
            Kind::Resourcepack => 12,
            Kind::Shader => 6552,
            Kind::Datapack => 6945,
        }
    }

    pub(crate) fn from_curseforge_class(class: Option<u64>) -> Self {
        match class {
            Some(12) => Kind::Resourcepack,
            Some(6552) => Kind::Shader,
            Some(6945) => Kind::Datapack,
            _ => Kind::Mod,
        }
    }

    /// The folder in the game folder; data packs' folder is inside a world.
    pub(crate) fn folder(self) -> &'static str {
        match self {
            Kind::Mod => "mods",
            Kind::Resourcepack => "resourcepacks",
            Kind::Shader => "shaderpacks",
            Kind::Datapack => "datapacks",
        }
    }
}

/// A file in one of the instance's content folders.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledMod {
    /// The file's name, without ".disabled". Packs can also be folders.
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

/// What a file has to match: the game version and, for mods, the loader's names on each site.
pub struct Target {
    kind: Kind,
    game: String,
    modrinth_loaders: Vec<&'static str>,
    curseforge_loader: Option<u32>,
}

impl Target {
    pub fn for_instance(instance: &Instance) -> Result<Self> {
        Self::new(instance, Kind::Mod)
    }

    pub fn new(instance: &Instance, kind: Kind) -> Result<Self> {
        let game = instance.version.clone();
        // Modrinth files say what reads them: the game itself, a shader mod, or a data pack.
        let packs = |loaders: &[&'static str]| Ok(Self { kind, game: game.clone(), modrinth_loaders: loaders.to_vec(), curseforge_loader: None });
        match kind {
            Kind::Mod => {}
            Kind::Resourcepack => return packs(&["minecraft"]),
            Kind::Shader => return packs(&["iris", "optifine"]),
            Kind::Datapack => return packs(&["datapack"]),
        }
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
        Ok(Self { kind, game, modrinth_loaders: modrinth.to_vec(), curseforge_loader: curseforge })
    }

    /// The loader filter for CurseForge, which only mods have.
    fn curseforge_loader(&self) -> Result<Option<u32>> {
        if self.kind != Kind::Mod {
            return Ok(None);
        }
        self.curseforge_loader.map(Some).ok_or_else(|| msg("CurseForge doesn't list mods for this loader. Try Modrinth."))
    }
}

/// One kind's folder in an instance and the record of where its files came from.
pub struct Shelf {
    pub kind: Kind,
    dir: PathBuf,
    records: PathBuf,
}

impl Shelf {
    pub fn mods(paths: &Paths, instance: &Instance) -> Self {
        Self::new(paths, instance, Kind::Mod, None).expect("mods need no world")
    }

    /// Data packs belong to a world, named by its folder.
    pub fn new(paths: &Paths, instance: &Instance, kind: Kind, world: Option<&str>) -> Result<Self> {
        let game = paths.instance(&instance.id).join("minecraft");
        if kind != Kind::Datapack {
            let records = paths.instance(&instance.id).join(format!("{}.json", kind.folder()));
            return Ok(Self { kind, dir: game.join(kind.folder()), records });
        }
        let world = world.ok_or_else(|| msg("Data packs belong to a world. Choose one first."))?;
        let dir = crate::worlds::world_dir(paths, instance, world)?;
        Ok(Self { kind, records: dir.join("clover-datapacks.json"), dir: dir.join("datapacks") })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
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
            let kind = target.kind.modrinth_type();
            let mut facets = vec![vec![format!("project_type:{kind}")], vec![format!("versions:{}", target.game)]];
            // Every resource pack works with the plain game, so only the others filter by loader.
            if target.kind != Kind::Resourcepack {
                facets.push(target.modrinth_loaders.iter().map(|l| format!("categories:{l}")).collect());
            }
            let facets = serde_json::json!(facets);
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
                        page_url: format!("https://modrinth.com/{kind}/{}", text(h, "slug")),
                    })
                    .collect(),
            })
        }
        Source::Curseforge => {
            let loader = target.curseforge_loader()?;
            // Sort field 2 is popularity; it suits an empty search, relevance suits a typed one.
            let sort = if query.trim().is_empty() { "2" } else { "1" };
            let mut request = sites.curseforge(format!("{CURSEFORGE}/mods/search"))?.query(&[
                ("gameId", CURSEFORGE_MINECRAFT.to_string()),
                ("classId", target.kind.curseforge_class().to_string()),
                ("searchFilter", query.to_string()),
                ("gameVersion", target.game.clone()),
                ("sortField", sort.into()),
                ("sortOrder", "desc".into()),
                ("pageSize", PAGE_SIZE.to_string()),
                ("index", offset.to_string()),
            ]);
            if let Some(loader) = loader {
                request = request.query(&[("modLoaderType", loader)]);
            }
            let res = json(request).await?;
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
    let requires = if target.kind == Kind::Mod { required_on_modrinth(version) } else { vec![] };
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
    let mut request =
        sites.curseforge(format!("{CURSEFORGE}/mods/{project}/files"))?.query(&[("gameVersion", target.game.clone()), ("pageSize", "50".into())]);
    if let Some(loader) = loader {
        request = request.query(&[("modLoaderType", loader)]);
    }
    let files = json(request).await?;
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
    // Relation type 3 is a required dependency. Packs' "dependencies" are mods, which don't belong in their folder.
    let requires = if target.kind != Kind::Mod {
        vec![]
    } else {
        file["dependencies"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|d| d["relationType"] == 3)
            .filter_map(|d| d["modId"].as_u64().map(|id| id.to_string()))
            .collect()
    };
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

fn required_on_modrinth(version: &Value) -> Vec<String> {
    version["dependencies"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|d| d["dependency_type"] == "required")
        .filter_map(|d| d["project_id"].as_str().map(String::from))
        .collect()
}

fn load_records(shelf: &Shelf) -> Vec<InstalledMod> {
    std::fs::read(&shelf.records).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

pub(crate) fn save_records(shelf: &Shelf, records: &[InstalledMod]) -> Result<()> {
    std::fs::write(&shelf.records, serde_json::to_vec_pretty(records)?)?;
    Ok(())
}

/// Whether a name in a folder is something the game reads: jars for mods; zips or
/// unpacked folders for packs.
fn is_content(shelf: &Shelf, name: &str, is_dir: bool) -> bool {
    match shelf.kind {
        Kind::Mod => name.ends_with(".jar") && !is_dir,
        _ => !name.starts_with('.') && (is_dir || name.ends_with(".zip")),
    }
}

/// A folder as it is now, with what's known about each file.
pub fn installed(shelf: &Shelf) -> Result<Vec<InstalledMod>> {
    let records = load_records(shelf);
    let Ok(entries) = std::fs::read_dir(&shelf.dir) else { return Ok(vec![]) };
    let mut mods: Vec<InstalledMod> = entries
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let is_dir = entry.file_type().ok()?.is_dir();
            let name = entry.file_name().to_string_lossy().into_owned();
            let (file, enabled) = match name.strip_suffix(".disabled") {
                Some(file) => (file.to_string(), false),
                None => (name, true),
            };
            if !is_content(shelf, &file, is_dir) {
                return None;
            }
            let mut record = records.iter().find(|r| r.file == file).cloned().unwrap_or_else(|| InstalledMod {
                title: file.trim_end_matches(".jar").trim_end_matches(".zip").to_string(),
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
/// Returns the titles of what was added, the picked one first.
pub async fn install(sites: &Sites<'_>, shelf: &Shelf, target: &Target, source: Source, project: &str) -> Result<Vec<String>> {
    install_queue(sites, shelf, target, source, vec![(project.to_string(), 0)]).await
}

/// Well-known mods that make the game faster without changing how it plays, from Modrinth.
/// Embeddium stands in for Sodium where Sodium has no build (older Forge).
const PERFORMANCE: [&str; 7] = ["sodium", "embeddium", "lithium", "ferrite-core", "immediatelyfast", "entityculling", "modernfix"];

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackResult {
    /// Titles of what was installed, dependencies included.
    pub added: Vec<String>,
    /// How many of the pack's mods have no version for this game and loader.
    pub unavailable: usize,
}

/// Installs the performance mods that exist for the instance's version and loader,
/// skipping any already installed.
pub async fn install_performance(sites: &Sites<'_>, shelf: &Shelf, target: &Target) -> Result<PackResult> {
    let present: HashSet<String> = load_records(shelf).into_iter().filter_map(|r| r.project_id).collect();
    let mut result = PackResult { added: vec![], unavailable: 0 };
    let mut have_renderer = false;
    for slug in PERFORMANCE {
        if slug == "embeddium" && have_renderer {
            continue;
        }
        // Records hold project IDs, so compare against the project's ID too.
        let info = json(sites.client.get(format!("{MODRINTH}/project/{slug}"))).await?;
        if present.contains(&text(&info, "id")) {
            have_renderer |= slug == "sodium";
            continue;
        }
        match install(sites, shelf, target, Source::Modrinth, slug).await {
            Ok(added) => {
                have_renderer |= slug == "sodium" || slug == "embeddium";
                result.added.extend(added);
            }
            Err(e) if e.to_string().contains("has no version") => {
                if slug != "embeddium" && slug != "sodium" {
                    result.unavailable += 1;
                }
            }
            Err(e) => return Err(e),
        }
    }
    if !have_renderer {
        result.unavailable += 1;
    }
    Ok(result)
}

/// Installs projects and what they require. Depth 0 is a picked mod (updated if present);
/// deeper entries are dependencies, left alone when already installed.
async fn install_queue(
    sites: &Sites<'_>,
    shelf: &Shelf,
    target: &Target,
    source: Source,
    mut queue: Vec<(String, usize)>,
) -> Result<Vec<String>> {
    let dir = &shelf.dir;
    std::fs::create_dir_all(dir)?;
    let mut records = load_records(shelf);
    let present: HashSet<String> = installed(shelf)?.into_iter().map(|m| m.file).collect();
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
            Source::Modrinth => resolve_modrinth(sites, target, &project).await?,
            Source::Curseforge => resolve_curseforge(sites, target, &project).await?,
        };
        check_file_name(shelf.kind, &resolved.record.file)?;
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

    save_records(shelf, &records)?;
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

/// Looks for newer versions of every file. Modrinth is asked about every file by its hash,
/// so it also finds updates for ones added by hand; CurseForge's are checked one by one.
pub async fn check_updates(sites: &Sites<'_>, shelf: &Shelf, target: &Target) -> Result<UpdateCheck> {
    let dir = &shelf.dir;
    let mods = installed(shelf)?;
    let mut updates = Vec::new();
    let mut skipped = None;

    let by_hash: HashMap<String, &InstalledMod> = mods
        .iter()
        .filter(|m| m.source != Some(Source::Curseforge))
        .filter_map(|m| Some((hex::encode(Sha1::digest(std::fs::read(file_on_disk(dir, &m.file)).ok()?)), m)))
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
                requires: if target.kind == Kind::Mod { required_on_modrinth(new) } else { vec![] },
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
            skipped = Some("Files from CurseForge weren't checked because there's no CurseForge API key.".into());
        } else {
            for installed in from_curseforge {
                let Some(project) = &installed.project_id else { continue };
                // A mod whose newest file can't be downloaded here is skipped, not an error.
                let Ok(newest) = resolve_curseforge(sites, target, project).await else { continue };
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
pub async fn update(sites: &Sites<'_>, shelf: &Shelf, target: &Target, update: &ModUpdate) -> Result<Vec<String>> {
    check_file_name(shelf.kind, &update.file)?;
    check_file_name(shelf.kind, &update.new_file)?;
    let trusted = ["https://cdn.modrinth.com/", "https://edge.forgecdn.net/", "https://mediafilez.forgecdn.net/"];
    if !trusted.iter().any(|host| update.url.starts_with(host)) {
        return Err(msg("That update doesn't come from Modrinth or CurseForge."));
    }
    let dir = &shelf.dir;
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

    let mut records = load_records(shelf);
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
    save_records(shelf, &records)?;

    let mut added = vec![update.title.clone()];
    let needed = update.requires.iter().map(|p| (p.clone(), 1)).collect();
    added.extend(install_queue(sites, shelf, target, update.source, needed).await?);
    Ok(added)
}

/// Applies updates one after another. Returns the titles of everything updated or added.
pub async fn update_all(sites: &Sites<'_>, shelf: &Shelf, target: &Target, updates: &[ModUpdate]) -> Result<Vec<String>> {
    let mut changed = Vec::new();
    for item in updates {
        changed.extend(update(sites, shelf, target, item).await?);
    }
    Ok(changed)
}

pub fn set_enabled(shelf: &Shelf, file: &str, enabled: bool) -> Result<()> {
    check_file_name(shelf.kind, file)?;
    let dir = &shelf.dir;
    let (on, off) = (dir.join(file), dir.join(format!("{file}.disabled")));
    let (from, to) = if enabled { (off, on) } else { (on, off) };
    if from.exists() {
        std::fs::rename(from, to)?;
    }
    Ok(())
}

pub fn remove(shelf: &Shelf, file: &str) -> Result<()> {
    check_file_name(shelf.kind, file)?;
    for path in [shelf.dir.join(file), shelf.dir.join(format!("{file}.disabled"))] {
        // An unpacked pack is a folder someone may have made themselves, so it goes to the Trash.
        if path.is_dir() {
            crate::instances::move_to_trash(&path)?;
        } else {
            let _ = std::fs::remove_file(path);
        }
    }
    let mut records = load_records(shelf);
    records.retain(|r| r.file != file);
    save_records(shelf, &records)
}

/// File names come from the sites; never let one reach outside its folder.
fn check_file_name(kind: Kind, file: &str) -> Result<()> {
    let safe = !file.is_empty() && !file.contains(['/', '\\']) && !file.starts_with('.');
    if !safe || (kind == Kind::Mod && !file.ends_with(".jar")) {
        return Err(msg(if kind == Kind::Mod { "That isn't a mod file." } else { "That isn't a pack file." }));
    }
    Ok(())
}

pub fn folder(shelf: &Shelf) -> Result<PathBuf> {
    std::fs::create_dir_all(&shelf.dir)?;
    Ok(shelf.dir.clone())
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
            play_seconds: 0,
            options: Default::default(),
        }
    }

    #[test]
    fn rejects_unsafe_file_names() {
        assert!(check_file_name(Kind::Mod, "sodium.jar").is_ok());
        assert!(check_file_name(Kind::Mod, "../escape.jar").is_err());
        assert!(check_file_name(Kind::Mod, "notes.txt").is_err());
        assert!(check_file_name(Kind::Shader, "Complementary.zip").is_ok());
        assert!(check_file_name(Kind::Resourcepack, "..").is_err());
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
        let shelf = Shelf::mods(&paths, &instance);
        let target = Target::for_instance(&instance).unwrap();
        let mods = shelf.dir().to_path_buf();
        std::fs::create_dir_all(&mods).unwrap();
        let bytes = client.get(text(&oldest["files"][0], "url")).send().await.unwrap().bytes().await.unwrap();
        std::fs::write(mods.join(&old_file), bytes).unwrap();
        set_enabled(&shelf, &old_file, false).unwrap();

        let check = check_updates(&sites, &shelf, &target).await.unwrap();
        let sodium = check.updates.iter().find(|u| u.title == "Sodium").expect("Sodium update found by hash");
        println!("{} → {}", sodium.current_version.as_deref().unwrap_or("?"), sodium.new_version);
        update(&sites, &shelf, &target, sodium).await.unwrap();

        let after = installed(&shelf).unwrap();
        assert_eq!(after.len(), 1);
        assert_eq!(after[0].file, sodium.new_file);
        assert_eq!(after[0].source, Some(Source::Modrinth));
        assert!(!after[0].enabled, "a disabled mod stays disabled");
        assert!(check_updates(&sites, &shelf, &target).await.unwrap().updates.is_empty());
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

        let shelf = Shelf::mods(&paths, &instance);
        let target = Target::for_instance(&instance).unwrap();
        let page = search(&sites, &target, Source::Modrinth, "iris", 0).await.unwrap();
        let iris = page.hits.iter().find(|h| h.title.starts_with("Iris")).expect("Iris in results");
        let added = install(&sites, &shelf, &target, Source::Modrinth, &iris.id).await.unwrap();
        println!("added: {added:?}");
        assert!(added.iter().any(|t| t.starts_with("Sodium")), "Sodium comes along as a dependency");

        let mods = installed(&shelf).unwrap();
        let sodium = mods.iter().find(|m| m.title.starts_with("Sodium")).unwrap();
        assert!(sodium.dependency && sodium.enabled);
        set_enabled(&shelf, &sodium.file, false).unwrap();
        assert!(!installed(&shelf).unwrap().iter().find(|m| m.file == sodium.file).unwrap().enabled);
        remove(&shelf, &sodium.file).unwrap();
        assert_eq!(installed(&shelf).unwrap().len(), mods.len() - 1);
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// Installs a shader pack and a resource pack into a vanilla instance and a data pack into
    /// one of its worlds. Run with `cargo test installs_packs -- --ignored`.
    #[tokio::test]
    #[ignore]
    async fn installs_packs() {
        let dir = std::env::temp_dir().join(format!("clover-content-{}", std::process::id()));
        let paths = Paths::at(dir.clone());
        let instance = crate::instances::create(&paths, "Packs", "1.21.4", "vanilla", None).unwrap();
        let client = Client::builder().user_agent("CloverNetwork/Clover-Launcher/test").build().unwrap();
        let sites = Sites { client: &client, curseforge_key: None };
        let world = paths.instance(&instance.id).join("minecraft/saves/World");
        std::fs::create_dir_all(&world).unwrap();
        std::fs::write(world.join("level.dat"), b"").unwrap();

        for (kind, query, world) in
            [(Kind::Shader, "complementary", None), (Kind::Resourcepack, "fresh animations", None), (Kind::Datapack, "terralith", Some("World"))]
        {
            let shelf = Shelf::new(&paths, &instance, kind, world).unwrap();
            let target = Target::new(&instance, kind).unwrap();
            let page = search(&sites, &target, Source::Modrinth, query, 0).await.unwrap();
            let hit = &page.hits[0];
            println!("{kind:?}: {} ({})", hit.title, hit.page_url);
            let added = install(&sites, &shelf, &target, Source::Modrinth, &hit.id).await.unwrap();
            assert_eq!(added.len(), 1, "packs bring no dependencies");
            let items = installed(&shelf).unwrap();
            assert_eq!(items.len(), 1);
            assert!(items[0].file.ends_with(".zip"), "{}", items[0].file);
            assert!(check_updates(&sites, &shelf, &target).await.unwrap().updates.is_empty());
        }
        assert!(world.join("datapacks").read_dir().unwrap().count() == 1);
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// Installs the performance pack into a Fabric instance, then again, which adds nothing.
    /// Run with `cargo test installs_the_performance_pack -- --ignored --nocapture`.
    #[tokio::test]
    #[ignore]
    async fn installs_the_performance_pack() {
        let dir = std::env::temp_dir().join(format!("clover-perf-{}", std::process::id()));
        let paths = Paths::at(dir.clone());
        let instance = instance("fabric");
        let client = Client::builder().user_agent("CloverNetwork/Clover-Launcher/test").build().unwrap();
        let sites = Sites { client: &client, curseforge_key: None };
        let (shelf, target) = (Shelf::mods(&paths, &instance), Target::for_instance(&instance).unwrap());
        let first = install_performance(&sites, &shelf, &target).await.unwrap();
        println!("{first:?}");
        assert!(first.added.iter().any(|t| t == "Sodium") && first.added.len() >= 5);
        assert!(!first.added.iter().any(|t| t.contains("Embeddium")));
        let again = install_performance(&sites, &shelf, &target).await.unwrap();
        assert!(again.added.is_empty(), "{again:?}");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
