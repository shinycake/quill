//! Bounded GitHub release checks and explicit installation states.
#![deny(clippy::indexing_slicing)]
use semver::Version;
use serde::{Deserialize, Serialize};

pub const RELEASES_URL: &str = "https://github.com/shinycake/quill/releases";
pub const LATEST_API_URL: &str = "https://api.github.com/repos/shinycake/quill/releases/latest";
/// The beta channel reads the newest releases, pre-releases included
/// (`/releases/latest` never returns a pre-release).
pub const RELEASES_API_URL: &str =
    "https://api.github.com/repos/shinycake/quill/releases?per_page=30";
const MAX_RESPONSE_BYTES: u64 = 1_048_576;

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

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum UpdateState {
    #[default]
    Idle,
    Checking,
    Available(ReleaseInfo),
    Downloading(ReleaseInfo),
    Installing(ReleaseInfo),
    DownloadFailed(ReleaseInfo, &'static str),
    InstallFailed(ReleaseInfo, &'static str),
    Installed(ReleaseInfo),
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
            Self::Downloading(_) => "Downloading and verifying update…".into(),
            Self::Installing(_) => "Installing update and restarting…".into(),
            Self::DownloadFailed(_, message) | Self::InstallFailed(_, message) => (*message).into(),
            Self::Installed(release) => format!("Updated to Quill {}", release.version),
            Self::UpToDate => "Quill is up to date.".into(),
            Self::NoRelease => "No published Quill release is available yet.".into(),
            Self::Failed(message) => (*message).into(),
        }
    }
}

