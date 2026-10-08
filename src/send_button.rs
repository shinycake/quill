//! The composer's primary round button, after Telegram Desktop's
//! `Ui::SendButton` (ui/controls/send_button.cpp): which state it shows,
//! the cross-fade between states, and the slow-mode countdown text.
//!
//! Pure logic only (no GPUI), so the state choice and the animation
//! timeline are unit tested; `src/ui/send_button_ui.rs` draws it.

/// What the button currently is. Mirrors `SendButton::Type`; `Stop` and
/// `Cancel` have no Quill counterpart yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendButtonKind {
    /// Microphone (voice message) or camera (round video message).
    Record {
        video: bool,
    },
    Send,
    /// Editing a message: a check mark, "Save".
    Save,
    /// A send time is set: a clock, "Schedule".
    Schedule,
    /// Slow mode is counting down: the remaining "m:ss" replaces the icon.
    Slowmode,
}

/// Inputs that decide the button state.
#[derive(Debug, Clone, Copy, Default)]
pub struct SendButtonInputs {
    /// A message is being edited.
    pub editing: bool,
    /// There is something to send (text, an attachment, an edit).
    pub sendable: bool,
    /// Voice/video recording is available (the composer is a regular one).
    pub can_record: bool,
    /// The record button is in video (round message) mode.
    pub record_video: bool,
    /// A scheduled send time is set.
    pub scheduled: bool,
    /// Seconds of slow mode left, when active.
    pub slow_mode_wait: Option<u64>,
}

/// `HistoryWidget::computeSendButtonType` + `updateSendButtonType`: editing
/// always wins (Save, even under slow mode: the delay is zeroed for
/// Save/Cancel/Stop); otherwise an active slow mode takes over the button;
/// then Record while there is nothing to send, Schedule when a send time is
/// set, else Send.
pub fn send_button_kind(inputs: SendButtonInputs) -> SendButtonKind {
    if inputs.editing {
        SendButtonKind::Save
    } else if inputs.slow_mode_wait.is_some_and(|secs| secs > 0) {
        SendButtonKind::Slowmode
    } else if inputs.can_record && !inputs.sendable {
        SendButtonKind::Record {
            video: inputs.record_video,
        }
    } else if inputs.scheduled {
        SendButtonKind::Schedule
    } else {
        SendButtonKind::Send
    }
}

impl SendButtonKind {
    /// The send-options (right-click / long-press) menu exists for Send and
    /// Schedule only: Save, Record and the slow-mode counter have none
    /// (`sendButtonMenuDetails` returns no details for them; Quill keeps it
    /// on Schedule so the send time can still be changed).
    pub fn has_send_menu(self) -> bool {
        matches!(self, SendButtonKind::Send | SendButtonKind::Schedule)
    }

    /// Pressing the button submits the composer.
    pub fn submits(self) -> bool {
        !matches!(self, SendButtonKind::Record { .. })
    }

    /// Filled accent circle (true) or a bare glyph (Record, Slowmode:
    /// `windowSubTextFg` text, no fill).
    pub fn filled(self) -> bool {
        matches!(
            self,
            SendButtonKind::Send | SendButtonKind::Save | SendButtonKind::Schedule
        )
    }

    /// Tooltip text; `wait` is the slow-mode seconds left.
    pub fn tooltip(self, wait: Option<u64>) -> String {
        match self {
            SendButtonKind::Send => "Send · right-click for options".into(),
            SendButtonKind::Save => "Save".into(),
            SendButtonKind::Schedule => "Schedule · right-click for options".into(),
            SendButtonKind::Slowmode => {
                format!(
                    "Slow mode: {} left",
                    format_slowmode_words(wait.unwrap_or(0))
                )
            }
            SendButtonKind::Record { video: false } => {
                "Click to record audio · right-click for video".into()
            }
            SendButtonKind::Record { video: true } => {
                "Click to record video · right-click for audio".into()
            }
        }
    }
}

