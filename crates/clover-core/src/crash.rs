//! Crash help: reads the game log and the newest crash report after Minecraft quits with an
//! error, recognizes the common causes, and offers a fix where Clover can apply one.

use std::io::Read;
use std::path::Path;
use std::sync::LazyLock;
use std::time::UNIX_EPOCH;

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::instances::Instance;
use crate::mods::{self, Kind, Shelf};
use crate::paths::Paths;
use crate::settings::Settings;

/// Something Clover can do about a finding, from a button next to it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Fix {
    /// Install a mod from Modrinth by its project ID or slug.
    InstallMod { project: String },
    /// Turn off a mod in the mods folder.
    TurnOffMod { file: String },
    SetMemory { mb: u32 },
    /// Clear the instance's own Java so Clover picks the right one.
    AutomaticJava,
    /// Open Edit Instance, for settings only the person can choose.
    EditInstance,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    pub title: String,
    pub detail: String,
    pub fix: Option<Fix>,
    /// The button's label for `fix`.
    pub action: Option<String>,
}

impl Finding {
    fn new(title: impl Into<String>, detail: impl Into<String>) -> Self {
        Self { title: title.into(), detail: detail.into(), fix: None, action: None }
    }

    fn with(mut self, action: impl Into<String>, fix: Fix) -> Self {
        self.action = Some(action.into());
        self.fix = Some(fix);
        self
    }
}

/// What's known about the instance, so findings can name files and suggest real numbers.
pub struct Context<'a> {
    pub instance: &'a Instance,
    pub settings: &'a Settings,
    /// Mod ID → (file, display name), read from each jar's metadata.
    pub mods: Vec<ModInfo>,
}

#[derive(Debug, Clone)]
pub struct ModInfo {
    pub id: String,
    pub name: String,
    pub file: String,
}

macro_rules! pattern {
    ($name:ident, $re:expr) => {
        static $name: LazyLock<Regex> = LazyLock::new(|| Regex::new($re).expect("valid pattern"));
    };
}

// Java: "compiled by a more recent version of the Java Runtime (class file version 65.0)".
pattern!(CLASS_VERSION, r"class file version (\d+)\.0\), this version of the Java Runtime only recognizes class file versions up to (\d+)");
// Fabric and Quilt: "Mod 'Iris' (iris) 1.8.8 requires any version of mod sodium, which is missing!"
// A mod that's present is named "'Sodium' (sodium)" instead.
pattern!(FABRIC_MISSING, r"requires .+? of (?:mod )?(?:'([^']+)' \()?([A-Za-z0-9_\-]+)\)?, which is missing");
// Fabric's suggestions: "- Install fabric-api, any version."
pattern!(FABRIC_INSTALL, r"^\s*-\s*Install ([a-z0-9_\-]+), ");
// Fabric: "Mod 'A' (a) 1.0 is incompatible with any version of mod 'B' (b), but a matching version is present"
pattern!(FABRIC_BREAKS, r"Mod '([^']+)' \(([a-z0-9_\-]+)\)[^']*?is incompatible with [^']*?'([^']+)' \(([a-z0-9_\-]+)\)");
// Fabric: "Mod 'A' (a) 1.0 requires version 0.5 or later of 'B' (b), but only the wrong version is present: 0.4!"
pattern!(FABRIC_WRONG, r"Mod '([^']+)' \(([a-z0-9_\-]+)\)[^']*? requires (.+?) of [^']*?'([^']+)' \(([a-z0-9_\-]+)\), but only the wrong version is present");
// NeoForge and Forge: "Mod ID: 'geckolib', Requested by: 'mowziesmobs', Expected range: '[4.4,)', Actual version: '[MISSING]'"
pattern!(FORGE_MISSING, r"Mod ID: '([^']+)', Requested by: '([^']+)', Expected range: '([^']*)', Actual version: '([^']*)'");
// Mixins: "Mixin apply for mod iris failed" or "... from mod sodium ..." in an injection error.
pattern!(MIXIN, r"(?:Mixin apply for mod ([a-z0-9_\-]+) failed|InvalidInjectionException.*?from mod ([a-z0-9_\-]+))");
pattern!(DUPLICATE, r"(?:Duplicate|duplicate) mods? (?:found|for mod ID|ID)[^a-z0-9]*'?([a-z0-9_\-]+)");
pattern!(VM_OPTION, r"Unrecognized VM option '([^']+)'");

