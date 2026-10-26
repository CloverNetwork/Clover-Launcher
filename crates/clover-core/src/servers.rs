//! The instance's server list, kept in the game's own `servers.dat` so the in-game
//! Multiplayer screen shows the same servers, and a status ping like the one that screen does.
//! References: https://minecraft.wiki/w/Servers.dat_format and
//! https://minecraft.wiki/w/Java_Edition_protocol/Server_List_Ping

use std::path::PathBuf;
use std::time::{Duration, Instant};

use base64::Engine;
use fastnbt::Value;
use serde::Serialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use crate::error::{Result, msg};
use crate::instances::Instance;
use crate::paths::Paths;

const DEFAULT_PORT: u16 = 25565;
const TIMEOUT: Duration = Duration::from_secs(6);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Server {
    pub name: String,
    pub address: String,
    /// The icon the game saved from the last ping, as a data URL.
    pub icon: Option<String>,
}

fn file(paths: &Paths, instance: &Instance) -> PathBuf {
    paths.instance(&instance.id).join("minecraft").join("servers.dat")
}

/// The whole file, so fields Clover doesn't know about survive a save.
fn read_root(paths: &Paths, instance: &Instance) -> Result<Value> {
    match std::fs::read(file(paths, instance)) {
        Ok(bytes) => fastnbt::from_bytes(&bytes).map_err(|e| msg(format!("The server list can't be read ({e})."))),
        Err(_) => Ok(Value::Compound(Default::default())),
    }
}

fn entries(root: &mut Value) -> Result<&mut Vec<Value>> {
    let Value::Compound(map) = root else { return Err(msg("The server list can't be read.")) };
    let list = map.entry("servers".into()).or_insert_with(|| Value::List(vec![]));
    match list {
        Value::List(items) => Ok(items),
        _ => Err(msg("The server list can't be read.")),
    }
}

fn string(entry: &Value, key: &str) -> Option<String> {
    match entry {
        Value::Compound(map) => match map.get(key) {
            Some(Value::String(s)) => Some(s.clone()),
            _ => None,
        },
        _ => None,
    }
}

pub fn list(paths: &Paths, instance: &Instance) -> Result<Vec<Server>> {
    let mut root = read_root(paths, instance)?;
    Ok(entries(&mut root)?
        .iter()
        .filter_map(|entry| {
            Some(Server {
                name: string(entry, "name").unwrap_or_else(|| "Minecraft Server".into()),
                address: string(entry, "ip")?,
                icon: string(entry, "icon").filter(|i| !i.is_empty()).map(|i| format!("data:image/png;base64,{i}")),
            })
        })
        .collect())
}

fn save(paths: &Paths, instance: &Instance, root: &Value) -> Result<()> {
    let path = file(paths, instance);
    std::fs::create_dir_all(path.parent().expect("has parent"))?;
    let bytes = fastnbt::to_bytes(root).map_err(|e| msg(format!("Couldn't save the server list ({e}).")))?;
    let partial = path.with_extension("dat.part");
    std::fs::write(&partial, bytes)?;
    std::fs::rename(partial, path)?;
    Ok(())
}

fn check_address(address: &str) -> Result<String> {
    let address = address.trim();
    if address.is_empty() || address.contains(char::is_whitespace) || address.contains('/') {
        return Err(msg("Enter a server address, such as mc.example.com."));
    }
    Ok(address.to_string())
}

/// Adds a server at the top, where the game also puts new ones.
pub fn add(paths: &Paths, instance: &Instance, name: &str, address: &str) -> Result<Vec<Server>> {
    let address = check_address(address)?;
    let name = if name.trim().is_empty() { "Minecraft Server" } else { name.trim() };
    let mut root = read_root(paths, instance)?;
    let entry = Value::Compound([("name".to_string(), Value::String(name.into())), ("ip".to_string(), Value::String(address))].into());
    entries(&mut root)?.insert(0, entry);
    save(paths, instance, &root)?;
    list(paths, instance)
}

/// Changes a server's name and address, keeping its icon and the game's other settings.
pub fn edit(paths: &Paths, instance: &Instance, index: usize, name: &str, address: &str) -> Result<Vec<Server>> {
    let address = check_address(address)?;
    let mut root = read_root(paths, instance)?;
    let Some(Value::Compound(entry)) = entries(&mut root)?.get_mut(index) else { return Err(msg("That server is no longer in the list.")) };
    entry.insert("name".into(), Value::String(name.trim().into()));
    // The saved icon belongs to the old address.
    if string(&Value::Compound(entry.clone()), "ip").as_deref() != Some(address.as_str()) {
        entry.remove("icon");
    }
    entry.insert("ip".into(), Value::String(address));
    save(paths, instance, &root)?;
    list(paths, instance)
}

