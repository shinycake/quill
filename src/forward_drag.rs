//! Dragging messages out of a history onto a chat in the list, after
//! Telegram Desktop's `ListWidget::prepareDrag` (the `application/x-td-forward`
//! mime) and `MainWidget::filesOrForwardDrop`: the pure rules, with no UI
//! types.
//!
//! A drag carries the selection when the pressed message is part of it, and
//! the pressed message alone otherwise. It does not start at all in a chat
//! whose content is protected or when any carried message cannot be
//! forwarded (`canForwardAll`, `hasCopyRestriction`). A drop on a row opens
//! that chat with the messages waiting in its forward bar, unless the chat
//! takes no forwards or is a forum that first asks for a topic.
#![deny(clippy::indexing_slicing)]

use crate::ids::{ChatId, MessageId};
use std::sync::Arc;

/// The messages a drag carries and the chat they come from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForwardDrag {
    from: ChatId,
    /// Shared so the per-move clones the list makes stay cheap.
    ids: Arc<[MessageId]>,
}

/// Why a press does not start a drag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// The chat restricts copying and forwarding (`has_protected_content`).
    Protected,
    /// A carried message cannot be forwarded (or is not sent yet).
    NotForwardable,
}

impl ForwardDrag {
    /// The drag of a press on `pressed`: the whole `selected` set when it
    /// holds the pressed message (`uponSelected`), else `pressed` alone.
    pub fn begin(
        from: ChatId,
        pressed: MessageId,
        selected: &[MessageId],
        protected: bool,
        forwardable: impl Fn(MessageId) -> bool,
    ) -> Result<Self, Refusal> {
        if protected {
            return Err(Refusal::Protected);
        }
        let mut ids: Vec<MessageId> = if selected.contains(&pressed) {
            selected.to_vec()
        } else {
            vec![pressed]
        };
        ids.sort_by_key(|id| id.0);
        ids.dedup();
        if ids.is_empty() || ids.iter().any(|&id| id.0 <= 0 || !forwardable(id)) {
            return Err(Refusal::NotForwardable);
        }
        Ok(Self {
            from,
            ids: ids.into(),
        })
    }

    pub fn from_chat(&self) -> ChatId {
        self.from
    }

    /// The carried messages, oldest first.
    pub fn ids(&self) -> &[MessageId] {
        &self.ids
    }

    pub fn count(&self) -> usize {
        self.ids.len()
    }
}

/// What the chat under a dropped forward drag can do with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Destination {
    /// The chat takes messages (a channel the user cannot post in does not).
    pub can_receive: bool,
    /// A forum or a direct messages group: the topic comes first.
    pub needs_topic: bool,
}

/// Where a drop on a row goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Drop {
    /// Open the chat with the messages in its forward bar.
    Forward,
    /// The chat asks for a topic first.
    ChooseTopic,
    /// The chat takes no forwards (`lng_forward_cant`).
    Cannot,
}

impl Destination {
    pub fn route(self) -> Drop {
        if !self.can_receive {
            Drop::Cannot
        } else if self.needs_topic {
            Drop::ChooseTopic
        } else {
            Drop::Forward
        }
    }
}

/// Whether the chat list takes a forward drag at all: not in one column
/// (`_dragForward = !isOneColumn() && has x-td-forward`).
pub fn list_accepts(one_column: bool) -> bool {
    !one_column
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHAT: ChatId = ChatId(7);

    fn ids(values: &[i64]) -> Vec<MessageId> {
        values.iter().map(|&value| MessageId(value)).collect()
    }

    #[test]
    fn a_pressed_selected_message_carries_the_whole_selection() {
        let drag =
            ForwardDrag::begin(CHAT, MessageId(30), &ids(&[50, 30, 10]), false, |_| true).unwrap();
        assert_eq!(drag.ids(), ids(&[10, 30, 50]).as_slice());
        assert_eq!(drag.from_chat(), CHAT);
        assert_eq!(drag.count(), 3);
    }

    #[test]
    fn a_pressed_message_outside_the_selection_goes_alone() {
        let drag =
            ForwardDrag::begin(CHAT, MessageId(20), &ids(&[10, 30]), false, |_| true).unwrap();
        assert_eq!(drag.ids(), ids(&[20]).as_slice());
    }

    #[test]
    fn a_protected_chat_never_starts_a_drag() {
        let result = ForwardDrag::begin(CHAT, MessageId(20), &[], true, |_| true);
        assert_eq!(result, Err(Refusal::Protected));
    }

    #[test]
    fn one_message_that_cannot_be_forwarded_refuses_the_whole_drag() {
        let result = ForwardDrag::begin(CHAT, MessageId(10), &ids(&[10, 20, 30]), false, |id| {
            id.0 != 20
        });
        assert_eq!(result, Err(Refusal::NotForwardable));
    }

    #[test]
    fn a_message_still_being_sent_is_not_forwardable() {
        let result = ForwardDrag::begin(CHAT, MessageId(-4), &[], false, |_| true);
        assert_eq!(result, Err(Refusal::NotForwardable));
    }

    #[test]
    fn the_list_takes_forward_drags_beside_a_chat_only() {
        assert!(list_accepts(false));
        assert!(!list_accepts(true));
    }

    #[test]
    fn a_drop_routes_by_what_the_chat_takes() {
        let route = |can_receive, needs_topic| {
            Destination {
                can_receive,
                needs_topic,
            }
            .route()
        };
        assert_eq!(route(true, false), Drop::Forward);
        assert_eq!(route(true, true), Drop::ChooseTopic);
        assert_eq!(route(false, false), Drop::Cannot);
        assert_eq!(route(false, true), Drop::Cannot);
    }
}
