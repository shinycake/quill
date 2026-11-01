#![deny(clippy::indexing_slicing)]

use super::app::{PaneMode, QuillApp};
use super::message_media::format_bytes;
use super::status_note::StatusNote;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::alert::Alert;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{Sizable as _, h_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyWindowHandle, Context, IntoElement as _, ParentElement as _, Styled as _, Window,
    transparent_black,
};
use quill::auth::{AuthAction, AuthView};
use quill::disk_guard::{
    CONFIRM_READ, DataReading, DiskClients, DiskFloors, DiskState, DiskStep, FLOORS, data_floors,
    data_root_device, fall_per_sec, next_read_in, read_data,
};
use quill::lang::{keys, tr, tr_with};
use std::path::PathBuf;
use std::time::Duration;

pub(crate) struct DiskUi {
    state: DiskState,
    free: Option<u64>,
    floors: DiskFloors,
    /// The last reading was below close and the next one decides.
    confirming: bool,
    /// The wait before the last reading, to measure how fast the disk falls.
    waited: Duration,
    /// The last reading found a data folder TDLib cannot write.
    unwritable: bool,
    /// The on-screen client was refused below the open floor. The screen
    /// says so, while clients already running keep going until a reading
    /// falls below close.
    screen_refused: bool,
    /// At Full, the on-screen client stays open for its call, and closes
    /// at the first reading after the call ends.
    held_for_call: bool,
    /// How many times this run the full-disk screen went up, each with one
    /// system notification.
    full_notices: u32,
    /// The on-screen account's cached files, while that folder exists.
    cache_folder: Option<PathBuf>,
    /// The on-screen account's message database, for Manage local storage.
    pub(super) database_bytes: Option<u64>,
    main_window: AnyWindowHandle,
    start_refused: bool,
}

impl DiskUi {
    pub(crate) fn start(window: &Window, cx: &mut Context<QuillApp>) -> Self {
        // A UI test feeds its readings through `apply_disk_reading`: a read
        // of the machine's own disk would make it pass or fail by how full
        // that disk is.
        if !cfg!(test)
            && let Some(root) = quill::settings::safe_app_root()
        {
            watch(root, cx);
        }
        Self {
            state: DiskState::default(),
            free: None,
            floors: FLOORS,
            confirming: false,
            waited: Duration::ZERO,
            unwritable: false,
            screen_refused: false,
            held_for_call: false,
            full_notices: 0,
            cache_folder: None,
            database_bytes: None,
            main_window: window.window_handle(),
            start_refused: false,
        }
    }

    pub(super) fn is_full(&self) -> bool {
        self.state == DiskState::Full
    }

    /// The full-disk screen is up: the disk is full, or the on-screen
    /// client was refused.
    pub(super) fn shows_screen(&self) -> bool {
        (self.is_full() && !self.held_for_call) || self.screen_refused
    }

    pub(super) fn holds_auto_downloads(&self) -> bool {
        self.state != DiskState::Healthy
    }

    /// A client made after the last reading starts out knowing it, so a
    /// low disk holds its automatic downloads from its first file.
    pub(super) fn seed<S: quill::connect::JsonSender>(
        &self,
        driver: &mut quill::connect::ConnectDriver<S>,
    ) {
        let _ = driver.note_disk(self.state, self.free, &self.floors);
    }

    pub(super) fn refused_start(&mut self) {
        self.start_refused = true;
    }
}

fn watch(root: PathBuf, cx: &Context<QuillApp>) {
    QuillApp::spawn_main(cx, async move |this, cx| {
        let mut device = None;
        loop {
            let folder = root.clone();
            let (reading, floors, seen_on, account) = cx
                .background_executor()
                .spawn(async move {
                    let reading = match demo_reading() {
                        Some(free) => DataReading {
                            free: Some(free),
                            unwritable: false,
                        },
                        None => read_data(&folder, device),
                    };
                    let floors = data_floors(&folder);
                    (
                        reading,
                        floors,
                        data_root_device(&folder),
                        account_files(&folder),
                    )
                })
                .await;
            device = seen_on.or(device);
            let Ok(wait) = this.update(cx, |this, cx| {
                (this.disk.cache_folder, this.disk.database_bytes) = account;
                this.apply_data_reading(reading, floors, cx)
            }) else {
                break;
            };
            cx.background_executor().timer(wait).await;
        }
    })
    .detach();
}

/// The on-screen account's cache folder, while it exists, and the size of
/// its message database.
fn account_files(root: &std::path::Path) -> (Option<PathBuf>, Option<u64>) {
    let active = quill::settings::active_account(root);
    let paths = quill::settings::AccountPaths::for_root(root, &active);
    let database = paths
        .database_exists()
        .then(|| quill::disk_guard::database_size(&paths.tdlib_database));
    let files = paths.tdlib_files;
    (files.is_dir().then_some(files), database)
}

