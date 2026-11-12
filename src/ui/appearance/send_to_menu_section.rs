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
/// answers, the write answers (or fails and the truth is read again), and
/// the task that does the work is held by the stage, so replacing the
/// stage cancels it.
#[derive(Default)]
pub(in crate::ui) enum SendToMenu {
    /// Not read yet: the section draws and asks for it.
    #[default]
    Unread,
    /// The read is on its way.
    Reading {
        _read: Task<()>,
    },
    Known(bool),
    /// A write is on its way; `queued` is a click that came during it.
    Applying {
        want: bool,
        queued: Option<bool>,
        _write: Task<()>,
    },
}

impl SendToMenu {
    fn is_unread(&self) -> bool {
        matches!(self, Self::Unread)
    }

    fn shown(&self) -> bool {
        match self {
            Self::Unread | Self::Reading { .. } => false,
            Self::Known(on) => *on,
            Self::Applying { want, queued, .. } => queued.unwrap_or(*want),
        }
    }

    /// A click for `on`: starts a write through `start` when the switch is
    /// settled with another value, queues the wish during a write.
    fn click(&mut self, on: bool, start: impl FnOnce(bool) -> Task<()>) {
        match self {
            Self::Known(current) if *current != on => {
                *self = Self::Applying {
                    want: on,
                    queued: None,
                    _write: start(on),
                };
            }
            Self::Applying { queued, .. } => *queued = Some(on),
            _ => {}
        }
    }

    /// The write of `want` ended with `ok`: the next stage, and a new write
    /// through `start` when a click waited with another wish.
    fn written(&mut self, want: bool, ok: bool, start: impl FnOnce(bool) -> Task<()>) {
        let Self::Applying { queued, .. } = self else {
            return;
        };
        if !ok {
            // What the shell holds is unknown now: read it again.
            *self = Self::Unread;
            return;
        }
        *self = match queued.take() {
            Some(next) if next != want => Self::Applying {
                want: next,
                queued: None,
                _write: start(next),
            },
            _ => Self::Known(want),
        };
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
        let state = &self.preferences.send_to;
        if state.is_unread() {
            self.defer_screen(cx, |this, cx| this.read_send_to_menu(cx));
        }
        let control = Switch::new("general-send-to-switch")
            .checked(state.shown())
            .disabled(matches!(
                state,
                SendToMenu::Unread | SendToMenu::Reading { .. }
            ))
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
        if !self.preferences.send_to.is_unread() {
            return;
        }
        // screen-scope: the Send To menu is the app's, not an account's.
        let read = cx.spawn(async move |this, cx| {
            let on = cx
                .background_spawn(async move { quill::send_to_menu::is_enabled() })
                .await;
            let _ = this.update(cx, |this, cx| {
                if matches!(this.preferences.send_to, SendToMenu::Reading { .. }) {
                    this.preferences.send_to = SendToMenu::Known(on);
                }
                cx.notify();
            });
        });
        self.preferences.send_to = SendToMenu::Reading { _read: read };
        cx.notify();
    }

    fn click_send_to_menu(&mut self, on: bool, cx: &mut Context<Self>) {
        let mut stage = std::mem::take(&mut self.preferences.send_to);
        stage.click(on, |want| Self::write_send_to_menu(want, cx));
        self.preferences.send_to = stage;
        cx.notify();
    }

    /// The shell writes or deletes a shortcut file: not on the UI thread.
    /// The returned task belongs to the `Applying` stage.
    fn write_send_to_menu(want: bool, cx: &mut Context<Self>) -> Task<()> {
        // screen-scope: the Send To menu is the app's; its failure note goes to whichever account is on screen.
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move { quill::send_to_menu::set_enabled(want) })
                .await;
            let _ = this.update(cx, |this, cx| {
                let mut stage = std::mem::take(&mut this.preferences.send_to);
                stage.written(want, result.is_ok(), |next| {
                    Self::write_send_to_menu(next, cx)
                });
                this.preferences.send_to = stage;
                if let Err(error) = &result {
                    this.set_note(StatusNote::error(tr_with(
                        keys::Q_PREFS_COULD_NOT_UPDATE_THE_SEND,
                        &[("error", &error.to_string())],
                    )));
                }
                cx.notify();
            });
        })
    }
}

#[cfg(test)]
mod tests {
    use super::SendToMenu::{self, *};
    use gpui_kit::Task;

    fn start(_: bool) -> Task<()> {
        Task::ready(())
    }

    fn known(on: bool) -> SendToMenu {
        Known(on)
    }

    #[test]
    fn the_last_click_decides_and_writes_run_one_at_a_time() {
        let mut stage = known(false);
        stage.click(true, start);
        assert!(matches!(stage, Applying { want: true, .. }));
        // A click during the write waits; the switch already shows it.
        let mut starts = Vec::new();
        stage.click(false, |w| {
            starts.push(w);
            Task::ready(())
        });
        assert!(starts.is_empty());
        assert!(!stage.shown());
        // The first write ends: the queued wish starts its own write.
        stage.written(true, true, |w| {
            starts.push(w);
            Task::ready(())
        });
        assert_eq!(starts, [false]);
        assert!(matches!(stage, Applying { want: false, .. }));
        stage.written(false, true, |w| {
            starts.push(w);
            Task::ready(())
        });
        assert_eq!(starts, [false]);
        assert!(matches!(stage, Known(false)));
    }

    #[test]
    fn a_failed_write_reads_the_shortcut_again() {
        let mut stage = known(false);
        stage.click(true, start);
        stage.written(true, false, start);
        assert!(matches!(stage, Unread));
    }

    #[test]
    fn nothing_is_asked_before_the_read_answers() {
        let mut stage = Reading {
            _read: Task::ready(()),
        };
        let mut asked = false;
        stage.click(true, |_| {
            asked = true;
            Task::ready(())
        });
        assert!(!asked);
        assert!(matches!(stage, Reading { .. }));
        let mut stage = known(true);
        stage.click(true, start);
        assert!(matches!(stage, Known(true)));
    }
}
