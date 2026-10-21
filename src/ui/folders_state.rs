//! Chat folders UI: the tab strip, editor, manager, share and invite boxes.
#![deny(clippy::indexing_slicing)]

use super::*;

pub(crate) struct FolderUi {
    /// Phase 7.1: selected folder tab (`None` = Main). Folder membership
    /// comes from chat positions (`chatListFolder`); the tab only filters.
    pub(super) tab: Option<i32>,
    pub(super) editor: Option<FolderEditorDialog>,
    pub(super) delete_confirm: Option<FolderDeleteConfirm>,
    /// Share Folder (invite links) dialog.
    pub(super) share: Option<FolderShareDialog>,
    /// "Add folder" for an `addlist` link.
    pub(super) invite: Option<FolderInviteDialog>,
    pub(super) menu_open: bool,
    /// Right-click menu of a folder tab (`None` folder = the All tab).
    pub(super) tab_menu: Option<super::folder_extras::FolderTabMenu>,
    /// The shared folder's "N new chats" join dialog.
    pub(super) new_chats_dialog: Option<super::folder_extras::FolderNewChatsDialog>,
    /// A folder limit box (or the tag Premium notice).
    pub(super) limit_box: Option<quill::folder_limits::FolderLimitKind>,
    /// Scroll state of the top tab strip, shared with the kit `TabBar`.
    pub(super) tabs_scroll: gpui_kit::ScrollHandle,
    /// What the selected tab was last scrolled into view for.
    pub(super) tabs_revealed: std::cell::Cell<Option<super::folder_tabs_fit::RevealKey>>,
    /// Scroll state of the topic tab strip (`subsection_tabs`).
    pub(super) topic_tabs_scroll: gpui_kit::ScrollHandle,
    /// What the selected topic tab was last scrolled into view for.
    pub(super) topic_tabs_revealed: std::cell::Cell<Option<super::folder_tabs_fit::RevealKey>>,
}

impl FolderUi {
    pub(super) fn new() -> Self {
        Self {
            tab: None,
            editor: None,
            delete_confirm: None,
            share: None,
            invite: None,
            menu_open: false,
            tab_menu: None,
            new_chats_dialog: None,
            limit_box: None,
            tabs_scroll: gpui_kit::ScrollHandle::new(),
            tabs_revealed: std::cell::Cell::new(None),
            topic_tabs_scroll: gpui_kit::ScrollHandle::new(),
            topic_tabs_revealed: std::cell::Cell::new(None),
        }
    }
}
