use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::Command;
use tokio::sync::oneshot;

use crate::error::{Result, msg};

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

pub fn spawn(app: AppHandle, instance_id: String, mut command: Command, java_major: Option<u32>) -> Result<()> {
    command.stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
    let mut child = command.spawn().map_err(|e| {
        let needs = java_major.map(|v| format!(" This version needs Java {v}.")).unwrap_or_default();
        msg(format!("Couldn't start Java ({e}). Check the Java path in Settings.{needs}"))
    })?;
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

fn pipe(app: AppHandle, instance_id: String, stream: impl AsyncRead + Unpin + Send + 'static) {
    tauri::async_runtime::spawn(async move {
        let mut lines = BufReader::new(stream).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let _ = app.emit("game-log", LogLine { instance_id: instance_id.clone(), line });
        }
    });
}
