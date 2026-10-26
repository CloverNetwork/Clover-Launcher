//! Clover Launcher's engine: accounts, versions, mod loaders, Java, mods, modpacks, skins
//! and launching, with no user interface. The Tauri app in `src-tauri` is one front end;
//! any other front end can be built on the same calls.

pub mod auth;
pub mod download;
pub mod error;
pub mod export;
pub mod game;
pub mod hooks;
pub mod icon;
pub mod install;
pub mod instances;
pub mod java;
pub mod launch;
pub mod loaders;
pub mod manifest;
pub mod mclogs;
pub mod modpacks;
pub mod mods;
pub mod paths;
pub mod profile;
pub mod settings;
pub mod skins;
pub mod worlds;
#[cfg(test)]
mod smoke;

/// The name and version the game and services see.
pub const LAUNCHER_NAME: &str = "clover-launcher";
pub const LAUNCHER_VERSION: &str = env!("CARGO_PKG_VERSION");
