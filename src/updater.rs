//! Read-only GitHub release checks. Downloads and installation are separate.
use semver::Version;
use serde::Deserialize;

pub const RELEASES_URL: &str = "https://github.com/shinycake/quill/releases";
pub const LATEST_API_URL: &str = "https://api.github.com/repos/shinycake/quill/releases/latest";
const MAX_RESPONSE_BYTES: u64 = 1_048_576;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseInfo {
    pub version: String,
    pub notes: String,
    pub url: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum UpdateState {
    #[default]
    Idle,
    Checking,
    Available(ReleaseInfo),
    UpToDate,
    NoRelease,
    Failed(&'static str),
}

impl UpdateState {
    pub fn label(&self) -> String {
        match self {
            Self::Idle => format!("Quill {}", env!("CARGO_PKG_VERSION")),
            Self::Checking => "Checking for updates…".into(),
            Self::Available(release) => format!("Quill {} is available", release.version),
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
    let Ok(version) = Version::parse(
        release
            .tag_name
            .strip_prefix('v')
            .unwrap_or(&release.tag_name),
    ) else {
        return UpdateState::Failed("Unrecognized release version. Retry the check.");
    };
    let Ok(current) = Version::parse(current) else {
        return UpdateState::Failed("Unrecognized installed version.");
    };
    if release.draft || release.prerelease || !version.pre.is_empty() {
        return UpdateState::Failed("No stable release information. Retry the check.");
    }
    if !version.cmp_precedence(&current).is_gt() {
        return UpdateState::UpToDate;
    }
    if !release
        .html_url
        .starts_with(&format!("{RELEASES_URL}/tag/"))
    {
        return UpdateState::Failed("Invalid release link. Retry the check.");
    }
    UpdateState::Available(ReleaseInfo {
        version: version.to_string(),
        notes: release.body.unwrap_or_default(),
        url: release.html_url,
    })
}

/// Blocking, bounded check; callers run it on the background executor.
#[cfg(feature = "ui")]
pub fn check_latest_release() -> UpdateState {
    let agent = ureq::Agent::config_builder()
        .https_only(true)
        .max_redirects(0)
        .timeout_global(Some(std::time::Duration::from_secs(15)))
        .http_status_as_error(false)
        .build()
        .new_agent();
    let response = agent
        .get(LATEST_API_URL)
        .header("User-Agent", concat!("Quill/", env!("CARGO_PKG_VERSION")))
        .header("Accept", "application/vnd.github+json")
        .call();
    let Ok(mut response) = response else {
        return UpdateState::Failed(
            "Could not reach the release server. Check your connection and retry.",
        );
    };
    let status = response.status().as_u16();
    if status != 200 {
        return interpret_release(status, &[], env!("CARGO_PKG_VERSION"));
    }
    let Ok(body) = response
        .body_mut()
        .with_config()
        .limit(MAX_RESPONSE_BYTES)
        .read_to_vec()
    else {
        return UpdateState::Failed("Could not read release information. Retry the check.");
    };
    interpret_release(status, &body, env!("CARGO_PKG_VERSION"))
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
