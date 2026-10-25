use std::collections::{HashMap, VecDeque};
use std::process::Stdio;
use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::Command;
use tokio::sync::oneshot;

use crate::error::{Result, msg};

const MAX_LOG_LINES: usize = 2000;

/// Recent output per instance, so a log window opened mid-game shows what came before.
#[derive(Default)]
pub struct GameLogs(Mutex<HashMap<String, VecDeque<String>>>);

impl GameLogs {
    pub fn lines(&self, instance_id: &str) -> Vec<String> {
        self.0.lock().unwrap().get(instance_id).map(|l| l.iter().cloned().collect()).unwrap_or_default()
    }
}

/// Records a line of game or installer output and sends it to every window.
pub fn emit_log(app: &AppHandle, instance_id: &str, line: String) {
    {
        let logs = app.state::<GameLogs>();
        let mut logs = logs.0.lock().unwrap();
        let lines = logs.entry(instance_id.to_string()).or_default();
        if lines.len() == MAX_LOG_LINES {
            lines.pop_front();
        }
        lines.push_back(line.clone());
    }
    let _ = app.emit("game-log", LogLine { instance_id: instance_id.to_string(), line });
}

/// Starts a fresh log for a new launch, in every window.
pub fn clear_log(app: &AppHandle, instance_id: &str) {
    app.state::<GameLogs>().0.lock().unwrap().remove(instance_id);
    let _ = app.emit("game-log-cleared", instance_id);
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
        .build()?;
    Ok(())
}

/// Running games, keyed by instance ID. Sending on the channel stops the game.
#[derive(Default)]
pub struct Running(Mutex<HashMap<String, oneshot::Sender<()>>>);

impl Running {
    pub fn is_running(&self, id: &str) -> bool {
        self.0.lock().unwrap().contains_key(id)
    }

    pub fn stop(&self, id: &str) {
        if let Some(stop) = self.0.lock().unwrap().remove(id) {
            let _ = stop.send(());
        }
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct LogLine {
    instance_id: String,
    line: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct GameExit {
    instance_id: String,
    code: Option<i32>,
}

pub fn spawn(app: AppHandle, instance_id: String, mut command: Command) -> Result<()> {
    command.stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
    let mut child = command.spawn().map_err(java_start_error)?;
    if let Some(out) = child.stdout.take() {
        pipe(app.clone(), instance_id.clone(), out);
    }
    if let Some(err) = child.stderr.take() {
        pipe(app.clone(), instance_id.clone(), err);
    }

    let (stop_tx, stop_rx) = oneshot::channel();
    app.state::<Running>().0.lock().unwrap().insert(instance_id.clone(), stop_tx);

    tauri::async_runtime::spawn(async move {
        let code = tokio::select! {
            status = child.wait() => status.ok().and_then(|s| s.code()),
            _ = stop_rx => {
                let _ = child.kill().await;
                None
            }
        };
        app.state::<Running>().0.lock().unwrap().remove(&instance_id);
        let _ = app.emit("game-exit", GameExit { instance_id, code });
    });
    Ok(())
}

/// Explains why Java didn't start. Error 86 is macOS refusing an Intel program because
/// Rosetta isn't installed, which happens with the Java 8 that old versions use.
pub fn java_start_error(e: std::io::Error) -> crate::error::Error {
    if cfg!(target_os = "macos") && e.raw_os_error() == Some(86) {
        msg("This version runs on Intel Java, which needs Rosetta. Install it by running \
             “softwareupdate --install-rosetta” in Terminal, then try again.")
    } else {
        msg(format!("Couldn't start Java ({e}). Check the Java setting in Settings, or leave it empty to use automatic Java."))
    }
}

fn pipe(app: AppHandle, instance_id: String, stream: impl AsyncRead + Unpin + Send + 'static) {
    tauri::async_runtime::spawn(async move {
        let mut lines = BufReader::new(stream).lines();
        let mut log4j = Log4jLines::default();
        while let Ok(Some(line)) = lines.next_line().await {
            if let Some(line) = log4j.push(&line) {
                emit_log(&app, &instance_id, line);
            }
        }
    });
}

/// Turns the XML events Mojang's log config prints into plain `[thread/LEVEL] message`
/// lines, passing everything else through.
#[derive(Default)]
pub struct Log4jLines {
    event: Option<Event>,
}

#[derive(Default)]
struct Event {
    prefix: String,
    text: String,
    in_cdata: bool,
}

impl Log4jLines {
    pub fn push(&mut self, line: &str) -> Option<String> {
        let trimmed = line.trim_start();
        let Some(event) = &mut self.event else {
            if !trimmed.starts_with("<log4j:Event") {
                return Some(line.to_string());
            }
            let attr = |name: &str| {
                let start = trimmed.find(&format!("{name}=\""))? + name.len() + 2;
                Some(trimmed[start..].split('"').next()?.to_string())
            };
            let prefix = format!("[{}/{}]", attr("thread").unwrap_or_default(), attr("level").unwrap_or_default());
            self.event = Some(Event { prefix, ..Default::default() });
            return None;
        };

        if !event.in_cdata && trimmed.starts_with("</log4j:Event>") {
            let event = self.event.take()?;
            return Some(format!("{} {}", event.prefix, event.text.trim_end()));
        }
        // Message and throwable text sit in CDATA sections that may span lines.
        let mut rest = line;
        loop {
            if event.in_cdata {
                match rest.split_once("]]>") {
                    Some((text, after)) => {
                        event.text.push_str(text);
                        event.in_cdata = false;
                        rest = after;
                    }
                    None => {
                        event.text.push_str(rest);
                        event.text.push('\n');
                        return None;
                    }
                }
            } else {
                match rest.split_once("<![CDATA[") {
                    Some((_, after)) => {
                        if !event.text.is_empty() && !event.text.ends_with('\n') {
                            event.text.push('\n');
                        }
                        event.in_cdata = true;
                        rest = after;
                    }
                    None => return None,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Log4jLines;

    #[test]
    fn formats_log4j_events() {
        let mut log = Log4jLines::default();
        let input = [
            r#"  <log4j:Event logger="FabricLoader" timestamp="1" level="INFO" thread="main">"#,
            r#"    <log4j:Message><![CDATA[Loading Minecraft 1.21.4 with Fabric Loader 0.20.1]]></log4j:Message>"#,
            r#"  </log4j:Event>"#,
            r#"plain line"#,
            r#"<log4j:Event logger="x" timestamp="1" level="ERROR" thread="Render">"#,
            r#"<log4j:Message><![CDATA[Failed]]></log4j:Message>"#,
            r#"<log4j:Throwable><![CDATA[java.io.IOException: nope"#,
            r#"	at Foo.bar(Foo.java:1)"#,
            r#"]]></log4j:Throwable>"#,
            r#"</log4j:Event>"#,
        ];
        let out: Vec<String> = input.iter().filter_map(|l| log.push(l)).collect();
        assert_eq!(
            out,
            [
                "[main/INFO] Loading Minecraft 1.21.4 with Fabric Loader 0.20.1",
                "plain line",
                "[Render/ERROR] Failed\njava.io.IOException: nope\n\tat Foo.bar(Foo.java:1)",
            ]
        );
    }
}
