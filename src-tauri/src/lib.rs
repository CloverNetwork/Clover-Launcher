mod auth;
mod download;
mod error;
mod game;
mod install;
mod instances;
mod java;
mod launch;
mod loaders;
mod manifest;
mod menu;
mod paths;
mod profile;
mod settings;
mod skins;
#[cfg(test)]
mod smoke;

use reqwest::Client;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_opener::OpenerExt;

use crate::error::{Result, msg};
use crate::game::Running;
use crate::download::Verify;
use crate::install::AppReporter;
use crate::instances::Instance;
use crate::loaders::{LoaderSource, LoaderVersion};
use crate::manifest::VersionEntry;
use crate::paths::Paths;
use crate::settings::Settings;

struct AppState {
    paths: Paths,
    http: Client,
    sessions: auth::SessionCache,
}

impl AppState {
    /// The active account's token for Mojang's profile API. Offline accounts have none.
    async fn ms_session(&self) -> Result<auth::Session> {
        let session = auth::session(&self.http, &self.paths, &self.sessions).await?;
        if session.profile.offline {
            return Err(msg("Skins and capes need a Microsoft account."));
        }
        Ok(session)
    }

    /// Runs a wardrobe change, dropping the cached token if Mojang rejected it.
    async fn with_wardrobe<F, Fut>(&self, change: F) -> Result<skins::Wardrobe>
    where
        F: FnOnce(Client, String) -> Fut,
        Fut: std::future::Future<Output = Result<skins::Wardrobe>>,
    {
        let session = self.ms_session().await?;
        let result = change(self.http.clone(), session.access_token).await;
        if result.is_err() {
            self.sessions.forget(&session.profile.uuid);
        }
        result
    }
}

#[tauri::command]
/// Releases, plus snapshots and old alphas and betas when asked (legacy loaders such as Babric need them).
async fn list_versions(state: State<'_, AppState>, include_all: bool) -> Result<Vec<VersionEntry>> {
    let manifest = manifest::fetch_manifest(&state.http).await?;
    Ok(manifest.versions.into_iter().filter(|v| include_all || v.kind == "release").collect())
}

#[tauri::command]
fn list_instances(state: State<'_, AppState>) -> Result<Vec<Instance>> {
    instances::list(&state.paths)
}

#[tauri::command]
fn list_loaders(state: State<'_, AppState>) -> Vec<LoaderSource> {
    loaders::all(&settings::load(&state.paths))
}

#[tauri::command]
async fn list_loader_versions(state: State<'_, AppState>, loader: String, game_version: String) -> Result<Vec<LoaderVersion>> {
    let source = loaders::find(&settings::load(&state.paths), &loader)?;
    loaders::versions(&state.http, &source, &game_version).await
}

#[tauri::command]
fn add_loader_source(state: State<'_, AppState>, name: String, url: String, loader_path: String) -> Result<LoaderSource> {
    let source = loaders::custom_source(&name, &url, &loader_path)?;
    let mut settings = settings::load(&state.paths);
    if loaders::all(&settings).iter().any(|s| s.id == source.id) {
        return Err(msg(format!("There's already a loader named “{}”.", source.name)));
    }
    settings.loader_sources.push(source.clone());
    settings::save(&state.paths, &settings)?;
    Ok(source)
}

#[tauri::command]
fn remove_loader_source(state: State<'_, AppState>, id: String) -> Result<()> {
    let mut settings = settings::load(&state.paths);
    settings.loader_sources.retain(|s| s.id != id);
    settings::save(&state.paths, &settings)
}

#[tauri::command]
fn create_instance(
    state: State<'_, AppState>,
    name: String,
    version: String,
    loader: String,
    loader_version: Option<String>,
) -> Result<Instance> {
    if loader != loaders::VANILLA && loader_version.is_none() {
        return Err(msg("Choose a mod loader version."));
    }
    instances::create(&state.paths, &name, &version, &loader, loader_version)
}

/// Creates an instance from any launcher profile JSON, for loaders Clover doesn't list.
#[tauri::command]
fn create_instance_from_profile(state: State<'_, AppState>, name: String, path: String) -> Result<Instance> {
    let bytes = std::fs::read(&path).map_err(|e| msg(format!("Couldn't read that file ({e}).")))?;
    let profile_id = loaders::save_profile(&state.paths, &bytes)?;
    let json = profile::load(&state.paths, &profile_id)?;
    let game_version = json.inherits_from.unwrap_or_else(|| profile_id.clone());
    instances::create(&state.paths, &name, &game_version, loaders::CUSTOM, Some(profile_id))
}

#[tauri::command]
fn delete_instance(state: State<'_, AppState>, running: State<'_, Running>, id: String) -> Result<()> {
    if running.is_running(&id) {
        return Err(msg("Stop the game before deleting this instance."));
    }
    instances::delete(&state.paths, &id)
}

