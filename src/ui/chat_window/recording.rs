//! A recording belongs to the window it was started in.
//!
//! One voice or video message records at a time (`RecordingUi` is the
//! app's, not a window's), started from one window's composer or the story
//! viewer. The capture carries that window (`Capture::origin`, set by
//! `hold_capture`), so it leaves with the capture when it is sent or
//! dropped. Its record bar shows in that window, it is sent to that
//! window's chat (also when a video message reaches its time limit and goes
//! out by itself), its "recording…" chat action goes to that chat, and it
//! ends when that window closes.

use super::super::app::QuillApp;
use crate::ui::status_note::StatusNote;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::lang::{keys, tr};

impl QuillApp {
    /// A recording runs and belongs to the window whose state is in place:
    /// its record bar shows here, and Esc, a chat switch or the media
    /// panel here end it.
    pub(in crate::ui) fn recording_here(&self) -> bool {
        self.recording.origin() == Some(self.windows.scope)
    }

    /// Leaving this window's chat or opening its media panel ends what
    /// records here, and a capture still starting here.
    pub(in crate::ui) fn cancel_recording_here(&mut self, cx: &mut Context<Self>) {
        let opening_here = self.recording.opening_origin() == Some(self.windows.scope);
        if self.recording_here() || opening_here {
            self.cancel_recording(cx);
        }
    }

    /// Send the finished recording to the chat of the window it was
    /// recorded in, whichever window asks.
    pub(in crate::ui) fn send_recording(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(origin) = self.recording.origin() else {
            return;
        };
        let sent = self.in_window_scope(origin, cx, |app, cx| app.send_recording_here(window, cx));
        if sent.is_none() {
            self.discard_recording_of_closed_window(cx);
        }
    }

    /// A video message reached its time limit (`check_recording`): send it
    /// now through the window it was recorded in. GPUI does not draw a
    /// covered or minimized window, so this does not wait for a draw.
    pub(in crate::ui) fn send_recording_at_time_limit(&mut self, cx: &mut Context<Self>) {
        let Some(origin) = self.recording.origin() else {
            return;
        };
        let handle = match origin {
            Some(chat_id) => self.windows.window_for(chat_id).map(|open| open.handle),
            None => self.windows.main,
        };
        let Some(handle) = handle else {
            self.discard_recording_of_closed_window(cx);
            return;
        };
        let app = cx.weak_entity();
        // Deferred: updating the window needs the app entity, leased now.
        // window-scope: send_recording puts the recording's window in place
        cx.defer(move |cx| {
            let _ = handle.update(cx, |_, window, cx| {
                app.update(cx, |app, cx| app.send_recording(window, cx))
            });
        });
    }

    /// The recording's 100 ms tick, run as the window it was recorded in
    /// (`recording_tick`). `false` once it ended.
    pub(in crate::ui) fn recording_tick_in_window(&mut self, cx: &mut Context<Self>) -> bool {
        let origin = self.recording.origin().flatten();
        match self.in_window_scope(origin, cx, Self::recording_tick) {
            Some(running) => running,
            None => {
                self.discard_recording_of_closed_window(cx);
                false
            }
        }
    }

    /// `chat_id`'s window closed: a recording made there ends with it.
    pub(super) fn end_recording_of_window(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        if self.recording.origin() == Some(Some(chat_id)) {
            self.discard_recording_of_closed_window(cx);
        }
    }

    fn discard_recording_of_closed_window(&mut self, cx: &mut Context<Self>) {
        if !self.recording_active() {
            return;
        }
        self.cancel_recording(cx);
        self.connection.set_note(StatusNote::silent(tr(
            keys::Q_MISC_RECORDING_DISCARDED_ITS_WINDOW_CLOSED,
        )));
        cx.notify();
    }
}
