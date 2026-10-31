//! What one installed file is: its project page on Modrinth or CurseForge when it came from
//! there (or Modrinth recognizes its hash), and otherwise what the file says about itself in
//! `fabric.mod.json`, `quilt.mod.json`, `mods.toml`, `mcmod.info` or `pack.mcmeta`.

use std::io::Read;
use std::path::Path;

use serde::Serialize;
use serde_json::Value;
use sha1::{Digest, Sha1};

use crate::error::Result;
use crate::mods::{self, CURSEFORGE, Kind, MODRINTH, Shelf, Sites, Source, json, text};

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModDetails {
    pub title: String,
    pub summary: Option<String>,
    pub authors: Vec<String>,
    pub version: Option<String>,
    pub license: Option<String>,
    pub icon_url: Option<String>,
    pub downloads: Option<u64>,
    pub categories: Vec<String>,
    pub gallery: Vec<String>,
    pub links: Vec<Link>,
    /// The mod IDs inside a jar, which crash reports and other mods refer to.
    pub mod_ids: Vec<String>,
    pub size: u64,
    /// Where the online details came from, if anywhere.
    pub source: Option<Source>,
    /// Why the online details are missing, when they should be there.
    pub online_error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Link {
    pub label: String,
    pub url: String,
}

/// Everything known about `file` in `shelf`.
pub async fn details(sites: &Sites<'_>, shelf: &Shelf, file: &str) -> Result<ModDetails> {
    let installed = mods::installed(shelf)?;
    let record = installed.iter().find(|m| m.file == file).ok_or_else(|| crate::error::msg("That file isn't there any more."))?;
    let path = mods::file_on_disk(shelf.dir(), file);
    let metadata = std::fs::metadata(&path)?;

    let mut details = local(&path, shelf.kind);
    details.size = if metadata.is_dir() { 0 } else { metadata.len() };
    details.title = record.title.clone();
    if record.version.is_some() {
        details.version = record.version.clone();
    }
    details.icon_url = record.icon_url.clone();

    let online = match (record.source, &record.project_id) {
        (Some(Source::Modrinth), Some(id)) => modrinth(sites, id).await.map(Some),
        (Some(Source::Curseforge), Some(id)) if sites.curseforge_key.is_some() => curseforge(sites, id).await.map(Some),
        (Some(Source::Curseforge), Some(_)) => Ok(None),
        // Files added by hand are often on Modrinth anyway.
        _ if metadata.is_file() => match modrinth_by_hash(sites, &path).await {
            Ok(Some(id)) => modrinth(sites, &id).await.map(Some),
            other => other.map(|_| None),
        },
        _ => Ok(None),
    };
    match online {
        Ok(Some(found)) => merge(&mut details, found),
        Ok(None) => {}
        Err(e) => details.online_error = Some(e.to_string()),
    }
    Ok(details)
}

/// Online details win over the file's own, which are often placeholders or out of date.
fn merge(details: &mut ModDetails, online: ModDetails) {
    details.title = online.title;
    details.source = online.source;
    details.summary = online.summary.or(details.summary.take());
    if !online.authors.is_empty() {
        details.authors = online.authors;
    }
    details.license = online.license.or(details.license.take());
    details.icon_url = online.icon_url.or(details.icon_url.take());
    details.downloads = online.downloads;
    details.categories = online.categories;
    details.gallery = online.gallery;
    let mut links = online.links;
    for link in details.links.drain(..) {
        if !links.iter().any(|l| l.url.trim_end_matches('/') == link.url.trim_end_matches('/')) {
            links.push(link);
        }
    }
    details.links = links;
}

async fn modrinth_by_hash(sites: &Sites<'_>, path: &Path) -> Result<Option<String>> {
    let hash = hex::encode(Sha1::digest(std::fs::read(path)?));
    let res = sites.client.get(format!("{MODRINTH}/version_file/{hash}?algorithm=sha1")).send().await?;
    if res.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    let version: Value = res.error_for_status()?.json().await?;
    Ok(version["project_id"].as_str().map(String::from))
}

async fn modrinth(sites: &Sites<'_>, id: &str) -> Result<ModDetails> {
    let project = json(sites.client.get(format!("{MODRINTH}/project/{id}"))).await?;
    // The team is a nicety; the page still shows without it.
    let members = json(sites.client.get(format!("{MODRINTH}/project/{id}/members"))).await.unwrap_or_default();
    let authors = members
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| m["user"]["username"].as_str().map(String::from))
        .collect();
    let page = format!("https://modrinth.com/{}/{}", text(&project, "project_type"), text(&project, "slug"));
    let mut links = vec![Link { label: "Modrinth".into(), url: page }];
    for (key, label) in [("source_url", "Source Code"), ("issues_url", "Issues"), ("wiki_url", "Wiki"), ("discord_url", "Discord")] {
        push_link(&mut links, label, project[key].as_str());
    }
    Ok(ModDetails {
        title: text(&project, "title"),
        summary: project["description"].as_str().map(String::from),
        authors,
        license: license_name(&project["license"]),
        icon_url: project["icon_url"].as_str().map(String::from),
        downloads: project["downloads"].as_u64(),
        categories: strings(&project["categories"]),
        gallery: project["gallery"].as_array().into_iter().flatten().filter_map(|g| g["url"].as_str().map(String::from)).collect(),
        links,
        source: Some(Source::Modrinth),
        ..Default::default()
    })
}