pub fn analyze(log: &[String], crash_report: Option<&str>, context: &Context) -> Vec<Finding> {
    let mut findings: Vec<Finding> = Vec::new();
    let mut push = |finding: Finding| {
        if !findings.iter().any(|f| f.title == finding.title) {
            findings.push(finding);
        }
    };
    let text: Vec<&str> = log.iter().map(String::as_str).chain(crash_report.into_iter().flat_map(str::lines)).collect();
    let name_of = |id: &str| context.mods.iter().find(|m| m.id == id).map(|m| m.name.clone());
    let file_of = |id: &str| context.mods.iter().find(|m| m.id == id).map(|m| m.file.clone());
    let java_path = context.instance.options.java_path.as_deref().is_some_and(|p| !p.trim().is_empty());

    for line in &text {
        if let Some(c) = CLASS_VERSION.captures(line) {
            let (needed, has) = (class_to_java(&c[1]), class_to_java(&c[2]));
            let finding = Finding::new(
                format!("This needs Java {needed}"),
                format!("Something in the game was built for Java {needed}, but it ran on Java {has}."),
            );
            push(if java_path { finding.with("Use Automatic Java", Fix::AutomaticJava) } else { finding.with("Edit Instance…", Fix::EditInstance) });
        }
        if let Some(c) = FABRIC_MISSING.captures(line) {
            let id = &c[2];
            push(missing_mod(id, c.get(1).map_or(id, |m| m.as_str())));
        }
        if let Some(c) = FABRIC_INSTALL.captures(line) {
            let id = &c[1];
            if !["minecraft", "fabricloader", "java"].contains(&id) {
                push(missing_mod(id, &name_of(id).unwrap_or_else(|| id.to_string())));
            }
        }
        if let Some(c) = FORGE_MISSING.captures(line) {
            let (id, by, range, actual) = (&c[1], &c[2], &c[3], &c[4]);
            let by = name_of(by).unwrap_or_else(|| by.to_string());
            if actual == "[MISSING]" {
                let mut finding = missing_mod(id, id);
                finding.detail = format!("{by} needs {id}{}, which isn't installed.", range_text(range));
                push(finding);
            } else {
                let title = format!("{by} needs a different {}", name_of(id).unwrap_or_else(|| id.to_string()));
                push(Finding::new(title, format!("It needs version {range}; version {actual} is installed. Update one of them.")));
            }
        }
        if let Some(c) = FABRIC_WRONG.captures(line) {
            let (who, wanted, dep_name) = (&c[1], &c[3], &c[4]);
            push(Finding::new(format!("{who} needs a different {dep_name}"), format!("It needs {wanted}. Check the Mods section for updates.")));
        }
        if let Some(c) = FABRIC_BREAKS.captures(line) {
            let (a_name, a, b_name) = (&c[1], &c[2], &c[3]);
            let finding = Finding::new(format!("{a_name} doesn't work with {b_name}"), "Turn off one of them, or look for versions that work together.");
            push(match file_of(a) {
                Some(file) => finding.with(format!("Turn Off {a_name}"), Fix::TurnOffMod { file }),
                None => finding,
            });
        }
        if let Some(c) = MIXIN.captures(line) {
            let id = c.get(1).or(c.get(2)).map_or("", |m| m.as_str());
            let name = name_of(id).unwrap_or_else(|| id.to_string());
            let finding = Finding::new(
                format!("{name} failed to load"),
                format!("It may not match Minecraft {} or another mod. Update it, or turn it off.", context.instance.version),
            );
            push(match file_of(id) {
                Some(file) => finding.with(format!("Turn Off {name}"), Fix::TurnOffMod { file }),
                None => finding,
            });
        }
        if let Some(c) = DUPLICATE.captures(line) {
            let id = &c[1];
            let name = name_of(id).unwrap_or_else(|| id.to_string());
            push(Finding::new(format!("{name} is installed twice"), "Remove one of the copies from the Mods section."));
        }
        if line.contains("java.lang.OutOfMemoryError") {
            let current = context.instance.options.memory_mb.unwrap_or(context.settings.memory_mb);
            let more = ((current + current / 2).div_ceil(1024) * 1024).max(current + 1024).min(32768);
            let finding = Finding::new("Minecraft ran out of memory", format!("It had {}. Give it more, or use fewer mods and resource packs.", gigabytes(current)));
            push(if more > current { finding.with(format!("Use {}", gigabytes(more)), Fix::SetMemory { mb: more }) } else { finding });
        }
        if line.contains("Could not reserve enough space for") || line.contains("Initial heap size set to a larger value than the maximum") {
            let current = context.instance.options.memory_mb.unwrap_or(context.settings.memory_mb);
            let less = (current / 2).max(2048);
            push(Finding::new("Java couldn't get the memory it was given", format!("{} is more than this computer can spare.", gigabytes(current)))
                .with(format!("Use {}", gigabytes(less)), Fix::SetMemory { mb: less }));
        }
        if let Some(c) = VM_OPTION.captures(line) {
            push(Finding::new("Java doesn't know a JVM argument", format!("“{}” isn't a Java option. Check JVM Arguments.", &c[1])).with("Edit Instance…", Fix::EditInstance));
        }
        if line.contains("Pixel format not accelerated") || line.contains("driver does not appear to support OpenGL") || line.contains("GLFW error 65542") {
            push(Finding::new("The graphics driver can't run Minecraft", "Update the graphics driver from the GPU maker's website, then try again."));
        }
    }

    // A crash report's description, when nothing above explained it.
    if findings.is_empty()
        && let Some(description) = crash_report.and_then(|r| r.lines().find_map(|l| l.strip_prefix("Description: ")))
    {
        findings.push(Finding::new("Minecraft crashed", format!("The crash report says: {description}.")));
    }
    findings
}

