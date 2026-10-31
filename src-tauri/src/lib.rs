//! The Tauri front end: commands the windows call, built on clover-core.

mod chrome;
mod events;
mod files;
mod i18n;
mod menu;
mod presence;
mod shortcuts;

use clover_core::download::Verify;
use clover_core::error::{Result, msg};
use clover_core::game::{self, GameLogs, Running};
use clover_core::instances::{self, Instance};
use clover_core::loaders::{self, LoaderSource, LoaderVersion};
use clover_core::manifest::{self, VersionEntry};
use clover_core::paths::Paths;
use clover_core::settings::{self, Settings};
use clover_core::{auth, crash, export, servers, hooks, install, launch, mclogs, mod_info, modpacks, mods, skins, worlds};
use reqwest::Client;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_opener::OpenerExt;

use crate::events::{AppReporter, ImportReporter};

/// Tauri's own errors, worded for the person using the app.
fn ui<T>(result: tauri::Result<T>) -> Result<T> {
    result.map_err(|e| msg(e.to_string()))
}

/// An instance named with `--launch` when Clover started, waiting for the window to ask.
struct PendingLaunch(std::sync::Mutex<Option<String>>);

#[tauri::command]
fn take_pending_launch(pending: State<'_, PendingLaunch>) -> Option<String> {
    pending.0.lock().unwrap().take()
}

/// Adds an instance to the OS's launcher (Applications, the app menu, or the desktop).
#[tauri::command]
fn create_shortcut(app: AppHandle, state: State<'_, AppState>, id: String) -> Result<String> {
    let instance = instances::get(&state.paths, &id)?;
    let home = ui(app.path().home_dir())?;
    let desktop = app.path().desktop_dir().unwrap_or_else(|_| home.join("Desktop"));
    let place = shortcuts::create(&instance, &home, &desktop)?;
    menu::set_shortcut_label(&app, true);
    // Show it the way people read paths: ~/Applications/… rather than /Users/name/….
    let shown = place.strip_prefix(&home).map(|p| format!("~/{}", p.display())).unwrap_or_else(|_| place.display().to_string());
    Ok(shown)
}

/// Whether the instance has a shortcut. Also renames the menu bar's item to match, since it
/// acts on the selected instance.
#[tauri::command]
fn has_shortcut(app: AppHandle, state: State<'_, AppState>, id: String) -> Result<bool> {
    let instance = instances::get(&state.paths, &id)?;
    let home = ui(app.path().home_dir())?;
    let desktop = app.path().desktop_dir().unwrap_or_else(|_| home.join("Desktop"));
    let exists = shortcuts::find(&instance, &home, &desktop).is_some();
    menu::set_shortcut_label(&app, exists);
    Ok(exists)
}

/// Removes an instance's shortcut. Returns where it was, written like `create_shortcut`'s.
#[tauri::command]
fn remove_shortcut(app: AppHandle, state: State<'_, AppState>, id: String) -> Result<String> {
    let instance = instances::get(&state.paths, &id)?;
    let home = ui(app.path().home_dir())?;
    let desktop = app.path().desktop_dir().unwrap_or_else(|_| home.join("Desktop"));
    let place = shortcuts::remove(&instance, &home, &desktop)?;
    menu::set_shortcut_label(&app, false);
    Ok(place.strip_prefix(&home).map(|p| format!("~/{}", p.display())).unwrap_or_else(|_| place.display().to_string()))
}

/// The command that starts an instance, for Hyprland keybinds and scripts.
#[tauri::command]
fn launch_command(state: State<'_, AppState>, id: String) -> Result<String> {
    shortcuts::launch_command(&instances::get(&state.paths, &id)?)
}

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
    instances::create(&state.paths, &name, &version, &loader, loader_version)
}

/// Creates an instance from any launcher profile JSON, for loaders Clover doesn't list.
#[tauri::command]
fn create_instance_from_profile(state: State<'_, AppState>, name: String, path: String) -> Result<Instance> {
    loaders::create_from_profile(&state.paths, &name, std::path::Path::new(&path))
}

#[tauri::command]
fn update_instance(state: State<'_, AppState>, running: State<'_, Running>, id: String, changes: instances::Changes) -> Result<Instance> {
    if running.is_running(&id) {
        return Err(msg("Quit the game before editing this instance."));
    }
    instances::update(&state.paths, &id, changes)
}