fn license_name(license: &Value) -> Option<String> {
    let name = license["name"].as_str().filter(|n| !n.is_empty()).or(license["id"].as_str())?;
    // Custom licenses come as "LicenseRef-Polyform-Shield-License-1.0.0".
    Some(match name.strip_prefix("LicenseRef-") {
        Some("Custom" | "All-Rights-Reserved") | Some("") => "Custom".into(),
        Some(custom) => custom.replace('-', " "),
        None => name.to_string(),
    })
}

async fn curseforge(sites: &Sites<'_>, id: &str) -> Result<ModDetails> {
    let project = json(sites.curseforge(format!("{CURSEFORGE}/mods/{id}"))?).await?;
    let project = &project["data"];
    let mut links = Vec::new();
    push_link(&mut links, "CurseForge", project["links"]["websiteUrl"].as_str());
    for (key, label) in [("sourceUrl", "Source Code"), ("issuesUrl", "Issues"), ("wikiUrl", "Wiki")] {
        push_link(&mut links, label, project["links"][key].as_str());
    }
    Ok(ModDetails {
        title: text(project, "name"),
        summary: project["summary"].as_str().map(String::from),
        authors: project["authors"].as_array().into_iter().flatten().filter_map(|a| a["name"].as_str().map(String::from)).collect(),
        icon_url: project["logo"]["thumbnailUrl"].as_str().map(String::from),
        downloads: project["downloadCount"].as_f64().map(|d| d as u64),
        categories: project["categories"].as_array().into_iter().flatten().filter_map(|c| c["name"].as_str().map(String::from)).collect(),
        gallery: project["screenshots"].as_array().into_iter().flatten().filter_map(|s| s["url"].as_str().map(String::from)).collect(),
        links,
        source: Some(Source::Curseforge),
        ..Default::default()
    })
}

fn push_link(links: &mut Vec<Link>, label: &str, url: Option<&str>) {
    if let Some(url) = url.map(str::trim).filter(|u| u.starts_with("https://") || u.starts_with("http://")) {
        if !links.iter().any(|l| l.url == url) {
            links.push(Link { label: label.into(), url: url.into() });
        }
    }
}

fn strings(value: &Value) -> Vec<String> {
    value.as_array().into_iter().flatten().filter_map(|v| v.as_str().map(String::from)).collect()
}

/// What the file says about itself. Folders (unzipped packs) are read the same way.
fn local(path: &Path, kind: Kind) -> ModDetails {
    let mut read = reader(path);
    let mut details = ModDetails::default();
    if kind != Kind::Mod {
        if let Some(meta) = read("pack.mcmeta").and_then(|t| serde_json::from_str::<Value>(&t).ok()) {
            details.summary = pack_description(&meta["pack"]["description"]);
        }
        return details;
    }
    if let Some(json) = read("fabric.mod.json").and_then(|t| serde_json::from_str::<Value>(&t).ok()) {
        fabric(&mut details, &json);
    } else if let Some(json) = read("quilt.mod.json").and_then(|t| serde_json::from_str::<Value>(&t).ok()) {
        quilt(&mut details, &json["quilt_loader"]);
    } else if let Some(toml) = read("META-INF/neoforge.mods.toml").or_else(|| read("META-INF/mods.toml")) {
        forge_toml(&mut details, &toml, read("META-INF/MANIFEST.MF"));
    } else if let Some(json) = read("mcmod.info").and_then(|t| serde_json::from_str::<Value>(&t).ok()) {
        mcmod_info(&mut details, &json);
    }
    details.version = details.version.filter(|v| !v.contains("${"));
    details
}

