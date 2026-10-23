//! Screenshot demos: platform (connection state, unread badges, tray,
//! updates).
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use crate::ui::demo::{seed_ready_unread_read_session, seed_ready_unread_session};
use crate::ui::screenshot_demo::{DemoSpec, register_demos};

register_demos![
    DemoSpec::chat_list("ready-tray-behavior")
        .tray()
        .setup(|app, _, _| app.appearance.minimize_to_tray = true),
    DemoSpec::ready(
        "ready-unread",
        seed_ready_unread_session,
        "screenshot demo — unread badge (injected updates, no live Telegram)"
    ),
    DemoSpec::ready(
        "ready-unread-read",
        seed_ready_unread_read_session,
        "screenshot demo — after mark-read (injected updates, no live Telegram)"
    ),
    DemoSpec::chat_list("ready-update-changelog")
        .setup(|app, _, _| app.demo_update(UpdateDemo::Changelog)),
    DemoSpec::chat_list("ready-update-failure")
        .setup(|app, _, _| app.demo_update(UpdateDemo::Failure)),
    DemoSpec::chat_list("ready-update-install")
        .setup(|app, _, _| app.demo_update(UpdateDemo::Install)),
    DemoSpec::chat_list("ready-update-staged")
        .setup(|app, _, _| app.demo_update(UpdateDemo::Staged)),
    DemoSpec::chat_list("ready-update-downloading")
        .setup(|app, _, _| app.demo_update(UpdateDemo::Downloading)),
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum UpdateDemo {
    Install,
    Downloading,
    Staged,
    Changelog,
    Failure,
}

impl QuillApp {
    /// Settings → Updates with a fake 0.2.0 release (no network).
    fn demo_update(&mut self, demo: UpdateDemo) {
        // The demo window is not an installed copy: show the update section
        // as on an install that can replace itself, or with
        // `QUILL_DEMO_SELF_UPDATE=<reason>` (`0`, `translocated`,
        // `disk-image`, `read-only`, `package-manager`, `not-writable`,
        // `owned-by-another`, `checking`) as on one that cannot, for that
        // reason.
        #[cfg(feature = "demo-capture")]
        {
            let availability = match std::env::var("QUILL_DEMO_SELF_UPDATE") {
                Ok(name) => {
                    quill::update_install::Unavailable::from_demo_name(&name).map_or(Ok(()), Err)
                }
                Err(_) => Ok(()),
            };
            quill::update_install::force_self_update_available(Some(availability));
        }
        let release = quill::updater::ReleaseInfo {
            version: "0.2.0".into(),
            notes: "Update complete: improved navigation, video playback and accessibility.".into(),
            url: format!("{}/tag/v0.2.0", quill::updater::RELEASES_URL),
            asset: Some(quill::updater::ReleaseAsset {
                url: format!(
                    "{}/download/v0.2.0/{}",
                    quill::updater::RELEASES_URL,
                    quill::updater::binary_asset_name()
                ),
                size: 48 * 1024 * 1024,
                sha256: "a".repeat(64),
            }),
        };
        self.settings.update_state = match demo {
            UpdateDemo::Changelog => quill::updater::UpdateState::Installed(release),
            UpdateDemo::Failure => quill::updater::UpdateState::DownloadFailed(
                release,
                "Download interrupted. Retry the update.",
            ),
            UpdateDemo::Install => quill::updater::UpdateState::Available(release),
            UpdateDemo::Downloading => {
                let progress = quill::updater::DownloadProgress::default();
                progress.add(19 * 1024 * 1024);
                quill::updater::UpdateState::Downloading(release, progress)
            }
            UpdateDemo::Staged => quill::updater::UpdateState::Staged(
                quill::updater::ReleaseInfo {
                    notes: "Improved navigation, video playback and accessibility.".into(),
                    ..release
                },
                "update-plan.json".into(),
            ),
        };
        // `QUILL_DEMO_UPDATE_IN=chats`: the chat list and its update button.
        if std::env::var("QUILL_DEMO_UPDATE_IN").is_ok_and(|v| v == "chats") {
            return;
        }
        self.open_settings_on(quill::settings_catalog::Page::System);
    }
}
