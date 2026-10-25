//! Starting an instance from outside Clover: the `--launch <instance>` argument (for
//! Hyprland keybinds and scripts) and shortcuts in the OS's own launcher. When Clover is
//! already open, a second start hands its arguments to the running app instead.

use std::path::{Path, PathBuf};

use clover_core::error::{Result, msg};
use clover_core::icon;
use clover_core::instances::Instance;

pub const LAUNCH_FLAG: &str = "--launch";

/// The instance named by `--launch <id>` or `--launch=<id>`, if any.
pub fn launch_arg(args: impl IntoIterator<Item = String>) -> Option<String> {
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        if arg == LAUNCH_FLAG {
            return args.next().filter(|id| !id.is_empty());
        }
        if let Some(id) = arg.strip_prefix("--launch=") {
            return Some(id.to_string()).filter(|id| !id.is_empty());
        }
    }
    None
}

/// The program to run. Inside an AppImage the running file is a temporary mount, so the
/// AppImage itself is what shortcuts must point at.
fn executable() -> Result<PathBuf> {
    if let Some(appimage) = std::env::var_os("APPIMAGE") {
        return Ok(PathBuf::from(appimage));
    }
    Ok(std::env::current_exe()?)
}

/// A command that starts the instance, for keybinds and scripts.
pub fn launch_command(instance: &Instance) -> Result<String> {
    Ok(format!("{} {LAUNCH_FLAG} {}", shell_quote(&executable()?.to_string_lossy()), instance.id))
}

fn shell_quote(text: &str) -> String {
    if text.chars().all(|c| c.is_ascii_alphanumeric() || "/._-+:=@".contains(c)) {
        text.to_string()
    } else {
        format!("'{}'", text.replace('\'', r"'\''"))
    }
}

/// Adds the instance to the OS's launcher. Returns where it went, for the confirmation.
pub fn create(instance: &Instance, home: &Path, desktop: &Path) -> Result<PathBuf> {
    let exe = executable()?;
    if cfg!(target_os = "macos") {
        mac_app(instance, &exe, home)
    } else if cfg!(windows) {
        windows_link(instance, &exe, desktop)
    } else {
        desktop_entry(instance, &exe, home)
    }
}

/// Characters that file systems or launchers trip over, replaced.
fn safe_name(name: &str) -> String {
    let cleaned: String = name.chars().map(|c| if matches!(c, '/' | '\\' | ':' | '"' | '<' | '>' | '|' | '?' | '*') { '-' } else { c }).collect();
    Some(cleaned.trim().trim_start_matches('.').to_string()).filter(|n| !n.is_empty()).unwrap_or_else(|| "Instance".into())
}

/// A tiny app in ~/Applications/Clover Instances, which Spotlight and Launchpad list. It
/// asks Clover to start the instance and quits.
fn mac_app(instance: &Instance, exe: &Path, home: &Path) -> Result<PathBuf> {
    let bundle = home.join("Applications").join("Clover Instances").join(format!("{}.app", safe_name(&instance.name)));
    let contents = bundle.join("Contents");
    std::fs::create_dir_all(contents.join("MacOS"))?;
    std::fs::create_dir_all(contents.join("Resources"))?;

    // From the installed app, `open -n` starts Clover properly (or hands the request to the
    // open one); a development build runs the binary directly.
    let app_bundle = exe.ancestors().find(|p| p.extension().is_some_and(|e| e == "app"));
    let script = match app_bundle {
        Some(app) => format!(
            "#!/bin/sh\nexec open -n -a {} --args {LAUNCH_FLAG} {}\n",
            shell_quote(&app.to_string_lossy()),
            instance.id
        ),
        None => format!("#!/bin/sh\nexec {} {LAUNCH_FLAG} {}\n", shell_quote(&exe.to_string_lossy()), instance.id),
    };
    let launcher = contents.join("MacOS").join("launch");
    std::fs::write(&launcher, script)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&launcher, std::fs::Permissions::from_mode(0o755))?;
    }

    // macOS wants an .icns icon; sips, which ships with macOS, converts the PNG.
    let png = contents.join("Resources").join("icon.png");
    std::fs::write(&png, icon::png(&instance.id, 512)?)?;
    let icns = contents.join("Resources").join("icon.icns");
    let converted = std::process::Command::new("sips")
        .args(["-s", "format", "icns"])
        .arg(&png)
        .arg("--out")
        .arg(&icns)
        .output()
        .is_ok_and(|o| o.status.success());
    let _ = std::fs::remove_file(&png);

    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleExecutable</key><string>launch</string>
  <key>CFBundleIdentifier</key><string>dev.blackdevlabs.clover.instance.{id}</string>
  <key>CFBundleName</key><string>{name}</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>1.0</string>
  {icon}
  <key>LSUIElement</key><{background}/>
</dict>
</plist>
"#,
        id = instance.id,
        name = xml_escape(&instance.name),
        icon = if converted { "<key>CFBundleIconFile</key><string>icon</string>" } else { "" },
        // The stub only hands over to Clover; keep it out of the Dock when that's all it does.
        background = if app_bundle.is_some() { "true" } else { "false" },
    );
    std::fs::write(contents.join("Info.plist"), plist)?;
    Ok(bundle)
}

fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// A .desktop entry in ~/.local/share/applications, with the block icon beside it.
fn desktop_entry(instance: &Instance, exe: &Path, home: &Path) -> Result<PathBuf> {
    let share = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).unwrap_or_else(|| home.join(".local/share"));
    let icon_path = share.join("icons/hicolor/256x256/apps").join(format!("clover-{}.png", instance.id));
    std::fs::create_dir_all(icon_path.parent().expect("has parent"))?;
    std::fs::write(&icon_path, icon::png(&instance.id, 256)?)?;

    let entry = share.join("applications").join(format!("clover-{}.desktop", instance.id));
    std::fs::create_dir_all(entry.parent().expect("has parent"))?;
    std::fs::write(&entry, desktop_file(instance, exe, &icon_path))?;
    Ok(entry)
}