#[tauri::command]
fn duplicate_instance(state: State<'_, AppState>, id: String) -> Result<Instance> {
    instances::duplicate(&state.paths, &id)
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
async fn launch_instance(
    app: AppHandle,
    state: State<'_, AppState>,
    running: State<'_, Running>,
    id: String,
    quick_play: Option<launch::QuickPlay>,
) -> Result<Instance> {
    if running.is_running(&id) {
        return Err(msg("This instance is already running."));
    }
    let mut instance = instances::get(&state.paths, &id)?;
    events::clear_log(&app, &id);
    // Sign in first so a missing account fails before a long download.
    let session = auth::session(&state.http, &state.paths, &state.sessions).await?;
    let settings = settings::load(&state.paths);
    let mut reporter = AppReporter::new(app.clone(), id.clone());
    // Only missing files download; Verify Game Files re-hashes everything.
    let installed = install::install(&state.http, &state.paths, &settings, &instance, Verify::Existing, &mut reporter).await?;
    let command =
        launch::build_command(&state.paths, &instance, &installed.resolved, &session, &settings, &installed.java, quick_play.as_ref())?;
    if !instance.options.pre_launch.trim().is_empty() {
        clover_core::install::Reporter::stage(&mut reporter, "Running the pre-launch command…");
        hooks::run(&state.paths, &instance, &instance.options.pre_launch, Some(&installed.java))
            .await
            .map_err(|e| msg(format!("The pre-launch command failed: {e}")))?;
    }
    let stop = running.start(&id);
    let (log_app, log_id) = (app.clone(), id.clone());
    let (exit_app, exit_id) = (app.clone(), id.clone());
    let (hook_paths, hook_instance, hook_java) = (state.paths.clone(), instance.clone(), installed.java.clone());
    let started_at = std::time::Instant::now();
    let started = game::spawn(
        command,
        stop,
        move |line| events::emit_log(&log_app, &log_id, line),
        move |code| {
            exit_app.state::<Running>().finished(&exit_id);
            let _ = instances::add_play_time(&hook_paths, &exit_id, started_at.elapsed().as_secs());
            presence::game_stopped(&exit_app);
            events::emit_exit(&exit_app, &exit_id, code);
            let command = hook_instance.options.post_exit.clone();
            if !command.trim().is_empty() {
                tauri::async_runtime::spawn(async move {
                    if let Err(e) = hooks::run(&hook_paths, &hook_instance, &command, Some(&hook_java)).await {
                        events::emit_log(&exit_app, &exit_id, format!("[Clover] The post-exit command failed: {e}"));
                    }
                });
            }
        },
    );
    if let Err(e) = started {
        running.finished(&id);
        return Err(e);
    }
    presence::game_started(&app, &settings, &instance, &session.profile.name);
    instance.installed = true;
    instance.profile_id = Some(installed.resolved.id);
    instance.last_played = Some(instances::now());
    instances::save(&state.paths, &instance)?;
    Ok(instance)
}

#[tauri::command]
fn game_log(logs: State<'_, GameLogs>, id: String) -> Vec<String> {
    logs.lines(&id)
}

/// Which folder a content command acts on: mods by default, or a kind of pack. Data packs
/// also name their world.
#[derive(Default, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Place {
    kind: mods::Kind,
    world: Option<String>,
}

impl Place {
    fn shelf(&self, paths: &Paths, instance: &Instance) -> Result<mods::Shelf> {
        mods::Shelf::new(paths, instance, self.kind, self.world.as_deref())
    }
}

fn sites<'a>(state: &'a AppState, settings: &'a Settings) -> mods::Sites<'a> {
    mods::Sites { client: &state.http, curseforge_key: settings.curseforge_key() }
}

#[tauri::command]
async fn search_mods(
    state: State<'_, AppState>,
    id: String,
    place: Option<Place>,
    source: mods::Source,
    query: String,
    offset: u32,
) -> Result<mods::SearchPage> {
    let instance = instances::get(&state.paths, &id)?;
    let settings = settings::load(&state.paths);
    let target = mods::Target::new(&instance, place.unwrap_or_default().kind)?;
    mods::search(&sites(&state, &settings), &target, source, &query, offset).await
}