fn missing_mod(id: &str, name: &str) -> Finding {
    // Mod IDs are usually their Modrinth slugs; a few well-known ones aren't.
    let project = match id {
        "fabric" => "fabric-api",
        "cloth_config" | "cloth-config2" => "cloth-config",
        "geckolib3" => "geckolib",
        other => other,
    };
    let name = if name == id { pretty(id) } else { name.to_string() };
    Finding::new(format!("{name} is missing"), format!("Another mod needs {name}, which isn't installed."))
        .with(format!("Install {name}"), Fix::InstallMod { project: project.to_string() })
}

/// "fabric-api" → "Fabric API"-ish, for mods only known by ID.
fn pretty(id: &str) -> String {
    id.split(['-', '_'])
        .map(|w| match w {
            "api" => "API".to_string(),
            w => w.chars().next().map(|c| c.to_uppercase().collect::<String>() + &w[c.len_utf8()..]).unwrap_or_default(),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn range_text(range: &str) -> String {
    match range.trim_start_matches('[').trim_end_matches([')', ']']).split(',').next() {
        Some(min) if !min.is_empty() && range.starts_with('[') => format!(" {min} or newer"),
        _ => String::new(),
    }
}

/// Class file version 65 is Java 21.
fn class_to_java(class: &str) -> u32 {
    class.parse::<u32>().map(|v| v.saturating_sub(44)).unwrap_or(0)
}

fn gigabytes(mb: u32) -> String {
    let gb = mb as f64 / 1024.0;
    if gb.fract() == 0.0 { format!("{gb:.0} GB") } else { format!("{gb:.1} GB") }
}

/// The newest crash report written since `since` (seconds), if any.
pub fn newest_crash_report(paths: &Paths, instance: &Instance, since: u64) -> Option<String> {
    let dir = paths.instance(&instance.id).join("minecraft").join("crash-reports");
    let newest = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok())
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?.duration_since(UNIX_EPOCH).ok()?.as_secs(), e.path())))
        .filter(|(modified, path)| *modified + 5 >= since && path.extension().is_some_and(|x| x == "txt"))
        .max_by_key(|(modified, _)| *modified)?;
    std::fs::read_to_string(newest.1).ok()
}