fn desktop_file(instance: &Instance, exe: &Path, icon_path: &Path) -> String {
    let name = instance.name.replace(['\n', '\r'], " ");
    format!(
        "[Desktop Entry]\nType=Application\nName={name}\nComment=Play {name} in Clover Launcher\nExec={} {LAUNCH_FLAG} {}\nIcon={}\nCategories=Game;\nTerminal=false\n",
        desktop_exec_quote(&exe.to_string_lossy()),
        instance.id,
        icon_path.display(),
    )
}

/// Quoting for the Exec key of a desktop entry, per the Desktop Entry Specification.
fn desktop_exec_quote(text: &str) -> String {
    let mut quoted = String::from("\"");
    for c in text.chars() {
        if matches!(c, '"' | '`' | '$' | '\\') {
            quoted.push('\\');
        }
        quoted.push(c);
    }
    quoted.push('"');
    quoted.replace('%', "%%")
}

/// A .lnk shortcut on the desktop, made through Windows' own shell object.
fn windows_link(instance: &Instance, exe: &Path, desktop: &Path) -> Result<PathBuf> {
    let link = desktop.join(format!("{}.lnk", safe_name(&instance.name)));
    let ps_quote = |s: &str| format!("'{}'", s.replace('\'', "''"));
    let script = format!(
        "$s = (New-Object -ComObject WScript.Shell).CreateShortcut({link}); $s.TargetPath = {exe}; $s.Arguments = {args}; $s.IconLocation = {icon}; $s.Save()",
        link = ps_quote(&link.to_string_lossy()),
        exe = ps_quote(&exe.to_string_lossy()),
        args = ps_quote(&format!("{LAUNCH_FLAG} {}", instance.id)),
        icon = ps_quote(&format!("{},0", exe.to_string_lossy())),
    );
    let status = std::process::Command::new("powershell").args(["-NoProfile", "-NonInteractive", "-Command", &script]).status()?;
    if !status.success() {
        return Err(msg("Windows couldn't create the shortcut."));
    }
    Ok(link)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn instance(name: &str) -> Instance {
        Instance {
            id: "survival-a1b2c3".into(),
            name: name.into(),
            version: "1.21.4".into(),
            loader: "vanilla".into(),
            loader_version: None,
            profile_id: None,
            created_at: 0,
            last_played: None,
            installed: true,
        }
    }

    #[test]
    fn reads_the_launch_argument() {
        let args = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(launch_arg(args(&["clover", "--launch", "survival"])).as_deref(), Some("survival"));
        assert_eq!(launch_arg(args(&["clover", "--launch=survival"])).as_deref(), Some("survival"));
        assert_eq!(launch_arg(args(&["clover", "--launch"])), None);
        assert_eq!(launch_arg(args(&["clover"])), None);
    }

    #[test]
    fn writes_a_desktop_entry() {
        let file = desktop_file(&instance("My Pack"), Path::new("/opt/Clover Launcher/clover$1"), Path::new("/icons/clover.png"));
        assert!(file.contains("Name=My Pack\n"));
        assert!(file.contains(r#"Exec="/opt/Clover Launcher/clover\$1" --launch survival-a1b2c3"#), "{file}");
        assert!(file.contains("Categories=Game;"));
    }

    #[test]
    fn quotes_shell_commands() {
        assert_eq!(shell_quote("/usr/bin/clover"), "/usr/bin/clover");
        assert_eq!(shell_quote("/Applications/Clover Launcher.app"), "'/Applications/Clover Launcher.app'");
        assert_eq!(safe_name("a/b: c"), "a-b- c");
    }
}

#[cfg(all(test, target_os = "macos"))]
mod mac_tests {
    use super::*;

    /// Builds a shortcut app in a temporary "home" and checks what macOS needs is there.
    #[test]
    fn makes_a_mac_app() {
        let home = std::env::temp_dir().join(format!("clover-shortcut-{}", std::process::id()));
        let instance = Instance {
            id: "survival-a1b2c3".into(),
            name: "Survival: Hard".into(),
            version: "1.21.4".into(),
            loader: "vanilla".into(),
            loader_version: None,
            profile_id: None,
            created_at: 0,
            last_played: None,
            installed: true,
        };
        let bundle = mac_app(&instance, Path::new("/Applications/Clover Launcher.app/Contents/MacOS/clover-launcher"), &home).unwrap();
        assert!(bundle.ends_with("Applications/Clover Instances/Survival- Hard.app"));
        let script = std::fs::read_to_string(bundle.join("Contents/MacOS/launch")).unwrap();
        assert_eq!(script, "#!/bin/sh\nexec open -n -a '/Applications/Clover Launcher.app' --args --launch survival-a1b2c3\n");
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(bundle.join("Contents/MacOS/launch")).unwrap().permissions().mode() & 0o111, 0o111);
        let plist = std::fs::read_to_string(bundle.join("Contents/Info.plist")).unwrap();
        assert!(plist.contains("<string>launch</string>") && plist.contains("CFBundleIconFile"));
        assert!(bundle.join("Contents/Resources/icon.icns").exists(), "sips made the icon");
        // macOS itself accepts the property list.
        assert!(std::process::Command::new("plutil").arg("-lint").arg(bundle.join("Contents/Info.plist")).status().unwrap().success());
        std::fs::remove_dir_all(home).unwrap();
    }
}
