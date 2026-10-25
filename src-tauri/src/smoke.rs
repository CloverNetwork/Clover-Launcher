//! End-to-end checks against the real loader servers. They download hundreds of
//! megabytes, so they're ignored by default:
//!
//! ```text
//! cargo test smoke -- --ignored --nocapture --test-threads 1
//! ```
//!
//! `prepares_every_builtin_loader` installs each loader's profile and libraries and checks
//! the launch command. `boots_*` start the real game with a throwaway offline session
//! and wait for the loader's startup line.

use std::path::PathBuf;
use std::time::Duration;

use reqwest::Client;
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::auth::{Profile, Session};
use crate::download::{self, Task, Verify};
use crate::install::{Reporter, link_client_jar};
use crate::instances::Instance;
use crate::paths::Paths;
use crate::profile::{self, Resolved};
use crate::settings::Settings;
use crate::{launch, loaders};

struct Print;

impl Reporter for Print {
    fn stage(&mut self, text: &str) {
        println!("  {text}");
    }
    fn files(&mut self, _: usize, _: usize) {}
    fn log(&mut self, _: String) {}
}

fn root() -> PathBuf {
    std::env::temp_dir().join("clover-smoke")
}

fn instance(loader: &str, game: &str, loader_version: Option<String>) -> Instance {
    Instance {
        id: format!("smoke-{loader}"),
        name: loader.into(),
        version: game.into(),
        loader: loader.into(),
        loader_version,
        profile_id: None,
        created_at: 0,
        last_played: None,
        installed: false,
    }
}

fn session() -> Session {
    Session {
        profile: Profile { uuid: "00000000000000000000000000000000".into(), name: "Smoke".into(), offline: true },
        access_token: "0".into(),
        xuid: String::new(),
    }
}

/// Everything except assets, which the game can start without.
async fn prepare(client: &Client, paths: &Paths, instance: &Instance) -> Resolved {
    let settings = Settings::default();
    let id = loaders::ensure_profile(client, paths, &settings, instance, Verify::Existing, &mut Print).await.unwrap();
    let resolved = profile::resolve(client, paths, &id, Verify::Existing).await.unwrap();
    let mut tasks = vec![Task {
        url: resolved.client.url.clone(),
        path: profile::jar_path(paths, &resolved.jar_id),
        sha1: Some(resolved.client.sha1.clone()),
    }];
    for lib in &resolved.libraries {
        for file in lib.artifact().into_iter().chain(lib.native_artifact()) {
            if let Some(url) = file.url {
                tasks.push(Task { url, path: paths.libraries().join(file.path), sha1: file.sha1 });
            }
        }
    }
    download::run(client, tasks, Verify::Existing, |_, _| {}).await.unwrap();
    link_client_jar(paths, &resolved, false).unwrap();
    resolved
}

async fn latest(client: &Client, loader: &str, game: &str) -> String {
    let source = loaders::find(&Settings::default(), loader).unwrap();
    let versions = loaders::versions(client, &source, game).await.unwrap();
    let pick = versions.iter().find(|v| v.stable).or(versions.first());
    pick.unwrap_or_else(|| panic!("{loader} has no versions for {game}")).version.clone()
}

#[tokio::test]
#[ignore]
async fn prepares_every_builtin_loader() {
    let client = Client::new();
    let paths = Paths::at(root());
    let cases = [
        ("vanilla", "1.21.4"),
        ("fabric", "1.21.4"),
        ("quilt", "1.21.4"),
        ("neoforge", "1.21.4"),
        ("forge", "1.21.4"),
        ("legacy-fabric", "1.8.9"),
        ("ornithe", "1.8.9"),
        ("ornithe-quilt", "1.8.9"),
        ("babric", "b1.7.3"),
    ];
    for (loader, game) in cases {
        let version = if loader == "vanilla" { None } else { Some(latest(&client, loader, game).await) };
        println!("{loader} {} for {game}", version.as_deref().unwrap_or("-"));
        let instance = instance(loader, game, version);
        let resolved = prepare(&client, &paths, &instance).await;
        let command = launch::build_command(&paths, &instance, &resolved, &session(), &Settings::default()).unwrap();
        let args: Vec<String> = command.as_std().get_args().map(|a| a.to_string_lossy().into_owned()).collect();
        let classpath = &args[args.iter().position(|a| a == "-cp").expect("has -cp") + 1];
        for entry in classpath.split(':') {
            assert!(std::path::Path::new(entry).exists(), "{loader}: missing {entry}");
        }
        assert!(args.contains(&resolved.main_class), "{loader}: main class not passed");
        assert!(!args.iter().any(|a| a.contains("${")), "{loader}: unexpanded placeholder in {args:?}");
        println!("  ok: {} libraries, main class {}", resolved.libraries.len(), resolved.main_class);
    }
}