#[tauri::command]
fn list_mods(state: State<'_, AppState>, id: String, place: Option<Place>) -> Result<Vec<mods::InstalledMod>> {
    let instance = instances::get(&state.paths, &id)?;
    mods::installed(&place.unwrap_or_default().shelf(&state.paths, &instance)?)
}

/// What one installed file is, from its project page or its own metadata.
#[tauri::command]
async fn mod_details(state: State<'_, AppState>, id: String, place: Option<Place>, file: String) -> Result<mod_info::ModDetails> {
    let instance = instances::get(&state.paths, &id)?;
    let settings = settings::load(&state.paths);
    let shelf = place.unwrap_or_default().shelf(&state.paths, &instance)?;
    mod_info::details(&sites(&state, &settings), &shelf, &file).await
}

/// Installs a mod or pack, and a mod's required dependencies. Returns the titles of what was added.
#[tauri::command]
async fn install_mod(state: State<'_, AppState>, id: String, place: Option<Place>, source: mods::Source, project: String) -> Result<Vec<String>> {
    let instance = instances::get(&state.paths, &id)?;
    let place = place.unwrap_or_default();
    let settings = settings::load(&state.paths);
    let (shelf, target) = (place.shelf(&state.paths, &instance)?, mods::Target::new(&instance, place.kind)?);
    mods::install(&sites(&state, &settings), &shelf, &target, source, &project).await
}

#[tauri::command]
async fn check_curseforge_key(state: State<'_, AppState>, key: String) -> Result<()> {
    mods::check_curseforge_key(&state.http, &key).await
}

#[tauri::command]
async fn search_modpacks(state: State<'_, AppState>, source: mods::Source, query: String, offset: u32) -> Result<mods::SearchPage> {
    let settings = settings::load(&state.paths);
    let sites = mods::Sites { client: &state.http, curseforge_key: settings.curseforge_key() };
    modpacks::search(&sites, source, &query, offset).await
}

/// Downloads a pack from Modrinth or CurseForge and makes an instance from it.
#[tauri::command]
async fn install_modpack(app: AppHandle, state: State<'_, AppState>, source: mods::Source, project: String) -> Result<modpacks::Imported> {
    let settings = settings::load(&state.paths);
    let sites = mods::Sites { client: &state.http, curseforge_key: settings.curseforge_key() };
    let mut reporter = ImportReporter::new(app);
    clover_core::install::Reporter::stage(&mut reporter, "Downloading the modpack…");
    let file = modpacks::download(&sites, &state.paths, source, &project).await?;
    let result = modpacks::import(&sites, &state.paths, &file, &mut reporter).await;
    let _ = std::fs::remove_file(file);
    result
}

/// Makes an instance from a .mrpack or CurseForge zip on disk.
#[tauri::command]
async fn import_modpack_file(app: AppHandle, state: State<'_, AppState>, path: String) -> Result<modpacks::Imported> {
    let settings = settings::load(&state.paths);
    let sites = mods::Sites { client: &state.http, curseforge_key: settings.curseforge_key() };
    modpacks::import(&sites, &state.paths, std::path::Path::new(&path), &mut ImportReporter::new(app)).await
}

#[tauri::command]
async fn check_mod_updates(state: State<'_, AppState>, id: String, place: Option<Place>) -> Result<mods::UpdateCheck> {
    let instance = instances::get(&state.paths, &id)?;
    let place = place.unwrap_or_default();
    let settings = settings::load(&state.paths);
    let (shelf, target) = (place.shelf(&state.paths, &instance)?, mods::Target::new(&instance, place.kind)?);
    mods::check_updates(&sites(&state, &settings), &shelf, &target).await
}

/// Applies updates one after another. Returns the titles of everything updated or added.
#[tauri::command]
async fn update_mods(
    state: State<'_, AppState>,
    running: State<'_, Running>,
    id: String,
    place: Option<Place>,
    updates: Vec<mods::ModUpdate>,
) -> Result<Vec<String>> {
    let place = place.unwrap_or_default();
    if running.is_running(&id) && place.kind == mods::Kind::Mod {
        return Err(msg("Quit the game before updating its mods."));
    }
    let instance = instances::get(&state.paths, &id)?;
    let settings = settings::load(&state.paths);
    let (shelf, target) = (place.shelf(&state.paths, &instance)?, mods::Target::new(&instance, place.kind)?);
    mods::update_all(&sites(&state, &settings), &shelf, &target, &updates).await
}