/// Reads a file inside a jar, zip or folder.
fn reader(path: &Path) -> Box<dyn FnMut(&str) -> Option<String>> {
    if path.is_dir() {
        let path = path.to_path_buf();
        return Box::new(move |name| std::fs::read_to_string(path.join(name)).ok());
    }
    let archive = std::fs::File::open(path).ok().and_then(|f| zip::ZipArchive::new(f).ok());
    let Some(mut archive) = archive else { return Box::new(|_| None) };
    Box::new(move |name| {
        let mut entry = archive.by_name(name).ok()?;
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).ok()?;
        Some(String::from_utf8_lossy(&bytes).into_owned())
    })
}

/// A pack description is plain text or Minecraft's JSON text components.
fn pack_description(value: &Value) -> Option<String> {
    fn flatten(value: &Value, out: &mut String) {
        match value {
            Value::String(s) => out.push_str(s),
            Value::Array(parts) => parts.iter().for_each(|p| flatten(p, out)),
            Value::Object(o) => {
                if let Some(t) = o.get("text") {
                    flatten(t, out);
                }
                if let Some(extra) = o.get("extra") {
                    flatten(extra, out);
                }
            }
            _ => {}
        }
    }
    let mut out = String::new();
    flatten(value, &mut out);
    clean(&out)
}

/// Drops § formatting codes and blank text.
fn clean(text: &str) -> Option<String> {
    let mut out = String::new();
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '§' {
            chars.next();
        } else {
            out.push(c);
        }
    }
    let out = out.trim().to_string();
    (!out.is_empty()).then_some(out)
}

fn person(value: &Value) -> Option<String> {
    value.as_str().or(value["name"].as_str()).map(String::from)
}

fn fabric(details: &mut ModDetails, json: &Value) {
    details.mod_ids = json["id"].as_str().map(|id| vec![id.to_string()]).unwrap_or_default();
    details.summary = json["description"].as_str().and_then(clean);
    details.version = json["version"].as_str().map(String::from);
    details.authors = json["authors"].as_array().into_iter().flatten().filter_map(person).collect();
    details.license = json["license"].as_str().map(String::from).or_else(|| json["license"][0].as_str().map(String::from));
    let contact = &json["contact"];
    for (key, label) in [("homepage", "Website"), ("sources", "Source Code"), ("issues", "Issues")] {
        push_link(&mut details.links, label, contact[key].as_str());
    }
}

fn quilt(details: &mut ModDetails, loader: &Value) {
    let meta = &loader["metadata"];
    details.mod_ids = loader["id"].as_str().map(|id| vec![id.to_string()]).unwrap_or_default();
    details.summary = meta["description"].as_str().and_then(clean);
    details.version = loader["version"].as_str().map(String::from);
    details.authors = meta["contributors"].as_object().into_iter().flatten().map(|(name, _)| name.clone()).collect();
    details.license = meta["license"].as_str().map(String::from);
    for (key, label) in [("homepage", "Website"), ("sources", "Source Code"), ("issues", "Issues")] {
        push_link(&mut details.links, label, meta["contact"][key].as_str());
    }
}

fn forge_toml(details: &mut ModDetails, text: &str, manifest: Option<String>) {
    let Ok(toml) = text.parse::<toml::Table>() else { return };
    let mods = toml.get("mods").and_then(|m| m.as_array()).cloned().unwrap_or_default();
    details.mod_ids = mods.iter().filter_map(|m| m.get("modId")?.as_str().map(String::from)).collect();
    details.license = toml.get("license").and_then(|l| l.as_str()).map(String::from);
    push_link(&mut details.links, "Issues", toml.get("issueTrackerURL").and_then(|u| u.as_str()));
    let Some(first) = mods.first() else { return };
    let field = |key: &str| first.get(key).and_then(|v| v.as_str()).map(String::from);
    details.summary = field("description").and_then(|d| clean(&d));
    details.authors = field("authors").map(|a| a.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()).unwrap_or_default();
    push_link(&mut details.links, "Website", field("displayURL").as_deref());
    // "${file.jarVersion}" means the version is in the jar's manifest.
    details.version = field("version").and_then(|v| {
        if v.contains("${") {
            manifest.as_deref()?.lines().find_map(|l| l.strip_prefix("Implementation-Version:")).map(|v| v.trim().to_string())
        } else {
            Some(v)
        }
    });
}

