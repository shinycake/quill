//! "Adaptive layout for wide screens" (pure, no UI).
//!
//! Telegram Desktop's `Core::Settings::adaptiveForWide` and
//! `Window::Adaptive::ChatLayout`: a history at least
//! `adaptiveChatWideWidth` (880px, `window/window.style`) wide switches to
//! the wide chat layout (`HistoryWidget::recountChatWidth`), and with the
//! setting on the history is drawn in `ElementChatMode::Wide`
//! (`Adaptive::isChatWide`):
//!
//! - every message sits on the left, outgoing ones included
//!   (`Element::hasRightLayout` is false), so their tail is on the left too
//!   (`Message::countBubbleRounding`);
//! - private chats and groups show the sender's photo beside every message,
//!   your own included (`HistoryInner::canHaveFromUserpics`,
//!   `Message::hasFromPhoto`); channels keep their usual rule;
//! - service messages, date pills and the "Unread messages" label are
//!   centred in a column [`WIDE_CHAT_WIDTH`] wide at the left edge
//!   (`HistoryView::WideChatWidth`, `Service::countGeometry`,
//!   `ServiceMessagePainter::PaintDate`, `UnreadBar::paint`);
//! - the selection check sits right of that column instead of at the edge,
//!   and nothing slides aside for it
//!   (`Element::AdditionalSpaceForSelectionCheckbox`).
//!
//! The composer is not affected.
#![deny(clippy::indexing_slicing)]

use crate::bubble_layout::{MSG_MARGIN, MSG_MAX_WIDTH, MSG_PHOTO_SKIP};
use crate::telegram::envelope::ChatKind;

/// `adaptiveChatWideWidth`: from this history width on the chat layout is
/// wide.
pub const ADAPTIVE_CHAT_WIDE_WIDTH: f32 = 880.;

/// `HistoryView::WideChatWidth()`: `msgMaxWidth + 2 * msgPhotoSkip +
/// 2 * msgMargin.left()`, the column the wide layout centres service
/// messages and dates in.
pub const WIDE_CHAT_WIDTH: f32 = (MSG_MAX_WIDTH + 2 * MSG_PHOTO_SKIP + 2 * MSG_MARGIN.left) as f32;

/// `Window::Adaptive::ChatLayout`, from the history's width
/// (`HistoryWidget::recountChatWidth`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChatLayout {
    #[default]
    Normal,
    Wide,
}

impl ChatLayout {
    pub fn for_width(width: f32) -> Self {
        if width < ADAPTIVE_CHAT_WIDE_WIDTH {
            ChatLayout::Normal
        } else {
            ChatLayout::Wide
        }
    }
}

/// `Adaptive::isChatWide`: the setting is on and the history is wide.
pub fn is_chat_wide(adaptive_for_wide: bool, layout: ChatLayout) -> bool {
    adaptive_for_wide && layout == ChatLayout::Wide
}

/// `Element::hasRightLayout`: an outgoing message sits on the right except
/// in the wide layout.
pub fn has_right_layout(outgoing: bool, wide: bool) -> bool {
    outgoing && !wide
}

/// Whether a (non-service) message row shows its sender's photo.
///
/// Outside the wide layout this is Quill's rule: incoming messages in
/// groups. In the wide layout tdesktop gives every message a photo
/// (`Message::hasFromPhoto` returns `mode == Wide`) and private chats may
/// draw them (`canHaveFromUserpics` passes `_isChatWide`); channels keep
/// their own rule (`channel->signatureProfiles()`), which the wide layout
/// does not change.
pub fn shows_sender_photo(kind: &ChatKind, outgoing: bool, wide: bool) -> bool {
    let group = matches!(
        kind,
        ChatKind::BasicGroup { .. }
            | ChatKind::Supergroup {
                is_channel: false,
                ..
            }
    );
    if !wide {
        return group && !outgoing;
    }
    group || matches!(kind, ChatKind::Private { .. } | ChatKind::Secret { .. })
}

