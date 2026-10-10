//! Screenshot demos: platform (connection state, unread badges, tray,
//! updates).

use crate::ui::app::QuillApp;
use crate::ui::demo::{
    seed_ready_offline_session, seed_ready_reconnecting_session, seed_ready_unread_read_session,
    seed_ready_unread_session,
};
use crate::ui::screenshot_demo::{DemoSpec, register_demos};

register_demos![
    // Slice parity:platform-offline-indicator — the ReadyChats fixture
    // with `connection` forced to `WaitingForNetwork`, so the offline
    // banner renders below the title bar (injected, no live Telegram).
    DemoSpec::ready(
        "ready-offline",
        seed_ready_offline_session,
        "screenshot demo — offline indicator (injected updates, no live Telegram)"
    ),
    // Slice parity:platform-offline-errors — ReadyOffline fixture plus
    // the product offline-send toast ("You're offline — will send when
    // you reconnect") so kit notifications proof the note.
    DemoSpec::ready(
        "ready-offline-toast",
        seed_ready_offline_session,
        "You're offline — will send when you reconnect"
    ),
    // Slice parity:platform-reconnect-states — the ReadyChats fixture
    // with `connection` forced to `Updating`, so the transitional strip
    // renders with its per-state label ("Updating…").
    DemoSpec::ready(
        "ready-reconnecting",
        seed_ready_reconnecting_session,
        "screenshot demo — reconnecting indicator (injected updates, no live Telegram)"
    ),
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
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum UpdateDemo {
    Install,
    Changelog,
    Failure,
}

impl QuillApp {
    /// Settings → Updates with a fake 0.2.0 release (no network).
    fn demo_update(&mut self, demo: UpdateDemo) {
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
        };
        self.settings.appearance_open = true;
    }
}
