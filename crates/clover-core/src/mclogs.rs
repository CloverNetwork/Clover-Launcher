//! Shares a game log on mclo.gs.
//! Reference: https://api.mclo.gs (POST /1/log)

use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::error::{Result, msg};

const API: &str = "https://api.mclo.gs/1/log";
/// mclo.gs keeps at most this much; trimming here keeps the newest output, which
/// usually holds the crash, instead of the server cutting off the end.
const MAX_BYTES: usize = 10 * 1024 * 1024;
const MAX_LINES: usize = 25_000;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Uploaded {
    pub url: String,
}

#[derive(Deserialize)]
struct Response {
    success: bool,
    url: Option<String>,
    error: Option<String>,
}

/// Facts shown above the log on mclo.gs.
pub struct Details<'a> {
    pub minecraft: &'a str,
    pub loader: &'a str,
}

pub async fn upload(client: &Client, lines: &[String], secrets: &[String], details: Details<'_>) -> Result<Uploaded> {
    if lines.is_empty() {
        return Err(msg("There's no log to upload yet."));
    }
    let content = redact(&newest(lines), secrets);
    let body = json!({
        "content": content,
        "source": "Clover Launcher",
        "metadata": [
            { "key": "minecraft", "label": "Minecraft", "value": details.minecraft },
            { "key": "loader", "label": "Mod Loader", "value": details.loader },
            { "key": "launcher", "label": "Launcher", "value": format!("Clover Launcher {}", crate::LAUNCHER_VERSION) },
        ],
    });
    let response: Response = client.post(API).json(&body).send().await?.json().await?;
    match (response.success, response.url) {
        (true, Some(url)) => Ok(Uploaded { url }),
        _ => Err(msg(format!("mclo.gs didn't take the log: {}", response.error.unwrap_or_else(|| "no reason given".into())))),
    }
}

/// The newest lines that fit mclo.gs's limits.
fn newest(lines: &[String]) -> String {
    let mut bytes = 0;
    let mut kept: Vec<&str> = Vec::new();
    for line in lines.iter().rev().take(MAX_LINES) {
        bytes += line.len() + 1;
        if bytes > MAX_BYTES {
            break;
        }
        kept.push(line);
    }
    kept.reverse();
    kept.join("\n")
}

/// Removes what shouldn't leave this computer before upload. mclo.gs filters tokens,
/// IP addresses and home folders too; this doesn't rely on that.
fn redact(content: &str, secrets: &[String]) -> String {
    let mut content = content.to_string();
    for secret in secrets.iter().filter(|s| s.len() >= 8) {
        content = content.replace(secret.as_str(), "[access token hidden]");
    }
    // Minecraft access tokens are JWTs: three base64url parts starting "eyJ".
    content = replace_jwts(&content);
    if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
        let home = home.to_string_lossy().into_owned();
        if home.len() > 1 {
            content = content.replace(&home, "~");
        }
    }
    content
}

fn replace_jwts(content: &str) -> String {
    let is_token_char = |c: char| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.');
    let mut out = String::with_capacity(content.len());
    let mut rest = content;
    while let Some(start) = rest.find("eyJ") {
        out.push_str(&rest[..start]);
        let candidate = &rest[start..];
        let end = candidate.find(|c: char| !is_token_char(c)).unwrap_or(candidate.len());
        let token = &candidate[..end];
        if token.matches('.').count() == 2 && token.len() > 60 {
            out.push_str("[access token hidden]");
        } else {
            out.push_str(token);
        }
        rest = &candidate[end..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hides_tokens_and_home() {
        let jwt = format!("eyJhbGciOiJIUzI1NiJ9.{}.{}", "a".repeat(40), "b".repeat(30));
        let home = std::env::var("HOME").unwrap();
        let log = format!("--accessToken {jwt} --gameDir {home}/games secret-session-token eyJshort");
        let out = redact(&log, &["secret-session-token".into()]);
        assert!(!out.contains(&jwt) && !out.contains("secret-session-token") && !out.contains(&home));
        assert!(out.contains("--gameDir ~/games"));
        assert!(out.contains("eyJshort"), "short look-alikes stay");
    }

    #[test]
    fn keeps_the_newest_lines() {
        let lines: Vec<String> = (0..MAX_LINES + 10).map(|i| format!("line {i}")).collect();
        let kept = newest(&lines);
        assert!(kept.starts_with("line 10\n") && kept.ends_with(&format!("line {}", MAX_LINES + 9)));
    }
}
