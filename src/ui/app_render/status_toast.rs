//! The status toast: shows the current status note for a few seconds. Which
//! notes toast, and for how long, comes from the note's kind
//! (`ui/status_note.rs`), not from its words. The queue and the timers are
//! state (`ui/toast_queue.rs`); this file only wires them to the frame.

use super::*;
use crate::ui::status_note::{NoteKind, StatusNote};
use gpui_kit::TestSupportExt;
use quill::ids::ChatId;

/// The pill's widest line, so a long failure wraps instead of stretching.
const TOAST_MAX_WIDTH: f32 = 440.;
/// Longest toast text, in lines (tdesktop's multiline toast wraps too).
const TOAST_MAX_LINES: usize = 3;
/// Clear space between the toast and the composer's top border.
const TOAST_GAP: f32 = 16.;

/// Space the toast keeps from the window's left, right and bottom edges.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct ToastInsets {
    pub(super) left: Pixels,
    pub(super) right: Pixels,
    pub(super) bottom: Pixels,
}

/// With the media viewer open: no side insets, [`TOAST_GAP`] above its
/// bottom bar (`bar` tall).
pub(super) fn viewer_toast_insets(bar: Pixels) -> ToastInsets {
    ToastInsets {
        left: px(0.),
        right: px(0.),
        bottom: bar + px(TOAST_GAP),
    }
}

impl QuillApp {
    /// The toast on screen this frame, if any. Advances the queue's timers
    /// and schedules the re-render for the next deadline; a paused (hovered)
    /// toast has none, so it stays until the pointer leaves or it is
    /// clicked away. Without a live session the footer line carries the
    /// note instead, so nothing toasts.
    pub(super) fn status_toast_visible(&mut self, cx: &mut Context<Self>) -> Option<StatusNote> {
        self.drain_suggested_post_notes(cx);
        // `QUILL_TRACE_STATUS=1`: print every status note (toasted or not)
        // to stderr, for diagnosing a live session.
        if self.connection.status_note() != self.connection.status_traced {
            self.connection.status_traced = self.connection.status_note().to_string();
            if !self.connection.status_traced.is_empty()
                && std::env::var_os("QUILL_TRACE_STATUS").is_some()
            {
                eprintln!("status: {}", self.connection.status_traced);
            }
        }
        // Before a session exists the footer status line shows the note and
        // keeps showing it (a blocker stays until it is resolved), so it is
        // not toasted a second time.
        if self.live.is_none() && !self.demo_pins_toast() {
            self.connection.drop_toasts();
            return None;
        }
        let now = std::time::Instant::now();
        if !self.demo_pins_toast() {
            self.connection.advance_toasts(now);
        }
        let deadline = self.connection.toasts.deadline(now);
        if deadline != self.connection.toast_wake {
            self.connection.toast_wake = deadline;
            if let Some(deadline) = deadline {
                cx.spawn(async move |this, cx| {
                    cx.background_executor()
                        .timer(deadline.saturating_duration_since(std::time::Instant::now()))
                        .await;
                    let _ = this.update(cx, |this, cx| {
                        if this.connection.toast_wake == Some(deadline) {
                            this.connection.toast_wake = None;
                        }
                        cx.notify();
                    });
                })
                .detach();
            }
        }
        self.connection.toasts.visible().cloned()
    }

    /// The toast `host`'s window shows this frame (`None`: the main
    /// window). It shows in the window the last input happened in, where a
    /// kit dialog would open, so a note answers the window that asked.
    pub(in crate::ui) fn status_toast_in(
        &mut self,
        host: Option<ChatId>,
        cx: &mut Context<Self>,
    ) -> Option<StatusNote> {
        let note = self.status_toast_visible(cx)?;
        (self.windows.last_input == host).then_some(note)
    }

    /// A chat window's toast. Put in the box around its compose field, it
    /// sits [`TOAST_GAP`] above that box, centred over the conversation;
    /// with the media viewer open, put in the window's body, it sits above
    /// the viewer's bar.
    pub(in crate::ui) fn chat_window_toast(
        &mut self,
        note: &StatusNote,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        if self.viewer.state.is_open() {
            let insets = self.toast_insets(window, cx);
            return self.status_toast_layer(note, insets, cx);
        }
        let insets = ToastInsets {
            left: px(0.),
            right: px(0.),
            bottom: px(TOAST_GAP),
        };
        div()
            .absolute()
            .left_0()
            .right_0()
            .bottom(relative(1.))
            .child(self.status_toast_layer(note, insets, cx))
    }

    /// How far the toast sits above the window's bottom edge: clear of the
    /// composer by [`TOAST_GAP`], measured from the composer's laid-out
    /// height (a taller composer, with a reply or attachments, lifts it). A toast only shows with a session, when the footer
    /// line is absent (demos that pin the toast hide it).
    pub(super) fn toast_bottom(&self) -> Pixels {
        let composer = if self.session().is_some_and(|s| s.open_chat.is_some()) {
            self.slices.composer_height()
        } else {
            px(0.)
        };
        composer + px(TOAST_GAP)
    }

