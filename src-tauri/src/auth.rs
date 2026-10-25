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
    /// A development-only offline account; see `sign_in_offline`.
    #[serde(default)]
    pub offline: bool,
}

/// What the game needs at launch. Never sent to the frontend.
#[derive(Clone)]
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
    add(paths, session.profile.clone())?;
    Ok(session.profile)
}

/// Every signed-in account, and which one plays. Tokens live in the keychain, not here.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Accounts {
    pub active: Option<String>,
    pub accounts: Vec<Profile>,
}

impl Accounts {
    pub fn current(&self) -> Option<&Profile> {
        self.accounts.iter().find(|a| Some(&a.uuid) == self.active.as_ref())
    }
}

pub fn accounts(paths: &Paths) -> Accounts {
    let read = |path| serde_json::from_slice::<serde_json::Value>(&std::fs::read(path).ok()?).ok();
    let mut accounts = match read(paths.accounts_file()) {
        Some(json) => serde_json::from_value(json).unwrap_or_default(),
        // Migrate the single account from before switching existed.
        None => match read(paths.legacy_account_file()).and_then(|j| serde_json::from_value::<Profile>(j).ok()) {
            Some(profile) => Accounts { active: Some(profile.uuid.clone()), accounts: vec![profile] },
            None => Accounts::default(),
        },
    };
    // Release builds can't use offline accounts, so they don't list them.
    accounts.accounts.retain(|a| OFFLINE_ALLOWED || !a.offline);
    if accounts.current().is_none() {
        accounts.active = accounts.accounts.first().map(|a| a.uuid.clone());
    }
    accounts
}

fn save(paths: &Paths, accounts: &Accounts) -> Result<()> {
    std::fs::write(paths.accounts_file(), serde_json::to_vec_pretty(accounts)?)?;
    let _ = std::fs::remove_file(paths.legacy_account_file());
    Ok(())
}

/// Adds an account, or updates it after signing in again, and makes it the active one.
fn add(paths: &Paths, profile: Profile) -> Result<()> {
    let mut all = accounts(paths);
    all.active = Some(profile.uuid.clone());
    match all.accounts.iter_mut().find(|a| a.uuid == profile.uuid) {
        Some(existing) => *existing = profile,
        None => all.accounts.push(profile),
    }
    save(paths, &all)
}

pub fn switch(paths: &Paths, uuid: &str) -> Result<()> {
    let mut all = accounts(paths);
    if !all.accounts.iter().any(|a| a.uuid == uuid) {
        return Err(msg("That account isn't signed in any more."));
    }
    all.active = Some(uuid.into());
    save(paths, &all)
}

/// Signs an account out: forgets its refresh token and removes it from the list.
pub fn remove(paths: &Paths, uuid: &str) -> Result<()> {
    let mut all = accounts(paths);
    if let Some(profile) = all.accounts.iter().find(|a| a.uuid == uuid) {
        if !profile.offline {
            let _ = keyring::Entry::new(KEYRING_SERVICE, uuid)?.delete_credential();
        }
    }
    all.accounts.retain(|a| a.uuid != uuid);
    if all.active.as_deref() == Some(uuid) {
        all.active = all.accounts.first().map(|a| a.uuid.clone());
    }
    save(paths, &all)
}

pub fn current(paths: &Paths) -> Option<Profile> {
    accounts(paths).current().cloned()
}

/// Minecraft access tokens last 24 hours; reusing one skips four sign-in requests.
const SESSION_LIFETIME: Duration = Duration::from_secs(23 * 60 * 60);

/// Sessions minted this run, keyed by account UUID. Kept in memory only.
#[derive(Default)]
pub struct SessionCache(std::sync::Mutex<std::collections::HashMap<String, (Session, Instant)>>);

impl SessionCache {
    fn get(&self, uuid: &str) -> Option<Session> {
        let cache = self.0.lock().unwrap();
        cache.get(uuid).filter(|(_, minted)| minted.elapsed() < SESSION_LIFETIME).map(|(s, _)| s.clone())
    }

    fn put(&self, session: &Session) {
        self.0.lock().unwrap().insert(session.profile.uuid.clone(), (session.clone(), Instant::now()));
    }

    pub fn forget(&self, uuid: &str) {
        self.0.lock().unwrap().remove(uuid);
    }
}

