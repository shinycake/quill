//! The reply and reaction buttons beside a hovered bubble (tdesktop
//! `ListWidget::replyButtonParameters` / `reactionButtonParameters`, and
//! the `cornerReply` / `cornerReaction` settings). Kept pure so it is unit
//! tested.

/// One button's width plus the gap after it.
const SLOT: f32 = 28.;
/// Margin between the bubble and the first button.
const MARGIN: f32 = 6.;

/// Which of the (reply, react) buttons show. `prefs` is the two settings
/// (both on by default, as in Telegram Desktop; unknown means defaults).
/// Reply needs a sent message, react one that can take reactions.
pub fn visible(prefs: Option<(bool, bool)>, replyable: bool, reactable: bool) -> (bool, bool) {
    let (reply, react) = prefs.unwrap_or((true, true));
    (reply && replyable, react && reactable)
}

/// Room the buttons take beside the bubble: the "..." button always, plus
/// one slot for each extra.
pub fn span(reply: bool, react: bool) -> f32 {
    let buttons = 1 + usize::from(reply) + usize::from(react);
    MARGIN + SLOT * buttons as f32
}

#[cfg(test)]
mod tests {
    use super::{span, visible};

    #[test]
    fn both_buttons_show_by_default() {
        assert_eq!(visible(None, true, true), (true, true));
    }

    #[test]
    fn settings_hide_their_button() {
        assert_eq!(visible(Some((false, true)), true, true), (false, true));
        assert_eq!(visible(Some((true, false)), true, true), (true, false));
        assert_eq!(visible(Some((false, false)), true, true), (false, false));
    }

    #[test]
    fn unsent_or_unreactable_messages_lose_buttons() {
        assert_eq!(visible(None, false, false), (false, false));
        assert_eq!(visible(None, true, false), (true, false));
    }

    #[test]
    fn span_grows_with_the_buttons() {
        assert_eq!(span(false, false), 34.);
        assert!(span(true, false) > span(false, false));
        assert!(span(true, true) > span(true, false));
    }
}