#[tauri::command]
fn set_mod_enabled(state: State<'_, AppState>, id: String, place: Option<Place>, file: String, enabled: bool) -> Result<()> {
    let instance = instances::get(&state.paths, &id)?;
    mods::set_enabled(&place.unwrap_or_default().shelf(&state.paths, &instance)?, &file, enabled)
}

#[tauri::command]
fn remove_mod(state: State<'_, AppState>, id: String, place: Option<Place>, file: String) -> Result<()> {
    let instance = instances::get(&state.paths, &id)?;
    mods::remove(&place.unwrap_or_default().shelf(&state.paths, &instance)?, &file)
}

#[tauri::command]
async fn show_mods_folder(app: AppHandle, state: State<'_, AppState>, id: String, place: Option<Place>) -> Result<()> {
    let instance = instances::get(&state.paths, &id)?;
    let dir = mods::folder(&place.unwrap_or_default().shelf(&state.paths, &instance)?)?;
    files::open(&app, &dir)
}

#[tauri::command]
fn list_worlds(state: State<'_, AppState>, id: String) -> Result<Vec<worlds::World>> {
    worlds::list(&state.paths, &instances::get(&state.paths, &id)?)
}

/// Zips a world into the instance's backups folder and shows the zip in the file manager.
#[tauri::command]
async fn backup_world(app: AppHandle, state: State<'_, AppState>, id: String, folder: String, label: String) -> Result<()> {
    let instance = instances::get(&state.paths, &id)?;
    let paths = state.paths.clone();
    let zip = tauri::async_runtime::spawn_blocking(move || worlds::backup(&paths, &instance, &folder, &label))
        .await
        .map_err(|e| msg(e.to_string()))??;
    files::reveal(&app, &zip)
}

#[tauri::command]
fn trash_world(state: State<'_, AppState>, running: State<'_, Running>, id: String, folder: String) -> Result<()> {
    if running.is_running(&id) {
        return Err(msg("Quit the game before moving a world to the Trash."));
    }
    worlds::trash(&state.paths, &instances::get(&state.paths, &id)?, &folder)
}

#[tauri::command]
async fn show_world(app: AppHandle, state: State<'_, AppState>, id: String, folder: String) -> Result<()> {
    let dir = worlds::world_dir(&state.paths, &instances::get(&state.paths, &id)?, &folder)?;
    files::reveal(&app, &dir)
}

#[tauri::command]
fn list_screenshots(state: State<'_, AppState>, id: String) -> Result<Vec<worlds::Screenshot>> {
    worlds::screenshots(&state.paths, &instances::get(&state.paths, &id)?)
}

/// Opens a screenshot in the system's image viewer, or reveals it with `reveal`.
#[tauri::command]
async fn open_screenshot(app: AppHandle, state: State<'_, AppState>, id: String, file: String, reveal: bool) -> Result<()> {
    let path = worlds::screenshot_path(&state.paths, &instances::get(&state.paths, &id)?, &file)?;
    if reveal { files::reveal(&app, &path) } else { files::open(&app, &path) }
}

#[tauri::command]
fn trash_screenshot(state: State<'_, AppState>, id: String, file: String) -> Result<()> {
    worlds::trash_screenshot(&state.paths, &instances::get(&state.paths, &id)?, &file)
}

/// Writes the instance as a .mrpack where the person chose, then shows it.
#[tauri::command]
async fn export_modpack(app: AppHandle, state: State<'_, AppState>, id: String, path: String) -> Result<export::Exported> {
    let instance = instances::get(&state.paths, &id)?;
    let settings = settings::load(&state.paths);
    let out = std::path::PathBuf::from(path);
    let exported = export::export(&sites(&state, &settings), &state.paths, &instance, &out).await?;
    let _ = files::reveal(&app, &out);
    Ok(exported)
}