#[derive(Deserialize)]
struct GithubRelease {
    tag_name: String,
    html_url: String,
    body: Option<String>,
    draft: bool,
    prerelease: bool,
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
    asset.url == format!("{RELEASES_URL}/download/{tag}/{}", binary_asset_name())
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

fn release_version(tag: &str) -> Option<Version> {
    Version::parse(tag.strip_prefix('v').unwrap_or(tag)).ok()
}

/// The beta channel ("Install beta versions"): `body` is the releases list.
/// The newest published release wins, pre-release or not, and drafts never
/// count. An empty list is `NoRelease`.
pub fn interpret_release_list(status: u16, body: &[u8], current: &str) -> UpdateState {
    if status == 404 {
        return UpdateState::NoRelease;
    }
    if status != 200 {
        return UpdateState::Failed("Release server unavailable. Retry the check.");
    }
    if body.len() as u64 > MAX_RESPONSE_BYTES {
        return UpdateState::Failed("Release information is too large. Retry the check.");
    }
    let Ok(releases) = serde_json::from_slice::<Vec<GithubRelease>>(body) else {
        return UpdateState::Failed("Invalid release information. Retry the check.");
    };
    let Ok(current) = Version::parse(current) else {
        return UpdateState::Failed("Unrecognized installed version.");
    };
    let newest = releases
        .into_iter()
        .filter(|release| !release.draft)
        .filter_map(|release| Some((release_version(&release.tag_name)?, release)))
        .max_by(|(a, _), (b, _)| a.cmp_precedence(b));
    match newest {
        Some((version, release)) => release_state(release, version, &current),
        None => UpdateState::NoRelease,
    }
}

/// The state for a release that passed its channel's filter.
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
        if a.name != binary_asset_name() {
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

/// Blocking, bounded check; callers run it on the background executor.
/// "Install beta versions" (`advanced::update_channel`) picks the channel.
#[cfg(feature = "ui")]
pub fn check_latest_release() -> UpdateState {
    check_release(crate::advanced::update_channel::install_beta())
}

/// The check for one channel: the latest stable release, or with `beta` the
/// newest release of any kind.
#[cfg(feature = "ui")]
pub fn check_release(beta: bool) -> UpdateState {
    let interpret = |status: u16, body: &[u8]| {
        if beta {
            interpret_release_list(status, body, crate::version::APP)
        } else {
            interpret_release(status, body, crate::version::APP)
        }
    };
    let agent = ureq::Agent::config_builder()
        .https_only(true)
        .max_redirects(0)
        .timeout_global(Some(std::time::Duration::from_secs(15)))
        .http_status_as_error(false)
        .build()
        .new_agent();
    let response = agent
        .get(if beta {
            RELEASES_API_URL
        } else {
            LATEST_API_URL
        })
        .header("User-Agent", format!("Quill/{}", crate::version::APP))
        .header("Accept", "application/vnd.github+json")
        .call();
    let Ok(mut response) = response else {
        return UpdateState::Failed(
            "Could not reach the release server. Check your connection and retry.",
        );
    };
    let status = response.status().as_u16();
    if status != 200 {
        return interpret(status, &[]);
    }
    let Ok(body) = response
        .body_mut()
        .with_config()
        .limit(MAX_RESPONSE_BYTES)
        .read_to_vec()
    else {
        return UpdateState::Failed("Could not read release information. Retry the check.");
    };
    interpret(status, &body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
        let asset = json!({"name": binary_asset_name(), "browser_download_url": format!("{RELEASES_URL}/download/v0.2.0/{}", binary_asset_name()), "size": 120, "digest": format!("sha256:{}", "a".repeat(64))});
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
    fn listed(tag: &str, prerelease: bool, draft: bool) -> serde_json::Value {
        json!({"tag_name":tag,"html_url":format!("{RELEASES_URL}/tag/{tag}"),"body":"notes","draft":draft,"prerelease":prerelease})
    }

    #[test]
    fn the_beta_channel_takes_the_newest_published_release() {
        let list = |items: Vec<serde_json::Value>| serde_json::to_vec(&items).unwrap();
        let body = list(vec![
            listed("v2026.1012.1300", true, false),
            listed("v2026.1011.1200", false, false),
        ]);
        let UpdateState::Available(info) = interpret_release_list(200, &body, "2026.1011.1200")
        else {
            panic!("the pre-release is newer");
        };
        assert_eq!(info.version, "2026.1012.1300");
        // Stable only: the same list through the stable reader refuses a
        // pre-release outright.
        assert!(matches!(
            interpret_release(
                200,
                &serde_json::to_vec(&listed("v2026.1012.1300", true, false)).unwrap(),
                "2026.1011.1200"
            ),
            UpdateState::Failed(_)
        ));
        // A draft never counts, however new.
        let body = list(vec![
            listed("v2026.1013.1400", true, true),
            listed("v2026.1011.1200", false, false),
        ]);
        assert_eq!(
            interpret_release_list(200, &body, "2026.1011.1200"),
            UpdateState::UpToDate
        );
        // The order of the list does not matter.
        let body = list(vec![
            listed("v2026.1011.1200", false, false),
            listed("v2026.1012.1300", true, false),
        ]);
        assert!(matches!(
            interpret_release_list(200, &body, "2026.1011.1200"),
            UpdateState::Available(_)
        ));
        // A newer stable release beats an older pre-release.
        let body = list(vec![
            listed("v2026.1012.1300", true, false),
            listed("v2026.1014.1500", false, false),
        ]);
        let UpdateState::Available(info) = interpret_release_list(200, &body, "2026.1011.1200")
        else {
            panic!("newer stable");
        };
        assert_eq!(info.version, "2026.1014.1500");
    }

    #[test]
    fn the_beta_list_handles_empty_and_broken_answers() {
        assert_eq!(
            interpret_release_list(200, b"[]", "0.1.0"),
            UpdateState::NoRelease
        );
        assert_eq!(
            interpret_release_list(404, b"", "0.1.0"),
            UpdateState::NoRelease
        );
        for (status, body) in [
            (403, b"[]".as_slice()),
            (500, b"x"),
            (200, b"{}"),
            (200, b"nope"),
        ] {
            assert!(matches!(
                interpret_release_list(status, body, "0.1.0"),
                UpdateState::Failed(_)
            ));
        }
        assert!(matches!(
            interpret_release_list(200, &vec![b' '; MAX_RESPONSE_BYTES as usize + 1], "0.1.0"),
            UpdateState::Failed(_)
        ));
        // Tags that are not versions are skipped, not fatal.
        let body = serde_json::to_vec(&vec![listed("nightly", true, false)]).unwrap();
        assert_eq!(
            interpret_release_list(200, &body, "0.1.0"),
            UpdateState::NoRelease
        );
    }
}
