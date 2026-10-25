//! Skins and capes: the account's wardrobe on Mojang's servers, and a local library of
//! skin files to switch between.
//! Reference: https://minecraft.wiki/w/Mojang_API#Change_skin

use std::path::PathBuf;

use base64::Engine;
use reqwest::{Client, RequestBuilder, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha1::{Digest, Sha1};

use crate::error::{Result, msg};
use crate::paths::Paths;

const PROFILE_URL: &str = "https://api.minecraftservices.com/minecraft/profile";

/// What the account owns and wears, as Mojang reports it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Wardrobe {
    #[serde(default)]
    pub skins: Vec<RemoteSkin>,
    #[serde(default)]
    pub capes: Vec<RemoteCape>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteSkin {
    pub id: String,
    /// "ACTIVE" for the skin being worn.
    pub state: String,
    pub url: String,
    /// "CLASSIC" (4-pixel arms) or "SLIM" (3-pixel arms).
    pub variant: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteCape {
    pub id: String,
    /// "ACTIVE" for the cape being worn; every other owned cape is "INACTIVE".
    pub state: String,
    pub url: String,
    #[serde(default)]
    pub alias: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Variant {
    Classic,
    Slim,
}

impl Variant {
    fn api_name(self) -> &'static str {
        match self {
            Variant::Classic => "classic",
            Variant::Slim => "slim",
        }
    }
}

pub async fn wardrobe(client: &Client, token: &str) -> Result<Wardrobe> {
    send(client.get(PROFILE_URL).bearer_auth(token)).await
}

/// Uploads a skin file and wears it.
pub async fn upload(client: &Client, token: &str, png: Vec<u8>, variant: Variant) -> Result<Wardrobe> {
    let file = reqwest::multipart::Part::bytes(png).file_name("skin.png").mime_str("image/png")?;
    let form = reqwest::multipart::Form::new().text("variant", variant.api_name()).part("file", file);
    send(client.post(format!("{PROFILE_URL}/skins")).bearer_auth(token).multipart(form)).await
}

/// Re-applies a skin Mojang already hosts, e.g. to change the arm model of the current one.
pub async fn wear_url(client: &Client, token: &str, url: &str, variant: Variant) -> Result<Wardrobe> {
    let body = json!({ "variant": variant.api_name(), "url": url });
    send(client.post(format!("{PROFILE_URL}/skins")).bearer_auth(token).json(&body)).await
}

/// Puts on a cape the account owns, or takes the cape off with `None`.
pub async fn wear_cape(client: &Client, token: &str, cape_id: Option<&str>) -> Result<Wardrobe> {
    let url = format!("{PROFILE_URL}/capes/active");
    let request = match cape_id {
        Some(id) => client.put(url).json(&json!({ "capeId": id })),
        None => client.delete(url),
    };
    send(request.bearer_auth(token)).await
}

async fn send(request: RequestBuilder) -> Result<Wardrobe> {
    let res = request.send().await?;
    match res.status() {
        status if status.is_success() => Ok(res.json().await?),
        StatusCode::UNAUTHORIZED => Err(msg("Your session ended. Sign in to this account again.")),
        StatusCode::TOO_MANY_REQUESTS => Err(msg("Mojang limits how often skins change. Try again in a minute.")),
        StatusCode::BAD_REQUEST => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct ApiError {
                error_message: Option<String>,
            }
            let detail = res.json::<ApiError>().await.ok().and_then(|e| e.error_message);
            Err(msg(detail.unwrap_or_else(|| "Mojang didn't accept that skin.".into())))
        }
        status => Err(msg(format!("Mojang's skin service answered {status}. Try again later."))),
    }
}

/// Returns a texture from textures.minecraft.net as a data URL, caching it on disk.
/// Fetched here rather than in the window because that host doesn't send CORS headers.
pub async fn texture(client: &Client, paths: &Paths, url: &str) -> Result<String> {
    Ok(data_url(&texture_bytes(client, paths, url).await?))
}

pub async fn texture_bytes(client: &Client, paths: &Paths, url: &str) -> Result<Vec<u8>> {
    if !url.starts_with("http://textures.minecraft.net/") && !url.starts_with("https://textures.minecraft.net/") {
        return Err(msg("That isn't a Minecraft texture."));
    }
    let file = paths.root.join("textures").join(format!("{}.png", hex::encode(Sha1::digest(url))));
    let bytes = match std::fs::read(&file) {
        Ok(bytes) => bytes,
        Err(_) => {
            let bytes = client.get(url.replacen("http://", "https://", 1)).send().await?.error_for_status()?.bytes().await?;
            std::fs::create_dir_all(file.parent().expect("has parent"))?;
            std::fs::write(&file, &bytes)?;
            bytes.to_vec()
        }
    };
    Ok(bytes)
}

fn data_url(png: &[u8]) -> String {
    format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(png))
}

