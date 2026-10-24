use std::path::{Path, PathBuf};

use futures::{StreamExt, stream};
use reqwest::Client;
use sha1::{Digest, Sha1};

use crate::error::{Result, msg};

const CONCURRENCY: usize = 16;

pub struct Task {
    pub url: String,
    pub path: PathBuf,
    pub sha1: Option<String>,
}

/// Downloads every task in parallel, skipping files that already verify.
pub async fn run(client: &Client, tasks: Vec<Task>, mut on_progress: impl FnMut(usize, usize)) -> Result<()> {
    let total = tasks.len();
    let mut done = 0;
    let mut results = stream::iter(tasks.into_iter().map(|t| fetch(client, t))).buffer_unordered(CONCURRENCY);
    while let Some(result) = results.next().await {
        result?;
        done += 1;
        on_progress(done, total);
    }
    Ok(())
}

pub async fn fetch(client: &Client, task: Task) -> Result<()> {
    if is_valid(&task.path, task.sha1.as_deref()).await {
        return Ok(());
    }
    if let Some(parent) = task.path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let bytes = client.get(&task.url).send().await?.error_for_status()?.bytes().await?;
    if let Some(expected) = &task.sha1 {
        if &sha1_hex(&bytes) != expected {
            return Err(msg(format!("Checksum mismatch for {}", task.url)));
        }
    }
    // Write to a temp file first so a cancelled download never looks complete.
    let partial = task.path.with_extension("part");
    tokio::fs::write(&partial, &bytes).await?;
    tokio::fs::rename(&partial, &task.path).await?;
    Ok(())
}

async fn is_valid(path: &Path, sha1: Option<&str>) -> bool {
    match (tokio::fs::read(path).await, sha1) {
        (Ok(bytes), Some(expected)) => sha1_hex(&bytes) == expected,
        (Ok(_), None) => true,
        (Err(_), _) => false,
    }
}

fn sha1_hex(bytes: &[u8]) -> String {
    hex::encode(Sha1::digest(bytes))
}
