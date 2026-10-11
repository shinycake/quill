//! Where a message offers "Go To Message", and where it jumps to.
//!
//! Telegram Desktop adds the item in `AddGoToMessageAction`
//! (`history/view/history_view_context_menu.cpp`) only for messages shown
//! outside the chat's own history: in the pinned messages section and in the
//! chat peek preview. Search results, shared media, scheduled and thread
//! lists open the message with a plain click and have no such item.

use crate::ids::{ChatId, MessageId};

/// The list a message is shown in (Telegram Desktop `HistoryView::Context`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageListContext {
    /// The chat's own history.
    History,
    /// The pinned messages list.
    Pinned,
    /// The read-only peek at a chat from the chat list.
    ChatPreview,
    /// Saved Messages sublists and tag filters.
    SavedSublist,
    /// Comment and reply threads.
    Thread,
    /// Scheduled messages.
    Scheduled,
}

/// Whether the message menu offers "Go To Message" here. Only messages the
/// server has (`isRegular`: no local id, not still sending) qualify.
pub fn go_to_message_offered(
    context: MessageListContext,
    message_id: MessageId,
    pending: bool,
) -> bool {
    let regular = message_id.0 > 0 && !pending;
    regular
        && matches!(
            context,
            MessageListContext::Pinned | MessageListContext::ChatPreview
        )
}

/// What choosing the item does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoToTarget {
    /// The chat is open already: jump and highlight.
    SameChat { message_id: MessageId },
    /// Open the chat first, then jump and highlight.
    OtherChat {
        chat_id: ChatId,
        message_id: MessageId,
    },
}

/// The jump for a message of `chat_id` while `open_chat` is showing.
pub fn go_to_target(
    open_chat: Option<ChatId>,
    chat_id: ChatId,
    message_id: MessageId,
) -> GoToTarget {
    if open_chat == Some(chat_id) {
        GoToTarget::SameChat { message_id }
    } else {
        GoToTarget::OtherChat {
            chat_id,
            message_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{GoToTarget, MessageListContext as C, go_to_message_offered, go_to_target};
    use crate::ids::{ChatId, MessageId};

    #[test]
    fn offered_only_in_pinned_and_preview() {
        let id = MessageId(5);
        assert!(go_to_message_offered(C::Pinned, id, false));
        assert!(go_to_message_offered(C::ChatPreview, id, false));
        for context in [C::History, C::SavedSublist, C::Thread, C::Scheduled] {
            assert!(!go_to_message_offered(context, id, false));
        }
    }

    #[test]
    fn local_messages_never_offer_it() {
        assert!(!go_to_message_offered(C::Pinned, MessageId(5), true));
        assert!(!go_to_message_offered(C::ChatPreview, MessageId(0), false));
        assert!(!go_to_message_offered(C::Pinned, MessageId(-3), false));
    }

    #[test]
    fn target_depends_on_the_open_chat() {
        let message_id = MessageId(9);
        assert_eq!(
            go_to_target(Some(ChatId(1)), ChatId(1), message_id),
            GoToTarget::SameChat { message_id }
        );
        assert_eq!(
            go_to_target(Some(ChatId(2)), ChatId(1), message_id),
            GoToTarget::OtherChat {
                chat_id: ChatId(1),
                message_id
            }
        );
        assert_eq!(
            go_to_target(None, ChatId(1), message_id),
            GoToTarget::OtherChat {
                chat_id: ChatId(1),
                message_id
            }
        );
    }
}