/// Checks a file is a PNG skin: 64×64, or 64×32 for the pre-1.8 layout.
pub fn validate(png: &[u8]) -> Result<()> {
    const SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
    if png.len() < 24 || &png[..8] != SIGNATURE || &png[12..16] != b"IHDR" {
        return Err(msg("Skins need to be PNG images."));
    }
    let width = u32::from_be_bytes(png[16..20].try_into().expect("4 bytes"));
    let height = u32::from_be_bytes(png[20..24].try_into().expect("4 bytes"));
    if width != 64 || (height != 64 && height != 32) {
        return Err(msg(format!("Skins are 64×64 pixels (or 64×32 for old skins). This image is {width}×{height}.")));
    }
    Ok(())
}

/// A skin file kept in the library, shared by every account.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibrarySkin {
    pub id: String,
    pub name: String,
    pub variant: Variant,
}

fn library_dir(paths: &Paths) -> PathBuf {
    paths.root.join("skins")
}

fn library_file(paths: &Paths) -> PathBuf {
    library_dir(paths).join("library.json")
}

pub fn library(paths: &Paths) -> Vec<LibrarySkin> {
    std::fs::read(library_file(paths)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

fn save_library(paths: &Paths, skins: &[LibrarySkin]) -> Result<()> {
    std::fs::create_dir_all(library_dir(paths))?;
    std::fs::write(library_file(paths), serde_json::to_vec_pretty(skins)?)?;
    Ok(())
}

pub fn library_png(paths: &Paths, id: &str) -> Result<Vec<u8>> {
    if !id.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(msg("That skin isn't in the library."));
    }
    std::fs::read(library_dir(paths).join(format!("{id}.png"))).map_err(|_| msg("That skin isn't in the library any more."))
}

pub fn library_texture(paths: &Paths, id: &str) -> Result<String> {
    Ok(data_url(&library_png(paths, id)?))
}

/// Copies a skin file into the library. The same image added twice is kept once.
pub fn add_to_library(paths: &Paths, png: &[u8], name: &str, variant: Variant) -> Result<LibrarySkin> {
    validate(png)?;
    let id = hex::encode(&Sha1::digest(png)[..8]);
    let mut skins = library(paths);
    if let Some(existing) = skins.iter().find(|s| s.id == id) {
        return Ok(existing.clone());
    }
    std::fs::create_dir_all(library_dir(paths))?;
    std::fs::write(library_dir(paths).join(format!("{id}.png")), png)?;
    let name = Some(name.trim()).filter(|n| !n.is_empty()).unwrap_or("Skin");
    let skin = LibrarySkin { id, name: name.into(), variant };
    skins.push(skin.clone());
    save_library(paths, &skins)?;
    Ok(skin)
}

pub fn update_library_skin(paths: &Paths, id: &str, name: Option<&str>, variant: Option<Variant>) -> Result<LibrarySkin> {
    let mut skins = library(paths);
    let skin = skins.iter_mut().find(|s| s.id == id).ok_or_else(|| msg("That skin isn't in the library any more."))?;
    if let Some(name) = name.map(str::trim).filter(|n| !n.is_empty()) {
        skin.name = name.into();
    }
    if let Some(variant) = variant {
        skin.variant = variant;
    }
    let updated = skin.clone();
    save_library(paths, &skins)?;
    Ok(updated)
}

pub fn remove_from_library(paths: &Paths, id: &str) -> Result<()> {
    let mut skins = library(paths);
    skins.retain(|s| s.id != id);
    save_library(paths, &skins)?;
    let _ = std::fs::remove_file(library_dir(paths).join(format!("{id}.png")));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
        bytes.extend(width.to_be_bytes());
        bytes.extend(height.to_be_bytes());
        bytes.extend([8, 6, 0, 0, 0]);
        bytes
    }

    #[test]
    fn validates_skin_sizes() {
        assert!(validate(&png(64, 64)).is_ok());
        assert!(validate(&png(64, 32)).is_ok());
        assert!(validate(&png(128, 128)).is_err());
        assert!(validate(b"GIF89a not a png at all").is_err());
    }

    #[test]
    fn library_round_trip() {
        let dir = std::env::temp_dir().join(format!("clover-skins-{}", std::process::id()));
        let paths = Paths::at(dir.clone());
        let skin = add_to_library(&paths, &png(64, 64), "  Knight ", Variant::Classic).unwrap();
        assert_eq!(skin.name, "Knight");
        // Adding the same image again keeps one copy.
        assert_eq!(add_to_library(&paths, &png(64, 64), "Copy", Variant::Slim).unwrap().id, skin.id);
        assert_eq!(library(&paths).len(), 1);

        let updated = update_library_skin(&paths, &skin.id, Some("Paladin"), Some(Variant::Slim)).unwrap();
        assert_eq!((updated.name.as_str(), updated.variant), ("Paladin", Variant::Slim));
        assert!(library_texture(&paths, &skin.id).unwrap().starts_with("data:image/png;base64,"));
        assert!(library_png(&paths, "../secrets").is_err());

        remove_from_library(&paths, &skin.id).unwrap();
        assert!(library(&paths).is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