/// The active account's session, reusing a cached token while it's still valid.
pub async fn session(client: &Client, paths: &Paths, cache: &SessionCache) -> Result<Session> {
    let profile = current(paths).ok_or_else(|| msg("Sign in with a Microsoft account to play."))?;
    if let Some(session) = cache.get(&profile.uuid) {
        return Ok(session);
    }
    let session = fresh_session(client, profile).await?;
    cache.put(&session);
    Ok(session)
}

/// Exchanges the stored refresh token for a fresh Minecraft session.
async fn fresh_session(client: &Client, profile: Profile) -> Result<Session> {
    if profile.offline {
        if !OFFLINE_ALLOWED {
            return Err(msg("Offline accounts only work in development builds. Sign in with Microsoft to play."));
        }
        // The game accepts any token; it just can't reach Mojang's services with it.
        return Ok(Session { profile, access_token: "0".into(), xuid: String::new() });
    }
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

/// Offline accounts exist so the launcher can be tested before Microsoft approves the
/// app's Minecraft API access. They're compiled out of release builds, so a shipped Clover
/// always requires an account that owns the game.
pub const OFFLINE_ALLOWED: bool = cfg!(debug_assertions);

/// Signs in with just a player name, like a vanilla server in offline mode.
pub fn sign_in_offline(paths: &Paths, name: &str) -> Result<Profile> {
    if !OFFLINE_ALLOWED {
        return Err(msg("Offline accounts only work in development builds."));
    }
    let name = name.trim();
    let valid = (3..=16).contains(&name.len()) && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if !valid {
        return Err(msg("Use 3 to 16 letters, numbers or underscores."));
    }
    let profile = Profile { uuid: offline_uuid(name), name: name.into(), offline: true };
    add(paths, profile.clone())?;
    Ok(profile)
}

/// The UUID an offline-mode server gives a player: Java's `UUID.nameUUIDFromBytes`
/// (MD5, version 3) of "OfflinePlayer:<name>", so worlds keep the same player data.
fn offline_uuid(name: &str) -> String {
    use md5::{Digest, Md5};
    let hash: [u8; 16] = Md5::digest(format!("OfflinePlayer:{name}")).into();
    uuid::Builder::from_md5_bytes(hash).into_uuid().simple().to_string()
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
        profile: Profile { uuid: profile.id, name: profile.name, offline: false },
        access_token: login.access_token,
        xuid: xui.xid.clone().unwrap_or_default(),
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn offline_uuid_matches_vanilla_servers() {
        // Known value from an offline-mode server for the player "Notch".
        assert_eq!(super::offline_uuid("Notch"), "b50ad385829d3141a2167e7d7539ba7f");
    }

    #[test]
    fn offline_accounts_only_in_debug_builds() {
        let dir = std::env::temp_dir().join(format!("clover-offline-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let paths = crate::paths::Paths::at(dir.clone());
        let result = super::sign_in_offline(&paths, "Tester");
        assert_eq!(result.is_ok(), cfg!(debug_assertions));
        assert!(super::sign_in_offline(&paths, "no spaces").is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    #[cfg(debug_assertions)]
    fn switches_between_accounts() {
        let dir = std::env::temp_dir().join(format!("clover-accounts-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let paths = crate::paths::Paths::at(dir.clone());
        // An account saved before switching existed is migrated.
        let legacy = super::Profile { uuid: "a".repeat(32), name: "Legacy".into(), offline: true };
        std::fs::write(paths.legacy_account_file(), serde_json::to_vec(&legacy).unwrap()).unwrap();
        assert_eq!(super::current(&paths).unwrap().name, "Legacy");

        let alex = super::sign_in_offline(&paths, "Alex").unwrap();
        assert_eq!(super::current(&paths).unwrap().name, "Alex");
        assert_eq!(super::accounts(&paths).accounts.len(), 2);
        assert!(!paths.legacy_account_file().exists());

        super::switch(&paths, &legacy.uuid).unwrap();
        assert_eq!(super::current(&paths).unwrap().name, "Legacy");
        super::remove(&paths, &legacy.uuid).unwrap();
        assert_eq!(super::current(&paths).unwrap().uuid, alex.uuid);
        super::remove(&paths, &alex.uuid).unwrap();
        assert!(super::current(&paths).is_none());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
