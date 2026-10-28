//! Opening folders and files in the system's own apps, and showing a file in its folder.
//!
//! macOS and Windows use the opener plugin. Linux needs more care: an AppImage runs Clover
//! with its own libraries on `LD_LIBRARY_PATH` (and GTK and GIO paths pointing inside it),
//! which the file manager inherits and crashes on; and tiling setups such as Hyprland often
//! have no D-Bus file manager to show a file, or no default app set for folders. So Linux
//! gets a clean environment and a list of file managers to fall back on.

use std::path::Path;

use clover_core::error::{Result, msg};
use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;

/// Opens a folder in the file manager, or a file in its default app.
pub fn open(app: &AppHandle, path: &Path) -> Result<()> {
    if cfg!(target_os = "linux") {
        return linux::open(path);
    }
    app.opener().open_path(path.to_string_lossy(), None::<&str>).map_err(|e| msg(e.to_string()))
}

/// Shows a file or folder selected in its parent folder.
pub fn reveal(app: &AppHandle, path: &Path) -> Result<()> {
    if cfg!(target_os = "linux") {
        // The file manager's D-Bus interface selects the item; without one, open its folder.
        return match linux::reveal_over_dbus(path) {
            Ok(()) => Ok(()),
            Err(_) => linux::open(path.parent().unwrap_or(path)),
        };
    }
    app.opener().reveal_item_in_dir(path).map_err(|e| msg(e.to_string()))
}

mod linux {
    use std::path::Path;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    use clover_core::error::{Result, msg};

    /// Programs that open a path, the desktop's choice first. Each is tried until one works.
    const OPENERS: &[&[&str]] = &[
        &["xdg-open"],
        &["gio", "open"],
        &["nautilus"],
        &["dolphin"],
        &["thunar"],
        &["nemo"],
        &["pcmanfm-qt"],
        &["pcmanfm"],
        &["caja"],
        &["io.elementary.files"],
    ];

    pub fn open(path: &Path) -> Result<()> {
        let is_dir = path.is_dir();
        for opener in OPENERS {
            // File managers only make sense for folders; files need xdg-open or gio.
            if !is_dir && opener.len() == 1 && opener[0] != "xdg-open" {
                continue;
            }
            if run(opener, path) {
                return Ok(());
            }
        }
        Err(msg(if is_dir {
            "Couldn't open a file manager. Install one, such as Nautilus, Dolphin or Thunar, or set a default with “xdg-mime default <app>.desktop inode/directory”."
        } else {
            "Couldn't open the file. Set a default app for it with xdg-mime."
        }))
    }

    /// Starts an opener and gives it a moment to fail. xdg-open and gio exit with an error
    /// when nothing handles the path; a file manager that keeps running has opened it.
    fn run(opener: &[&str], path: &Path) -> bool {
        let mut command = Command::new(opener[0]);
        command.args(&opener[1..]).arg(path).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        clean_environment(&mut command);
        let Ok(mut child) = command.spawn() else { return false };
        let started = Instant::now();
        while started.elapsed() < Duration::from_secs(3) {
            match child.try_wait() {
                Ok(Some(status)) => return status.success(),
                Ok(None) => std::thread::sleep(Duration::from_millis(50)),
                Err(_) => return false,
            }
        }
        // Still running: it's the file manager itself, which stays open. Reap it later.
        std::thread::spawn(move || child.wait());
        true
    }

    /// The Freedesktop file manager interface, which opens the folder with the item selected.
    pub fn reveal_over_dbus(path: &Path) -> Result<()> {
        let uri = format!("file://{}", percent_encode(&path.canonicalize()?.to_string_lossy()));
        let mut command = Command::new("gdbus");
        command
            .args(["call", "--session", "--timeout", "3"])
            .args(["--dest", "org.freedesktop.FileManager1"])
            .args(["--object-path", "/org/freedesktop/FileManager1"])
            .args(["--method", "org.freedesktop.FileManager1.ShowItems"])
            .arg(format!("['{}']", uri.replace('\'', "%27")))
            .arg("")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        clean_environment(&mut command);
        match command.status() {
            Ok(status) if status.success() => Ok(()),
            _ => Err(msg("No file manager answers on D-Bus.")),
        }
    }

    fn percent_encode(path: &str) -> String {
        path.bytes()
            .map(|b| match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
                _ => format!("%{b:02X}"),
            })
            .collect()
    }

    /// Undoes what an AppImage adds to the environment, so other apps start with the user's own.
    fn clean_environment(command: &mut Command) {
        let Some(appdir) = std::env::var_os("APPDIR") else { return };
        let appdir = appdir.to_string_lossy().into_owned();
        for name in [
            "LD_LIBRARY_PATH",
            "LD_PRELOAD",
            "GDK_PIXBUF_MODULE_FILE",
            "GDK_PIXBUF_MODULEDIR",
            "GDK_BACKEND",
            "GIO_MODULE_DIR",
            "GIO_EXTRA_MODULES",
            "GTK_PATH",
            "GTK_EXE_PREFIX",
            "GTK_DATA_PREFIX",
            "GTK_IM_MODULE_FILE",
            "GTK_THEME",
            "GSETTINGS_SCHEMA_DIR",
            "PYTHONHOME",
            "PYTHONPATH",
            "PERLLIB",
            "QT_PLUGIN_PATH",
            "APPDIR",
            "APPIMAGE",
            "ARGV0",
            "OWD",
        ] {
            command.env_remove(name);
        }
        // Lists of folders keep the user's entries and lose the AppImage's.
        for name in ["PATH", "XDG_DATA_DIRS", "XDG_CONFIG_DIRS"] {
            if let Some(value) = std::env::var_os(name) {
                let kept: Vec<_> = std::env::split_paths(&value).filter(|p| !p.to_string_lossy().starts_with(&appdir)).collect();
                if let Ok(joined) = std::env::join_paths(kept) {
                    command.env(name, joined);
                }
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn encodes_file_uris() {
            assert_eq!(percent_encode("/home/me/My World's"), "/home/me/My%20World%27s");
        }
    }
}
