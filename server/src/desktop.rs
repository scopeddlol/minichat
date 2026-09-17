//! Desktop download lookup.
//!
//! The website offers the Windows app by asking GitHub for the newest release
//! rather than hardcoding a link, so a new build is available the moment it is
//! published. Answers are cached: GitHub allows 60 unauthenticated requests an
//! hour, and a busy instance would burn that in minutes.

use serde::Serialize;
use std::time::{Duration, Instant};

use crate::state::AppState;

const CACHE_TTL: Duration = Duration::from_secs(30 * 60);
/// Also cached, so a rate-limited or offline lookup doesn't retry on every
/// page load.
const FAILURE_TTL: Duration = Duration::from_secs(5 * 60);

#[derive(Clone, Debug, Serialize)]
pub struct DesktopAsset {
    pub name: String,
    pub url: String,
    pub size: i64,
    /// "msi" or "exe", so the client can label the primary download.
    pub kind: String,
}

#[derive(Clone, Debug, Serialize, Default)]
pub struct DesktopRelease {
    pub available: bool,
    pub version: String,
    pub published_at: String,
    pub release_url: String,
    pub assets: Vec<DesktopAsset>,
}

#[derive(Clone)]
pub struct CachedRelease {
    pub fetched_at: Instant,
    pub release: DesktopRelease,
}

impl CachedRelease {
    fn is_fresh(&self) -> bool {
        let ttl = if self.release.available {
            CACHE_TTL
        } else {
            FAILURE_TTL
        };
        self.fetched_at.elapsed() < ttl
    }
}

/// Newest Windows release, from cache when it is still fresh.
pub async fn latest(state: &AppState) -> DesktopRelease {
    if let Some(cached) = state.desktop_release.read().await.as_ref() {
        if cached.is_fresh() {
            return cached.release.clone();
        }
    }

    let release = fetch(&state.config.desktop_release_repo).await;
    *state.desktop_release.write().await = Some(CachedRelease {
        fetched_at: Instant::now(),
        release: release.clone(),
    });
    release
}

async fn fetch(repo: &str) -> DesktopRelease {
    let url = format!("https://api.github.com/repos/{repo}/releases/latest");

    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        // GitHub rejects requests without a User-Agent.
        .user_agent(concat!("MiniChat/", env!("CARGO_PKG_VERSION")))
        .build()
    {
        Ok(client) => client,
        Err(e) => {
            tracing::warn!("could not build the release-lookup client: {e}");
            return DesktopRelease::default();
        }
    };

    let response = match client
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
    {
        Ok(response) => response,
        Err(e) => {
            tracing::info!("desktop release lookup failed: {e}");
            return DesktopRelease::default();
        }
    };

    if !response.status().is_success() {
        tracing::info!(
            "desktop release lookup returned {} for {repo}",
            response.status()
        );
        return DesktopRelease::default();
    }

    let body: serde_json::Value = match response.json().await {
        Ok(body) => body,
        Err(e) => {
            tracing::info!("could not parse the release response: {e}");
            return DesktopRelease::default();
        }
    };

    parse_release(&body)
}

/// Split out from the request so the shape of GitHub's response can be tested
/// without reaching the network.
pub fn parse_release(body: &serde_json::Value) -> DesktopRelease {
    let assets: Vec<DesktopAsset> = body["assets"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|asset| {
                    let name = asset["name"].as_str()?.to_string();
                    let lower = name.to_lowercase();
                    // Only installers. Signatures, archives and everything
                    // else a release might carry are not downloads to offer.
                    let kind = if lower.ends_with(".msi") {
                        "msi"
                    } else if lower.ends_with(".exe") {
                        "exe"
                    } else {
                        return None;
                    };
                    Some(DesktopAsset {
                        name,
                        url: asset["browser_download_url"].as_str()?.to_string(),
                        size: asset["size"].as_i64().unwrap_or(0),
                        kind: kind.to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    if assets.is_empty() {
        return DesktopRelease::default();
    }

    DesktopRelease {
        available: true,
        version: body["tag_name"].as_str().unwrap_or("").to_string(),
        published_at: body["published_at"].as_str().unwrap_or("").to_string(),
        release_url: body["html_url"].as_str().unwrap_or("").to_string(),
        assets,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn picks_installers_and_ignores_everything_else() {
        let body = json!({
            "tag_name": "v0.3.0",
            "published_at": "2026-09-17T00:00:00Z",
            "html_url": "https://github.com/owner/repo/releases/tag/v0.3.0",
            "assets": [
                { "name": "MiniChat_0.3.0_x64_en-US.msi", "browser_download_url": "https://example/msi", "size": 9000 },
                { "name": "MiniChat_0.3.0_x64-setup.exe", "browser_download_url": "https://example/exe", "size": 8000 },
                { "name": "MiniChat.msi.sig", "browser_download_url": "https://example/sig", "size": 10 },
                { "name": "source.tar.gz", "browser_download_url": "https://example/tar", "size": 100 }
            ]
        });

        let release = parse_release(&body);
        assert!(release.available);
        assert_eq!(release.version, "v0.3.0");
        assert_eq!(release.assets.len(), 2);
        assert_eq!(release.assets[0].kind, "msi");
        assert_eq!(release.assets[1].kind, "exe");
    }

    #[test]
    fn a_release_with_no_installers_is_not_available() {
        let body = json!({ "tag_name": "v0.3.0", "assets": [
            { "name": "notes.txt", "browser_download_url": "https://example/txt", "size": 1 }
        ]});
        assert!(!parse_release(&body).available);
    }

    #[test]
    fn a_malformed_response_is_not_available() {
        assert!(!parse_release(&json!({})).available);
        assert!(!parse_release(&json!({ "assets": "nonsense" })).available);
    }
}
