//! Checks GitHub Releases for a version newer than this build, and (on
//! request) downloads and hands off the matching installer to the OS.
//!
//! Everything here does a blocking network round trip, so every public
//! entry point spawns a background thread and hands its result back
//! through a `mpsc::Receiver` - `GoblinApp` polls that non-blockingly
//! (`try_recv`) once per frame (see `poll_update_check`/`poll_download` in
//! `gui/mod.rs`). This is the same "fire a thread, poll a channel" pattern
//! any one-shot background task needs in an immediate-mode GUI that has no
//! async runtime of its own.

use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver};
use std::thread;

/// `owner/repo`, taken from `Cargo.toml`'s `repository` field rather than
/// hardcoded again here - see `Cargo.toml`'s `workspace.package.repository`.
const REPO: &str = env!("CARGO_PKG_REPOSITORY");
const USER_AGENT: &str = concat!("abyssal-thread-updater/", env!("CARGO_PKG_VERSION"));

#[derive(Debug, Clone)]
pub struct ReleaseAsset {
    pub name: String,
    pub download_url: String,
}

#[derive(Debug, Clone)]
pub struct ReleaseInfo {
    /// The release version with any leading `v` stripped, e.g. `"0.2.7"`.
    pub version: String,
    /// The release's GitHub page - what "Open release page" opens.
    pub html_url: String,
    pub assets: Vec<ReleaseAsset>,
}

#[derive(serde::Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
}

#[derive(serde::Deserialize)]
struct GithubRelease {
    tag_name: String,
    html_url: String,
    assets: Vec<GithubAsset>,
}

fn api_url() -> anyhow::Result<String> {
    let slug = REPO
        .strip_prefix("https://github.com/")
        .ok_or_else(|| anyhow::anyhow!("unexpected repository URL: {REPO}"))?;
    Ok(format!(
        "https://api.github.com/repos/{slug}/releases/latest"
    ))
}

fn fetch_latest_release() -> anyhow::Result<GithubRelease> {
    let url = api_url()?;
    let mut response = ureq::get(&url).header("User-Agent", USER_AGENT).call()?;
    Ok(response.body_mut().read_json()?)
}

/// `Some(ReleaseInfo)` if the latest GitHub release is newer than
/// `current_version` (this build's `CARGO_PKG_VERSION`); `None` if
/// already up to date or ahead of it (a local/dev build).
fn check(current_version: &str) -> anyhow::Result<Option<ReleaseInfo>> {
    let release = fetch_latest_release()?;
    let latest_str = release.tag_name.trim_start_matches('v');
    let latest = semver::Version::parse(latest_str)?;
    let current = semver::Version::parse(current_version)?;
    if latest <= current {
        return Ok(None);
    }
    Ok(Some(ReleaseInfo {
        version: latest_str.to_string(),
        html_url: release.html_url,
        assets: release
            .assets
            .into_iter()
            .map(|a| ReleaseAsset {
                name: a.name,
                download_url: a.browser_download_url,
            })
            .collect(),
    }))
}

/// Spawns the update check on a background thread. Poll the returned
/// receiver with `try_recv()` once per frame - never `recv()` (that would
/// block the GUI thread).
pub fn check_async(current_version: String) -> Receiver<anyhow::Result<Option<ReleaseInfo>>> {
    let (tx, rx) = channel();
    thread::spawn(move || {
        let _ = tx.send(check(&current_version));
    });
    rx
}

/// Picks the release asset matching this build's OS, using the naming
/// convention `.github/workflows/release.yml`'s "Rename assets" step
/// produces (`Abyssal-Thread_<version>_<arch>[-suffix].<ext>`). Prefers
/// the NSIS `-setup.exe` over the `.msi` on Windows (no elevation prompt
/// in the common per-user-install case) and the `.AppImage` over the
/// `.deb` on Linux (runs directly - no package-manager handoff needed).
pub fn pick_asset(assets: &[ReleaseAsset]) -> Option<&ReleaseAsset> {
    let suffixes: &[&str] = if cfg!(target_os = "windows") {
        &["-setup.exe", ".msi"]
    } else if cfg!(target_os = "macos") {
        &[".dmg"]
    } else {
        &[".AppImage", ".deb"]
    };
    suffixes
        .iter()
        .find_map(|suffix| assets.iter().find(|a| a.name.ends_with(suffix)))
}

/// Downloads `asset` to a temp file named after the asset itself (so the
/// OS's file-type handling - icon, which installer runs - matches a
/// manual download) and returns its path.
fn download(asset: &ReleaseAsset) -> anyhow::Result<PathBuf> {
    let mut path = std::env::temp_dir();
    path.push(&asset.name);
    let mut response = ureq::get(&asset.download_url)
        .header("User-Agent", USER_AGENT)
        .call()?;
    let mut file = std::fs::File::create(&path)?;
    std::io::copy(&mut response.body_mut().as_reader(), &mut file)?;
    Ok(path)
}

/// Spawns the download on a background thread - poll like `check_async`.
pub fn download_async(asset: ReleaseAsset) -> Receiver<anyhow::Result<PathBuf>> {
    let (tx, rx) = channel();
    thread::spawn(move || {
        let _ = tx.send(download(&asset));
    });
    rx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_url_derives_the_github_api_endpoint_from_the_repository_field() {
        // Guards against `Cargo.toml`'s `repository` field ever changing
        // shape (e.g. gaining a trailing slash) in a way that would
        // silently break the API URL this builds.
        assert!(
            REPO.starts_with("https://github.com/"),
            "unexpected repository field: {REPO}"
        );
        let url = api_url().expect("api_url should succeed for a github.com repository URL");
        assert!(url.starts_with("https://api.github.com/repos/"));
        assert!(url.ends_with("/releases/latest"));
    }

    #[test]
    fn pick_asset_prefers_the_platform_appropriate_installer() {
        let assets = vec![
            ReleaseAsset {
                name: "Abyssal-Thread_0.2.7_x64-setup.exe".into(),
                download_url: String::new(),
            },
            ReleaseAsset {
                name: "Abyssal-Thread_0.2.7_x64_en-US.msi".into(),
                download_url: String::new(),
            },
            ReleaseAsset {
                name: "Abyssal-Thread_0.2.7_amd64.deb".into(),
                download_url: String::new(),
            },
            ReleaseAsset {
                name: "Abyssal-Thread_0.2.7_amd64.AppImage".into(),
                download_url: String::new(),
            },
            ReleaseAsset {
                name: "Abyssal-Thread_0.2.7_aarch64.dmg".into(),
                download_url: String::new(),
            },
        ];
        let picked =
            pick_asset(&assets).expect("one of the fixture assets should match this platform");
        if cfg!(target_os = "windows") {
            assert!(picked.name.ends_with("-setup.exe"));
        } else if cfg!(target_os = "macos") {
            assert!(picked.name.ends_with(".dmg"));
        } else {
            assert!(picked.name.ends_with(".AppImage"));
        }
    }

    #[test]
    fn pick_asset_returns_none_when_nothing_matches_this_platform() {
        let assets = vec![ReleaseAsset {
            name: "checksums.txt".into(),
            download_url: String::new(),
        }];
        assert!(pick_asset(&assets).is_none());
    }
}
