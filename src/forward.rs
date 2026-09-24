//! Forward a history message (tdesktop ShareBox / Unigram `ChooseChatsViewModel`).
//!
//! Official desktop pick-a-chat then `forwardMessages`. Quill keeps a single-message
//! draft and a destination list over **already-loaded** chats (`ordered_chats` /
//! `local_search_chats`) — no second search stack.

use crate::ids::{ChatId, MessageId};
use crate::state::{ChatSummary, HistoryMessage};

/// Schema `messageProperties.can_be_forwarded` / `can_be_copied`.
/// Live clients fetch these via `getMessageProperties`. This slice uses
/// [`Self::ASSUMED`] when properties have not been fetched (same documented
/// default style as own-outgoing for edit/delete).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ForwardCapabilities {
    pub can_be_forwarded: bool,
    pub can_be_copied: bool,
}

impl ForwardCapabilities {
    /// Typical cloud text/photo/document: true forward, not a silent copy.
    pub const ASSUMED: Self = Self {
        can_be_forwarded: true,
        can_be_copied: true,
    };

    /// `Some(send_copy)` for `forwardMessages`. `None` hides the action.
    ///
    /// Unigram `ChooseChatsViewModel` sends `send_copy` only when copy-mode /
    /// remove-captions is on (`_sendAsCopy || _removeCaptions`). Default is
    /// a real forward (`send_copy: false`) when `can_be_forwarded`.
    pub fn send_copy(self) -> Option<bool> {
        if self.can_be_forwarded {
            Some(false)
        } else if self.can_be_copied {
            Some(true)
        } else {
            None
        }
    }
}

/// Pending pick-a-destination (tdesktop `setForwardDraft` / Unigram Forward).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForwardDraft {
    pub from_chat_id: ChatId,
    pub message_id: MessageId,
    pub preview: String,
    pub send_copy: bool,
}

impl ForwardDraft {
    /// Single already-sent history row. Pending / local ids stay out.
    pub fn from_history(
        chat_id: ChatId,
        message: &HistoryMessage,
        caps: ForwardCapabilities,
    ) -> Option<Self> {
        if message.pending {
            return None;
        }
        if message.chat_id != chat_id {
            return None;
        }
        let send_copy = caps.send_copy()?;
        Some(Self {
            from_chat_id: chat_id,
            message_id: message.id,
            preview: message.content.preview(),
            send_copy,
        })
    }
}

/// tdesktop `FieldHeader` Escape / Unigram picker dismiss: drop the draft.
pub fn cancel_forward_draft(draft: Option<ForwardDraft>) -> Option<ForwardDraft> {
    let _ = draft;
    None
}

/// Loaded, supported cloud chats whose title contains `query`.
/// Same local title filter as [`crate::state::Session::local_search_chats`].
pub fn forward_destinations<'a>(
    chats: impl IntoIterator<Item = &'a ChatSummary>,
    query: &str,
) -> Vec<&'a ChatSummary> {
    let needle = query.trim().to_lowercase();
    chats
        .into_iter()
        .filter(|chat| chat.supported())
        .filter(|chat| needle.is_empty() || chat.title.to_lowercase().contains(&needle))
        .collect()
}

/// Compact tdesktop/Unigram header: `Forwarded from {origin}`.
pub fn forwarded_from_header(origin_name: &str) -> String {
    let name = origin_name.trim();
    if name.is_empty() {
        "Forwarded message".into()
    } else {
        format!("Forwarded from {name}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::ChatId;
    use crate::ids::UserId;
    use crate::telegram::envelope::{ChatKind, MessageContent};

    fn text_row(chat: i64, id: i64, pending: bool) -> HistoryMessage {
        HistoryMessage {
            id: MessageId(id),
            chat_id: ChatId(chat),
            is_outgoing: false,
            content: MessageContent::Text("hello already here".into()),
            pending,
            reply_to: None,
            forward_info: None,
        }
    }

    fn private_chat(id: i64, title: &str) -> ChatSummary {
        ChatSummary {
            id: ChatId(id),
            title: title.into(),
            kind: ChatKind::Private {
                user_id: UserId(id),
            },
            unread_count: 0,
            last_read_inbox_message_id: MessageId(0),
            last_read_outbox_message_id: MessageId(0),
            order: id,
            is_pinned: false,
            in_main_list: true,
            last_preview: String::new(),
        }
    }

    #[test]
    fn assumed_is_true_forward_not_copy() {
        assert_eq!(ForwardCapabilities::ASSUMED.send_copy(), Some(false));
        assert_eq!(
            ForwardCapabilities {
                can_be_forwarded: false,
                can_be_copied: true,
            }
            .send_copy(),
            Some(true)
        );
        assert_eq!(
            ForwardCapabilities {
                can_be_forwarded: false,
                can_be_copied: false,
            }
            .send_copy(),
            None
        );
    }

    #[test]
    fn draft_skips_pending_and_wrong_chat() {
        let row = text_row(11, 101, false);
        let draft = ForwardDraft::from_history(ChatId(11), &row, ForwardCapabilities::ASSUMED)
            .expect("forwardable");
        assert_eq!(draft.message_id, MessageId(101));
        assert!(!draft.send_copy);
        assert_eq!(draft.preview, "hello already here");
        assert!(
            ForwardDraft::from_history(
                ChatId(11),
                &text_row(11, 1, true),
                ForwardCapabilities::ASSUMED
            )
            .is_none()
        );
        assert!(
            ForwardDraft::from_history(ChatId(12), &row, ForwardCapabilities::ASSUMED).is_none()
        );
        assert_eq!(cancel_forward_draft(Some(draft)), None);
    }

    #[test]
    fn destinations_are_supported_loaded_chats() {
        let alice = private_chat(11, "Demo chat A");
        let bob = private_chat(12, "Demo chat B");
        let channel = ChatSummary {
            id: ChatId(13),
            title: "Demo channel".into(),
            kind: ChatKind::Supergroup {
                supergroup_id: 13,
                is_channel: true,
            },
            unread_count: 0,
            last_read_inbox_message_id: MessageId(0),
            last_read_outbox_message_id: MessageId(0),
            order: 10,
            is_pinned: false,
            in_main_list: true,
            last_preview: String::new(),
        };
        let chats = [&alice, &bob, &channel];
        let all = forward_destinations(chats, "");
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].title, "Demo chat A");
        let filtered = forward_destinations(chats, "chat b");
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].id, ChatId(12));
    }

    #[test]
    fn forwarded_header_matches_official_prefix() {
        assert_eq!(
            forwarded_from_header("Ada Lovelace"),
            "Forwarded from Ada Lovelace"
        );
        assert_eq!(forwarded_from_header("  "), "Forwarded message");
    }
}