/// `msgSelectionOffset` and the `msgSelectionCheck` size.
pub const MSG_SELECTION_OFFSET: f32 = 30.;
pub const MSG_SELECTION_CHECK_SIZE: f32 = 20.;

/// Where the wide layout puts the selection check's column, from the
/// history's left edge (`Message::draw`): right of the message's widest
/// bubble and its photo, never past the history.
pub fn selection_right(bubble_width_limit: f32, history_width: f32) -> f32 {
    (bubble_width_limit
        + MSG_PHOTO_SKIP as f32
        + MSG_SELECTION_OFFSET
        + crate::bubble_layout::MSG_PADDING.left as f32
        + MSG_SELECTION_CHECK_SIZE)
        .min(history_width)
}

/// Width of the column service messages, dates and the unread label are
/// centred in: the whole history, capped at [`WIDE_CHAT_WIDTH`] in the wide
/// layout.
pub fn service_column_width(history_width: f32, wide: bool) -> f32 {
    if wide {
        history_width.min(WIDE_CHAT_WIDTH)
    } else {
        history_width
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::UserId;

    #[test]
    fn wide_chat_width_is_tdesktops() {
        // 430 + 2 * 40 + 2 * 16.
        assert_eq!(WIDE_CHAT_WIDTH, 542.);
    }

    #[test]
    fn layout_switches_at_880() {
        assert_eq!(ChatLayout::for_width(879.9), ChatLayout::Normal);
        assert_eq!(ChatLayout::for_width(880.), ChatLayout::Wide);
        assert_eq!(ChatLayout::for_width(1600.), ChatLayout::Wide);
    }

    #[test]
    fn wide_needs_the_setting_and_the_width() {
        assert!(is_chat_wide(true, ChatLayout::Wide));
        assert!(!is_chat_wide(false, ChatLayout::Wide));
        assert!(!is_chat_wide(true, ChatLayout::Normal));
    }

    #[test]
    fn outgoing_moves_left_only_when_wide() {
        assert!(has_right_layout(true, false));
        assert!(!has_right_layout(true, true));
        assert!(!has_right_layout(false, false));
        assert!(!has_right_layout(false, true));
    }

    #[test]
    fn sender_photos_follow_tdesktop() {
        let private = ChatKind::Private { user_id: UserId(1) };
        let group = ChatKind::BasicGroup { basic_group_id: 2 };
        let supergroup = ChatKind::Supergroup {
            supergroup_id: 3,
            is_channel: false,
        };
        let channel = ChatKind::Supergroup {
            supergroup_id: 4,
            is_channel: true,
        };
        // Normal layout: incoming group messages only.
        assert!(shows_sender_photo(&group, false, false));
        assert!(shows_sender_photo(&supergroup, false, false));
        assert!(!shows_sender_photo(&group, true, false));
        assert!(!shows_sender_photo(&private, false, false));
        assert!(!shows_sender_photo(&channel, false, false));
        // Wide layout: every message in private chats and groups.
        assert!(shows_sender_photo(&group, true, true));
        assert!(shows_sender_photo(&private, true, true));
        assert!(shows_sender_photo(&private, false, true));
        assert!(!shows_sender_photo(&channel, false, true));
        assert!(!shows_sender_photo(&ChatKind::Unknown, false, true));
    }

    #[test]
    fn selection_check_sits_right_of_the_column() {
        // 430 + 40 + 30 + 11 + 20.
        assert_eq!(selection_right(430., 1200.), 531.);
        assert_eq!(selection_right(900., 1000.), 1000.);
    }

    #[test]
    fn service_column_is_capped_when_wide() {
        assert_eq!(service_column_width(1200., false), 1200.);
        assert_eq!(service_column_width(1200., true), 542.);
        assert_eq!(service_column_width(400., true), 400.);
    }
}
