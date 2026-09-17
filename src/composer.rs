//! Composer send policy. IME composition must never send.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EnterEvent {
    /// True while an IME composition is marked (CJK/Hangul/etc.).
    pub composing: bool,
    /// Shift+Enter inserts a newline in chat-style inputs.
    pub shift: bool,
    /// Platform secondary modifier (Ctrl/Cmd) — treated as "do not send".
    pub secondary: bool,
}

/// Enter sends only when the composition is finished and no modifiers apply.
pub fn should_send_on_enter(event: EnterEvent) -> bool {
    !event.composing && !event.shift && !event.secondary
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

/// Snapshot of a send attempt: destination is frozen at submit time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComposerSnapshot {
    pub chat_id: i64,
    pub view_generation: u64,
    pub text: String,
}

impl ComposerSnapshot {
    pub fn is_empty(&self) -> bool {
        self.text.trim().is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ime_enter_does_not_send() {
        assert!(!should_send_on_enter(EnterEvent {
            composing: true,
            shift: false,
            secondary: false,
        }));
    }

    #[test]
    fn plain_enter_sends() {
        assert!(should_send_on_enter(EnterEvent {
            composing: false,
            shift: false,
            secondary: false,
        }));
    }

    #[test]
    fn shift_enter_is_newline() {
        assert!(!should_send_on_enter(EnterEvent {
            composing: false,
            shift: true,
            secondary: false,
        }));
    }

    #[test]
    fn secondary_enter_does_not_send() {
        assert!(!should_send_on_enter(EnterEvent {
            composing: false,
            shift: false,
            secondary: true,
        }));
    }

    #[test]
    fn kit_marked_text_range_is_the_composing_signal() {
        // Kit PressEnter has no composing field; a live IME mark must suppress send.
        let composing = enter_event_from_kit(false, false, Some(0..2));
        assert!(composing.composing);
        assert!(!should_send_on_enter(composing));

        let idle = enter_event_from_kit(false, false, None);
        assert!(!idle.composing);
        assert!(should_send_on_enter(idle));
    }
}
