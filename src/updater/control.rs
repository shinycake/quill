//! The one update button, derived from the update state on every render.
//!
//! tdesktop has a single control that changes with the updater: Settings'
//! "Check for updates" becomes "Update Telegram" once the update is
//! downloaded, and the chat list grows a full-width "Update Telegram" button
//! at its bottom (`Dialogs::Widget::checkUpdateStatus`).
//! Quill downloads a new release as soon as a check finds it, as tdesktop
//! does by default ([`UpdateControl::auto_download`]); the button offers
//! Update Quill only when that has not started (a failed download), shows
//! the download, then Restart to update.

use super::{DownloadProgress, ReleaseInfo, UpdateState, release_size};
use crate::update_install::Unavailable;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateControl {
    /// No release to act on: check for one. `busy` while a check runs.
    Check { busy: bool },
    /// A newer release this installation can download and install.
    Install(ReleaseInfo),
    /// The package is downloading and being verified.
    Downloading {
        progress: DownloadProgress,
        total: u64,
    },
    /// Downloaded and verified: restarting installs it.
    Restart,
    /// The helper is installing it and Quill is quitting.
    Restarting,
    /// A newer release this copy cannot install over itself (it runs from a
    /// disk image, a read-only or foreign folder): its download page.
    OpenRelease(String),
}

impl UpdateControl {
    /// The control for `state` on an installation whose self-update answer
    /// is `availability` (`update_install::self_update_availability`).
    pub fn of(state: &UpdateState, availability: Result<(), Unavailable>) -> Self {
        match state {
            UpdateState::Checking => Self::Check { busy: true },
            UpdateState::Available(release)
            | UpdateState::DownloadFailed(release, _)
            | UpdateState::InstallFailed(release, _) => match availability {
                Ok(()) if release.asset.is_some() => Self::Install(release.clone()),
                Ok(()) => Self::OpenRelease(release.url.clone()),
                // A package manager updates it, a source build is updated
                // by hand, and the startup answer is still coming.
                Err(Unavailable::PackageManager | Unavailable::NotPackaged)
                | Err(Unavailable::Checking) => Self::Check { busy: false },
                Err(_) => Self::OpenRelease(release.url.clone()),
            },
            UpdateState::Downloading(release, progress) => Self::Downloading {
                progress: progress.clone(),
                total: release_size(release),
            },
            UpdateState::Staged(..) => Self::Restart,
            UpdateState::Installing(_) => Self::Restarting,
            UpdateState::Idle
            | UpdateState::Installed(_)
            | UpdateState::UpToDate
            | UpdateState::NoRelease
            | UpdateState::Failed(_) => Self::Check { busy: false },
        }
    }

    /// The release to download without a click: a newer release a check
    /// just found, when this copy can install it. A failed download waits
    /// for the button instead of retrying on its own.
    pub fn auto_download(
        state: &UpdateState,
        availability: Result<(), Unavailable>,
    ) -> Option<ReleaseInfo> {
        let UpdateState::Available(release) = state else {
            return None;
        };
        (availability.is_ok() && crate::update_install::valid_release(release))
            .then(|| release.clone())
    }

    /// Whether the title bar shows it: only while there is an update to act
    /// on, as tdesktop's "Update Telegram" button under the chat list.
    pub fn in_title_bar(&self) -> bool {
        !matches!(self, Self::Check { .. })
    }

