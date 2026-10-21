//! Quill's releases with their notes, newest first, for the What's New
//! window. Drafts and tags that are not versions (asset-only releases such
//! as the emoji packs) never show.
#![deny(clippy::indexing_slicing)]

use crate::local_time::CivilTime;
#[cfg(feature = "ui")]
use crate::updater::{Fetch, fetch_release_json};
use crate::updater::{GithubRelease, MAX_RESPONSE_BYTES, RELEASES_URL, release_version};
use semver::Version;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseNote {
    pub version: String,
    /// Markdown, as written on the release.
    pub notes: String,
    pub url: String,
    pub prerelease: bool,
    /// When it was published, Unix seconds.
    pub published: Option<i64>,
}

/// How a release relates to the running Quill.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Standing {
    Installed,
    Newer,
    Older,
}

impl ReleaseNote {
    pub fn standing(&self, current: &str) -> Standing {
        let (Ok(this), Ok(current)) = (Version::parse(&self.version), Version::parse(current))
        else {
            return Standing::Older;
        };
        match this.cmp_precedence(&current) {
            std::cmp::Ordering::Equal => Standing::Installed,
            std::cmp::Ordering::Greater => Standing::Newer,
            std::cmp::Ordering::Less => Standing::Older,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotesError {
    Unreachable,
    Unavailable,
    Invalid,
}

#[cfg(feature = "ui")]
/// The releases list (`RELEASES_API_URL`, or the update fixture's
/// `releases.json`). Blocking and bounded; run it off the UI thread.
pub fn fetch() -> Result<Vec<ReleaseNote>, NotesError> {
    match fetch_release_json(true) {
        Ok((status, body)) => parse(status, &body),
        Err(Fetch::Unreachable) => Err(NotesError::Unreachable),
        Err(Fetch::Unreadable) => Err(NotesError::Invalid),
    }
}

pub fn parse(status: u16, body: &[u8]) -> Result<Vec<ReleaseNote>, NotesError> {
    if status != 200 {
        return Err(NotesError::Unavailable);
    }
    if body.len() as u64 > MAX_RESPONSE_BYTES {
        return Err(NotesError::Invalid);
    }
    let releases: Vec<GithubRelease> =
        serde_json::from_slice(body).map_err(|_| NotesError::Invalid)?;
    let mut notes: Vec<(Version, ReleaseNote)> = releases
        .into_iter()
        .filter(|release| !release.draft)
        .filter(|release| {
            release
                .html_url
                .starts_with(&format!("{RELEASES_URL}/tag/"))
        })
        .filter_map(|release| {
            let version = release_version(&release.tag_name)?;
            let note = ReleaseNote {
                version: version.to_string(),
                notes: release.body.unwrap_or_default().trim().to_string(),
                url: release.html_url,
                prerelease: release.prerelease,
                published: release.published_at.as_deref().and_then(parse_timestamp),
            };
            Some((version, note))
        })
        .collect();
    notes.sort_by(|(a, _), (b, _)| b.cmp_precedence(a));
    Ok(notes.into_iter().map(|(_, note)| note).collect())
}

/// GitHub's `2026-10-21T00:42:52Z` as Unix seconds.
fn parse_timestamp(text: &str) -> Option<i64> {
    let (date, time) = text.strip_suffix('Z')?.split_once('T')?;
    let mut date = date.split('-').map(str::parse::<i64>);
    let mut time = time.split(':').map(str::parse::<i64>);
    let (year, month, day) = (date.next()?.ok()?, date.next()?.ok()?, date.next()?.ok()?);
    let (hour, minute, second) = (time.next()?.ok()?, time.next()?.ok()?, time.next()?.ok()?);
    let civil = CivilTime {
        year,
        month: u8::try_from(month).ok().filter(|m| (1..=12).contains(m))?,
        day: u8::try_from(day).ok().filter(|d| (1..=31).contains(d))?,
        hour: 0,
        minute: 0,
        weekday: 0,
    };
    Some(civil.day_number() * 86_400 + hour * 3_600 + minute * 60 + second)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn release(tag: &str, draft: bool, prerelease: bool) -> serde_json::Value {
        json!({
            "tag_name": tag,
            "html_url": format!("{RELEASES_URL}/tag/{tag}"),
            "body": format!("## {tag}\n- a change\n"),
            "draft": draft,
            "prerelease": prerelease,
            "published_at": "2026-10-21T00:42:52Z",
        })
    }

    #[test]
    fn releases_are_listed_newest_first_without_drafts_or_asset_tags() {
        let body = serde_json::to_vec(&json!([
            release("v2026.1017.2948", false, false),
            release("emoji-sets-v1", false, false),
            release("v2026.1021.3203", false, false),
            release("v2026.1022.3300", true, false),
            release("v2026.1018.3075", false, true),
        ]))
        .unwrap();
        let notes = parse(200, &body).unwrap();
        let versions: Vec<&str> = notes.iter().map(|n| n.version.as_str()).collect();
        assert_eq!(
            versions,
            ["2026.1021.3203", "2026.1018.3075", "2026.1017.2948"]
        );
        assert!(notes.get(1).is_some_and(|n| n.prerelease));
        assert_eq!(
            notes.first().map(|n| n.notes.as_str()),
            Some("## v2026.1021.3203\n- a change")
        );
    }

    #[test]
    fn a_release_off_the_quill_repository_is_dropped() {
        let mut bad = release("v9.0.0", false, false);
        bad["html_url"] = json!("https://example.com/v9.0.0");
        let body = serde_json::to_vec(&json!([bad])).unwrap();
        assert_eq!(parse(200, &body), Ok(Vec::new()));
    }

    #[test]
    fn errors_are_named() {
        assert_eq!(parse(500, b""), Err(NotesError::Unavailable));
        assert_eq!(parse(200, b"{"), Err(NotesError::Invalid));
    }

    #[test]
    fn the_publish_time_is_read_as_utc() {
        assert_eq!(parse_timestamp("1970-01-02T00:00:01Z"), Some(86_401));
        assert_eq!(parse_timestamp("2026-13-01T00:00:00Z"), None);
        assert_eq!(parse_timestamp("garbage"), None);
    }

    #[test]
    fn standing_compares_with_the_running_version() {
        let note = |v: &str| ReleaseNote {
            version: v.into(),
            notes: String::new(),
            url: String::new(),
            prerelease: false,
            published: None,
        };
        assert_eq!(note("1.2.0").standing("1.2.0"), Standing::Installed);
        assert_eq!(note("1.3.0").standing("1.2.0"), Standing::Newer);
        assert_eq!(note("1.1.0").standing("1.2.0"), Standing::Older);
    }
}
