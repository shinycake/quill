#![deny(clippy::indexing_slicing)]

use quill::ids::ChatId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DirectConfirmKind {
    /// tdesktop `DeleteSublistBox`.
    Delete,
    Ban {
        channel: ChatId,
        user_id: i64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DirectConfirm {
    pub(super) chat_id: ChatId,
    pub(super) topic_id: i64,
    /// The person writing (tdesktop `peer->name()`).
    pub(super) name: String,
    pub(super) kind: DirectConfirmKind,
}

pub(crate) struct DirectMessagesUi {
    pub(super) confirm: Option<DirectConfirm>,
}

impl crate::ui::on_screen::Farewell for DirectMessagesUi {}

impl DirectMessagesUi {
    pub(super) fn new() -> Self {
        Self { confirm: None }
    }
}
