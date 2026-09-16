//! Background cleanup of orphaned uploads.
//!
//! `POST /api/uploads` writes to disk immediately, before the caller decides
//! whether to actually send the message or save the avatar. Anything they
//! abandon would otherwise sit on disk forever with nothing referencing it.
//!
//! The filesystem is treated as the source of truth and the database supplies
//! the set of URLs still in use, so no bookkeeping table can drift out of sync
//! with reality.

use std::collections::HashSet;
use std::time::{Duration, SystemTime};

use crate::state::AppState;

/// How long an unreferenced file is kept before deletion. Generous, because
/// the gap between uploading and sending is a human writing a message.
const GRACE_PERIOD: Duration = Duration::from_secs(24 * 60 * 60);
const SWEEP_INTERVAL: Duration = Duration::from_secs(60 * 60);

pub fn spawn(state: AppState) {
    tokio::spawn(async move {
        // Wait before the first run so startup stays fast.
        tokio::time::sleep(Duration::from_secs(120)).await;
        let mut ticker = tokio::time::interval(SWEEP_INTERVAL);
        loop {
            ticker.tick().await;
            match sweep(&state).await {
                Ok(0) => {}
                Ok(removed) => tracing::info!("swept {removed} orphaned upload(s)"),
                Err(e) => tracing::warn!("upload sweep failed: {e}"),
            }
        }
    });
}

/// Every upload URL the database still points at.
async fn referenced_urls(state: &AppState) -> Result<HashSet<String>, sqlx::Error> {
    let mut urls = HashSet::new();

    let queries = [
        "SELECT url FROM attachments",
        "SELECT avatar_url FROM users WHERE avatar_url IS NOT NULL",
        "SELECT banner_url FROM users WHERE banner_url IS NOT NULL",
        "SELECT icon_url FROM instance WHERE icon_url IS NOT NULL",
        "SELECT banner_url FROM instance WHERE banner_url IS NOT NULL",
        "SELECT avatar_url FROM webhooks WHERE avatar_url IS NOT NULL",
        "SELECT url FROM emojis",
    ];

    for sql in queries {
        let rows: Vec<String> = sqlx::query_scalar(sql).fetch_all(&state.db).await?;
        for url in rows {
            // Store the bare filename: that's what we can compare against disk.
            if let Some(name) = url.rsplit('/').next() {
                urls.insert(name.to_string());
            }
        }
    }
    Ok(urls)
}

pub async fn sweep(state: &AppState) -> Result<usize, String> {
    let referenced = referenced_urls(state).await.map_err(|e| e.to_string())?;
    let dir = state.config.uploads_dir();

    let mut entries = match tokio::fs::read_dir(&dir).await {
        Ok(entries) => entries,
        // No uploads directory yet is not an error.
        Err(_) => return Ok(0),
    };

    let now = SystemTime::now();
    let mut removed = 0usize;

    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if referenced.contains(name) {
            continue;
        }

        // Only delete files old enough that no in-flight composer still needs
        // them — a file uploaded thirty seconds ago is not an orphan yet.
        let old_enough = entry
            .metadata()
            .await
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|modified| now.duration_since(modified).ok())
            .is_some_and(|age| age > GRACE_PERIOD);

        if !old_enough {
            continue;
        }

        match tokio::fs::remove_file(&path).await {
            Ok(()) => removed += 1,
            Err(e) => tracing::warn!("could not remove orphaned upload {name}: {e}"),
        }
    }

    Ok(removed)
}
