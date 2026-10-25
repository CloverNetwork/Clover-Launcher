mod auth;
mod download;
mod error;
mod game;
mod install;
mod instances;
mod launch;
mod loaders;
mod manifest;
mod menu;
mod paths;
mod profile;
mod settings;
#[cfg(test)]
mod smoke;

use reqwest::Client;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_opener::OpenerExt;

use crate::error::{Result, msg};
use crate::game::Running;
use crate::install::AppReporter;
use crate::instances::Instance;
use crate::loaders::{LoaderSource, LoaderVersion};
use crate::manifest::VersionEntry;
use crate::paths::Paths;
use crate::settings::Settings;

struct AppState {
    paths: Paths,
    http: Client,
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

#[tauri::command]
async fn install_instance(app: AppHandle, state: State<'_, AppState>, id: String) -> Result<Instance> {
    let mut instance = instances::get(&state.paths, &id)?;
    let settings = settings::load(&state.paths);
    let mut reporter = AppReporter::new(app, id);
    install::install(&state.http, &state.paths, &settings, &instance, &mut reporter).await?;
    instance.installed = true;
    instances::save(&state.paths, &instance)?;
    Ok(instance)
}

#[tauri::command]
async fn launch_instance(app: AppHandle, state: State<'_, AppState>, running: State<'_, Running>, id: String) -> Result<Instance> {
    if running.is_running(&id) {
        return Err(msg("This instance is already running."));
    }
    let mut instance = instances::get(&state.paths, &id)?;
    // Sign in first so a missing account fails before a long download.
    let session = auth::session(&state.http, &state.paths).await?;
    // Always re-verify files; already-valid ones are skipped.
    let settings = settings::load(&state.paths);
    let mut reporter = AppReporter::new(app.clone(), id.clone());
    let version = install::install(&state.http, &state.paths, &settings, &instance, &mut reporter).await?;
    let command = launch::build_command(&state.paths, &instance, &version, &session, &settings)?;
    game::spawn(app, id, command, version.java_major)?;
    instance.installed = true;
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
fn get_account(state: State<'_, AppState>) -> Option<auth::Profile> {
    auth::current(&state.paths)
}

#[tauri::command]
async fn begin_sign_in(state: State<'_, AppState>) -> Result<auth::DeviceCode> {
    auth::begin(&state.http).await
}

#[tauri::command]
async fn finish_sign_in(state: State<'_, AppState>, code: auth::DeviceCode) -> Result<auth::Profile> {
    auth::finish(&state.http, &state.paths, &code.device_code, code.interval, code.expires_in).await
}

#[tauri::command]
fn sign_in_offline(state: State<'_, AppState>, name: String) -> Result<auth::Profile> {
    auth::sign_in_offline(&state.paths, &name)
}

#[tauri::command]
fn sign_out(state: State<'_, AppState>) -> Result<()> {
    auth::sign_out(&state.paths)
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
            app.manage(AppState { paths: Paths::new(app.handle())?, http });
            app.set_menu(menu::build(app.handle())?)?;
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
            install_instance,
            launch_instance,
            stop_instance,
            show_instance_folder,
            get_settings,
            save_settings,
            get_account,
            begin_sign_in,
            finish_sign_in,
            sign_in_offline,
            sign_out,
            open_settings,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
