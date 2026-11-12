//! Chat folders UI: the tab strip, editor, manager, share and invite boxes.
#![deny(clippy::indexing_slicing)]

use super::*;

pub(crate) struct FolderUi {
    pub(super) editor: Option<FolderEditorDialog>,
    pub(super) delete_confirm: Option<FolderDeleteConfirm>,
    /// Share Folder (invite links) dialog.
    pub(super) share: Option<FolderShareDialog>,
    /// "Add folder" for an `addlist` link.
    pub(super) invite: Option<FolderInviteDialog>,
    /// The Add to folder box, with the chat it was opened for.
    pub(super) menu_chat: Option<quill::ids::ChatId>,
    /// Right-click menu of a folder tab (`None` folder = the All tab).
    pub(super) tab_menu: Option<super::folder_extras::FolderTabMenu>,
    /// The shared folder's "N new chats" join dialog.
    pub(super) new_chats_dialog: Option<super::folder_extras::FolderNewChatsDialog>,
    /// Recommended folders being added from Settings > Folders: the
    /// request of each, by the recommendation's name. A second tap on the
    /// same one waits for the first; the answer ends it either way.
    pub(super) recommended_adds: std::collections::HashMap<String, quill::ids::RequestId>,
    /// The `deleteChatFolder` requests out. Each toast waits for its own
    /// answer, which ends its entry either way; a second delete of the same
    /// folder is a second entry. The set lives in this account's `OnScreen`,
    /// so a switch drops it with the screen.
    pub(super) deletes: std::collections::HashSet<quill::ids::RequestId>,
    /// A folder limit box (or the tag Premium notice).
    pub(super) limit_box: Option<quill::folder_limits::FolderLimitKind>,
    /// Scroll state of the top tab strip, shared with the kit `TabBar`.
    pub(super) tabs_scroll: gpui_kit::ScrollHandle,
    /// What the selected tab was last scrolled into view for.
    pub(super) tabs_revealed: std::cell::Cell<Option<super::folder_tabs_fit::RevealKey>>,
}

impl FolderUi {
    pub(super) fn new() -> Self {
        Self {
            editor: None,
            delete_confirm: None,
            share: None,
            invite: None,
            menu_chat: None,
            tab_menu: None,
            new_chats_dialog: None,
            recommended_adds: std::collections::HashMap::new(),
            deletes: std::collections::HashSet::new(),
            limit_box: None,
            tabs_scroll: gpui_kit::ScrollHandle::new(),
            tabs_revealed: std::cell::Cell::new(None),
        }
    }
}

/// Nothing to say: its state is the account's only app-side.
impl super::on_screen::Farewell for FolderUi {}
