#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::message_media::format_bytes;
use super::status_note::StatusNote;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::alert::Alert;
use gpui_kit::{Context, IntoElement as _, Styled as _};
use quill::auth::{AuthAction, AuthView};
use quill::disk_guard::{DiskState, FLOORS, available_bytes, next_read_in};
use quill::lang::{keys, tr, tr_with};
use std::path::PathBuf;

#[derive(Default)]
pub(crate) struct DiskUi {
    state: DiskState,
    free: Option<u64>,
}

impl DiskUi {
    pub(crate) fn start(cx: &mut Context<QuillApp>) -> Self {
        if let Some(root) = quill::settings::safe_app_root() {
            watch(root, cx);
        }
        Self::default()
    }

    pub(super) fn is_full(&self) -> bool {
        self.state == DiskState::Full
    }

    pub(super) fn refused_open(&mut self) {
        self.state = DiskState::Full;
    }
}

fn watch(root: PathBuf, cx: &Context<QuillApp>) {
    QuillApp::spawn_main(cx, async move |this, cx| {
        loop {
            let folder = root.clone();
            let free = cx
                .background_executor()
                .spawn(async move { available_bytes(&folder) })
                .await;
            if this
                .update(cx, |this, cx| this.apply_disk_reading(free, cx))
                .is_err()
            {
                break;
            }
            cx.background_executor()
                .timer(next_read_in(free, &FLOORS))
                .await;
        }
    })
    .detach();
}

impl QuillApp {
    pub(super) fn apply_disk_reading(&mut self, free: Option<u64>, cx: &mut Context<Self>) {
        let before = self.disk.state;
        let shown = self.disk.free;
        let after = before.next(free, &FLOORS);
        self.disk.state = after;
        self.disk.free = free.or(shown);
        let mut running = false;
        let background = self.background.clients_mut();
        for live in self.live.iter_mut().chain(background) {
            running = true;
            let _ = live.driver.note_disk(after, free, &FLOORS);
        }
        if after == DiskState::Full {
            if running {
                self.close_clients_for_full_disk(cx);
            }
        } else if before == DiskState::Full {
            if self.account.passcode.locked {
                self.account.passcode.deferred_connect = true;
            } else {
                self.start_connection(cx);
            }
        }
        if before != after || (after == DiskState::Full && self.disk.free != shown) {
            cx.notify();
        }
    }

    fn close_clients_for_full_disk(&mut self, cx: &mut Context<Self>) {
        self.flush_leaving_draft(cx);
        self.close_chat_windows_for_switch(cx);
        self.close_every_client_later(cx);
        self.connection.set_note(StatusNote::silent(""));
    }

    pub(super) fn disk_full_view(&self) -> Option<AuthView> {
        self.disk.is_full().then(|| AuthView {
            title: tr(keys::Q_DISK_FULL_TITLE),
            body: tr_with(
                keys::Q_DISK_FULL_BODY,
                &[
                    ("free", &disk_size(self.disk.free.unwrap_or(0))),
                    ("needed", &disk_size(FLOORS.open)),
                ],
            ),
            action: AuthAction::ProvideParameters,
            blocking: true,
        })
    }

    pub(super) fn disk_low_banner(&self, cx: &Context<Self>) -> Option<gpui_kit::AnyElement> {
        (self.disk.state == DiskState::Low).then(|| {
            Alert::warning("disk-low", tr(keys::Q_DISK_ALMOST_FULL))
                .text_color(cx.theme().foreground)
                .banner()
                .into_any_element()
        })
    }
}

fn disk_size(bytes: u64) -> String {
    format_bytes(i64::try_from(bytes).unwrap_or(i64::MAX))
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "disk_state_tests.rs"]
mod tests;
