//! Swipe-to-reply: a press on a message row, released more than 24 px to its
//! left, starts a reply. The press belongs to the gesture: any release ends
//! it, on the row or off it, and so does a change of chat.
use gpui_kit::*;
use quill::ids::{ChatId, MessageId};

/// How far left of the press a release must land to count as a swipe.
const SWIPE_DISTANCE: Pixels = px(24.);

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(in crate::ui) enum SwipeReply {
    #[default]
    Idle,
    /// The button went down on this row at this x.
    Pressed {
        chat: ChatId,
        message: MessageId,
        x: Pixels,
    },
}

impl SwipeReply {
    pub(in crate::ui) fn press(&mut self, chat: ChatId, message: MessageId, x: Pixels) {
        *self = Self::Pressed { chat, message, x };
    }

    /// The button came up on a row: the gesture ends, and a swipe is a
    /// release on the row that took the press, far enough to its left.
    pub(in crate::ui) fn release_on(
        &mut self,
        chat: ChatId,
        message: MessageId,
        x: Pixels,
    ) -> bool {
        matches!(
            std::mem::take(self),
            Self::Pressed { chat: c, message: m, x: start }
                if c == chat && m == message && start - x > SWIPE_DISTANCE
        )
    }

    /// The gesture ended some other way (a release off every row, a drag
    /// that took the press, a chat change).
    pub(in crate::ui) fn clear(&mut self) {
        *self = Self::Idle;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;

    #[test]
    fn a_release_far_enough_left_on_the_pressed_row_is_a_swipe() {
        let mut swipe = SwipeReply::default();
        swipe.press(ChatId(1), MessageId(5), px(300.));
        assert!(swipe.release_on(ChatId(1), MessageId(5), px(200.)));
        assert_eq!(swipe, SwipeReply::Idle, "the release ended the gesture");
    }

    #[test]
    fn a_short_or_other_row_release_is_no_swipe_and_ends_the_gesture() {
        let mut swipe = SwipeReply::default();
        swipe.press(ChatId(1), MessageId(5), px(300.));
        assert!(!swipe.release_on(ChatId(1), MessageId(5), px(290.)));
        swipe.press(ChatId(1), MessageId(5), px(300.));
        assert!(!swipe.release_on(ChatId(1), MessageId(6), px(100.)));
        swipe.press(ChatId(1), MessageId(5), px(300.));
        assert!(!swipe.release_on(ChatId(2), MessageId(5), px(100.)));
    }

    #[test]
    fn a_cleared_press_cannot_start_a_later_swipe() {
        let mut swipe = SwipeReply::default();
        swipe.press(ChatId(1), MessageId(5), px(300.));
        swipe.clear();
        assert!(!swipe.release_on(ChatId(1), MessageId(5), px(100.)));
    }
}
