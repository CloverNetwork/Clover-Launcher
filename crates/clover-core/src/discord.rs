//! Discord Rich Presence: "Playing Clover Launcher" with the instance and version, through
//! the Discord app's local IPC socket. Nothing is sent anywhere else, and when Discord isn't
//! running there's simply no status.
//! Reference: https://github.com/discord/discord-rpc/blob/master/documentation/hard-mode.md

use std::time::Duration;

use serde_json::{Value, json};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::error::{Result, msg};

const HANDSHAKE: u32 = 0;
const FRAME: u32 = 1;

trait Socket: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Socket for T {}

/// What the status shows.
pub struct Activity<'a> {
    /// The first line, such as the instance's name.
    pub details: &'a str,
    /// The second line, such as "Minecraft 1.21.4 · Fabric".
    pub state: &'a str,
    /// When play started, in seconds since 1970; Discord counts up from it.
    pub started: u64,
}

pub struct Presence {
    socket: Box<dyn Socket>,
}

#[cfg(unix)]
async fn open_socket() -> Option<Box<dyn Socket>> {
    let mut dirs: Vec<std::path::PathBuf> = ["XDG_RUNTIME_DIR", "TMPDIR", "TMP", "TEMP"]
        .iter()
        .filter_map(|key| std::env::var_os(key).map(std::path::PathBuf::from))
        .collect();
    dirs.push("/tmp".into());
    // Flatpak and Snap builds of Discord put the socket one folder down.
    let subfolders = ["", "app/com.discordapp.Discord", "app/com.discordapp.DiscordCanary", "snap.discord", ".flatpak/dev.vencord.Vesktop/xdg-run"];
    for dir in &dirs {
        for sub in subfolders {
            for n in 0..10 {
                let path = dir.join(sub).join(format!("discord-ipc-{n}"));
                if let Ok(stream) = tokio::net::UnixStream::connect(&path).await {
                    return Some(Box::new(stream));
                }
            }
        }
    }
    None
}

#[cfg(windows)]
async fn open_socket() -> Option<Box<dyn Socket>> {
    for n in 0..10 {
        if let Ok(pipe) = tokio::net::windows::named_pipe::ClientOptions::new().open(format!(r"\\.\pipe\discord-ipc-{n}")) {
            return Some(Box::new(pipe));
        }
    }
    None
}

impl Presence {
    /// Connects to the Discord app on this computer, if it's running.
    pub async fn connect(app_id: &str) -> Result<Self> {
        let socket = open_socket().await.ok_or_else(|| msg("Discord isn't running."))?;
        let mut presence = Self { socket };
        presence.send(HANDSHAKE, &json!({ "v": 1, "client_id": app_id })).await?;
        let ready = presence.receive().await?;
        if ready["evt"] != "READY" {
            return Err(msg(format!("Discord refused the connection: {}", ready["data"]["message"].as_str().unwrap_or("unknown reason"))));
        }
        Ok(presence)
    }

    async fn send(&mut self, op: u32, body: &Value) -> Result<()> {
        let body = serde_json::to_vec(body)?;
        let mut frame = Vec::with_capacity(8 + body.len());
        frame.extend_from_slice(&op.to_le_bytes());
        frame.extend_from_slice(&(body.len() as u32).to_le_bytes());
        frame.extend(body);
        self.socket.write_all(&frame).await?;
        Ok(())
    }

    async fn receive(&mut self) -> Result<Value> {
        let read = async {
            let _op = self.socket.read_u32_le().await?;
            let len = self.socket.read_u32_le().await? as usize;
            if len > 1 << 20 {
                return Err(msg("Discord sent too much."));
            }
            let mut body = vec![0; len];
            self.socket.read_exact(&mut body).await?;
            Ok(serde_json::from_slice(&body)?)
        };
        tokio::time::timeout(Duration::from_secs(5), read).await.map_err(|_| msg("Discord didn't answer."))?
    }

    /// Shows an activity, or clears it with `None`.
    pub async fn set(&mut self, activity: Option<&Activity<'_>>) -> Result<()> {
        let activity = activity.map(|a| {
            json!({
                "details": a.details,
                "state": a.state,
                "timestamps": { "start": a.started },
                "assets": { "large_image": "clover", "large_text": "Clover Launcher" },
            })
        });
        let command = json!({
            "cmd": "SET_ACTIVITY",
            "args": { "pid": std::process::id(), "activity": activity },
            "nonce": uuid::Uuid::new_v4().to_string(),
        });
        self.send(FRAME, &command).await?;
        let reply = self.receive().await?;
        if reply["evt"] == "ERROR" {
            return Err(msg(format!("Discord: {}", reply["data"]["message"].as_str().unwrap_or("error"))));
        }
        Ok(())
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    /// A stand-in for Discord's socket that answers like the real app.
    #[tokio::test]
    async fn speaks_discords_framing() {
        let dir = std::env::temp_dir().join(format!("clover-discord-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("discord-ipc-0");
        let listener = tokio::net::UnixListener::bind(&path).unwrap();
        let fake = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut seen = Vec::new();
            for reply in [json!({ "evt": "READY", "data": {} }), json!({ "evt": null, "cmd": "SET_ACTIVITY" })] {
                let op = socket.read_u32_le().await.unwrap();
                let len = socket.read_u32_le().await.unwrap() as usize;
                let mut body = vec![0; len];
                socket.read_exact(&mut body).await.unwrap();
                seen.push((op, serde_json::from_slice::<Value>(&body).unwrap()));
                let reply = serde_json::to_vec(&reply).unwrap();
                socket.write_all(&1u32.to_le_bytes()).await.unwrap();
                socket.write_all(&(reply.len() as u32).to_le_bytes()).await.unwrap();
                socket.write_all(&reply).await.unwrap();
            }
            seen
        });

        // Point the search at the fake socket.
        unsafe { std::env::set_var("XDG_RUNTIME_DIR", &dir) };
        let mut presence = Presence::connect("1234").await.unwrap();
        let activity = Activity { details: "Survival", state: "Minecraft 1.21.4 · Fabric", started: 1_700_000_000 };
        presence.set(Some(&activity)).await.unwrap();
        let seen = fake.await.unwrap();
        assert_eq!((seen[0].0, &seen[0].1["client_id"]), (HANDSHAKE, &json!("1234")));
        assert_eq!(seen[1].1["args"]["activity"]["details"], "Survival");
        assert_eq!(seen[1].1["args"]["activity"]["timestamps"]["start"], 1_700_000_000);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
