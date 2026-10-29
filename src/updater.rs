//! Bounded GitHub release checks and explicit installation states.
#![deny(clippy::indexing_slicing)]
use semver::Version;
use serde::{Deserialize, Serialize};

mod control;
pub use control::UpdateControl;

pub const RELEASES_URL: &str = "https://github.com/shinycake/quill/releases";
pub const LATEST_API_URL: &str = "https://api.github.com/repos/shinycake/quill/releases/latest";
/// The newest releases, pre-releases included (What's new reads them).
pub const RELEASES_API_URL: &str =
    "https://api.github.com/repos/shinycake/quill/releases?per_page=30";
pub(crate) const MAX_RESPONSE_BYTES: u64 = 1_048_576;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseInfo {
    pub version: String,
    pub notes: String,
    pub url: String,
    pub asset: Option<ReleaseAsset>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseAsset {
    pub url: String,
    pub size: u64,
    pub sha256: String,
}

pub fn binary_asset_name() -> String {
    format!("quill-{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

/// The release asset the in-app updater downloads: the whole package users
/// download too (the Linux tarball, the Windows zip, the macOS `Quill.app`
/// zip), so the bundled TDLib, call, FFmpeg and OpenSSL libraries always
/// match the new executable. Other platforms have no package.
pub fn update_asset_name() -> String {
    let name = binary_asset_name();
    if cfg!(target_os = "linux") {
        format!("{name}-bundle.tar.gz")
    } else if cfg!(any(windows, target_os = "macos")) {
        format!("{name}.zip")
    } else {
        name
    }
}

/// Test installs of the self-updater read a local release fixture instead
/// of GitHub: `QUILL_UPDATE_FIXTURE_URL=http://127.0.0.1:<port>` serves
/// `latest.json`, `releases.json` and `download/<tag>/<asset>`. Only test
/// and screenshot builds read the variable; release builds have no such
/// switch and only ever speak HTTPS to GitHub.
#[cfg(feature = "demo-capture")]
pub(crate) fn fixture_base() -> Option<String> {
    std::env::var("QUILL_UPDATE_FIXTURE_URL")
        .ok()
        .filter(|url| local_fixture_url(url))
        .map(|url| url.trim_end_matches('/').to_string())
}

/// Exactly `http://127.0.0.1:<port>`, optionally with a trailing slash: no
/// userinfo (`127.0.0.1:80@evil` would make `evil` the host), no path.
#[cfg(any(test, feature = "demo-capture"))]
pub(crate) fn local_fixture_url(url: &str) -> bool {
    url.strip_prefix("http://127.0.0.1:")
        .map(|port| port.trim_end_matches('/'))
        .is_some_and(|port| {
            (1..=5).contains(&port.len()) && port.bytes().all(|b| b.is_ascii_digit())
        })
}

/// Where a release asset is downloaded from.
// Downloading is part of the UI build only.
#[cfg_attr(not(feature = "ui"), allow(dead_code))]
pub(crate) struct DownloadSource {
    pub url: String,
    /// Only HTTPS is allowed (GitHub); the local fixture speaks HTTP.
    pub https_only: bool,
    /// GitHub serves assets through a redirect; the fixture must not
    /// redirect anywhere.
    pub redirects: u32,
}

/// The asset's GitHub URL, or its path under the test fixture.
// Downloading is part of the UI build only.
#[cfg_attr(not(feature = "ui"), allow(dead_code))]
pub(crate) fn download_source(asset_url: &str) -> DownloadSource {
    #[cfg(feature = "demo-capture")]
    if let Some(base) = fixture_base()
        && let Some(rest) = asset_url.strip_prefix(&format!("{RELEASES_URL}/download/"))
    {
        return DownloadSource {
            url: format!("{base}/download/{rest}"),
            https_only: false,
            redirects: 0,
        };
    }
    DownloadSource {
        url: asset_url.to_string(),
        https_only: true,
        redirects: 3,
    }
}

/// Bytes of the update package downloaded so far: the download thread adds
/// to it and the UI reads it on render.
#[derive(Debug, Clone, Default)]
pub struct DownloadProgress(std::sync::Arc<std::sync::atomic::AtomicU64>);

impl DownloadProgress {
    pub fn add(&self, bytes: u64) {
        self.0
            .fetch_add(bytes, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn bytes(&self) -> u64 {
        self.0.load(std::sync::atomic::Ordering::Relaxed)
    }
}

/// One download is equal only to itself.
impl PartialEq for DownloadProgress {
    fn eq(&self, other: &Self) -> bool {
        std::sync::Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for DownloadProgress {}

fn release_size(release: &ReleaseInfo) -> u64 {
    release.asset.as_ref().map_or(0, |asset| asset.size)
}

/// tdesktop `Ui::FormatDownloadText`: "12.3 / 45.6 MB".
pub fn progress_text(ready: u64, total: u64) -> String {
    const MB: u64 = 1024 * 1024;
    let tenths = |bytes: u64| {
        let tenths = bytes.saturating_mul(10) / MB;
        format!("{}.{}", tenths / 10, tenths % 10)
    };
    format!("{} / {} MB", tenths(ready.min(total)), tenths(total))
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum UpdateState {
    #[default]
    Idle,
    Checking,
    Available(ReleaseInfo),
    Downloading(ReleaseInfo, DownloadProgress),
    Installing(ReleaseInfo),
    DownloadFailed(ReleaseInfo, &'static str),
    InstallFailed(ReleaseInfo, &'static str),
    Installed(ReleaseInfo),
    /// Downloaded and verified in the background ("Install updates
    /// automatically"); the path is the install plan, applied the next time
    /// Quill starts or when the user presses "Restart to update".
    Staged(ReleaseInfo, std::path::PathBuf),
    UpToDate,
    NoRelease,
    Failed(&'static str),
}

impl UpdateState {
    pub fn label(&self) -> String {
        match self {
            Self::Idle => format!("Quill {}", crate::version::APP),
            Self::Checking => "Checking for updates…".into(),
            Self::Available(release) => format!("Quill {} is available", release.version),
            Self::Downloading(release, progress) => crate::lang::tr_with(
                crate::lang::keys::Q_OPS_DOWNLOADING_UPDATE,
                &[(
                    "progress",
                    progress_text(progress.bytes(), release_size(release)).as_str(),
                )],
            ),
            Self::Installing(_) => "Installing update and restarting…".into(),
            Self::DownloadFailed(_, message) | Self::InstallFailed(_, message) => (*message).into(),
            Self::Installed(release) => format!("Updated to Quill {}", release.version),
            Self::Staged(release, _) => crate::lang::tr_with(
                crate::lang::keys::Q_OPS_UPDATE_READY_RESTART,
                &[("release_version", release.version.as_str())],
            ),
            Self::UpToDate => "Quill is up to date.".into(),
            Self::NoRelease => "No published Quill release is available yet.".into(),
            Self::Failed(message) => (*message).into(),
        }
    }
}

#[derive(Deserialize)]
pub(crate) struct GithubRelease {
    pub(crate) tag_name: String,
    pub(crate) html_url: String,
    pub(crate) body: Option<String>,
    pub(crate) draft: bool,
    pub(crate) prerelease: bool,
    #[serde(default)]
    pub(crate) published_at: Option<String>,
    #[serde(default)]
    assets: Vec<GithubAsset>,
}

#[derive(Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
    size: u64,
    digest: Option<String>,
}

pub fn valid_asset(asset: &ReleaseAsset, tag: &str) -> bool {
    asset.url == format!("{RELEASES_URL}/download/{tag}/{}", update_asset_name())
        && (1..=536_870_912).contains(&asset.size)
        && asset.sha256.len() == 64
        && asset.sha256.bytes().all(|b| b.is_ascii_hexdigit())
}

pub fn interpret_release(status: u16, body: &[u8], current: &str) -> UpdateState {
    if status == 404 {
        return UpdateState::NoRelease;
    }
    if status != 200 {
        return UpdateState::Failed("Release server unavailable. Retry the check.");
    }
    if body.len() as u64 > MAX_RESPONSE_BYTES {
        return UpdateState::Failed("Release information is too large. Retry the check.");
    }
    let Ok(release) = serde_json::from_slice::<GithubRelease>(body) else {
        return UpdateState::Failed("Invalid release information. Retry the check.");
    };
    let Some(version) = release_version(&release.tag_name) else {
        return UpdateState::Failed("Unrecognized release version. Retry the check.");
    };
    let Ok(current) = Version::parse(current) else {
        return UpdateState::Failed("Unrecognized installed version.");
    };
    if release.draft || release.prerelease || !version.pre.is_empty() {
        return UpdateState::Failed("No stable release information. Retry the check.");
    }
    release_state(release, version, &current)
}

pub(crate) fn release_version(tag: &str) -> Option<Version> {
    Version::parse(tag.strip_prefix('v').unwrap_or(tag)).ok()
}

/// The state for a stable release.
fn release_state(release: GithubRelease, version: Version, current: &Version) -> UpdateState {
    if !version.cmp_precedence(current).is_gt() {
        return UpdateState::UpToDate;
    }
    if !release
        .html_url
        .starts_with(&format!("{RELEASES_URL}/tag/"))
    {
        return UpdateState::Failed("Invalid release link. Retry the check.");
    }
    let asset = release.assets.into_iter().find_map(|a| {
        if a.name != update_asset_name() {
            return None;
        }
        let asset = ReleaseAsset {
            url: a.browser_download_url,
            size: a.size,
            sha256: a.digest?.strip_prefix("sha256:")?.to_ascii_lowercase(),
        };
        valid_asset(&asset, &release.tag_name).then_some(asset)
    });
    UpdateState::Available(ReleaseInfo {
        version: version.to_string(),
        notes: release.body.unwrap_or_default(),
        url: release.html_url,
        asset,
    })
}

/// The release list or latest-release endpoint, and whether only HTTPS is
/// allowed: GitHub's API, or the test fixture's `releases.json` /
/// `latest.json`.
#[cfg(feature = "ui")]
fn check_endpoint(list: bool) -> (String, bool) {
    #[cfg(feature = "demo-capture")]
    if let Some(base) = fixture_base() {
        let file = if list { "releases.json" } else { "latest.json" };
        return (format!("{base}/{file}"), false);
    }
    let url = if list {
        RELEASES_API_URL
    } else {
        LATEST_API_URL
    };
    (url.to_string(), true)
}

/// Blocking, bounded check of the latest stable release; callers run it on
/// the background executor.
#[cfg(feature = "ui")]
pub fn check_latest_release() -> UpdateState {
    match fetch_release_json(false) {
        Ok((status, body)) => interpret_release(status, &body, crate::version::APP),
        Err(Fetch::Unreachable) => UpdateState::Failed(
            "Could not reach the release server. Check your connection and retry.",
        ),
        Err(Fetch::Unreadable) => {
            UpdateState::Failed("Could not read release information. Retry the check.")
        }
    }
}

#[cfg(feature = "ui")]
/// Why [`fetch_release_json`] got no answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Fetch {
    Unreachable,
    Unreadable,
}

#[cfg(feature = "ui")]
/// The latest release (`list == false`) or the releases list, as status and
/// body; the body is empty unless the status is 200. Blocking and bounded.
pub(crate) fn fetch_release_json(list: bool) -> Result<(u16, Vec<u8>), Fetch> {
    let (url, https_only) = check_endpoint(list);
    let agent = ureq::Agent::config_builder()
        .https_only(https_only)
        .max_redirects(0)
        .timeout_global(Some(std::time::Duration::from_secs(15)))
        .http_status_as_error(false)
        .build()
        .new_agent();
    let mut response = agent
        .get(&url)
        .header("User-Agent", format!("Quill/{}", crate::version::APP))
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|_| Fetch::Unreachable)?;
    let status = response.status().as_u16();
    if status != 200 {
        return Ok((status, Vec::new()));
    }
    let body = response
        .body_mut()
        .with_config()
        .limit(MAX_RESPONSE_BYTES)
        .read_to_vec()
        .map_err(|_| Fetch::Unreadable)?;
    Ok((status, body))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_staged_update_says_it_is_ready_and_waits_for_a_restart() {
        let release = ReleaseInfo {
            version: "9.9.9".into(),
            notes: String::new(),
            url: String::new(),
            asset: None,
        };
        let label = UpdateState::Staged(release, "plan.json".into()).label();
        assert!(
            label.contains("9.9.9") && label.contains("Restart to apply"),
            "{label}"
        );
    }

    #[test]
    fn update_asset_is_the_release_package() {
        let name = binary_asset_name();
        let asset = update_asset_name();
        if cfg!(target_os = "linux") {
            assert_eq!(asset, format!("{name}-bundle.tar.gz"));
        } else if cfg!(any(windows, target_os = "macos")) {
            assert_eq!(asset, format!("{name}.zip"));
        } else {
            assert_eq!(asset, name);
        }
        let github = format!("{RELEASES_URL}/download/v1.0.0/{asset}");
        let source = download_source(&github);
        assert_eq!(source.url, github);
        assert!(source.https_only);
        assert_eq!(source.redirects, 3);
    }

    #[test]
    fn only_a_bare_loopback_origin_is_a_fixture_url() {
        assert!(local_fixture_url("http://127.0.0.1:18765"));
        assert!(local_fixture_url("http://127.0.0.1:8/"));
        assert!(!local_fixture_url("http://127.0.0.1:80@evil.com/"));
        assert!(!local_fixture_url("http://127.0.0.1:80/path"));
        assert!(!local_fixture_url("http://127.0.0.1"));
        assert!(!local_fixture_url("https://127.0.0.1:443"));
        assert!(!local_fixture_url("http://localhost:80"));
        assert!(!local_fixture_url("http://127.0.0.1:123456"));
    }

    #[test]
    fn release_checks_handle_ordering_notes_absence_and_errors() {
        let release = |tag: &str| {
            serde_json::to_vec(&json!({"tag_name":tag,"html_url":format!("{RELEASES_URL}/tag/{tag}"),"body":"Changes 😀\nSecond line","draft":false,"prerelease":false})).unwrap()
        };
        let UpdateState::Available(info) = interpret_release(200, &release("v0.10.0"), "0.9.0")
        else {
            panic!("expected newer release");
        };
        assert_eq!(info.version, "0.10.0");
        assert_eq!(info.notes, "Changes 😀\nSecond line");
        assert!(info.asset.is_none());
        let asset = json!({"name": update_asset_name(), "browser_download_url": format!("{RELEASES_URL}/download/v0.2.0/{}", update_asset_name()), "size": 120, "digest": format!("sha256:{}", "a".repeat(64))});
        let mut downloadable = json!({"tag_name":"v0.2.0", "html_url":format!("{RELEASES_URL}/tag/v0.2.0"), "body":"Release notes", "draft":false,"prerelease":false,"assets":[asset]});
        let UpdateState::Available(info) =
            interpret_release(200, downloadable.to_string().as_bytes(), "0.1.0")
        else {
            panic!("new release absent")
        };
        assert_eq!(info.asset.unwrap().size, 120);
        for (field, value) in [
            ("digest", json!(null)),
            ("size", json!(0)),
            ("browser_download_url", json!("https://evil.invalid/update")),
        ] {
            let mut invalid = downloadable.clone();
            invalid["assets"][0][field] = value;
            let UpdateState::Available(info) =
                interpret_release(200, invalid.to_string().as_bytes(), "0.1.0")
            else {
                panic!("release absent")
            };
            assert!(info.asset.is_none());
        }
        downloadable["assets"][0]["name"] = json!("quill-other-platform");
        let UpdateState::Available(info) =
            interpret_release(200, downloadable.to_string().as_bytes(), "0.1.0")
        else {
            panic!("release absent")
        };
        assert!(info.asset.is_none());
        assert_eq!(
            interpret_release(200, &release("v0.10.0"), "0.10.0"),
            UpdateState::UpToDate
        );
        assert_eq!(
            interpret_release(200, &release("v0.9.0"), "0.10.0"),
            UpdateState::UpToDate
        );
        assert_eq!(
            interpret_release(200, &release("v0.10.0+build2"), "0.10.0"),
            UpdateState::UpToDate
        );
        assert_eq!(
            interpret_release(404, b"private raw error", "0.1.0"),
            UpdateState::NoRelease
        );
        for (status, body) in [
            (403, b"rate limit".as_slice()),
            (500, b"down"),
            (200, b"not json"),
        ] {
            assert!(matches!(
                interpret_release(status, body, "0.1.0"),
                UpdateState::Failed(_)
            ));
        }
        assert!(matches!(
            interpret_release(200, &release("v0.2.0-beta.1"), "0.1.0"),
            UpdateState::Failed(_)
        ));
        assert!(matches!(
            interpret_release(200, &release("invalid"), "0.1.0"),
            UpdateState::Failed(_)
        ));
        let bad = json!({"tag_name":"v0.2.0","html_url":"https://github.com.evil.invalid/shinycake/quill/releases/tag/v0.2.0","body":null,"draft":false,"prerelease":false});
        assert!(matches!(
            interpret_release(200, bad.to_string().as_bytes(), "0.1.0"),
            UpdateState::Failed(_)
        ));
        assert!(matches!(
            interpret_release(200, &vec![b' '; MAX_RESPONSE_BYTES as usize + 1], "0.1.0"),
            UpdateState::Failed(_)
        ));
    }
}
