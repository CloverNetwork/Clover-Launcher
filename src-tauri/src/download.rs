use std::path::{Path, PathBuf};
use std::time::Duration;

use futures::{StreamExt, stream};
use reqwest::Client;
use sha1::{Digest, Sha1};

use crate::error::{Result, msg};

const CONCURRENCY: usize = 16;
const RETRIES: u32 = 3;

pub struct Task {
    pub url: String,
    pub path: PathBuf,
    pub sha1: Option<String>,
}

/// How much to trust files already on disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verify {
    /// A file that exists is complete: downloads land under a temporary name and are
    /// checked before they're moved into place. Used on every launch.
    Existing,
    /// Re-hash every file and replace any that don't match. Used by Verify Game Files.
    Full,
}

/// Downloads the tasks whose files are missing (or, with `Verify::Full`, don't match their
/// hash). Progress counts only real downloads. Returns how many files were downloaded.
pub async fn run(client: &Client, tasks: Vec<Task>, verify: Verify, mut on_progress: impl FnMut(usize, usize)) -> Result<usize> {
    let needed: Vec<Task> = stream::iter(tasks)
        .map(|task| async move { (!is_present(&task, verify).await).then_some(task) })
        .buffer_unordered(CONCURRENCY * 4)
        .filter_map(|task| async move { task })
        .collect()
        .await;

    let total = needed.len();
    let mut done = 0;
    let mut results = stream::iter(needed.into_iter().map(|t| download(client, t))).buffer_unordered(CONCURRENCY);
    while let Some(result) = results.next().await {
        result?;
        done += 1;
        on_progress(done, total);
    }
    Ok(total)
}

/// Downloads one file unless it's already present. Returns whether it downloaded.
pub async fn fetch(client: &Client, task: Task, verify: Verify) -> Result<bool> {
    if is_present(&task, verify).await {
        return Ok(false);
    }
    download(client, task).await?;
    Ok(true)
}

async fn download(client: &Client, task: Task) -> Result<()> {
    if let Some(parent) = task.path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let bytes = get_with_retry(client, &task.url).await?;
    if let Some(expected) = &task.sha1 {
        if &sha1_hex(&bytes) != expected {
            return Err(msg(format!("Checksum mismatch for {}", task.url)));
        }
    }
    // Write to a temp file first so a cancelled download never looks complete.
    let mut partial = task.path.clone().into_os_string();
    partial.push(".part");
    tokio::fs::write(&partial, &bytes).await?;
    tokio::fs::rename(&partial, &task.path).await?;
    Ok(())
}

/// Retries connection failures, timeouts and server errors; a 404 fails straight away.
async fn get_with_retry(client: &Client, url: &str) -> Result<bytes::Bytes> {
    let mut attempt = 0;
    loop {
        let result = async { client.get(url).send().await?.error_for_status()?.bytes().await }.await;
        match result {
            Err(e) if attempt < RETRIES && (e.is_connect() || e.is_timeout() || e.is_request() || e.status().is_some_and(|s| s.is_server_error())) => {
                attempt += 1;
                tokio::time::sleep(Duration::from_millis(500 * 2u64.pow(attempt))).await;
            }
            result => return Ok(result?),
        }
    }
}

async fn is_present(task: &Task, verify: Verify) -> bool {
    match (verify, task.sha1.as_deref()) {
        (Verify::Full, Some(expected)) => is_valid(&task.path, expected).await,
        _ => tokio::fs::try_exists(&task.path).await.unwrap_or(false),
    }
}

async fn is_valid(path: &Path, sha1: &str) -> bool {
    tokio::fs::read(path).await.is_ok_and(|bytes| sha1_hex(&bytes) == sha1)
}

fn sha1_hex(bytes: &[u8]) -> String {
    hex::encode(Sha1::digest(bytes))
}
