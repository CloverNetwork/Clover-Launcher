//! Running the game: the process, its output and stopping it.

use std::collections::{HashMap, VecDeque};
use std::process::Stdio;
use std::sync::{Arc, Mutex};

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

    /// Keeps a line, dropping the oldest once there are too many.
    pub fn push(&self, instance_id: &str, line: String) {
        let mut logs = self.0.lock().unwrap();
        let lines = logs.entry(instance_id.to_string()).or_default();
        if lines.len() == MAX_LOG_LINES {
            lines.pop_front();
        }
        lines.push_back(line);
    }

    pub fn clear(&self, instance_id: &str) {
        self.0.lock().unwrap().remove(instance_id);
    }
}

/// Running games, keyed by instance ID. Sending on the channel stops the game.
#[derive(Default)]
pub struct Running(Mutex<HashMap<String, oneshot::Sender<()>>>);

impl Running {
    pub fn is_running(&self, id: &str) -> bool {
        self.0.lock().unwrap().contains_key(id)
    }

    /// Registers a game before it starts, so it counts as running straight away.
    /// Returns what tells `spawn` to stop it.
    pub fn start(&self, id: &str) -> oneshot::Receiver<()> {
        let (stop, stopped) = oneshot::channel();
        self.0.lock().unwrap().insert(id.to_string(), stop);
        stopped
    }

    pub fn stop(&self, id: &str) {
        if let Some(stop) = self.0.lock().unwrap().remove(id) {
            let _ = stop.send(());
        }
    }

    pub fn finished(&self, id: &str) {
        self.0.lock().unwrap().remove(id);
    }
}

/// Starts the game. Output goes to `on_line` as it's printed, with Mojang's log4j XML
/// turned into plain lines. `on_exit` runs once with the exit code, or `None` when the game
/// was stopped through `stop`.
pub fn spawn(
    mut command: Command,
    stop: oneshot::Receiver<()>,
    on_line: impl Fn(String) + Send + Sync + 'static,
    on_exit: impl FnOnce(Option<i32>) + Send + 'static,
) -> Result<()> {
    command.stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
    let mut child = command.spawn().map_err(java_start_error)?;
    let on_line = Arc::new(on_line);
    if let Some(out) = child.stdout.take() {
        pipe(out, on_line.clone());
    }
    if let Some(err) = child.stderr.take() {
        pipe(err, on_line);
    }
    tokio::spawn(async move {
        let code = tokio::select! {
            status = child.wait() => status.ok().and_then(|s| s.code()),
            _ = stop => {
                let _ = child.kill().await;
                None
            }
        };
        on_exit(code);
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

fn pipe(stream: impl AsyncRead + Unpin + Send + 'static, on_line: Arc<impl Fn(String) + Send + Sync + 'static>) {
    tokio::spawn(async move {
        let mut lines = BufReader::new(stream).lines();
        let mut log4j = Log4jLines::default();
        while let Ok(Some(line)) = lines.next_line().await {
            if let Some(line) = log4j.push(&line) {
                on_line(line);
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
    use super::{Log4jLines, Running, spawn};
    use std::sync::{Arc, Mutex};

    #[cfg(unix)]
    #[tokio::test]
    async fn runs_and_stops_a_process() {
        let lines = Arc::new(Mutex::new(Vec::new()));
        let running = Running::default();

        // A process that prints plain and log4j lines, then exits with code 3.
        let mut command = tokio::process::Command::new("sh");
        command.arg("-c").arg(
            r#"echo plain; echo '<log4j:Event logger="x" timestamp="1" level="INFO" thread="main">'; echo '<log4j:Message><![CDATA[hello]]></log4j:Message>'; echo '</log4j:Event>'; exit 3"#,
        );
        let (exit_tx, exit_rx) = tokio::sync::oneshot::channel();
        let seen = lines.clone();
        spawn(command, running.start("a"), move |l| seen.lock().unwrap().push(l), move |code| {
            let _ = exit_tx.send(code);
        })
        .unwrap();
        assert!(running.is_running("a"));
        assert_eq!(exit_rx.await.unwrap(), Some(3));
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert_eq!(*lines.lock().unwrap(), ["plain", "[main/INFO] hello"]);

        // A process that would run for a minute stops when asked.
        let mut command = tokio::process::Command::new("sleep");
        command.arg("60");
        let (exit_tx, exit_rx) = tokio::sync::oneshot::channel();
        spawn(command, running.start("b"), |_| {}, move |code| {
            let _ = exit_tx.send(code);
        })
        .unwrap();
        running.stop("b");
        let code = tokio::time::timeout(std::time::Duration::from_secs(5), exit_rx).await.expect("stopped in time").unwrap();
        assert_eq!(code, None);
        assert!(!running.is_running("b"));
    }

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
