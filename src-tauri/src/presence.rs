//! Keeps the Discord status in step with the game: shown when one starts, cleared when the
//! last one stops. Failures stay quiet, since Discord not running is the usual case.

use clover_core::discord::{Activity, Presence};
use clover_core::game::Running;
use clover_core::instances::{self, Instance};
use clover_core::loaders;
use clover_core::settings::Settings;
use tauri::{AppHandle, Manager};
use tokio::sync::Mutex;

#[derive(Default)]
pub struct Discord(Mutex<Option<Presence>>);

pub fn game_started(app: &AppHandle, settings: &Settings, instance: &Instance, player: &str) {
    let Some(app_id) = settings.discord_app_id().filter(|_| settings.discord_status).map(String::from) else { return };
    let details = instance.name.clone();
    let state = format!("Minecraft {} · {}", instance.version, loaders::label(settings, instance));
    let started = instances::now();
    let player = player.to_string();
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let discord = app.state::<Discord>();
        let mut presence = discord.0.lock().await;
        if presence.is_none() {
            *presence = Presence::connect(&app_id).await.ok();
        }
        let activity = Activity { details: &details, state: &state, started, player: Some(&player) };
        if let Some(connection) = presence.as_mut() {
            // A broken connection is dropped; the next game reconnects.
            if connection.set(Some(&activity)).await.is_err() {
                *presence = None;
            }
        }
    });
}

pub fn game_stopped(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if app.state::<Running>().any() {
            return;
        }
        let discord = app.state::<Discord>();
        if let Some(mut connection) = discord.0.lock().await.take() {
            let _ = connection.set(None).await;
        }
    });
}
