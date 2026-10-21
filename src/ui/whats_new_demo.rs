//! The What's New screenshot demo: three releases, the newest installed.

use super::QuillApp;
use quill::release_notes::ReleaseNote;

const NEWEST: &str = "## Highlights

- **Linux on arm64.** Linux packages now ship for arm64 machines next to x86_64.
- **One-line installers for Linux and Windows.** Paste one command in a terminal or PowerShell to install Quill.
- **Drop files on a chat in the list.** Drag files onto any chat row to send them there without opening it first.

## More new features

- Times follow your system's 12-hour or 24-hour clock.
- Empty search shows your top people.
- Context menus work from the keyboard, with a visible focus ring.";

const OLDER: &str = "## Highlights

- **Faster launch.** Quill connects before the window opens.
- **Lower idle memory.** Caches that sat unused are released.

## Fixes

- Quitting no longer crashes while Telegram's background thread shuts down.";

fn note(version: &str, notes: &str, prerelease: bool, published: i64) -> ReleaseNote {
    ReleaseNote {
        version: version.into(),
        notes: notes.into(),
        url: format!("{}/tag/v{version}", quill::updater::RELEASES_URL),
        prerelease,
        published: Some(published),
    }
}

impl QuillApp {
    pub(crate) fn demo_release_notes(&mut self) {
        let now = quill::local_time::now_unix();
        self.use_release_notes(vec![
            note(
                "2099.1.1",
                "- **Settings in a full window**, with search.",
                false,
                now,
            ),
            note(quill::version::APP, NEWEST, false, now - 86_400),
            note("2026.1018.3075", OLDER, true, now - 4 * 86_400),
        ]);
    }
}

crate::ui::screenshot_demo::register_demos![
    crate::ui::screenshot_demo::DemoSpec::chat_list("ready-whats-new").setup(|app, _, cx| {
        app.demo_release_notes();
        app.open_whats_new(cx);
    }),
];