pub fn remove(paths: &Paths, instance: &Instance, index: usize) -> Result<Vec<Server>> {
    let mut root = read_root(paths, instance)?;
    let servers = entries(&mut root)?;
    if index >= servers.len() {
        return Err(msg("That server is no longer in the list."));
    }
    servers.remove(index);
    save(paths, instance, &root)?;
    list(paths, instance)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    /// The message of the day, without formatting codes.
    pub motd: String,
    pub online: u64,
    pub max: u64,
    /// What the server calls its version, such as "Paper 1.21.4".
    pub version: String,
    pub latency_ms: u64,
    pub icon: Option<String>,
}

/// Splits "host:port", defaulting the port. IPv6 addresses go in brackets: "[::1]:25565".
fn host_port(address: &str) -> (String, Option<u16>) {
    if let Some(rest) = address.strip_prefix('[')
        && let Some((host, tail)) = rest.split_once(']')
    {
        return (host.to_string(), tail.strip_prefix(':').and_then(|p| p.parse().ok()));
    }
    match address.rsplit_once(':') {
        Some((host, port)) if !host.contains(':') => (host.to_string(), port.parse().ok()),
        _ => (address.to_string(), None),
    }
}

/// Where to connect: an address without a port may point elsewhere through a
/// `_minecraft._tcp` SRV record, as the game itself checks.
async fn resolve(address: &str) -> (String, u16) {
    let (host, port) = host_port(address);
    if let Some(port) = port {
        return (host, port);
    }
    if host.parse::<std::net::IpAddr>().is_err()
        && let Ok(Ok(resolver)) = hickory_resolver::Resolver::builder_tokio().map(|b| b.build())
        && let Ok(Ok(found)) = tokio::time::timeout(Duration::from_secs(3), resolver.srv_lookup(format!("_minecraft._tcp.{host}."))).await
        && let Some((target, port)) = found.answers().iter().find_map(|record| match &record.data {
            hickory_resolver::proto::rr::RData::SRV(srv) => Some((srv.target.to_utf8(), srv.port)),
            _ => None,
        })
    {
        return (target.trim_end_matches('.').to_string(), port);
    }
    (host, DEFAULT_PORT)
}

