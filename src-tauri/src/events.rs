//! How clover-core's progress and game output reach the windows: Tauri events, and the
//! separate game log windows.

use clover_core::game::GameLogs;
use clover_core::install::Reporter;
use serde::Serialize;
use tauri::window::{ProgressBarState, ProgressBarStatus};
use tauri::{AppHandle, Emitter, Manager, UserAttentionType, WebviewUrl, WebviewWindowBuilder};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct LogLine<'a> {
    instance_id: &'a str,
    line: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct GameExit<'a> {
    instance_id: &'a str,
    code: Option<i32>,
}

/// Records a line of game or installer output and sends it to every window.
pub fn emit_log(app: &AppHandle, instance_id: &str, line: String) {
    app.state::<GameLogs>().push(instance_id, line.clone());
    let _ = app.emit("game-log", LogLine { instance_id, line });
}

/// Starts a fresh log for a new launch, in every window.
pub fn clear_log(app: &AppHandle, instance_id: &str) {
    app.state::<GameLogs>().clear(instance_id);
    let _ = app.emit("game-log-cleared", instance_id);
}

pub fn emit_exit(app: &AppHandle, instance_id: &str, code: Option<i32>) {
    let _ = app.emit("game-exit", GameExit { instance_id, code });
    // A crash in the background bounces the Dock icon or flashes the taskbar button.
    if code.is_some_and(|c| c != 0) {
        if let Some(main) = app.get_webview_window("main") {
            if !main.is_focused().unwrap_or(true) {
                let _ = main.request_user_attention(Some(UserAttentionType::Informational));
            }
        }
    }
}

/// Download progress on the app's Dock icon (macOS) or taskbar button (Windows, and
/// Linux desktops that support it). `None` clears it.
fn icon_progress(app: &AppHandle, fraction: Option<f64>) {
    if let Some(main) = app.get_webview_window("main") {
        let state = match fraction {
            Some(f) => ProgressBarState { status: Some(ProgressBarStatus::Normal), progress: Some((f * 100.0).round() as u64) },
            None => ProgressBarState { status: Some(ProgressBarStatus::None), progress: None },
        };
        let _ = main.set_progress_bar(state);
    }
}

fn show_files_on_icon(app: &AppHandle, done: usize, total: usize) {
    icon_progress(app, (total > 0 && done < total).then(|| done as f64 / total as f64));
}

/// Opens an instance's log in its own window, or brings it forward if it's open.
pub fn open_log_window(app: &AppHandle, instance_id: &str, name: &str) -> tauri::Result<()> {
    let label = format!("log-{instance_id}");
    if let Some(window) = app.get_webview_window(&label) {
        return window.set_focus();
    }
    WebviewWindowBuilder::new(app, label, WebviewUrl::App(format!("log/{instance_id}").into()))
        .title(format!("{name} — Game Log"))
        .inner_size(760.0, 480.0)
        .min_inner_size(420.0, 240.0)
        .decorations(crate::chrome::decorated())
        // Elsewhere the page shows the window once drawn, so there's no white flash. macOS
        // opens it right away: a hidden window there doesn't draw its page at all.
        .visible(cfg!(target_os = "macos"))
        .build()?;
    Ok(())
}

/// Moves a log window's contents back under the instance page in the main window.
pub fn pop_in_log_window(app: &AppHandle, instance_id: &str) -> tauri::Result<()> {
    // Only the main window listens for this.
    app.emit("log-popped-in", instance_id)?;
    if let Some(main) = app.get_webview_window("main") {
        main.set_focus()?;
    }
    if let Some(window) = app.get_webview_window(&format!("log-{instance_id}")) {
        window.close()?;
    }
    Ok(())
}

/// Emits roughly every 1% so the UI stays smooth without flooding IPC.
fn worth_emitting(last: &mut usize, done: usize, total: usize) -> bool {
    let due = done == total || done < *last || done - *last >= (total / 100).max(1);
    if due {
        *last = done;
    }
    due
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Progress<'a> {
    instance_id: &'a str,
    stage: Option<&'a str>,
    done: usize,
    total: usize,
}

/// Sends install progress as `install-progress` and installer output as `game-log`.
pub struct AppReporter {
    app: AppHandle,
    instance_id: String,
    last_emitted: usize,
}

impl AppReporter {
    pub fn new(app: AppHandle, instance_id: String) -> Self {
        Self { app, instance_id, last_emitted: 0 }
    }
}

impl Reporter for AppReporter {
    fn stage(&mut self, text: &str) {
        let progress = Progress { instance_id: &self.instance_id, stage: Some(text), done: 0, total: 0 };
        let _ = self.app.emit("install-progress", progress);
    }

    fn files(&mut self, done: usize, total: usize) {
        if worth_emitting(&mut self.last_emitted, done, total) {
            let _ = self.app.emit("install-progress", Progress { instance_id: &self.instance_id, stage: None, done, total });
            show_files_on_icon(&self.app, done, total);
        }
    }

    fn log(&mut self, line: String) {
        emit_log(&self.app, &self.instance_id, line);
    }
}

/// However the job ends, the icon stops showing progress.
impl Drop for AppReporter {
    fn drop(&mut self) {
        icon_progress(&self.app, None);
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ImportProgress<'a> {
    stage: Option<&'a str>,
    done: usize,
    total: usize,
}

/// Progress for a modpack import, sent as `import-progress` since there's no instance
/// to name until the pack has been read.
pub struct ImportReporter {
    app: AppHandle,
    last_emitted: usize,
}

impl ImportReporter {
    pub fn new(app: AppHandle) -> Self {
        Self { app, last_emitted: 0 }
    }
}

impl Reporter for ImportReporter {
    fn stage(&mut self, text: &str) {
        let _ = self.app.emit("import-progress", ImportProgress { stage: Some(text), done: 0, total: 0 });
    }

    fn files(&mut self, done: usize, total: usize) {
        if worth_emitting(&mut self.last_emitted, done, total) {
            let _ = self.app.emit("import-progress", ImportProgress { stage: None, done, total });
            show_files_on_icon(&self.app, done, total);
        }
    }

    fn log(&mut self, _: String) {}
}

impl Drop for ImportReporter {
    fn drop(&mut self) {
        icon_progress(&self.app, None);
    }
}