/// Re-checks every file against its hash and replaces anything damaged or missing.
/// Returns the instance and how many files were replaced.
#[tauri::command]
async fn verify_instance(app: AppHandle, state: State<'_, AppState>, running: State<'_, Running>, id: String) -> Result<(Instance, usize)> {
    if running.is_running(&id) {
        return Err(msg("Stop the game before verifying its files."));
    }
    let mut instance = instances::get(&state.paths, &id)?;
    let settings = settings::load(&state.paths);
    let mut reporter = AppReporter::new(app, id);
    let installed = install::install(&state.http, &state.paths, &settings, &instance, Verify::Full, &mut reporter).await?;
    instance.installed = true;
    instance.profile_id = Some(installed.resolved.id);
    instances::save(&state.paths, &instance)?;
    Ok((instance, installed.downloaded))
}

#[tauri::command]
async fn launch_instance(app: AppHandle, state: State<'_, AppState>, running: State<'_, Running>, id: String) -> Result<Instance> {
    if running.is_running(&id) {
        return Err(msg("This instance is already running."));
    }
    let mut instance = instances::get(&state.paths, &id)?;
    // Sign in first so a missing account fails before a long download.
    let session = auth::session(&state.http, &state.paths, &state.sessions).await?;
    // Always re-verify files; already-valid ones are skipped.
    let settings = settings::load(&state.paths);
    let mut reporter = AppReporter::new(app.clone(), id.clone());
    // Only missing files download; Verify Game Files re-hashes everything.
    let installed = install::install(&state.http, &state.paths, &settings, &instance, Verify::Existing, &mut reporter).await?;
    let command = launch::build_command(&state.paths, &instance, &installed.resolved, &session, &settings, &installed.java)?;
    game::spawn(app, id, command)?;
    instance.installed = true;
    instance.profile_id = Some(installed.resolved.id);
    instance.last_played = Some(instances::now());
    instances::save(&state.paths, &instance)?;
    Ok(instance)
}

#[tauri::command]
fn stop_instance(running: State<'_, Running>, id: String) {
    running.stop(&id);
}

#[tauri::command]
fn show_instance_folder(app: AppHandle, state: State<'_, AppState>, id: String) -> Result<()> {
    let dir = state.paths.instance(&id).join("minecraft");
    std::fs::create_dir_all(&dir)?;
    app.opener().open_path(dir.to_string_lossy(), None::<&str>).map_err(|e| msg(e.to_string()))
}

#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> Settings {
    settings::load(&state.paths)
}

#[tauri::command]
fn save_settings(state: State<'_, AppState>, settings: Settings) -> Result<()> {
    // Loader sources change only through their own commands, so a stale copy can't drop one.
    let current = settings::load(&state.paths);
    settings::save(&state.paths, &Settings { loader_sources: current.loader_sources, ..settings })
}

#[tauri::command]
fn get_accounts(state: State<'_, AppState>) -> auth::Accounts {
    auth::accounts(&state.paths)
}

/// Rebuilds the menu bar's account list after any account change.
fn accounts_changed(app: &AppHandle, paths: &Paths) -> Result<auth::Accounts> {
    let accounts = auth::accounts(paths);
    app.set_menu(menu::build(app, &accounts)?)?;
    Ok(accounts)
}

#[tauri::command]
fn switch_account(app: AppHandle, state: State<'_, AppState>, uuid: String) -> Result<auth::Accounts> {
    auth::switch(&state.paths, &uuid)?;
    accounts_changed(&app, &state.paths)
}

#[tauri::command]
fn remove_account(app: AppHandle, state: State<'_, AppState>, uuid: String) -> Result<auth::Accounts> {
    auth::remove(&state.paths, &uuid)?;
    state.sessions.forget(&uuid);
    accounts_changed(&app, &state.paths)
}

#[tauri::command]
async fn begin_sign_in(state: State<'_, AppState>) -> Result<auth::DeviceCode> {
    auth::begin(&state.http).await
}

#[tauri::command]
async fn finish_sign_in(app: AppHandle, state: State<'_, AppState>, code: auth::DeviceCode) -> Result<auth::Accounts> {
    auth::finish(&state.http, &state.paths, &code.device_code, code.interval, code.expires_in).await?;
    accounts_changed(&app, &state.paths)
}

#[tauri::command]
fn sign_in_offline(app: AppHandle, state: State<'_, AppState>, name: String) -> Result<auth::Accounts> {
    auth::sign_in_offline(&state.paths, &name)?;
    accounts_changed(&app, &state.paths)
}