fn varint(mut value: i32, out: &mut Vec<u8>) {
    loop {
        let byte = (value & 0x7f) as u8;
        value = ((value as u32) >> 7) as i32;
        if value == 0 {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

async fn read_varint(stream: &mut TcpStream) -> Result<i32> {
    let mut value = 0i32;
    for shift in (0..35).step_by(7) {
        let byte = stream.read_u8().await?;
        value |= ((byte & 0x7f) as i32) << shift;
        if byte & 0x80 == 0 {
            return Ok(value);
        }
    }
    Err(msg("The server sent something that isn't Minecraft."))
}

async fn send_packet(stream: &mut TcpStream, id: i32, body: &[u8]) -> Result<()> {
    let mut payload = Vec::new();
    varint(id, &mut payload);
    payload.extend_from_slice(body);
    let mut frame = Vec::new();
    varint(payload.len() as i32, &mut frame);
    frame.extend(payload);
    stream.write_all(&frame).await?;
    Ok(())
}

/// Asks a server for its status: player count, version and message, and how long it takes to answer.
pub async fn ping(address: &str) -> Result<Status> {
    tokio::time::timeout(TIMEOUT, ping_inner(address)).await.map_err(|_| msg("The server didn't answer."))?
}

async fn ping_inner(address: &str) -> Result<Status> {
    let address = check_address(address)?;
    let (host, port) = resolve(&address).await;
    let mut stream = TcpStream::connect((host.as_str(), port)).await.map_err(|_| msg("Can't reach the server."))?;
    stream.set_nodelay(true)?;

    // Handshake: protocol version 47 is understood by every server since 1.8; next state 1 is status.
    let mut handshake = Vec::new();
    varint(47, &mut handshake);
    varint(host.len() as i32, &mut handshake);
    handshake.extend_from_slice(host.as_bytes());
    handshake.extend_from_slice(&port.to_be_bytes());
    varint(1, &mut handshake);
    send_packet(&mut stream, 0, &handshake).await?;
    send_packet(&mut stream, 0, &[]).await?;

    let _length = read_varint(&mut stream).await?;
    if read_varint(&mut stream).await? != 0 {
        return Err(msg("The server sent something that isn't Minecraft."));
    }
    let size = read_varint(&mut stream).await?;
    if !(0..=1 << 21).contains(&size) {
        return Err(msg("The server sent something that isn't Minecraft."));
    }
    let mut json = vec![0; size as usize];
    stream.read_exact(&mut json).await?;
    let status: serde_json::Value = serde_json::from_slice(&json).map_err(|_| msg("The server sent something that isn't Minecraft."))?;

    // Ping and pong, for the latency the game shows as bars.
    let started = Instant::now();
    send_packet(&mut stream, 1, &1i64.to_be_bytes()).await?;
    let latency = async {
        let _length = read_varint(&mut stream).await?;
        let _id = read_varint(&mut stream).await?;
        stream.read_i64().await?;
        Ok::<_, crate::error::Error>(started.elapsed())
    };
    let latency_ms = tokio::time::timeout(Duration::from_secs(3), latency).await.ok().and_then(|r| r.ok()).unwrap_or_else(|| started.elapsed()).as_millis() as u64;

    let icon = status["favicon"].as_str().filter(|f| f.starts_with("data:image/png;base64,")).map(String::from);
    Ok(Status {
        motd: strip_codes(&chat_text(&status["description"])).trim().to_string(),
        online: status["players"]["online"].as_u64().unwrap_or(0),
        max: status["players"]["max"].as_u64().unwrap_or(0),
        version: strip_codes(status["version"]["name"].as_str().unwrap_or_default()),
        latency_ms,
        icon,
    })
}

/// Saves the icon from a ping into the server list, as the game does.
pub fn remember_icon(paths: &Paths, instance: &Instance, address: &str, icon: &str) -> Result<()> {
    let Some(data) = icon.strip_prefix("data:image/png;base64,") else { return Ok(()) };
    if base64::engine::general_purpose::STANDARD.decode(data).is_err() {
        return Ok(());
    }
    let mut root = read_root(paths, instance)?;
    let mut changed = false;
    for entry in entries(&mut root)? {
        if string(entry, "ip").as_deref() == Some(address)
            && string(entry, "icon").as_deref() != Some(data)
            && let Value::Compound(map) = entry
        {
            map.insert("icon".into(), Value::String(data.into()));
            changed = true;
        }
    }
    if changed { save(paths, instance, &root) } else { Ok(()) }
}

/// The plain text of a chat component: a string, or `text` plus its `extra` parts.
fn chat_text(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(parts) => parts.iter().map(chat_text).collect(),
        serde_json::Value::Object(_) => {
            let mut text = value["text"].as_str().unwrap_or_default().to_string();
            for part in value["extra"].as_array().into_iter().flatten() {
                text.push_str(&chat_text(part));
            }
            text
        }
        _ => String::new(),
    }
}

/// Removes "§a"-style color and format codes.
fn strip_codes(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '§' {
            chars.next();
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_the_games_server_list_intact() {
        let root = std::env::temp_dir().join(format!("clover-servers-{}", std::process::id()));
        let paths = Paths::at(root.clone());
        let instance = crate::instances::create(&paths, "Servers", "1.21.4", "vanilla", None).unwrap();
        // A list as the game writes it, with a field Clover doesn't know about.
        let game = Value::Compound(
            [(
                "servers".to_string(),
                Value::List(vec![Value::Compound(
                    [
                        ("name".to_string(), Value::String("Old".into())),
                        ("ip".to_string(), Value::String("old.example.com".into())),
                        ("acceptTextures".to_string(), Value::Byte(1)),
                    ]
                    .into(),
                )]),
            )]
            .into(),
        );
        save(&paths, &instance, &game).unwrap();

        let servers = add(&paths, &instance, "Hypixel", " mc.hypixel.net ").unwrap();
        assert_eq!(servers.iter().map(|s| s.address.as_str()).collect::<Vec<_>>(), ["mc.hypixel.net", "old.example.com"]);
        let servers = edit(&paths, &instance, 1, "Renamed", "old.example.com").unwrap();
        assert_eq!(servers[1].name, "Renamed");
        let mut saved = read_root(&paths, &instance).unwrap();
        assert!(matches!(&entries(&mut saved).unwrap()[1], Value::Compound(m) if m.get("acceptTextures") == Some(&Value::Byte(1))));
        assert_eq!(remove(&paths, &instance, 0).unwrap().len(), 1);
        assert!(add(&paths, &instance, "", "not an address").is_err());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn reads_addresses_and_messages() {
        assert_eq!(host_port("mc.example.com"), ("mc.example.com".into(), None));
        assert_eq!(host_port("mc.example.com:25566"), ("mc.example.com".into(), Some(25566)));
        assert_eq!(host_port("[::1]:25570"), ("::1".into(), Some(25570)));
        let motd = serde_json::json!({ "text": "§aHello ", "extra": [{ "text": "World", "color": "gold" }] });
        assert_eq!(strip_codes(&chat_text(&motd)), "Hello World");
    }

    /// Pings a large public server. Run with `cargo test pings_a_server -- --ignored --nocapture`.
    #[tokio::test]
    #[ignore]
    async fn pings_a_server() {
        let status = ping("mc.hypixel.net").await.unwrap();
        println!("{} / {} · {} · {} ms · {}", status.online, status.max, status.version, status.latency_ms, status.motd);
        assert!(status.max > 0 && status.icon.is_some());
        // 2b2t.org points to its real host through an SRV record.
        assert_eq!(resolve("2b2t.org").await, ("connect.2b2t.org".into(), 25565));
    }
}