    /// The share of the package downloaded, 0 to 1.
    pub fn fraction(&self) -> Option<f32> {
        let Self::Downloading { progress, total } = self else {
            return None;
        };
        Some(if *total == 0 {
            0.0
        } else {
            (progress.bytes().min(*total) as f64 / *total as f64) as f32
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::updater::{RELEASES_URL, ReleaseAsset, update_asset_name};

    fn release(with_asset: bool) -> ReleaseInfo {
        ReleaseInfo {
            version: "999.0.0".into(),
            notes: String::new(),
            url: format!("{RELEASES_URL}/tag/v999.0.0"),
            asset: with_asset.then(|| ReleaseAsset {
                url: format!("{RELEASES_URL}/download/v999.0.0/{}", update_asset_name()),
                sha256: "a".repeat(64),
                size: 200,
            }),
        }
    }

    #[test]
    fn the_button_walks_from_check_to_install_to_restart() {
        let found = UpdateState::Available(release(true));
        assert_eq!(
            UpdateControl::of(&UpdateState::Idle, Ok(())),
            UpdateControl::Check { busy: false }
        );
        assert_eq!(
            UpdateControl::of(&UpdateState::Checking, Ok(())),
            UpdateControl::Check { busy: true }
        );
        assert_eq!(
            UpdateControl::of(&found, Ok(())),
            UpdateControl::Install(release(true))
        );
        let progress = DownloadProgress::default();
        progress.add(50);
        let downloading = UpdateControl::of(
            &UpdateState::Downloading(release(true), progress.clone()),
            Ok(()),
        );
        assert_eq!(downloading.fraction(), Some(0.25));
        progress.add(1000);
        assert_eq!(downloading.fraction(), Some(1.0));
        assert_eq!(
            UpdateControl::of(
                &UpdateState::Staged(release(true), "plan.json".into()),
                Ok(())
            ),
            UpdateControl::Restart
        );
        assert_eq!(
            UpdateControl::of(&UpdateState::Installing(release(true)), Ok(())),
            UpdateControl::Restarting
        );
    }

    #[test]
    fn a_failed_download_offers_the_install_again() {
        let failed = UpdateState::DownloadFailed(release(true), "x");
        assert_eq!(
            UpdateControl::of(&failed, Ok(())),
            UpdateControl::Install(release(true))
        );
    }

    #[test]
    fn a_copy_that_cannot_replace_itself_links_the_release_or_stays_quiet() {
        let found = UpdateState::Available(release(true));
        let page = UpdateControl::OpenRelease(release(true).url);
        assert_eq!(UpdateControl::of(&found, Err(Unavailable::DiskImage)), page);
        assert_eq!(
            UpdateControl::of(&UpdateState::Available(release(false)), Ok(())),
            page
        );
        for quiet in [
            Unavailable::PackageManager,
            Unavailable::NotPackaged,
            Unavailable::Checking,
        ] {
            let control = UpdateControl::of(&found, Err(quiet));
            assert!(!control.in_title_bar(), "{quiet:?}");
        }
    }

    #[test]
    fn the_title_bar_shows_it_only_with_an_update_to_act_on() {
        assert!(!UpdateControl::of(&UpdateState::UpToDate, Ok(())).in_title_bar());
        assert!(!UpdateControl::of(&UpdateState::Checking, Ok(())).in_title_bar());
        assert!(UpdateControl::of(&UpdateState::Available(release(true)), Ok(())).in_title_bar());
        assert!(
            UpdateControl::of(
                &UpdateState::Staged(release(true), "plan.json".into()),
                Ok(())
            )
            .in_title_bar()
        );
    }

    #[test]
    fn a_found_release_downloads_on_its_own_when_this_copy_can_install_it() {
        let found = UpdateState::Available(release(true));
        assert_eq!(
            UpdateControl::auto_download(&found, Ok(())),
            Some(release(true))
        );
        assert_eq!(
            UpdateControl::auto_download(&found, Err(Unavailable::PackageManager)),
            None
        );
        // No package for this platform, or not newer: nothing to download.
        assert_eq!(
            UpdateControl::auto_download(&UpdateState::Available(release(false)), Ok(())),
            None
        );
        let mut old = release(true);
        old.version = "0.0.1".into();
        old.url = format!("{RELEASES_URL}/tag/v0.0.1");
        assert_eq!(
            UpdateControl::auto_download(&UpdateState::Available(old), Ok(())),
            None
        );
        for other in [
            UpdateState::UpToDate,
            UpdateState::Checking,
            UpdateState::DownloadFailed(release(true), "x"),
            UpdateState::Staged(release(true), "plan.json".into()),
            UpdateState::Downloading(release(true), DownloadProgress::default()),
        ] {
            assert_eq!(UpdateControl::auto_download(&other, Ok(())), None);
        }
    }

    #[test]
    fn progress_reads_like_tdesktop() {
        assert_eq!(super::super::progress_text(0, 0), "0.0 / 0.0 MB");
        assert_eq!(
            super::super::progress_text(12 * 1024 * 1024 + 400_000, 45 * 1024 * 1024),
            "12.3 / 45.0 MB"
        );
        // A server sending more than promised never reads past the total.
        assert_eq!(
            super::super::progress_text(9 * 1024 * 1024, 1024 * 1024),
            "1.0 / 1.0 MB"
        );
    }
}