/// The active account's skins and capes. `None` for offline accounts, which have no wardrobe.
#[tauri::command]
async fn get_wardrobe(state: State<'_, AppState>) -> Result<Option<skins::Wardrobe>> {
    match auth::current(&state.paths) {
        Some(profile) if !profile.offline => Ok(Some(state.with_wardrobe(|http, token| async move { skins::wardrobe(&http, &token).await }).await?)),
        _ => Ok(None),
    }
}

#[tauri::command]
fn list_skin_library(state: State<'_, AppState>) -> Vec<skins::LibrarySkin> {
    skins::library(&state.paths)
}

#[tauri::command]
fn add_skin_to_library(state: State<'_, AppState>, path: String, variant: skins::Variant) -> Result<skins::LibrarySkin> {
    let png = std::fs::read(&path).map_err(|e| msg(format!("Couldn't read that file ({e}).")))?;
    let name = std::path::Path::new(&path).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    skins::add_to_library(&state.paths, &png, &name, variant)
}

#[tauri::command]
fn update_library_skin(
    state: State<'_, AppState>,
    id: String,
    name: Option<String>,
    variant: Option<skins::Variant>,
) -> Result<skins::LibrarySkin> {
    skins::update_library_skin(&state.paths, &id, name.as_deref(), variant)
}

#[tauri::command]
fn remove_library_skin(state: State<'_, AppState>, id: String) -> Result<()> {
    skins::remove_from_library(&state.paths, &id)
}

#[tauri::command]
fn library_skin_texture(state: State<'_, AppState>, id: String) -> Result<String> {
    skins::library_texture(&state.paths, &id)
}

#[tauri::command]
async fn skin_texture(state: State<'_, AppState>, url: String) -> Result<String> {
    skins::texture(&state.http, &state.paths, &url).await
}

/// Keeps a copy of the skin the account is wearing, so it can be worn again later.
#[tauri::command]
async fn save_worn_skin(state: State<'_, AppState>, url: String, variant: skins::Variant, name: String) -> Result<skins::LibrarySkin> {
    let png = skins::texture_bytes(&state.http, &state.paths, &url).await?;
    skins::add_to_library(&state.paths, &png, &name, variant)
}

/// Uploads a library skin to the active account and wears it.
#[tauri::command]
async fn wear_library_skin(state: State<'_, AppState>, id: String) -> Result<skins::Wardrobe> {
    let skin = skins::library(&state.paths).into_iter().find(|s| s.id == id).ok_or_else(|| msg("That skin isn't in the library any more."))?;
    let png = skins::library_png(&state.paths, &id)?;
    state.with_wardrobe(|http, token| async move { skins::upload(&http, &token, png, skin.variant).await }).await
}

/// Switches the worn skin between classic and slim arms.
#[tauri::command]
async fn set_skin_variant(state: State<'_, AppState>, variant: skins::Variant) -> Result<skins::Wardrobe> {
    state
        .with_wardrobe(|http, token| async move {
            let current = skins::wardrobe(&http, &token).await?;
            let active = current.skins.iter().find(|s| s.state == "ACTIVE").ok_or_else(|| msg("This account isn't wearing a skin."))?;
            skins::wear_url(&http, &token, &active.url, variant).await
        })
        .await
}

#[tauri::command]
async fn wear_cape(state: State<'_, AppState>, cape_id: Option<String>) -> Result<skins::Wardrobe> {
    state.with_wardrobe(|http, token| async move { skins::wear_cape(&http, &token, cape_id.as_deref()).await }).await
}

#[tauri::command]
fn open_settings(app: AppHandle) {
    menu::open_settings(&app);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(Running::default())
        .setup(|app| {
            let http = Client::builder()
                .user_agent(concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION")))
                .build()?;
            let paths = Paths::new(app.handle())?;
            app.set_menu(menu::build(app.handle(), &auth::accounts(&paths))?)?;
            app.manage(AppState { paths, http, sessions: auth::SessionCache::default() });
            app.on_menu_event(|app, event| menu::handle(app, event.id().as_ref()));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_versions,
            list_instances,
            list_loaders,
            list_loader_versions,
            add_loader_source,
            remove_loader_source,
            create_instance,
            create_instance_from_profile,
            delete_instance,
            verify_instance,
            launch_instance,
            stop_instance,
            show_instance_folder,
            get_settings,
            save_settings,
            get_accounts,
            switch_account,
            remove_account,
            begin_sign_in,
            finish_sign_in,
            sign_in_offline,
            open_settings,
            get_wardrobe,
            list_skin_library,
            add_skin_to_library,
            update_library_skin,
            remove_library_skin,
            library_skin_texture,
            skin_texture,
            wear_library_skin,
            save_worn_skin,
            set_skin_variant,
            wear_cape,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
