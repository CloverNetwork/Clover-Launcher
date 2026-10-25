use std::collections::HashMap;
use std::fs::File;
use std::path::Path;

use tokio::process::Command;

use crate::auth::Session;
use crate::error::Result;
use crate::install::client_jar_path;
use crate::instances::Instance;
use crate::manifest::{ArgValue, Argument, rules_allow};
use crate::paths::Paths;
use crate::profile::Resolved;
use crate::settings::Settings;

const CLASSPATH_SEPARATOR: &str = if cfg!(windows) { ";" } else { ":" };

/// Builds the `java` command for an installed version, substituting the
/// `${placeholders}` that version JSONs use in their arguments.
pub fn build_command(
    paths: &Paths,
    instance: &Instance,
    version: &Resolved,
    session: &Session,
    settings: &Settings,
    java: &Path,
) -> Result<Command> {
    let game_dir = paths.instance(&instance.id).join("minecraft");
    let natives_dir = paths.instance(&instance.id).join("natives");
    std::fs::create_dir_all(&game_dir)?;
    extract_natives(paths, version, &natives_dir)?;

    let mut classpath: Vec<String> = version
        .libraries
        .iter()
        .filter_map(|l| l.artifact())
        .map(|f| paths.libraries().join(f.path).to_string_lossy().into_owned())
        .collect();
    classpath.push(client_jar_path(paths, version).to_string_lossy().into_owned());

    let vars: HashMap<&str, String> = HashMap::from([
        ("auth_player_name", session.profile.name.clone()),
        ("auth_uuid", session.profile.uuid.clone()),
        ("auth_access_token", session.access_token.clone()),
        ("auth_session", session.access_token.clone()),
        ("auth_xuid", session.xuid.clone()),
        ("clientid", crate::auth::CLIENT_ID.to_string()),
        ("user_type", if session.profile.offline { "legacy" } else { "msa" }.into()),
        ("user_properties", "{}".into()),
        ("version_name", version.id.clone()),
        ("version_type", version.kind.clone()),
        ("game_directory", game_dir.to_string_lossy().into_owned()),
        ("assets_root", paths.assets().to_string_lossy().into_owned()),
        ("game_assets", paths.assets().to_string_lossy().into_owned()),
        ("assets_index_name", version.assets.clone()),
        ("natives_directory", natives_dir.to_string_lossy().into_owned()),
        ("library_directory", paths.libraries().to_string_lossy().into_owned()),
        ("classpath_separator", CLASSPATH_SEPARATOR.into()),
        ("classpath", classpath.join(CLASSPATH_SEPARATOR)),
        ("launcher_name", crate::LAUNCHER_NAME.into()),
        ("launcher_version", crate::LAUNCHER_VERSION.into()),
    ]);
    let expand = |arg: &str| {
        vars.iter().fold(arg.to_string(), |acc, (key, value)| acc.replace(&format!("${{{key}}}"), value))
    };

    let mut args = vec![format!("-Xmx{}M", settings.memory_mb)];
    args.extend(flatten(&version.jvm_args));
    // Legacy versions declare no JVM arguments, and some loaders add a few without a classpath.
    if !args.iter().any(|a| a.contains("${classpath}")) {
        args.extend(["-Djava.library.path=${natives_directory}".into(), "-cp".into(), "${classpath}".into()]);
    }
    if let Some(logging) = &version.logging {
        let config = paths.assets().join("log_configs").join(&logging.file.id);
        args.push(logging.argument.replace("${path}", &config.to_string_lossy()));
    }
    args.push(version.main_class.clone());
    let game_args = flatten(&version.game_args);
    match (&version.legacy_game_args, game_args.is_empty()) {
        (Some(legacy), true) => args.extend(legacy.split_whitespace().map(String::from)),
        _ => args.extend(game_args),
    }

    let mut command = Command::new(java);
    command.args(args.iter().map(|a| expand(a))).current_dir(&game_dir);
    Ok(command)
}

fn flatten(arguments: &[Argument]) -> Vec<String> {
    arguments
        .iter()
        .flat_map(|arg| match arg {
            Argument::Plain(s) => vec![s.clone()],
            Argument::Conditional { rules, value } if rules_allow(rules) => match value {
                ArgValue::One(s) => vec![s.clone()],
                ArgValue::Many(v) => v.clone(),
            },
            Argument::Conditional { .. } => vec![],
        })
        .collect()
}

/// Versions before 1.19 ship natives as classifier jars that must be unpacked.
/// Newer LWJGL builds load natives from the classpath themselves.
fn extract_natives(paths: &Paths, version: &Resolved, natives_dir: &Path) -> Result<()> {
    std::fs::create_dir_all(natives_dir)?;
    for lib in &version.libraries {
        let Some(file) = lib.native_artifact() else { continue };
        let excludes = lib.extract.as_ref().map(|e| e.exclude.as_slice()).unwrap_or_default();
        let mut archive = zip::ZipArchive::new(File::open(paths.libraries().join(file.path))?)?;
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i)?;
            let Some(name) = entry.enclosed_name() else { continue };
            let name_str = name.to_string_lossy();
            if entry.is_dir() || excludes.iter().any(|e| name_str.starts_with(e.as_str())) {
                continue;
            }
            let out = natives_dir.join(&name);
            if let Some(parent) = out.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::io::copy(&mut entry, &mut File::create(out)?)?;
        }
    }
    Ok(())
}