/// `SendButton`'s counter text: minutes and zero-padded seconds, "0:07",
/// "1:05".
pub fn format_slowmode(seconds: u64) -> String {
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

/// `FormatDurationWordsSlowmode`: "7 seconds", "1 minute 5 seconds".
pub fn format_slowmode_words(seconds: u64) -> String {
    fn plural(n: u64, unit: &str) -> String {
        if n == 1 {
            format!("1 {unit}")
        } else {
            format!("{n} {unit}s")
        }
    }
    if seconds > 59 {
        format!(
            "{} {}",
            plural(seconds / 60, "minute"),
            plural(seconds % 60, "second")
        )
    } else {
        plural(seconds, "second")
    }
}

/// `st::universalDuration`: how long a state change animates.
pub const MORPH_DURATION_MS: u64 = 120;
/// How small the outgoing/incoming glyph gets mid-fade (tdesktop scales the
/// grabbed content by `kWideScale` towards the center; a modest zoom reads
/// the same at a 16px glyph).
pub const MORPH_MIN_SCALE: f32 = 0.5;

/// The cross-fade timeline between two button states.
#[derive(Debug, Clone, Copy)]
pub struct SendMorph {
    from: SendButtonKind,
    to: SendButtonKind,
    started_ms: u64,
}

impl SendMorph {
    pub fn new(kind: SendButtonKind) -> Self {
        Self {
            from: kind,
            to: kind,
            started_ms: 0,
        }
    }

    /// Feed the state computed this frame. A change restarts the fade from
    /// the state that was showing. Slow-mode second ticks are not state
    /// changes: only entering/leaving slow mode fades (tdesktop's
    /// `hasSlowmodeChanged`).
    pub fn set(&mut self, kind: SendButtonKind, now_ms: u64) {
        if kind == self.to {
            return;
        }
        self.from = self.to;
        self.to = kind;
        self.started_ms = now_ms;
    }

    pub fn target(&self) -> SendButtonKind {
        self.to
    }

    pub fn source(&self) -> SendButtonKind {
        self.from
    }

    /// 0.0 at the start of a change, 1.0 once settled.
    pub fn progress(&self, now_ms: u64) -> f32 {
        if self.from == self.to {
            return 1.0;
        }
        let elapsed = now_ms.saturating_sub(self.started_ms);
        (elapsed as f32 / MORPH_DURATION_MS as f32).clamp(0.0, 1.0)
    }

    pub fn is_animating(&self, now_ms: u64) -> bool {
        self.from != self.to && self.progress(now_ms) < 1.0
    }
}

/// (opacity, scale) of the outgoing and incoming glyphs at `progress`.
pub fn morph_layers(progress: f32) -> ((f32, f32), (f32, f32)) {
    let p = progress.clamp(0.0, 1.0);
    let scale = |t: f32| MORPH_MIN_SCALE + (1.0 - MORPH_MIN_SCALE) * t;
    ((1.0 - p, scale(1.0 - p)), (p, scale(p)))
}

#[cfg(test)]
mod tests {
    use super::{
        MORPH_DURATION_MS, SendButtonInputs, SendButtonKind, SendMorph, format_slowmode,
        format_slowmode_words, morph_layers, send_button_kind,
    };

    fn inputs() -> SendButtonInputs {
        SendButtonInputs {
            can_record: true,
            ..SendButtonInputs::default()
        }
    }

    #[test]
    fn empty_composer_records_and_text_sends() {
        assert_eq!(
            send_button_kind(inputs()),
            SendButtonKind::Record { video: false }
        );
        assert_eq!(
            send_button_kind(SendButtonInputs {
                record_video: true,
                ..inputs()
            }),
            SendButtonKind::Record { video: true }
        );
        assert_eq!(
            send_button_kind(SendButtonInputs {
                sendable: true,
                ..inputs()
            }),
            SendButtonKind::Send
        );
    }

    #[test]
    fn editing_is_save_and_beats_slow_mode() {
        let kind = send_button_kind(SendButtonInputs {
            editing: true,
            sendable: true,
            can_record: false,
            slow_mode_wait: Some(30),
            ..inputs()
        });
        assert_eq!(kind, SendButtonKind::Save);
        assert!(!kind.has_send_menu());
        assert_eq!(kind.tooltip(None), "Save");
    }

    #[test]
    fn scheduled_send_shows_schedule_with_menu() {
        let kind = send_button_kind(SendButtonInputs {
            sendable: true,
            scheduled: true,
            ..inputs()
        });
        assert_eq!(kind, SendButtonKind::Schedule);
        assert!(kind.has_send_menu());
    }

    #[test]
    fn slow_mode_takes_over_send_and_record_but_not_at_zero() {
        for sendable in [false, true] {
            assert_eq!(
                send_button_kind(SendButtonInputs {
                    sendable,
                    slow_mode_wait: Some(5),
                    ..inputs()
                }),
                SendButtonKind::Slowmode
            );
        }
        assert_eq!(
            send_button_kind(SendButtonInputs {
                sendable: true,
                slow_mode_wait: Some(0),
                ..inputs()
            }),
            SendButtonKind::Send
        );
        assert!(!SendButtonKind::Slowmode.has_send_menu());
    }

    #[test]
    fn only_send_schedule_and_save_are_filled() {
        assert!(SendButtonKind::Send.filled());
        assert!(SendButtonKind::Save.filled());
        assert!(!SendButtonKind::Slowmode.filled());
        assert!(!SendButtonKind::Record { video: false }.filled());
        assert!(!SendButtonKind::Record { video: true }.submits());
    }

    #[test]
    fn slowmode_countdown_text() {
        assert_eq!(format_slowmode(0), "0:00");
        assert_eq!(format_slowmode(7), "0:07");
        assert_eq!(format_slowmode(59), "0:59");
        assert_eq!(format_slowmode(60), "1:00");
        assert_eq!(format_slowmode(65), "1:05");
        assert_eq!(format_slowmode(3600), "60:00");
        assert_eq!(format_slowmode_words(1), "1 second");
        assert_eq!(format_slowmode_words(7), "7 seconds");
        assert_eq!(format_slowmode_words(65), "1 minute 5 seconds");
        assert_eq!(format_slowmode_words(120), "2 minutes 0 seconds");
        assert_eq!(
            SendButtonKind::Slowmode.tooltip(Some(65)),
            "Slow mode: 1 minute 5 seconds left"
        );
    }

    #[test]
    fn morph_runs_120ms_and_settles() {
        let mut morph = SendMorph::new(SendButtonKind::Record { video: false });
        assert!(!morph.is_animating(0));
        morph.set(SendButtonKind::Send, 1_000);
        assert!(morph.is_animating(1_000));
        assert_eq!(morph.progress(1_000), 0.0);
        assert!((morph.progress(1_060) - 0.5).abs() < 1e-6);
        assert_eq!(morph.progress(1_000 + MORPH_DURATION_MS), 1.0);
        assert!(!morph.is_animating(1_000 + MORPH_DURATION_MS));
        assert_eq!(morph.source(), SendButtonKind::Record { video: false });
        assert_eq!(morph.target(), SendButtonKind::Send);
    }

    #[test]
    fn same_state_does_not_restart_and_change_restarts() {
        let mut morph = SendMorph::new(SendButtonKind::Send);
        morph.set(SendButtonKind::Send, 500);
        assert!(!morph.is_animating(500));
        morph.set(SendButtonKind::Slowmode, 500);
        morph.set(SendButtonKind::Slowmode, 550);
        assert!((morph.progress(560) - 0.5).abs() < 1e-6);
        morph.set(SendButtonKind::Send, 560);
        assert_eq!(morph.progress(560), 0.0);
        assert_eq!(morph.source(), SendButtonKind::Slowmode);
    }

    #[test]
    fn layers_cross_fade_and_scale() {
        let ((out_o, out_s), (in_o, in_s)) = morph_layers(0.0);
        assert_eq!((out_o, out_s, in_o), (1.0, 1.0, 0.0));
        assert!(in_s < 1.0);
        let ((out_o, _), (in_o, in_s)) = morph_layers(1.0);
        assert_eq!((out_o, in_o, in_s), (0.0, 1.0, 1.0));
    }
}
