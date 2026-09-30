use super::super::app::QuillApp;
use gpui_kit::component::input::TextareaState;
use gpui_kit::*;
/// Slice G1: which chat to create. Basic groups use
/// `createNewBasicGroupChat` (schema 1.8.67, line 13327); supergroups
/// and channels use `createNewSupergroupChat` (line 13337) with the
/// `is_channel` flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreateChatKind {
    BasicGroup,
    Supergroup,
    Channel,
}

impl CreateChatKind {
    pub(crate) fn title(self) -> &'static str {
        match self {
            Self::BasicGroup => "New group",
            Self::Supergroup => "New supergroup",
            Self::Channel => "New channel",
        }
    }

    /// Only basic groups take members at creation
    /// (`createNewBasicGroupChat.user_ids`); supergroup members are
    /// added afterwards from the member dialog.
    pub(crate) fn picks_members(self) -> bool {
        matches!(self, Self::BasicGroup)
    }
}

/// Slice G1: group/supergroup/channel creation dialog (sidebar "New"
/// entries). Title input (plus description for supergroups/channels)
/// and, for basic groups, a contact picker with multi-select whose ids
/// ride `createNewBasicGroupChat.user_ids`.
pub struct CreateChatDialog {
    pub(crate) kind: CreateChatKind,
    pub(crate) title_input: Entity<TextareaState>,
    pub(crate) description_input: Entity<TextareaState>,
    pub(crate) search_input: Entity<TextareaState>,
    pub(crate) selected_users: Vec<i64>,
}

impl CreateChatDialog {
    pub(crate) fn new(
        window: &mut Window,
        cx: &mut Context<QuillApp>,
        kind: CreateChatKind,
    ) -> Self {
        let title_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Name")
                .auto_grow(1, 1)
                .submit_on_enter(false)
        });
        let description_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Description (optional)")
                .auto_grow(1, 3)
                .submit_on_enter(false)
        });
        let search_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Search contacts")
                .auto_grow(1, 1)
                .submit_on_enter(false)
        });
        Self {
            kind,
            title_input,
            description_input,
            search_input,
            selected_users: Vec::new(),
        }
    }
}