/// Forge before 1.13: a list of mods, or `{ "modList": [...] }`.
fn mcmod_info(details: &mut ModDetails, json: &Value) {
    let list = json.as_array().or(json["modList"].as_array()).cloned().unwrap_or_default();
    details.mod_ids = list.iter().filter_map(|m| m["modid"].as_str().map(String::from)).collect();
    let Some(first) = list.first() else { return };
    details.summary = first["description"].as_str().and_then(clean);
    details.version = first["version"].as_str().map(String::from);
    details.authors = strings(&first["authorList"]).into_iter().chain(strings(&first["authors"])).collect();
    push_link(&mut details.links, "Website", first["url"].as_str());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_old_forge_mcmod_info() {
        let mut details = ModDetails::default();
        let json = serde_json::json!([{
            "modid": "entityculling", "name": "EntityCulling", "description": "§aHides entities\n",
            "version": "1.5.0", "url": "https://modrinth.com/mod/entityculling", "authorList": ["tr7zw"]
        }]);
        mcmod_info(&mut details, &json);
        assert_eq!(details.mod_ids, ["entityculling"]);
        assert_eq!(details.summary.as_deref(), Some("Hides entities"));
        assert_eq!(details.authors, ["tr7zw"]);
        assert_eq!(details.links[0].label, "Website");
    }

    #[test]
    fn reads_mods_toml_with_a_manifest_version() {
        let mut details = ModDetails::default();
        let toml = "modLoader=\"javafml\"\nlicense=\"MIT\"\n[[mods]]\nmodId=\"jei\"\nversion=\"${file.jarVersion}\"\ndescription='''\nShows items\n'''\nauthors=\"mezz, others\"\n";
        forge_toml(&mut details, toml, Some("Manifest-Version: 1.0\nImplementation-Version: 19.21.0\n".into()));
        assert_eq!(details.mod_ids, ["jei"]);
        assert_eq!(details.version.as_deref(), Some("19.21.0"));
        assert_eq!(details.summary.as_deref(), Some("Shows items"));
        assert_eq!(details.authors, ["mezz", "others"]);
        assert_eq!(details.license.as_deref(), Some("MIT"));
    }

    #[test]
    fn flattens_pack_descriptions() {
        let value = serde_json::json!([{ "text": "§6Faithful", "extra": [" 32x"] }]);
        assert_eq!(pack_description(&value).as_deref(), Some("Faithful 32x"));
    }

    /// A jar dropped in by hand is still recognized on Modrinth by its hash.
    #[tokio::test]
    #[ignore]
    async fn finds_a_hand_added_jar_on_modrinth() {
        let dir = std::env::temp_dir().join(format!("clover-info-{}", std::process::id()));
        let paths = crate::paths::Paths::at(dir.clone());
        let instance = crate::instances::create(&paths, "Info", "1.21.4", "fabric", Some("0.16.10".into())).unwrap();
        let client = reqwest::Client::builder().user_agent("CloverNetwork/Clover-Launcher/test").build().unwrap();
        let sites = Sites { client: &client, curseforge_key: None };
        let versions = json(client.get(format!("{MODRINTH}/project/sodium/version")).query(&[
            ("loaders", r#"["fabric"]"#),
            ("game_versions", r#"["1.21.4"]"#),
        ]))
        .await
        .unwrap();
        let file = &versions[0]["files"][0];
        let shelf = Shelf::mods(&paths, &instance);
        std::fs::create_dir_all(shelf.dir()).unwrap();
        let bytes = client.get(text(file, "url")).send().await.unwrap().bytes().await.unwrap();
        std::fs::write(shelf.dir().join(text(file, "filename")), bytes).unwrap();

        let info = details(&sites, &shelf, &text(file, "filename")).await.unwrap();
        println!("{info:#?}");
        assert_eq!(info.source, Some(Source::Modrinth));
        assert_eq!(info.title, "Sodium");
        assert_eq!(info.mod_ids, ["sodium"]);
        assert!(!info.authors.is_empty() && info.downloads.is_some() && info.summary.is_some());
        assert!(info.links.iter().any(|l| l.url.starts_with("https://modrinth.com/mod/sodium")));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