/// The window's language, for the menu bar and window titles. Rebuilds the menu if it changed.
#[tauri::command]
fn set_language(app: AppHandle, state: State<'_, AppState>, language: String) -> Result<()> {
    if i18n::set(&language) && cfg!(target_os = "macos") {
        ui(menu::build(&app, &auth::accounts(&state.paths)).and_then(|menu| app.set_menu(menu)))?;
    }
    Ok(())
}

/// Uses a jar the person picked instead of Mojang's client jar.
#[tauri::command]
fn set_custom_jar(state: State<'_, AppState>, running: State<'_, Running>, id: String, path: String) -> Result<Instance> {
    if running.is_running(&id) {
        return Err(msg("Quit the game first."));
    }
    clover_core::custom_jar::set(&state.paths, &id, std::path::Path::new(&path))
}

#[tauri::command]
fn clear_custom_jar(state: State<'_, AppState>, running: State<'_, Running>, id: String) -> Result<Instance> {
    if running.is_running(&id) {
        return Err(msg("Quit the game first."));
    }
    clover_core::custom_jar::clear(&state.paths, &id)
}

#[tauri::command]
fn get_instance(state: State<'_, AppState>, id: String) -> Result<Instance> {
    instances::get(&state.paths, &id)
}

#[tauri::command]
fn discord_built_in() -> bool {
    true
}

/// Installs the performance mods that exist for the instance.
#[tauri::command]
async fn install_performance_mods(state: State<'_, AppState>, id: String) -> Result<mods::PackResult> {
    let instance = instances::get(&state.paths, &id)?;
    let settings = settings::load(&state.paths);
    let (shelf, target) = (mods::Shelf::mods(&state.paths, &instance), mods::Target::for_instance(&instance)?);
    mods::install_performance(&sites(&state, &settings), &shelf, &target).await
}

#[tauri::command]
fn list_servers(state: State<'_, AppState>, id: String) -> Result<Vec<servers::Server>> {
    servers::list(&state.paths, &instances::get(&state.paths, &id)?)
}

/// Adds a server, or changes the one at `index`. Returns the new list.
#[tauri::command]
fn save_server(state: State<'_, AppState>, id: String, index: Option<usize>, name: String, address: String) -> Result<Vec<servers::Server>> {
    let instance = instances::get(&state.paths, &id)?;
    match index {
        Some(index) => servers::edit(&state.paths, &instance, index, &name, &address),
        None => servers::add(&state.paths, &instance, &name, &address),
    }
}

#[tauri::command]
fn remove_server(state: State<'_, AppState>, id: String, index: usize) -> Result<Vec<servers::Server>> {
    servers::remove(&state.paths, &instances::get(&state.paths, &id)?, index)
}

/// Pings a server and keeps its icon in the list, as the game would.
#[tauri::command]
async fn ping_server(state: State<'_, AppState>, id: String, address: String) -> Result<servers::Status> {
    let status = servers::ping(&address).await?;
    if let (Some(icon), Ok(instance)) = (&status.icon, instances::get(&state.paths, &id)) {
        let _ = servers::remember_icon(&state.paths, &instance, &address, icon);
    }
    Ok(status)
}

/// Explains why the game quit with an error, from its log and newest crash report.
#[tauri::command]
fn analyze_crash(state: State<'_, AppState>, logs: State<'_, GameLogs>, id: String) -> Result<Vec<crash::Finding>> {
    let instance = instances::get(&state.paths, &id)?;
    let settings = settings::load(&state.paths);
    let report = crash::newest_crash_report(&state.paths, &instance, instance.last_played.unwrap_or(0));
    let context = crash::Context { instance: &instance, settings: &settings, mods: crash::mod_infos(&state.paths, &instance) };
    Ok(crash::analyze(&logs.lines(&id), report.as_deref(), &context))
}