/// Reads each jar's mod ID and name from Fabric's, Quilt's or (Neo)Forge's metadata.
pub fn mod_infos(paths: &Paths, instance: &Instance) -> Vec<ModInfo> {
    let shelf = Shelf::mods(paths, instance);
    let Ok(installed) = mods::installed(&shelf) else { return vec![] };
    installed
        .into_iter()
        .filter(|m| m.enabled)
        .flat_map(|m| {
            let ids = read_ids(&shelf.dir().join(&m.file));
            ids.into_iter().map(move |(id, name)| ModInfo { id, name: name.unwrap_or_else(|| m.title.clone()), file: m.file.clone() })
        })
        .collect()
}

fn read_ids(jar: &Path) -> Vec<(String, Option<String>)> {
    let Ok(file) = std::fs::File::open(jar) else { return vec![] };
    let Ok(mut archive) = zip::ZipArchive::new(file) else { return vec![] };
    let mut read = |name: &str| -> Option<String> {
        let mut entry = archive.by_name(name).ok()?;
        let mut text = String::new();
        entry.read_to_string(&mut text).ok()?;
        Some(text)
    };
    if let Some(json) = read("fabric.mod.json").and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()) {
        return json["id"].as_str().map(|id| vec![(id.to_string(), json["name"].as_str().map(String::from))]).unwrap_or_default();
    }
    if let Some(json) = read("quilt.mod.json").and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()) {
        let loader = &json["quilt_loader"];
        return loader["id"].as_str().map(|id| vec![(id.to_string(), loader["metadata"]["name"].as_str().map(String::from))]).unwrap_or_default();
    }
    let toml = read("META-INF/neoforge.mods.toml").or_else(|| read("META-INF/mods.toml")).unwrap_or_default();
    // A small read of `modId = "x"` and `displayName = "X"` pairs, enough without a TOML parser.
    pattern!(TOML_ID, r#"(?m)^\s*modId\s*=\s*"([^"]+)""#);
    pattern!(TOML_NAME, r#"(?m)^\s*displayName\s*=\s*"([^"]+)""#);
    let names: Vec<String> = TOML_NAME.captures_iter(&toml).map(|c| c[1].to_string()).collect();
    TOML_ID.captures_iter(&toml).enumerate().map(|(i, c)| (c[1].to_string(), names.get(i).cloned())).collect()
}

/// Applies a fix that changes only the instance. Installing a mod goes through `mods::install`.
pub fn apply_to_instance(instance: &mut Instance, fix: &Fix) {
    match fix {
        Fix::SetMemory { mb } => instance.options.memory_mb = Some(*mb),
        Fix::AutomaticJava => instance.options.java_path = None,
        _ => {}
    }
}

pub fn turn_off(paths: &Paths, instance: &Instance, file: &str) -> crate::error::Result<()> {
    mods::set_enabled(&Shelf::new(paths, instance, Kind::Mod, None)?, file, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context(instance: &Instance) -> Context<'_> {
        static SETTINGS: LazyLock<Settings> = LazyLock::new(Settings::default);
        Context {
            instance,
            settings: &SETTINGS,
            mods: vec![ModInfo { id: "iris".into(), name: "Iris".into(), file: "iris-1.8.jar".into() }],
        }
    }

    fn instance() -> Instance {
        Instance {
            id: "crashy".into(),
            name: "Crashy".into(),
            version: "1.21.4".into(),
            loader: "fabric".into(),
            loader_version: None,
            profile_id: None,
            created_at: 0,
            last_played: None,
            installed: true,
            play_seconds: 0,
            custom_jar: None,
            options: Default::default(),
        }
    }

    fn lines(text: &str) -> Vec<String> {
        text.lines().map(String::from).collect()
    }

    #[test]
    fn finds_missing_fabric_mods() {
        let log = lines(
            "net.fabricmc.loader.impl.FormattedException: Mod resolution encountered an incompatible mod set!\n\
             A potential solution has been determined, this may resolve your problem:\n\
             \t - Install fabric-api, any version.\n\
             More details:\n\
             \t - Mod 'Iris' (iris) 1.8.8 requires version 0.6.0 or later of mod sodium, which is missing!\n\
             \t - Mod 'Mod Menu' (modmenu) 13.0.0 requires any version of mod 'Cloth Config' (cloth-config2), which is missing!",
        );
        let instance = instance();
        let found = analyze(&log, None, &context(&instance));
        assert_eq!(found.len(), 3, "{found:#?}");
        assert_eq!(found[0].title, "Fabric API is missing");
        assert_eq!(found[0].fix, Some(Fix::InstallMod { project: "fabric-api".into() }));
        assert_eq!(found[1].title, "Sodium is missing");
        assert_eq!(found[1].fix, Some(Fix::InstallMod { project: "sodium".into() }));
        assert_eq!((found[2].title.as_str(), found[2].fix.clone()), ("Cloth Config is missing", Some(Fix::InstallMod { project: "cloth-config".into() })));
    }

    #[test]
    fn finds_forge_dependencies_java_memory_and_mixins() {
        let log = lines(
            "Missing or unsupported mandatory dependencies:\n\
             \tMod ID: 'geckolib', Requested by: 'mowziesmobs', Expected range: '[4.4,)', Actual version: '[MISSING]'\n\
             java.lang.UnsupportedClassVersionError: x has been compiled by a more recent version of the Java Runtime (class file version 65.0), this version of the Java Runtime only recognizes class file versions up to 61.0\n\
             java.lang.OutOfMemoryError: Java heap space\n\
             Mixin apply for mod iris failed iris.mixins.json:MixinLevelRenderer",
        );
        let instance = instance();
        let found = analyze(&log, None, &context(&instance));
        let titles: Vec<&str> = found.iter().map(|f| f.title.as_str()).collect();
        assert_eq!(titles, ["Geckolib is missing", "This needs Java 21", "Minecraft ran out of memory", "Iris failed to load"]);
        assert_eq!(found[0].detail, "mowziesmobs needs geckolib 4.4 or newer, which isn't installed.");
        assert_eq!(found[1].fix, Some(Fix::EditInstance));
        assert_eq!(found[2].fix, Some(Fix::SetMemory { mb: 6144 }));
        assert_eq!(found[3].fix, Some(Fix::TurnOffMod { file: "iris-1.8.jar".into() }));
    }

    #[test]
    fn falls_back_to_the_crash_report() {
        let report = "---- Minecraft Crash Report ----\nTime: now\nDescription: Rendering overlay\n\njava.lang.NullPointerException";
        let instance = instance();
        let found = analyze(&[], Some(report), &context(&instance));
        assert_eq!(found, [Finding::new("Minecraft crashed", "The crash report says: Rendering overlay.")]);
        assert!(analyze(&lines("[Render thread/INFO]: Stopping!"), None, &context(&instance)).is_empty());
    }

    #[test]
    fn reads_fixes_as_the_app_sends_them() {
        let fix: Fix = serde_json::from_str(r#"{"type":"installMod","project":"sodium"}"#).unwrap();
        assert_eq!(fix, Fix::InstallMod { project: "sodium".into() });
        let fix: Fix = serde_json::from_str(r#"{"type":"setMemory","mb":6144}"#).unwrap();
        assert_eq!(fix, Fix::SetMemory { mb: 6144 });
        assert_eq!(serde_json::to_string(&Fix::TurnOffMod { file: "a.jar".into() }).unwrap(), r#"{"type":"turnOffMod","file":"a.jar"}"#);
    }
}
