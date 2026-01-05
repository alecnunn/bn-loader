use crate::config::cache_dir;
use semver::Version;
use serde::Deserialize;
use std::fs;
use std::io::Write;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const GITHUB_REPO: &str = "alecnunn/bn-loader";
const CACHE_FILE: &str = "update-check.json";
const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60); // 24 hours
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Deserialize)]
struct GitHubRelease {
    tag_name: String,
    html_url: String,
}

#[derive(serde::Serialize, serde::Deserialize, Default)]
struct UpdateCache {
    last_check: u64,
    latest_version: Option<String>,
    release_url: Option<String>,
}

pub(crate) struct UpdateInfo {
    pub current: String,
    pub latest: String,
    pub url: String,
}

/// Check for updates if enough time has passed since last check.
/// Returns Some(UpdateInfo) if an update is available, None otherwise.
pub(crate) fn check_for_updates() -> Option<UpdateInfo> {
    let cache = load_cache();

    // Check if we should skip (checked recently)
    if !should_check(&cache) {
        // Still return update info if we have cached data showing update available
        return check_cached_update(&cache);
    }

    // Fetch latest release from GitHub
    match fetch_latest_release() {
        Ok((latest_version, release_url)) => {
            // Save to cache
            save_cache(&latest_version, &release_url);

            // Compare versions
            compare_versions(&latest_version, &release_url)
        }
        Err(_) => {
            // Silently fail - don't bother user with network errors
            None
        }
    }
}

/// Force check for updates, bypassing cache. Used for --check-update flag.
/// Returns Some(UpdateInfo) if update available, None if on latest or error.
pub(crate) fn check_for_updates_forced() -> Option<UpdateInfo> {
    match fetch_latest_release() {
        Ok((latest_version, release_url)) => {
            save_cache(&latest_version, &release_url);
            compare_versions(&latest_version, &release_url)
        }
        Err(e) => {
            eprintln!("Error checking for updates: {e}");
            None
        }
    }
}

fn should_check(cache: &UpdateCache) -> bool {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    now.saturating_sub(cache.last_check) > CHECK_INTERVAL.as_secs()
}

fn check_cached_update(cache: &UpdateCache) -> Option<UpdateInfo> {
    let latest = cache.latest_version.as_ref()?;
    let url = cache.release_url.as_ref()?;
    compare_versions(latest, url)
}

fn compare_versions(latest: &str, url: &str) -> Option<UpdateInfo> {
    // Strip 'v' prefix if present
    let latest_clean = latest.strip_prefix('v').unwrap_or(latest);

    let current = Version::parse(CURRENT_VERSION).ok()?;
    let latest_ver = Version::parse(latest_clean).ok()?;

    if latest_ver > current {
        Some(UpdateInfo {
            current: CURRENT_VERSION.to_string(),
            latest: latest_clean.to_string(),
            url: url.to_string(),
        })
    } else {
        None
    }
}

fn fetch_latest_release() -> Result<(String, String), String> {
    let url = format!("https://api.github.com/repos/{GITHUB_REPO}/releases/latest");

    let body = ureq::get(&url)
        .header("User-Agent", "bn-loader")
        .header("Accept", "application/vnd.github.v3+json")
        .call()
        .map_err(|e| format!("Failed to fetch release info: {e}"))?
        .into_body()
        .read_to_string()
        .map_err(|e| format!("Failed to read response: {e}"))?;

    let release: GitHubRelease =
        serde_json::from_str(&body).map_err(|e| format!("Failed to parse release info: {e}"))?;

    Ok((release.tag_name, release.html_url))
}

fn cache_path() -> Option<std::path::PathBuf> {
    cache_dir().map(|dir| dir.join(CACHE_FILE))
}

fn load_cache() -> UpdateCache {
    let Some(path) = cache_path() else {
        return UpdateCache::default();
    };

    if !path.exists() {
        return UpdateCache::default();
    }

    fs::read_to_string(&path)
        .ok()
        .and_then(|content| serde_json::from_str(&content).ok())
        .unwrap_or_default()
}

fn save_cache(latest_version: &str, release_url: &str) {
    let Some(path) = cache_path() else {
        return;
    };

    // Ensure cache directory exists
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let cache = UpdateCache {
        last_check: now,
        latest_version: Some(latest_version.to_string()),
        release_url: Some(release_url.to_string()),
    };

    if let Ok(json) = serde_json::to_string_pretty(&cache)
        && let Ok(mut file) = fs::File::create(&path)
    {
        let _ = file.write_all(json.as_bytes());
    }
}

/// Print update notification to stderr (so it doesn't interfere with stdout)
pub(crate) fn print_update_notice(info: &UpdateInfo) {
    eprintln!();
    eprintln!("  +-------------------------------------------------+");
    eprintln!(
        "  |  Update available: v{} -> v{:<16} |",
        info.current, info.latest
    );
    eprintln!("  |  {:<47}  |", info.url);
    eprintln!("  +-------------------------------------------------+");
    eprintln!();
}
