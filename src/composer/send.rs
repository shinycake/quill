//! Enter-to-send rules, send options, scheduling and the "send started" note.

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EnterEvent {
    /// True while an IME composition is marked (CJK/Hangul/etc.).
    pub composing: bool,
    /// Shift+Enter inserts a newline in chat-style inputs.
    pub shift: bool,
    /// Platform secondary modifier (Ctrl/Cmd).
    pub secondary: bool,
}

/// Which keystroke sends a chat message (parity:settings-enter-send,
/// parity:settings-ctrlenter-send). Telegram Desktop calls this
/// "Send with Enter".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SendKeyMode {
    /// Plain Enter sends; Shift+Enter inserts a newline.
    #[default]
    Enter,
    /// Plain Enter inserts a newline; Ctrl/Cmd+Enter sends.
    CtrlEnter,
}

/// Whether the keystroke described by `event` sends the message under
/// `mode`. IME composition never sends, in either mode.
pub fn should_send_on_enter(event: EnterEvent, mode: SendKeyMode) -> bool {
    if event.composing {
        return false;
    }
    match mode {
        SendKeyMode::Enter => !event.shift && !event.secondary,
        SendKeyMode::CtrlEnter => event.secondary && !event.shift,
    }
}

/// Text to actually send for a `PressEnter` that passed
/// `should_send_on_enter`. In CtrlEnter mode kit inserts the newline
/// before emitting the event (the composer runs with
/// `submit_on_enter(false)`), so strip that single trailing newline —
/// otherwise Ctrl+Enter sends a trailing blank line.
pub fn send_text_on_enter(text: String, mode: SendKeyMode) -> String {
    match mode {
        SendKeyMode::CtrlEnter => text.strip_suffix('\n').unwrap_or(&text).to_string(),
        SendKeyMode::Enter => text,
    }
}

/// Map Kit `InputEvent::PressEnter` plus the IME mark from
/// `EntityInputHandler::marked_text_range`.
///
/// gpui-base 0.6.1 `InputEvent::PressEnter { secondary, shift }` has **no**
/// composing field. `InputBaseState::enter` always emits `PressEnter` and does
/// not consult `ime_marked_range` (Escape does). Callers must read the mark.
pub fn enter_event_from_kit(
    shift: bool,
    secondary: bool,
    marked_text_range: Option<std::ops::Range<usize>>,
) -> EnterEvent {
    EnterEvent {
        composing: marked_text_range.is_some(),
        shift,
        secondary,
    }
}

/// M1: `messageSendOptions` choices for a send (TDLib 1.8.67,
/// `schema/td_api.tl:5934` —
/// `messageSendOptions suggested_post_info disable_notification
/// from_background protect_content allow_paid_broadcast
/// paid_message_star_count update_order_of_installed_sticker_sets
/// scheduling_state effect_id sending_id only_preview`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SendOptions {
    /// `disable_notification` — silent send.
    pub disable_notification: bool,
    pub scheduling: ComposerScheduling,
    /// `linkPreviewOptions.is_disabled` — the composer preview toggle.
    /// Secret chats force this on the driver side regardless.
    pub link_preview_disabled: bool,
    /// MED4b: `linkPreviewOptions.show_above_text` (TDLib 1.8.67,
    /// `schema/td_api.tl:2236`) — preview above the message text instead
    /// of below. Ignored in secret chats.
    pub link_preview_above_text: bool,
    /// B5: which detected link drives the preview (tdesktop "choose
    /// link"); 0 is the first URL.
    pub link_preview_link: usize,
    /// MED4b: `linkPreviewOptions.force_small_media` /
    /// `force_large_media` (TDLib 1.8.67, `schema/td_api.tl:2234-2235`) —
    /// TGX's large/small toggle cycles these. Ignored in secret chats or
    /// when the URL isn't explicitly specified (the send path sets `url`
    /// whenever this isn't `Auto`).
    pub link_preview_media: PreviewMediaSize,
    /// M1 fix-up: the driver sets this when the target chat is a secret
    /// chat. `textEntityTypeBlockQuote` is not supported in secret chats
    /// (schema 1.8.67), so `send_text` strips blockquote entities instead
    /// of letting TDLib drop them.
    pub is_secret: bool,
    /// S15: `messageSendOptions.update_order_of_installed_sticker_sets`
    /// (TDLib 1.8.67, `schema/td_api.tl:5934`) — pass true when the user
    /// explicitly chose a sticker from an installed set so TDLib moves
    /// that set to the front of the installed order.
    pub update_order_of_installed_sticker_sets: bool,
}

