use super::super::app::QuillApp;
use gpui_kit::component::input::TextareaState;
use gpui_kit::*;
use quill::folders::FolderEditor;
use quill::telegram::envelope::ChatFolderSpec;
use std::collections::HashSet;
/// Parity slice: create/edit chat-folder dialog. The editable folder model
/// is [`FolderEditor`]; on save it freezes to a [`ChatFolderSpec`] sent via
/// `createChatFolder` / `editChatFolder`.
pub struct FolderEditorDialog {
    /// `None` = create; `Some(id)` = edit.
    pub(crate) folder_id: Option<i32>,
    pub(crate) editor: FolderEditor,
    pub(crate) name_input: Entity<TextareaState>,
    /// Edit flow: waiting on `getChatFolder` before the editor prefills.
    pub(crate) fetch_pending: bool,
    pub(crate) error: Option<String>,
}

impl FolderEditorDialog {
    pub(crate) fn new(
        window: &mut Window,
        cx: &mut Context<QuillApp>,
        folder_id: Option<i32>,
    ) -> Self {
        let name_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Folder name (1–12 characters)")
                .auto_grow(1, 1)
                .submit_on_enter(false)
        });
        Self {
            folder_id,
            editor: FolderEditor::new(),
            name_input,
            fetch_pending: folder_id.is_some(),
            error: None,
        }
    }

    /// Prefill from the `getChatFolder` response (edit flow).
    pub(crate) fn prefill_from_spec(
        &mut self,
        spec: &ChatFolderSpec,
        window: &mut Window,
        cx: &mut Context<QuillApp>,
    ) {
        self.editor = FolderEditor::from_spec(spec);
        self.fetch_pending = false;
        self.error = None;
        let name = spec.name.clone();
        self.name_input.update(cx, |input, cx| {
            input.set_value(name, window, cx);
        });
    }

    pub(crate) fn name(&self, cx: &App) -> String {
        self.name_input.read(cx).value().to_string()
    }
}

/// What the Share Folder dialog shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FolderShareView {
    /// The folder's invite links.
    List,
    /// Naming a link and choosing its chats; `None` creates a new link.
    Edit { link: Option<String> },
}

/// Share Folder (tdesktop `boxes/filters/edit_filter_links.cpp`): the
/// folder's invite links, and the create / edit form.
pub struct FolderShareDialog {
    pub(crate) folder_id: i32,
    pub(crate) view: FolderShareView,
    pub(crate) name_input: Entity<TextareaState>,
    /// Chats ticked in the create / edit form.
    pub(crate) selected: HashSet<i64>,
    /// A create / edit / delete request is in flight.
    pub(crate) busy: bool,
    /// The link whose delete is waiting for confirmation.
    pub(crate) confirm_delete: Option<String>,
    /// The link last copied to the clipboard.
    pub(crate) copied: Option<String>,
    pub(crate) error: Option<String>,
}

impl FolderShareDialog {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<QuillApp>, folder_id: i32) -> Self {
        let name_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Link name (optional)")
                .auto_grow(1, 1)
                .submit_on_enter(false)
        });
        Self {
            folder_id,
            view: FolderShareView::List,
            name_input,
            selected: HashSet::new(),
            busy: false,
            confirm_delete: None,
            copied: None,
            error: None,
        }
    }
}

/// "Add folder" for a shared-folder (`addlist`) link (tdesktop
/// `ui/chatlist_box`'s `ShowImportToast` / `ToggleChatsBox`).
pub struct FolderInviteDialog {
    pub(crate) link: String,
    /// Chats of the link left ticked; seeded with every missing chat once
    /// the link has been checked.
    pub(crate) selected: HashSet<i64>,
    pub(crate) seeded: bool,
    /// `addChatFolderByInviteLink` is in flight.
    pub(crate) adding: bool,
}

impl FolderInviteDialog {
    pub(crate) fn new(link: String) -> Self {
        Self {
            link,
            selected: HashSet::new(),
            seeded: false,
            adding: false,
        }
    }
}

/// Delete-folder confirmation (tdesktop `RemoveComplexChatFilter`). A
/// shared folder also offers the chats Telegram suggests leaving
/// (`getChatFolderChatsToLeave`), all ticked at first.
pub struct FolderDeleteConfirm {
    pub(crate) folder_id: i32,
    pub(crate) name: String,
    /// The user made invite links for the folder; deleting kills them.
    pub(crate) has_links: bool,
    /// A shared folder: chats can be left along with it.
    pub(crate) shared: bool,
    /// Suggested chats the user unticked (so they stay).
    pub(crate) keep: HashSet<i64>,
}