/// Applies a crash fix. Returns the instance, which changes for memory and Java fixes.
#[tauri::command]
async fn apply_crash_fix(state: State<'_, AppState>, running: State<'_, Running>, id: String, fix: crash::Fix) -> Result<Instance> {
    if running.is_running(&id) {
        return Err(msg("Quit the game first."));
    }
    let mut instance = instances::get(&state.paths, &id)?;
    match &fix {
        crash::Fix::InstallMod { project } => {
            let settings = settings::load(&state.paths);
            let (shelf, target) = (mods::Shelf::mods(&state.paths, &instance), mods::Target::for_instance(&instance)?);
            mods::install(&sites(&state, &settings), &shelf, &target, mods::Source::Modrinth, project)
                .await
                .map_err(|e| msg(format!("Couldn't install it from Modrinth: {e}")))?;
        }
        crash::Fix::TurnOffMod { file } => crash::turn_off(&state.paths, &instance, file)?,
        crash::Fix::SetMemory { .. } | crash::Fix::AutomaticJava => {
            crash::apply_to_instance(&mut instance, &fix);
            instances::save(&state.paths, &instance)?;
        }
        crash::Fix::EditInstance => {}
    }
    Ok(instance)
}

/// Shares an instance's log on mclo.gs, with access tokens and the home folder removed.
#[tauri::command]
async fn upload_log(state: State<'_, AppState>, logs: State<'_, GameLogs>, id: String) -> Result<mclogs::Uploaded> {
    let instance = instances::get(&state.paths, &id)?;
    let loader = loaders::label(&settings::load(&state.paths), &instance);
    let details = mclogs::Details { minecraft: &instance.version, loader: &loader };
    mclogs::upload(&state.http, &logs.lines(&id), &state.sessions.tokens(), details).await
}

#[tauri::command]
fn open_log_window(app: AppHandle, state: State<'_, AppState>, id: String) -> Result<()> {
    let instance = instances::get(&state.paths, &id)?;
    ui(events::open_log_window(&app, &instance.id, &instance.name))
}

#[tauri::command]
fn pop_in_log_window(app: AppHandle, id: String) -> Result<()> {
    ui(events::pop_in_log_window(&app, &id))
}

#[tauri::command]
fn stop_instance(running: State<'_, Running>, id: String) {
    running.stop(&id);
}

#[tauri::command]
async fn show_instance_folder(app: AppHandle, state: State<'_, AppState>, id: String) -> Result<()> {
    let dir = state.paths.instance(&id).join("minecraft");
    std::fs::create_dir_all(&dir)?;
    files::open(&app, &dir)
}

#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> Settings {
    settings::load(&state.paths)
}

#[tauri::command]
fn save_settings(app: AppHandle, state: State<'_, AppState>, settings: Settings) -> Result<()> {
    // Loader sources change only through their own commands, so a stale copy can't drop one.
    let current = settings::load(&state.paths);
    let settings = Settings { loader_sources: current.loader_sources, ..settings };
    settings::save(&state.paths, &settings)?;
    if settings.show_window_buttons != current.show_window_buttons {
        let _ = app.emit("window-chrome", chrome::current(&settings));
    }
    Ok(())
}

#[tauri::command]
fn window_chrome(state: State<'_, AppState>) -> chrome::WindowChrome {
    chrome::current(&settings::load(&state.paths))
}

/// The Quit button on title-bar-less windows: closes every window, like ⌘Q.
#[tauri::command]
fn quit(app: AppHandle) {
    app.exit(0);
}

#[tauri::command]
fn get_accounts(state: State<'_, AppState>) -> auth::Accounts {
    auth::accounts(&state.paths)
}

/// Rebuilds the menu bar's account list after any account change.
fn accounts_changed(app: &AppHandle, paths: &Paths) -> Result<auth::Accounts> {
    let accounts = auth::accounts(paths);
    if cfg!(target_os = "macos") {
        ui(menu::build(app, &accounts).and_then(|menu| app.set_menu(menu)))?;
    }
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
    let result = auth::finish(&state.http, &state.paths, &code.device_code, code.interval, code.expires_in).await;
    // Signed in, timed out or turned down: either way the Microsoft page has done its job.
    close_sign_in_window(app.clone());
    result?;
    accounts_changed(&app, &state.paths)
}

