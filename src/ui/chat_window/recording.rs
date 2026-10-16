//! A recording belongs to the window it was started in.
//!
//! One voice or video message records at a time (`RecordingUi` is the
//! app's, not a window's), started from one window's composer
//! (`RecordingUi::window`). Its record bar shows in that window, it is sent
//! to that window's chat (also when a video message reaches its time limit
//! and goes out by itself), its "recording…" chat action goes to that
//! chat, and it ends when that window closes.

use super::super::app::QuillApp;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::lang::{keys, tr};

impl QuillApp {
    /// A recording runs and belongs to the window whose state is in place:
    /// its record bar shows here, and Esc, a chat switch or the media
    /// panel here end it.
    pub(in crate::ui) fn recording_here(&self) -> bool {
        self.recording_active() && self.recording.window == self.windows.scope
    }

    /// Send the finished recording to the chat of the window it was
    /// recorded in, whichever window asks.
    pub(in crate::ui) fn send_recording(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let origin = self.recording.window;
        let sent = self.in_window_scope(origin, cx, |app, cx| app.send_recording_here(window, cx));
        if sent.is_none() {
            self.discard_recording_of_closed_window(cx);
        }
    }

    /// A video message reached its time limit (`check_recording`): it goes
    /// out at the next draw of the window it was recorded in, with that
    /// window's state in place. Called by every window's render.
    pub(in crate::ui) fn flush_recording_auto_send(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        if !self.recording.auto_send || self.recording.window != self.windows.scope {
            return;
        }
        self.recording.auto_send = false;
        self.defer_scoped(window, cx, |app, window, cx| app.send_recording(window, cx));
    }

    /// The recording's 100 ms tick, run as the window it was recorded in
    /// (`recording_tick`). `false` once it ended.
    pub(in crate::ui) fn recording_tick_in_window(&mut self, cx: &mut Context<Self>) -> bool {
        let origin = self.recording.window;
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
        if self.recording.window == Some(chat_id) {
            self.discard_recording_of_closed_window(cx);
        }
    }

    fn discard_recording_of_closed_window(&mut self, cx: &mut Context<Self>) {
        self.recording.window = None;
        if !self.recording_active() {
            return;
        }
        self.cancel_recording(cx);
        self.connection.status_note = tr(keys::Q_MISC_RECORDING_DISCARDED_ITS_WINDOW_CLOSED).into();
        cx.notify();
    }
}