/// `QUILL_DEMO_DISK_FREE_MB` stands in for the disk in a capture build. It
/// lives here, not in `disk_guard`, so the crash breadcrumb and the open
/// refusal only ever see the real disk.
fn demo_reading() -> Option<u64> {
    if !cfg!(feature = "demo-capture") {
        return None;
    }
    std::env::var("QUILL_DEMO_DISK_FREE_MB")
        .ok()?
        .parse::<u64>()
        .ok()?
        .checked_mul(1 << 20)
}

impl QuillApp {
    /// A reading with the floors for what the data folders hold now.
    /// Returns how long to wait for the next one.
    pub(super) fn apply_data_reading(
        &mut self,
        reading: DataReading,
        floors: DiskFloors,
        cx: &mut Context<Self>,
    ) -> Duration {
        self.disk.floors = floors;
        if reading.unwritable != self.disk.unwritable {
            self.disk.unwritable = reading.unwritable;
            cx.notify();
        }
        // TDLib can write nothing there: the same as no room at all.
        let free = if reading.unwritable {
            Some(0)
        } else {
            reading.free
        };
        let fall = fall_per_sec(self.disk.free, free, self.disk.waited);
        self.apply_disk_reading(free, cx);
        let wait = if self.disk.confirming {
            CONFIRM_READ
        } else {
            next_read_in(free, &floors, fall)
        };
        self.disk.waited = wait;
        wait
    }

    pub(super) fn apply_disk_reading(&mut self, free: Option<u64>, cx: &mut Context<Self>) {
        let shown = self.disk.free;
        let floors = self.disk.floors;
        let (after, confirming) = self
            .disk
            .state
            .confirmed(free, &floors, self.disk.confirming);
        self.disk.confirming = confirming;
        self.disk.free = free.or(shown);
        let mut running = false;
        let repaused = self
            .live
            .as_mut()
            .and_then(|live| live.driver.note_disk(after, free, &floors).ok())
            .unwrap_or(0);
        running |= self.live.is_some();
        for live in self.background.clients_mut() {
            running = true;
            let _ = live.driver.note_disk(after, free, &floors);
        }
        if repaused > 0 {
            self.connection
                .set_note(StatusNote::silent(tr(keys::Q_DISK_DOWNLOAD_NO_ROOM)));
        }
        let room = free.is_some_and(|free| free >= floors.open);
        let retry_starts = self.disk.start_refused && room;
        let retry_screen = self.disk.screen_refused && room && after != DiskState::Full;
        let before = self.enter_disk_state(after, cx);
        let step = before.step(
            after,
            DiskClients {
                running,
                retry_starts,
            },
        );
        match step {
            DiskStep::Hold => {}
            DiskStep::Close => self.close_clients_for_full_disk(cx),
            DiskStep::Reconnect => self.reconnect_after_disk(cx),
            DiskStep::RetryStarts => {
                self.disk.start_refused = false;
                self.ensure_background_accounts(cx);
            }
        }
        if retry_screen && step != DiskStep::Reconnect {
            self.reconnect_after_disk(cx);
        }
        if before != after || (self.disk.shows_screen() && self.disk.free != shown) {
            cx.notify();
        }
    }

    fn reconnect_after_disk(&mut self, cx: &mut Context<Self>) {
        self.disk.screen_refused = false;
        if self.account.passcode.locked {
            self.account.passcode.deferred_connect = true;
        } else {
            self.start_connection(cx);
        }
    }

    /// `prepare_connect` refused the on-screen client below the open floor.
    /// Only that client waits: the state stays with the readings, so other
    /// running clients close only when a reading falls below close.
    pub(super) fn refuse_screen_open(&mut self, cx: &mut Context<Self>) {
        if !self.disk.shows_screen() {
            self.announce_full_disk(cx);
        }
        self.disk.screen_refused = true;
        cx.notify();
    }

    fn enter_disk_state(&mut self, after: DiskState, cx: &mut Context<Self>) -> DiskState {
        let before = std::mem::replace(&mut self.disk.state, after);
        if after != DiskState::Full {
            self.disk.held_for_call = false;
        }
        if after == DiskState::Full && before != DiskState::Full && !self.disk.screen_refused {
            self.announce_full_disk(cx);
        }
        if after == DiskState::Healthy && before != DiskState::Healthy {
            self.auto_download_update(cx);
        }
        before
    }

    /// The screen goes up: Quill hidden in the tray would otherwise show
    /// nothing, so the main window comes forward and the system shows one
    /// notification.
    fn announce_full_disk(&mut self, cx: &mut Context<Self>) {
        self.bring_main_window_forward(cx);
        self.disk.full_notices += 1;
        let notice = full_disk_notice(self.disk.unwritable);
        let locked = self.account.passcode.locked;
        self.show_os_notification(notice, None, None, locked, cx);
    }

