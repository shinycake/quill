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
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum UpdateDemo {
    Install,
    Staged,
    Changelog,
    Failure,
}

impl QuillApp {
    /// Settings → Updates with a fake 0.2.0 release (no network).
    fn demo_update(&mut self, demo: UpdateDemo) {
        // The demo window is not an installed copy: show the update switches
        // as on an install that can replace itself, or with
        // `QUILL_DEMO_SELF_UPDATE=0` as on one that cannot.
        // `QUILL_DEMO_AUTO_UPDATE=1` starts with "Install updates
        // automatically" on.
        #[cfg(feature = "demo-capture")]
        {
            let supported = std::env::var("QUILL_DEMO_SELF_UPDATE")
                .ok()
                .is_none_or(|v| v != "0");
            quill::update_install::force_can_install_here(Some(supported));
            quill::advanced::auto_update::set_install_automatically(
                std::env::var("QUILL_DEMO_AUTO_UPDATE").is_ok_and(|v| v == "1"),
            );
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
                size: 120,
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
            UpdateDemo::Staged => {
                quill::updater::UpdateState::Staged(release, "update-plan.json".into())
            }
        };
        self.settings.appearance_open = true;
    }
}