/// Waits for the loader's own startup line and then the sound engine, which only starts
/// once the game is past loading and showing its title screen.
async fn boots(loader: &str, game: &str, loader_marker: &str) {
    let client = Client::new();
    let paths = Paths::at(root());
    let instance = instance(loader, game, Some(latest(&client, loader, game).await));
    // The full install, assets and log config included, exactly as the app does it.
    let (resolved, _) = crate::install::install(&client, &paths, &Settings::default(), &instance, Verify::Existing, &mut Print).await.unwrap();
    let mut command = launch::build_command(&paths, &instance, &resolved, &session(), &Settings::default()).unwrap();
    command.stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).kill_on_drop(true);
    let mut child = command.spawn().unwrap();
    let mut stderr = BufReader::new(child.stderr.take().unwrap()).lines();
    tokio::spawn(async move {
        while let Ok(Some(line)) = stderr.next_line().await {
            println!("  stderr: {line}");
        }
    });
    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
    let mut recent = Vec::new();
    let mut markers = vec![loader_marker, "Sound engine started"];
    let found = tokio::time::timeout(Duration::from_secs(180), async {
        let mut log4j = crate::game::Log4jLines::default();
        while let Ok(Some(raw)) = lines.next_line().await {
            let Some(line) = log4j.push(&raw) else { continue };
            if markers.first().is_some_and(|m| line.contains(m)) {
                println!("  {line}");
                markers.remove(0);
                if markers.is_empty() {
                    return true;
                }
            }
            recent.push(line);
        }
        for line in recent.iter().rev().take(60).rev() {
            println!("  stdout: {line}");
        }
        false
    })
    .await
    .unwrap_or(false);
    let _ = child.kill().await;
    assert!(found, "{loader} didn't finish booting");
}

#[tokio::test]
#[ignore]
async fn boots_fabric() {
    boots("fabric", "1.21.4", "Loading Minecraft 1.21.4 with Fabric Loader").await;
}

#[tokio::test]
#[ignore]
async fn boots_neoforge() {
    boots("neoforge", "1.21.4", "Loading ImmediateWindowProvider").await;
}

#[tokio::test]
#[ignore]
async fn boots_quilt() {
    boots("quilt", "1.21.4", "with Quilt Loader").await;
}

#[tokio::test]
#[ignore]
async fn boots_forge() {
    boots("forge", "1.21.4", "ModLauncher").await;
}

#[tokio::test]
#[ignore]
async fn boots_legacy_fabric() {
    boots("legacy-fabric", "1.8.9", "with Fabric Loader").await;
}

/// Vanilla 1.8.9 ships Intel-only LWJGL natives, so on Apple silicon this needs an
/// x86_64 Java under Rosetta until Clover swaps in arm64 natives.
#[tokio::test]
#[ignore]
async fn boots_ornithe() {
    boots("ornithe", "1.8.9", "with Fabric Loader").await;
}

/// Beta 1.7.3 uses `java.applet`, which Java 26 removed: point Settings at Java 8 to run it.
#[tokio::test]
#[ignore]
async fn boots_babric() {
    boots("babric", "b1.7.3", "with Fabric Loader").await;
}

/// A second launch must download nothing and finish quickly, and Verify Game Files must
/// find and replace a damaged file.
#[tokio::test]
#[ignore]
async fn relaunch_downloads_nothing() {
    let client = Client::new();
    let paths = Paths::at(root());
    let settings = Settings::default();
    let mut instance = instance("fabric", "1.21.4", Some(latest(&client, "fabric", "1.21.4").await));

    let (resolved, first) = crate::install::install(&client, &paths, &settings, &instance, Verify::Existing, &mut Print).await.unwrap();
    println!("  first install: {first} files");
    instance.profile_id = Some(resolved.id.clone());

    let started = std::time::Instant::now();
    let (_, second) = crate::install::install(&client, &paths, &settings, &instance, Verify::Existing, &mut Print).await.unwrap();
    println!("  second launch: {second} files in {:?}", started.elapsed());
    assert_eq!(second, 0, "a second launch downloaded files");
    assert!(started.elapsed() < Duration::from_secs(3), "a second launch took {:?}", started.elapsed());

    let client_jar = profile::jar_path(&paths, &resolved.jar_id);
    std::fs::write(&client_jar, b"damaged").unwrap();
    let (_, repaired) = crate::install::install(&client, &paths, &settings, &instance, Verify::Full, &mut Print).await.unwrap();
    println!("  verify: replaced {repaired} files");
    assert_eq!(repaired, 1, "Verify Game Files should replace exactly the damaged jar");
}