    fn bring_main_window_forward(&self, cx: &mut Context<Self>) {
        let main_window = self.disk.main_window;
        cx.defer(move |cx| {
            let _ = main_window.update(cx, |_, window, cx| {
                super::link_inbox::raise_main_window(window, cx);
            });
        });
    }

    fn close_clients_for_full_disk(&mut self, cx: &mut Context<Self>) {
        let held = self
            .live
            .as_ref()
            .is_some_and(|live| live.driver.call_in_progress());
        if held != self.disk.held_for_call {
            self.disk.held_for_call = held;
            cx.notify();
        }
        self.close_background_clients_later(cx);
        if held {
            return;
        }
        self.flush_leaving_draft(cx);
        self.close_chat_windows_for_switch(cx);
        self.close_screen_client(cx);
        self.connection.set_note(StatusNote::silent(""));
    }

    pub(super) fn disk_full_view(&self) -> Option<AuthView> {
        if !self.disk.shows_screen() {
            return None;
        }
        if self.disk.unwritable {
            return Some(AuthView {
                title: tr(keys::Q_DISK_UNWRITABLE_TITLE),
                body: tr(keys::Q_DISK_UNWRITABLE_BODY).to_string(),
                action: AuthAction::ProvideParameters,
                blocking: true,
            });
        }
        let needed = disk_size(self.disk.floors.open);
        let sizes = match self.disk.free {
            Some(free) => tr_with(
                keys::Q_DISK_FULL_SIZES,
                &[("free", &disk_size(free)), ("needed", &needed)],
            ),
            None => tr_with(keys::Q_DISK_FULL_NEEDED, &[("needed", &needed)]),
        };
        Some(AuthView {
            title: tr(keys::Q_DISK_FULL_TITLE),
            body: format!("{}\n\n{sizes}", tr(keys::Q_DISK_FULL_BODY)),
            action: AuthAction::ProvideParameters,
            blocking: true,
        })
    }

    /// On the full-disk screen, a link that shows the account's cached files
    /// in the file manager, so they can be removed there. Quill deletes
    /// nothing itself while TDLib is closed: TDLib keeps their records.
    pub(super) fn disk_full_reveal(&self) -> Option<gpui_kit::AnyElement> {
        if !self.disk.shows_screen() || self.disk.unwritable {
            return None;
        }
        let folder = self.disk.cache_folder.clone()?;
        Some(
            Button::new("disk-full-reveal")
                .label(tr(keys::Q_DISK_SHOW_CACHED_FILES))
                .link()
                .small()
                .on_click(move |_, _, _| {
                    let _ = quill::platform::reveal_in_file_manager(&folder);
                })
                .into_any_element(),
        )
    }

    /// The kit's banner has no action slot, so the row around it carries the
    /// tint and the button.
    pub(super) fn disk_low_banner(&self, cx: &Context<Self>) -> Option<gpui_kit::AnyElement> {
        let held = self.disk.is_full() && self.disk.held_for_call;
        let low = self.disk.state == DiskState::Low && !self.disk.shows_screen();
        (low || held).then(|| {
            let message = if held {
                tr(keys::Q_DISK_FULL_IN_CALL)
            } else {
                tr(keys::Q_DISK_ALMOST_FULL)
            };
            let settings_can_show = self.pane_mode() != PaneMode::Connecting;
            h_flex()
                .w_full()
                .flex_none()
                .pr_4()
                .bg(cx.theme().warning.opacity(0.1))
                .border_b_1()
                .border_color(cx.theme().border)
                .child(
                    Alert::warning("disk-low", message)
                        .text_color(cx.theme().foreground)
                        .banner()
                        .flex_1()
                        .min_w_0()
                        .bg(transparent_black())
                        .border_0(),
                )
                .when(settings_can_show, |this| {
                    this.child(
                        Button::new("disk-low-storage")
                            .label(tr(keys::SETTINGS_MANAGE_LOCAL_STORAGE))
                            .outline()
                            .small()
                            .on_click(cx.listener(|this, _, _, cx| this.open_local_storage(cx))),
                    )
                })
                .into_any_element()
        })
    }
}

/// The system notification for the full-disk screen.
pub(super) fn full_disk_notice(unwritable: bool) -> quill::notify::OsNotification {
    let (title, body) = if unwritable {
        (
            keys::Q_DISK_UNWRITABLE_TITLE,
            keys::Q_DISK_UNWRITABLE_NOTICE,
        )
    } else {
        (keys::Q_DISK_FULL_TITLE, keys::Q_DISK_FULL_NOTICE)
    };
    quill::notify::OsNotification::about_app(tr(title).to_string(), tr(body).to_string())
}

fn disk_size(bytes: u64) -> String {
    format_bytes(i64::try_from(bytes).unwrap_or(i64::MAX))
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "disk_state_tests.rs"]
mod tests;