/// Microsoft's code page in a window of Clover's own, with the code filled in, so it can be
/// closed again once sign-in finishes. A browser tab would stay open.
#[tauri::command]
async fn open_sign_in_window(app: AppHandle, code: auth::DeviceCode) -> Result<()> {
    if let Some(window) = app.get_webview_window(SIGN_IN_WINDOW) {
        let _ = window.unminimize();
        return window.set_focus().map_err(|e| msg(e.to_string()));
    }
    let url = format!("{}?otc={}", code.verification_uri, code.user_code);
    let url = url.parse().map_err(|_| msg("Microsoft sent an unusable sign-in address."))?;
    let mut builder = tauri::WebviewWindowBuilder::new(&app, SIGN_IN_WINDOW, tauri::WebviewUrl::External(url))
        .title(i18n::tr("Sign In with Microsoft"))
        .inner_size(480.0, 640.0)
        .minimizable(false)
        .maximizable(false);
    if let Some(main) = app.get_webview_window("main") {
        builder = builder.parent(&main).map_err(|e| msg(e.to_string()))?;
    }
    builder.build().map_err(|e| msg(e.to_string()))?;
    Ok(())
}

#[tauri::command]
fn close_sign_in_window(app: AppHandle) {
    if let Some(window) = app.get_webview_window(SIGN_IN_WINDOW) {
        let _ = window.close();
    }
}

const SIGN_IN_WINDOW: &str = "sign-in";

/// Whether this is a development build, which allows offline accounts and skips update checks.
#[tauri::command]
fn development_build() -> bool {
    auth::OFFLINE_ALLOWED
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
    i18n::init();
    tauri::Builder::default()
        // First, so a second start hands its `--launch` to the open app and quits.
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            if let Some(id) = shortcuts::launch_arg(argv) {
                let _ = app.emit("launch-request", id);
            }
            if let Some(main) = app.get_webview_window("main") {
                let _ = main.unminimize();
                let _ = main.show();
                let _ = main.set_focus();
            }
        }))
        // Windows reopen where they were, at the size they were. The page decides when a
        // window becomes visible, so that isn't restored.
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(tauri_plugin_window_state::StateFlags::all() & !tauri_plugin_window_state::StateFlags::VISIBLE)
                .build(),
        )
        .manage(PendingLaunch(std::sync::Mutex::new(shortcuts::launch_arg(std::env::args()))))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(Running::default())
        .manage(presence::Discord::default())
        .manage(GameLogs::default())
        .setup(|app| {
            let http = Client::builder()
                // Modrinth asks clients to name themselves this way.
                .user_agent(concat!("CloverNetwork/Clover-Launcher/", env!("CARGO_PKG_VERSION")))
                .build()?;
            let paths = Paths::new(app.path().app_data_dir()?)?;
            // Only macOS has a system menu bar. On Windows and Linux a menu would sit inside the
            // window, which modern apps there don't do; the window handles the shortcuts instead.
            if cfg!(target_os = "macos") {
                app.set_menu(menu::build(app.handle(), &auth::accounts(&paths))?)?;
            }
            app.manage(AppState { paths, http, sessions: auth::SessionCache::default() });
            app.on_menu_event(|app, event| menu::handle(app, event.id().as_ref()));
            if !chrome::decorated() {
                if let Some(main) = app.get_webview_window("main") {
                    main.set_decorations(false)?;
                }
            }
            // The page shows the window once drawn; if it can't, show it anyway.
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_secs(3));
                if let Some(main) = handle.get_webview_window("main") {
                    let _ = main.show();
                }
            });
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
            update_instance,
            duplicate_instance,
            delete_instance,
            verify_instance,
            launch_instance,
            stop_instance,
            game_log,
            search_mods,
            list_mods,
            check_curseforge_key,
            install_mod,
            check_mod_updates,
            search_modpacks,
            install_modpack,
            import_modpack_file,
            update_mods,
            set_mod_enabled,
            remove_mod,
            show_mods_folder,
            mod_details,
            list_worlds,
            backup_world,
            trash_world,
            show_world,
            list_screenshots,
            open_screenshot,
            trash_screenshot,
            export_modpack,
            analyze_crash,
            list_servers,
            install_performance_mods,
            discord_built_in,
            get_instance,
            set_custom_jar,
            clear_custom_jar,
            set_language,
            has_shortcut,
            development_build,
            remove_shortcut,
            save_server,
            remove_server,
            ping_server,
            apply_crash_fix,
            upload_log,
            open_log_window,
            pop_in_log_window,
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
            open_sign_in_window,
            close_sign_in_window,
            window_chrome,
            quit,
            take_pending_launch,
            create_shortcut,
            launch_command,
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
