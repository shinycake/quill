use super::super::app::QuillApp;
use gpui_kit::component::input::TextareaState;
use gpui_kit::*;
use quill::ids::ChatId;
/// Slice G2: forum-topic management dialog (info panel → "Manage
/// topics", admins with `can_manage_topics` only). `new_topic_input`
/// feeds `createForumTopic`; `editing_topic` + `edit_input` drive the
/// inline rename row (`editForumTopic`); the rest are one-shot action
/// buttons per row (`toggleForumTopicIsClosed`,
/// `toggleForumTopicIsPinned`, `deleteForumTopic`,
/// `toggleGeneralForumTopicIsHidden`).
pub struct ForumManageDialog {
    pub(crate) chat_id: ChatId,
    pub(crate) new_topic_input: Entity<TextareaState>,
    pub(crate) editing_topic: Option<i32>,
    pub(crate) edit_input: Entity<TextareaState>,
}

impl ForumManageDialog {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<QuillApp>, chat_id: ChatId) -> Self {
        let new_topic_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("New topic name")
                .auto_grow(1, 1)
                .submit_on_enter(false)
        });
        let edit_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Topic name")
                .auto_grow(1, 1)
                .submit_on_enter(false)
        });
        Self {
            chat_id,
            new_topic_input,
            editing_topic: None,
            edit_input,
        }
    }
}

/// Slice G2: one-shot forum-topic actions from the management dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ForumTopicAction {
    Close,
    Reopen,
    Pin,
    Unpin,
    Delete,
    HideGeneral,
    ShowGeneral,
}