/// MED4b: media-size half of `linkPreviewOptions` (TDLib 1.8.67,
/// `schema/td_api.tl:2234-2235`). Two bools on the wire but at most one
/// may be set — the enum makes the invalid both-true state
/// unrepresentable. TGX (`MessagesController.onRequestToggleLargeMedia`
/// → `LinkPreview.toggleLargeMedia`) flips these relative to the
/// preview's current effective size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PreviewMediaSize {
    /// No force flags — TDLib uses the preview's own default.
    #[default]
    Auto,
    /// `force_small_media: true`.
    ForceSmall,
    /// `force_large_media: true`.
    ForceLarge,
}

impl PreviewMediaSize {
    /// TGX toggle semantics: flip relative to the currently effective
    /// size; never returns to `Auto`.
    pub fn toggle(self, effective_large: bool) -> Self {
        if effective_large {
            Self::ForceSmall
        } else {
            Self::ForceLarge
        }
    }

    /// Currently effective size given the preview's server default.
    pub fn effective_large(self, preview_show_large_media: bool) -> bool {
        match self {
            Self::Auto => preview_show_large_media,
            Self::ForceSmall => false,
            Self::ForceLarge => true,
        }
    }
}

/// M1: `MessageSchedulingState` for a send (TDLib 1.8.67,
/// `schema/td_api.tl:5902` `messageSchedulingStateSendAtDate
/// send_date repeat_period` / `:5905` `messageSchedulingStateSendWhenOnline`).
/// `repeat_period` is always 0 (premium-only, never surfaced).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ComposerScheduling {
    #[default]
    None,
    /// Unix timestamp of the scheduled send.
    SendAtDate(i64),
    SendWhenOnline,
}

/// Send-started copy reflects both connectivity and the submitted schedule.
pub fn send_started_note(offline: bool, scheduling: ComposerScheduling, online_note: &str) -> &str {
    if !offline {
        return online_note;
    }
    match scheduling {
        ComposerScheduling::None => "You're offline — will send when you reconnect",
        ComposerScheduling::SendAtDate(_) => "You're offline — will schedule when you reconnect",
        ComposerScheduling::SendWhenOnline => "You're offline — will send when they're online",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offline_send_started_notes_follow_scheduling() {
        for (scheduling, expected) in [
            (
                ComposerScheduling::None,
                "You're offline — will send when you reconnect",
            ),
            (
                ComposerScheduling::SendAtDate(1_800_000_000),
                "You're offline — will schedule when you reconnect",
            ),
            (
                ComposerScheduling::SendWhenOnline,
                "You're offline — will send when they're online",
            ),
        ] {
            assert_eq!(send_started_note(true, scheduling, "sending…"), expected);
        }
    }

    #[test]
    fn online_send_started_notes_preserve_provided_copy() {
        for scheduling in [
            ComposerScheduling::None,
            ComposerScheduling::SendAtDate(1_800_000_000),
            ComposerScheduling::SendWhenOnline,
        ] {
            for online_note in ["sending…", "saving edit…", "retrying send…"] {
                assert_eq!(
                    send_started_note(false, scheduling, online_note),
                    online_note
                );
            }
        }
    }

    #[test]
    fn ime_enter_does_not_send() {
        assert!(!should_send_on_enter(
            EnterEvent {
                composing: true,
                shift: false,
                secondary: false,
            },
            SendKeyMode::Enter,
        ));
    }

    #[test]
    fn plain_enter_sends() {
        assert!(should_send_on_enter(
            EnterEvent {
                composing: false,
                shift: false,
                secondary: false,
            },
            SendKeyMode::Enter,
        ));
    }

    #[test]
    fn shift_enter_is_newline() {
        assert!(!should_send_on_enter(
            EnterEvent {
                composing: false,
                shift: true,
                secondary: false,
            },
            SendKeyMode::Enter,
        ));
    }

    #[test]
    fn secondary_enter_does_not_send() {
        assert!(!should_send_on_enter(
            EnterEvent {
                composing: false,
                shift: false,
                secondary: true,
            },
            SendKeyMode::Enter,
        ));
    }

    #[test]
    fn kit_marked_text_range_is_the_composing_signal() {
        // Kit PressEnter has no composing field; a live IME mark must suppress send.
        let composing = enter_event_from_kit(false, false, Some(0..2));
        assert!(composing.composing);
        assert!(!should_send_on_enter(composing, SendKeyMode::Enter));

        let idle = enter_event_from_kit(false, false, None);
        assert!(!idle.composing);
        assert!(should_send_on_enter(idle, SendKeyMode::Enter));
    }
}
