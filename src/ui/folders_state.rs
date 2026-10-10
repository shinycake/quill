//! Chat folders UI: the tab strip, editor, manager, share and invite boxes.

use super::*;

pub(crate) struct FolderUi {
    /// Phase 7.1: selected folder tab (`None` = Main). Folder membership
    /// comes from chat positions (`chatListFolder`); the tab only filters.
    pub(super) tab: Option<i32>,
    /// Parity slice: folder management (manage dialog / editor / delete
    /// confirm / per-chat folder menu).
    pub(super) manage_open: bool,
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
}

impl FolderUi {
    pub(super) fn new() -> Self {
        Self {
            tab: None,
            manage_open: false,
            editor: None,
            delete_confirm: None,
            share: None,
            invite: None,
            menu_open: false,
            tab_menu: None,
            new_chats_dialog: None,
            limit_box: None,
        }
    }
}
