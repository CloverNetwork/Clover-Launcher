//! Microsoft account sign-in: device code -> Xbox Live -> XSTS -> Minecraft services.
//! Reference: https://minecraft.wiki/w/Microsoft_authentication
//!
//! The refresh token lives in the OS keychain. Only the public profile is written to disk.

use std::time::{Duration, Instant};

use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::error::{Result, msg};
use crate::paths::Paths;

/// Register a public client app (personal Microsoft accounts) in Azure, then request
/// Minecraft API access for it at https://aka.ms/mce-reviewappid before release.
pub const CLIENT_ID: &str = "00000000-0000-0000-0000-000000000000";

const SCOPE: &str = "XboxLive.signin offline_access";
const DEVICE_CODE_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode";
const TOKEN_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/token";
const KEYRING_SERVICE: &str = "dev.blackdevlabs.clover";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub uuid: String,
    pub name: String,
}

/// What the game needs at launch. Never sent to the frontend.
pub struct Session {
    pub profile: Profile,
    pub access_token: String,
    pub xuid: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceCode {
    #[serde(alias = "user_code")]
    pub user_code: String,
    #[serde(alias = "device_code")]
    pub device_code: String,
    #[serde(alias = "verification_uri")]
    pub verification_uri: String,
    pub interval: u64,
    #[serde(alias = "expires_in")]
    pub expires_in: u64,
}

#[derive(Deserialize)]
struct MsToken {
    access_token: String,
    refresh_token: String,
}

#[derive(Deserialize)]
struct MsError {
    error: String,
}

pub async fn begin(client: &Client) -> Result<DeviceCode> {
    if CLIENT_ID.starts_with("00000000") {
        return Err(msg("Sign-in isn't configured. Set CLIENT_ID in src-tauri/src/auth.rs."));
    }
    let res = client.post(DEVICE_CODE_URL).form(&[("client_id", CLIENT_ID), ("scope", SCOPE)]).send().await?;
    Ok(res.error_for_status()?.json().await?)
}

/// Polls until the person finishes signing in on microsoft.com, then completes the chain.
pub async fn finish(client: &Client, paths: &Paths, device_code: &str, interval: u64, expires_in: u64) -> Result<Profile> {
    let deadline = Instant::now() + Duration::from_secs(expires_in);
    let mut interval = interval.max(1);
    let token = loop {
        if Instant::now() > deadline {
            return Err(msg("The sign-in code expired. Try signing in again."));
        }
        tokio::time::sleep(Duration::from_secs(interval)).await;
        let res = client
            .post(TOKEN_URL)
            .form(&[
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ("client_id", CLIENT_ID),
                ("device_code", device_code),
            ])
            .send()
            .await?;
        if res.status().is_success() {
            break res.json::<MsToken>().await?;
        }
        match res.json::<MsError>().await?.error.as_str() {
            "authorization_pending" => continue,
            "slow_down" => interval += 5,
            "authorization_declined" => return Err(msg("Sign-in was cancelled.")),
            "expired_token" => return Err(msg("The sign-in code expired. Try signing in again.")),
            other => return Err(msg(format!("Microsoft sign-in failed: {other}"))),
        }
    };
    let session = minecraft_session(client, &token.access_token).await?;
    keyring::Entry::new(KEYRING_SERVICE, &session.profile.uuid)?.set_password(&token.refresh_token)?;
    std::fs::write(paths.account_file(), serde_json::to_vec(&session.profile)?)?;
    Ok(session.profile)
}

pub fn current(paths: &Paths) -> Option<Profile> {
    serde_json::from_slice(&std::fs::read(paths.account_file()).ok()?).ok()
}

/// Exchanges the stored refresh token for a fresh Minecraft session.
pub async fn session(client: &Client, paths: &Paths) -> Result<Session> {
    let profile = current(paths).ok_or_else(|| msg("Sign in with a Microsoft account to play."))?;
    let entry = keyring::Entry::new(KEYRING_SERVICE, &profile.uuid)?;
    let refresh = entry.get_password().map_err(|_| msg("Your session ended. Sign in again to play."))?;
    let res = client
        .post(TOKEN_URL)
        .form(&[
            ("grant_type", "refresh_token"),
            ("client_id", CLIENT_ID),
            ("refresh_token", refresh.as_str()),
            ("scope", SCOPE),
        ])
        .send()
        .await?;
    if !res.status().is_success() {
        return Err(msg("Your session ended. Sign in again to play."));
    }
    let token: MsToken = res.json().await?;
    entry.set_password(&token.refresh_token)?;
    minecraft_session(client, &token.access_token).await
}

pub fn sign_out(paths: &Paths) -> Result<()> {
    if let Some(profile) = current(paths) {
        let _ = keyring::Entry::new(KEYRING_SERVICE, &profile.uuid)?.delete_credential();
    }
    let _ = std::fs::remove_file(paths.account_file());
    Ok(())
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct XboxToken {
    token: String,
    display_claims: DisplayClaims,
}

#[derive(Deserialize)]
struct DisplayClaims {
    xui: Vec<Xui>,
}

#[derive(Deserialize)]
struct Xui {
    uhs: String,
    xid: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct XstsError {
    x_err: u64,
}

#[derive(Deserialize)]
struct McLogin {
    access_token: String,
}

#[derive(Deserialize)]
struct McProfile {
    id: String,
    name: String,
}

async fn minecraft_session(client: &Client, ms_access_token: &str) -> Result<Session> {
    let xbl: XboxToken = client
        .post("https://user.auth.xboxlive.com/user/authenticate")
        .json(&json!({
            "Properties": {
                "AuthMethod": "RPS",
                "SiteName": "user.auth.xboxlive.com",
                "RpsTicket": format!("d={ms_access_token}"),
            },
            "RelyingParty": "http://auth.xboxlive.com",
            "TokenType": "JWT",
        }))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let res = client
        .post("https://xsts.auth.xboxlive.com/xsts/authorize")
        .json(&json!({
            "Properties": { "SandboxId": "RETAIL", "UserTokens": [xbl.token] },
            "RelyingParty": "rp://api.minecraftservices.com/",
            "TokenType": "JWT",
        }))
        .send()
        .await?;
    if res.status() == reqwest::StatusCode::UNAUTHORIZED {
        let err: XstsError = res.json().await?;
        return Err(msg(match err.x_err {
            2148916233 => "This Microsoft account has no Xbox profile. Create one at xbox.com, then sign in again.",
            2148916235 => "Xbox Live isn't available in your region.",
            2148916238 => "This is a child account. An adult needs to add it to a Microsoft family first.",
            _ => "Xbox Live sign-in failed.",
        }));
    }
    let xsts: XboxToken = res.error_for_status()?.json().await?;
    let xui = xsts.display_claims.xui.first().ok_or_else(|| msg("Xbox Live returned no user."))?;

    let login: McLogin = client
        .post("https://api.minecraftservices.com/authentication/login_with_xbox")
        .json(&json!({ "identityToken": format!("XBL3.0 x={};{}", xui.uhs, xsts.token) }))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let res = client
        .get("https://api.minecraftservices.com/minecraft/profile")
        .bearer_auth(&login.access_token)
        .send()
        .await?;
    if res.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(msg("This Microsoft account doesn't own Minecraft: Java Edition."));
    }
    let profile: McProfile = res.error_for_status()?.json().await?;

    Ok(Session {
        profile: Profile { uuid: profile.id, name: profile.name },
        access_token: login.access_token,
        xuid: xui.xid.clone().unwrap_or_default(),
    })
}