    /// The toast's insets this frame: over the conversation pane above the
    /// composer, or, with the media viewer open (it covers the whole
    /// window), centred on the window above the viewer's bottom bar.
    pub(super) fn toast_insets(&mut self, window: &mut Window, cx: &App) -> ToastInsets {
        if self.viewer.state.is_open() {
            let bar = self.viewer_bottom_chrome(window, cx);
            return viewer_toast_insets(bar);
        }
        let (left, right) = self.chat_insets();
        ToastInsets {
            left,
            right,
            bottom: self.toast_bottom(),
        }
    }

    /// The toast, centred over the conversation pane (`left` and `right`
    /// are the space the other columns take). Hovering pauses its timer, a
    /// click dismisses it; failures are announced as alerts.
    pub(super) fn status_toast_layer(
        &self,
        note: &StatusNote,
        insets: ToastInsets,
        cx: &mut Context<Self>,
    ) -> Div {
        let ToastInsets {
            left,
            right,
            bottom,
        } = insets;
        let error = note.kind() == NoteKind::Error;
        let role = if error { Role::Alert } else { Role::Status };
        div()
            .absolute()
            .left(left)
            .right(right)
            .bottom(bottom)
            .px_4()
            .flex()
            .justify_center()
            .child(
                div()
                    .id("status-toast")
                    .test_support()
                    .role(role)
                    .aria_label(note.text().to_string())
                    .cursor_pointer()
                    .max_w(px(TOAST_MAX_WIDTH))
                    .px_3()
                    .py_1p5()
                    .rounded_xl()
                    .border_1()
                    .border_color(if error {
                        cx.theme().danger
                    } else {
                        cx.theme().border
                    })
                    .bg(cx.theme().popover)
                    .text_color(cx.theme().popover_foreground)
                    .shadow_md()
                    .text_sm()
                    .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                        this.hover_status_toast(*hovered, cx);
                    }))
                    .on_click(cx.listener(|this, _, _, cx| this.dismiss_status_toast(cx)))
                    .child(
                        div()
                            .line_clamp(TOAST_MAX_LINES)
                            .child(note.text().to_string()),
                    ),
            )
    }

    /// Pointer over the toast pauses its timer; leaving resumes it.
    fn hover_status_toast(&mut self, hovered: bool, cx: &mut Context<Self>) {
        self.connection
            .toasts
            .set_hovered(hovered, std::time::Instant::now());
        cx.notify();
    }

    /// A click on the toast sends it away and brings the next one; a
    /// "saved" toast first shows its file in the file manager.
    fn dismiss_status_toast(&mut self, cx: &mut Context<Self>) {
        let reveal = self
            .connection
            .toasts
            .visible()
            .and_then(|note| note.reveal().map(std::path::Path::to_path_buf));
        if let Some(path) = reveal {
            self.reveal_in_folder(&path, cx);
        }
        self.connection.toasts.dismiss(std::time::Instant::now());
        self.connection.toast_wake = None;
        cx.notify();
    }

    /// Screenshot demos have no live connection, so no toast; with
    /// `QUILL_DEMO_SHOW_TOAST=1` the demo's toasts stay up (their timers do
    /// not run), for the captures that prove how a toast looks.
    #[cfg(feature = "demo-capture")]
    pub(super) fn demo_pins_toast(&self) -> bool {
        self.live.is_none()
            && self.demo_session.is_some()
            && (std::env::var_os("QUILL_DEMO_SHOW_TOAST").is_some() || pinned_for_tests())
    }

    #[cfg(not(feature = "demo-capture"))]
    pub(super) fn demo_pins_toast(&self) -> bool {
        false
    }
}

#[cfg(test)]
thread_local! {
    static PIN_FOR_TESTS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Tests on this thread keep the demo's toasts up, as
/// `QUILL_DEMO_SHOW_TOAST=1` does, without touching the environment.
#[cfg(test)]
pub(in crate::ui) fn pin_toasts_for_tests() {
    PIN_FOR_TESTS.with(|pin| pin.set(true));
}

#[cfg(feature = "demo-capture")]
fn pinned_for_tests() -> bool {
    #[cfg(test)]
    return PIN_FOR_TESTS.with(std::cell::Cell::get);
    #[cfg(not(test))]
    false
}

#[cfg(test)]
mod tests {
    use super::{TOAST_GAP, ToastInsets, viewer_toast_insets};
    use gpui_kit::px;

    #[test]
    fn with_the_viewer_open_the_toast_centres_on_the_window_above_its_bar() {
        let insets = viewer_toast_insets(px(96.));
        assert_eq!(
            insets,
            ToastInsets {
                left: px(0.),
                right: px(0.),
                bottom: px(96. + TOAST_GAP),
            }
        );
    }

    #[test]
    fn a_caption_lifts_the_toast_with_the_bar() {
        let plain = viewer_toast_insets(px(96.));
        let captioned = viewer_toast_insets(px(96. + 40. + 8.));
        assert_eq!(captioned.bottom - plain.bottom, px(48.));
    }
}
