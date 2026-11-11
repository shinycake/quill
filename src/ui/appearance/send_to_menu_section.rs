//! General settings: the Windows "Send to" menu entry
//! (`quill::send_to_menu`, tdesktop's "Place Telegram in \"Send to\" menu").
//!
//! The shell is read and written off the UI thread, once per stage: the
//! switch shows what was read, then what the click asked for while the
//! write runs. A click during a write waits its turn, so the last click
//! decides the shortcut.

use super::*;
use crate::ui::status_note::StatusNote;
use quill::lang::{keys, tr, tr_with};

/// Where the Send To switch stands. Every stage has a way out: the read
/// answers, the write answers (or fails and the truth is read again).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(in crate::ui) enum SendToMenu {
    /// Not read yet: the section draws and asks for it.
    #[default]
    Unread,
    /// The read is on its way.
    Reading,
    Known(bool),
    /// A write is on its way; `queued` is a click that came during it.
    Applying {
        want: bool,
        queued: Option<bool>,
    },
}

impl SendToMenu {
    fn shown(self) -> bool {
        match self {
            Self::Unread | Self::Reading => false,
            Self::Known(on) => on,
            Self::Applying { want, queued } => queued.unwrap_or(want),
        }
    }

    /// A click for `on`: the stage it leads to, and whether a write starts.
    fn click(self, on: bool) -> (Self, bool) {
        match self {
            Self::Known(current) if current != on => (
                Self::Applying {
                    want: on,
                    queued: None,
                },
                true,
            ),
            Self::Applying { want, .. } => (
                Self::Applying {
                    want,
                    queued: Some(on),
                },
                false,
            ),
            other => (other, false),
        }
    }

    /// The write of `want` ended with `ok`: the next stage, and a write to
    /// start when a click waited with another wish.
    fn written(self, want: bool, ok: bool) -> (Self, Option<bool>) {
        if !ok {
            // What the shell holds is unknown now: read it again.
            return (Self::Unread, None);
        }
        match self {
            Self::Applying {
                queued: Some(next), ..
            } if next != want => (
                Self::Applying {
                    want: next,
                    queued: None,
                },
                Some(next),
            ),
            _ => (Self::Known(want), None),
        }
    }
}

impl QuillApp {
    /// The switch that puts Quill in Explorer's "Send to" menu; none where
    /// there is no such menu (tdesktop shows the row on Windows only).
    pub(in crate::ui) fn general_send_to_section(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !quill::send_to_menu::supported() {
            return None;
        }
        let state = self.preferences.send_to;
        if state == SendToMenu::Unread {
            self.defer_screen(cx, |this, cx| this.read_send_to_menu(cx));
        }
        let control = Switch::new("general-send-to-switch")
            .checked(state.shown())
            .disabled(matches!(state, SendToMenu::Unread | SendToMenu::Reading))
            .accessibility_label(tr(keys::Q_PREFS_PLACE_QUILL_IN_THE_SEND))
            .on_click(cx.listener(|this, &on, _, cx| this.click_send_to_menu(on, cx)));
        let row = self.appearance_toggle_section(
            cx,
            tr(keys::Q_PREFS_PLACE_QUILL_IN_SEND_TO),
            tr(keys::Q_PREFS_RIGHT_CLICK_FILES_IN_EXPLORER),
            control.into_any_element(),
        );
        Some(self.setting_spot(&["system.send_to_menu"], row, cx))
    }

    /// Read the shortcut off the UI thread (`SHGetKnownFolderPath` and a
    /// file check block on the shell).
    fn read_send_to_menu(&mut self, cx: &mut Context<Self>) {
        if self.preferences.send_to != SendToMenu::Unread {
            return;
        }
        self.preferences.send_to = SendToMenu::Reading;
        // screen-scope: the Send To menu is the app's, not an account's.
        cx.spawn(async move |this, cx| {
            let on = cx
                .background_spawn(async move { quill::send_to_menu::is_enabled() })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.preferences.send_to == SendToMenu::Reading {
                    this.preferences.send_to = SendToMenu::Known(on);
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn click_send_to_menu(&mut self, on: bool, cx: &mut Context<Self>) {
        let (next, start) = self.preferences.send_to.click(on);
        self.preferences.send_to = next;
        if start {
            self.write_send_to_menu(on, cx);
        }
        cx.notify();
    }

    /// The shell writes or deletes a shortcut file: not on the UI thread.
    fn write_send_to_menu(&mut self, want: bool, cx: &mut Context<Self>) {
        // screen-scope: the Send To menu is the app's; its failure note goes to whichever account is on screen.
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move { quill::send_to_menu::set_enabled(want) })
                .await;
            let _ = this.update(cx, |this, cx| {
                let (next, again) = this.preferences.send_to.written(want, result.is_ok());
                this.preferences.send_to = next;
                if let Err(error) = &result {
                    this.set_note(StatusNote::error(tr_with(
                        keys::Q_PREFS_COULD_NOT_UPDATE_THE_SEND,
                        &[("error", &error.to_string())],
                    )));
                }
                if let Some(next) = again {
                    this.write_send_to_menu(next, cx);
                }
                cx.notify();
            });
        })
        .detach();
    }
}

#[cfg(test)]
mod tests {
    use super::SendToMenu::*;

    #[test]
    fn the_last_click_decides_and_writes_run_one_at_a_time() {
        let (stage, start) = Known(false).click(true);
        assert!(start);
        // A click during the write waits; the switch already shows it.
        let (stage, start) = stage.click(false);
        assert!(!start);
        assert!(!stage.shown());
        // The first write ends: the queued wish starts its own write.
        let (stage, again) = stage.written(true, true);
        assert_eq!(again, Some(false));
        let (stage, again) = stage.written(false, true);
        assert_eq!((stage, again), (Known(false), None));
    }

    #[test]
    fn a_failed_write_reads_the_shortcut_again() {
        let (stage, _) = Known(false).click(true);
        assert_eq!(stage.written(true, false), (Unread, None));
    }

    #[test]
    fn nothing_is_asked_before_the_read_answers() {
        assert_eq!(Reading.click(true), (Reading, false));
        assert_eq!(Known(true).click(true), (Known(true), false));
    }
}
