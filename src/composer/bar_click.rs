//! What a click on the reply or edit bar does (tdesktop `FieldHeader`
//! click handling, `history_view_compose_controls.cpp`): the edit bar
//! jumps to the edited message; the reply bar opens the quote picker, and
//! a Ctrl/Cmd click or a right click jumps to the replied message.

/// Which bar was clicked, with what its message allows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BarKind {
    Edit,
    Reply {
        has_text: bool,
        can_reply_elsewhere: bool,
        same_chat: bool,
    },
}

/// How it was clicked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BarClick {
    Left,
    /// Left click with Ctrl (Cmd on macOS) held.
    LeftModified,
    Right,
}

/// What the click does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BarAction {
    /// Scroll to the message and highlight it.
    JumpToMessage,
    /// Pick the part of the replied message to quote.
    PickQuote,
    /// Choose another chat for the reply (a message with nothing to quote).
    ReplyElsewhere,
    Nothing,
}

pub fn bar_click_action(kind: BarKind, click: BarClick) -> BarAction {
    match (kind, click) {
        (BarKind::Edit, BarClick::Left | BarClick::LeftModified) => BarAction::JumpToMessage,
        // tdesktop's right click on an edit bar opens its media edit menu;
        // Quill keeps those tools as buttons on the bar itself.
        (BarKind::Edit, BarClick::Right) => BarAction::Nothing,
        (BarKind::Reply { same_chat, .. }, BarClick::LeftModified | BarClick::Right) => {
            if same_chat {
                BarAction::JumpToMessage
            } else {
                BarAction::Nothing
            }
        }
        // tdesktop opens the options box; Quill's first entry that applies.
        (
            BarKind::Reply {
                has_text,
                can_reply_elsewhere,
                ..
            },
            BarClick::Left,
        ) => {
            if has_text {
                BarAction::PickQuote
            } else if can_reply_elsewhere {
                BarAction::ReplyElsewhere
            } else {
                BarAction::Nothing
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{BarAction, BarClick, BarKind, bar_click_action};

    const TEXT_REPLY: BarKind = BarKind::Reply {
        has_text: true,
        can_reply_elsewhere: true,
        same_chat: true,
    };

    #[test]
    fn edit_bar_jumps_to_the_edited_message() {
        assert_eq!(
            bar_click_action(BarKind::Edit, BarClick::Left),
            BarAction::JumpToMessage
        );
        assert_eq!(
            bar_click_action(BarKind::Edit, BarClick::Right),
            BarAction::Nothing
        );
    }

    #[test]
    fn reply_bar_left_click_picks_a_quote() {
        assert_eq!(
            bar_click_action(TEXT_REPLY, BarClick::Left),
            BarAction::PickQuote
        );
        let media = BarKind::Reply {
            has_text: false,
            can_reply_elsewhere: true,
            same_chat: true,
        };
        assert_eq!(
            bar_click_action(media, BarClick::Left),
            BarAction::ReplyElsewhere
        );
        let locked = BarKind::Reply {
            has_text: false,
            can_reply_elsewhere: false,
            same_chat: true,
        };
        assert_eq!(bar_click_action(locked, BarClick::Left), BarAction::Nothing);
    }

    #[test]
    fn reply_bar_modified_or_right_click_jumps_in_the_same_chat() {
        for click in [BarClick::LeftModified, BarClick::Right] {
            assert_eq!(
                bar_click_action(TEXT_REPLY, click),
                BarAction::JumpToMessage
            );
            let elsewhere = BarKind::Reply {
                has_text: true,
                can_reply_elsewhere: true,
                same_chat: false,
            };
            assert_eq!(bar_click_action(elsewhere, click), BarAction::Nothing);
        }
    }
}
