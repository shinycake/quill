use super::super::app::QuillApp;
use gpui_kit::component::input::{InputEvent, TextareaState};
use gpui_kit::*;
use quill::folder_picker::PickerMode;
use quill::folders::FolderEditor;
use quill::ids::RequestId;
use quill::lang::{keys, tr};
use quill::telegram::envelope::{ChatFolderInviteLinkInfo, ChatFolderSpec};
use std::collections::HashSet;
/// Where the folder editor's form stands: an edited folder is fetched first
/// (Save would send a blank spec over the real folder), a failed fetch
/// offers Retry or closing, and a Save waits for its own answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FolderLoad {
    Loading,
    Failed(String),
    Ready,
    /// Save found the folder changed since the form opened: the fresh
    /// folder is being fetched so the person's edits go onto it, not over
    /// it (`FolderEditor::merged_onto`). A failed fetch is back in `Ready`
    /// with the error, the edits kept.
    Merging,
    /// Save went out: the box closes on the answer to this request and
    /// shows the error (back in `Ready`) when it fails.
    Saving(RequestId),
}

/// Parity slice: create/edit chat-folder dialog. The editable folder model
/// is [`FolderEditor`]; on save it freezes to a [`ChatFolderSpec`] sent via
/// `createChatFolder` / `editChatFolder`.
pub struct FolderEditorDialog {
    /// `None` = create; `Some(id)` = edit.
    pub(crate) folder_id: Option<i32>,
    pub(crate) editor: FolderEditor,
    /// The folder as the form was filled from it (`getChatFolder`): what
    /// the person's edits are measured against when the folder changed.
    pub(crate) base: ChatFolderSpec,
    pub(crate) name_input: Entity<TextareaState>,
    /// Edit flow: `getChatFolder` before the editor prefills.
    pub(crate) load: FolderLoad,
    pub(crate) error: Option<String>,
    /// The chat picker is showing for this list (`None` = the form).
    pub(crate) picker: Option<PickerMode>,
    pub(crate) picker_search: Entity<TextareaState>,
}

impl FolderEditorDialog {
    pub(crate) fn new(
        window: &mut Window,
        cx: &mut Context<QuillApp>,
        folder_id: Option<i32>,
    ) -> Self {
        let name_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder(tr(keys::Q_OPS_FOLDER_NAME_1_12_CHARACTERS))
                .auto_grow(1, 1)
                .submit_on_enter(false)
        });
        let picker_search = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder(tr(keys::Q_OPS_SEARCH))
                .auto_grow(1, 1)
                .submit_on_enter(false)
        });
        // The picker rows follow the query on the next render.
        cx.subscribe(&picker_search, |_this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                cx.notify();
            }
        })
        .detach();
        Self {
            folder_id,
            editor: FolderEditor::new(),
            base: ChatFolderSpec::default(),
            name_input,
            load: if folder_id.is_some() {
                FolderLoad::Loading
            } else {
                FolderLoad::Ready
            },
            error: None,
            picker: None,
            picker_search,
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
        self.base = spec.clone();
        self.load = FolderLoad::Ready;
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

/// The link request the Share Folder box waits on. Only the answer to this
/// request settles it; leaving the form lets go of it, so a late answer
/// does not land in the next form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkOp {
    Idle,
    Saving(RequestId),
}

/// Share Folder (tdesktop `boxes/filters/edit_filter_links.cpp`): the
/// folder's invite links, and the create / edit form.
pub struct FolderShareDialog {
    pub(crate) folder_id: i32,
    pub(crate) view: FolderShareView,
    pub(crate) name_input: Entity<TextareaState>,
    /// Chats ticked in the create / edit form.
    pub(crate) selected: HashSet<i64>,
    /// The create / edit / delete request the box waits on.
    pub(crate) op: LinkOp,
    /// The link whose delete is waiting for confirmation.
    pub(crate) confirm_delete: Option<String>,
    /// The link last copied to the clipboard, until the check mark times
    /// out.
    pub(crate) copied: CopyMark,
    pub(crate) error: Option<String>,
}

impl FolderShareDialog {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<QuillApp>, folder_id: i32) -> Self {
        let name_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder(tr(keys::Q_OPS_LINK_NAME_OPTIONAL))
                .auto_grow(1, 1)
                .submit_on_enter(false)
        });
        Self {
            folder_id,
            view: FolderShareView::List,
            name_input,
            selected: HashSet::new(),
            op: LinkOp::Idle,
            confirm_delete: None,
            copied: CopyMark::Idle,
            error: None,
        }
    }
}

/// The copy button's check mark. A timer clears it by the time of its copy,
/// so a second copy of the same link keeps its own full time.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum CopyMark {
    #[default]
    Idle,
    Copied {
        link: String,
        at: std::time::Instant,
    },
}

impl CopyMark {
    pub(crate) fn is_copied(&self, url: &str) -> bool {
        matches!(self, CopyMark::Copied { link, .. } if link == url)
    }
}

/// "Add folder" for a shared-folder (`addlist`) link (tdesktop
/// `ui/chatlist_box`'s `ShowImportToast` / `ToggleChatsBox`).
pub struct FolderInviteDialog {
    pub(crate) link: String,
    pub(crate) stage: InviteStage,
    /// Chats of the link left ticked; seeded with every missing chat when
    /// the check answers.
    pub(crate) selected: HashSet<i64>,
    /// Chats whose title `getChat` was asked for: each is asked once, so a
    /// failed answer does not repeat on every poll tick.
    pub(crate) asked_chats: HashSet<i64>,
}

/// Where the "Add folder" box stands. The check and the add each wait for
/// the answer to their own request; a failed check ends in `Failed` (Close),
/// a failed add goes back to `Ready` with the error.
#[derive(Debug, Clone, PartialEq)]
pub enum InviteStage {
    Checking(RequestId),
    Ready {
        info: Box<ChatFolderInviteLinkInfo>,
        error: Option<String>,
    },
    Adding {
        request: RequestId,
        info: Box<ChatFolderInviteLinkInfo>,
    },
    Failed(String),
}

impl FolderInviteDialog {
    pub(crate) fn new(link: String, stage: InviteStage) -> Self {
        Self {
            link,
            stage,
            selected: HashSet::new(),
            asked_chats: HashSet::new(),
        }
    }

    /// What the link offers, once checked.
    pub(crate) fn info(&self) -> Option<&ChatFolderInviteLinkInfo> {
        match &self.stage {
            InviteStage::Ready { info, .. } | InviteStage::Adding { info, .. } => Some(info),
            InviteStage::Checking(_) | InviteStage::Failed(_) => None,
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
