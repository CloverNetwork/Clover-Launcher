mod auth;
mod download;
mod error;
mod game;
mod install;
mod instances;
mod launch;
mod manifest;
mod menu;
mod paths;
mod settings;

use reqwest::Client;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_opener::OpenerExt;

use crate::error::{Result, msg};
use crate::game::Running;
use crate::instances::Instance;
use crate::manifest::VersionEntry;
use crate::paths::Paths;
use crate::settings::Settings;

struct AppState {
    paths: Paths,
    http: Client,
}

#[tauri::command]
async fn list_versions(state: State<'_, AppState>, include_snapshots: bool) -> Result<Vec<VersionEntry>> {
    let manifest = manifest::fetch_manifest(&state.http).await?;
    Ok(manifest
        .versions
        .into_iter()
        .filter(|v| v.kind == "release" || (include_snapshots && v.kind == "snapshot"))
        .collect())
}

#[tauri::command]
fn list_instances(state: State<'_, AppState>) -> Result<Vec<Instance>> {
    instances::list(&state.paths)
}

#[tauri::command]
fn create_instance(state: State<'_, AppState>, name: String, version: String) -> Result<Instance> {
    instances::create(&state.paths, &name, &version)
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
    install::install(&app, &state.http, &state.paths, &id, &instance.version).await?;
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
    let version = install::install(&app, &state.http, &state.paths, &id, &instance.version).await?;
    let settings = settings::load(&state.paths);
    let command = launch::build_command(&state.paths, &instance, &version, &session, &settings)?;
    game::spawn(app, id, command, version.java_version.as_ref().map(|j| j.major_version))?;
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
    settings::save(&state.paths, &settings)
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
            create_instance,
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
            sign_out,
            open_settings,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
