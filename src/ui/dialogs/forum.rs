use super::super::app::QuillApp;
use gpui_kit::component::input::TextareaState;
use gpui_kit::*;
use quill::ids::ChatId;

/// The topic being created or edited in the manage dialog (tdesktop
/// `EditForumTopicBox`): a name, a color for the plain icon and an
/// optional custom emoji from `getForumTopicDefaultIcons`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TopicEditor {
    /// `None` creates a topic; `Some(id)` edits that topic.
    pub(crate) target: Option<i32>,
    /// `forumTopicIcon.color`; fixed once the topic exists.
    pub(crate) color: i32,
    /// `forumTopicIcon.custom_emoji_id`; 0 keeps the colored letter icon.
    pub(crate) icon_emoji: i64,
    /// The emoji the topic had when the editor opened.
    pub(crate) original_emoji: i64,
}

/// Slice G2: forum-topic management dialog (info panel → "Manage
/// topics", admins with `can_manage_topics` only). `name_input` feeds
/// `createForumTopic` / `editForumTopic` through the [`TopicEditor`]; the
/// rest are one-shot action buttons per row (`toggleForumTopicIsClosed`,
/// `toggleForumTopicIsPinned`, `deleteForumTopic`,
/// `toggleGeneralForumTopicIsHidden`).
pub struct ForumManageDialog {
    pub(crate) chat_id: ChatId,
    pub(crate) name_input: Entity<TextareaState>,
    pub(crate) editor: Option<TopicEditor>,
}

impl ForumManageDialog {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<QuillApp>, chat_id: ChatId) -> Self {
        let name_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Topic name")
                .auto_grow(1, 1)
                .submit_on_enter(false)
        });
        Self {
            chat_id,
            name_input,
            editor: None,
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
