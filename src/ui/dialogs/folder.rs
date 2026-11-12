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
    /// `getChatFolder` is out as this request; the form fills from its
    /// answer, or it ends in `Failed` with Retry.
    Loading(RequestId),
    Failed(String),
    /// The form. `error` is the line the last Save or merge left on it
    /// (cleared by the next stage, so it never draws beside a load error).
    Ready {
        error: Option<String>,
    },
    /// Save found the folder changed since the form opened: the fresh
    /// folder is being fetched so the person's edits go onto it, not over
    /// it (`FolderEditor::merged_onto`). The fetch is this request; a failed
    /// one is back in `Ready` with the error, the edits kept.
    Merging(RequestId),
    /// Save went out: the box closes on the answer to this request and
    /// shows the error (back in `Ready`) when it fails.
    Saving(RequestId),
}

impl FolderLoad {
    /// The form with no line under it.
    pub(crate) fn ready() -> Self {
        FolderLoad::Ready { error: None }
    }

    /// The form with `line` under it.
    pub(crate) fn refused(line: impl Into<String>) -> Self {
        FolderLoad::Ready {
            error: Some(line.into()),
        }
    }

    pub(crate) fn is_ready(&self) -> bool {
        matches!(self, FolderLoad::Ready { .. })
    }

    /// The line under the form.
    pub(crate) fn error(&self) -> Option<&str> {
        match self {
            FolderLoad::Ready { error } => error.as_deref(),
            _ => None,
        }
    }
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
    /// The chat picker is showing for this list (`None` = the form).
    pub(crate) picker: Option<PickerMode>,
    pub(crate) picker_search: Entity<TextareaState>,
}

impl FolderEditorDialog {
    pub(crate) fn new(
        window: &mut Window,
        cx: &mut Context<QuillApp>,
        folder_id: Option<i32>,
        load: FolderLoad,
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
            load,
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
        self.load = FolderLoad::ready();
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
pub enum FolderShareView {
    /// The folder's invite links; `confirm_delete` is the link whose delete
    /// is waiting for confirmation, `copied` the link last copied to the
    /// clipboard until its check mark times out. Leaving the list takes
    /// both along.
    List {
        confirm_delete: Option<String>,
        copied: CopyMark,
        /// The delete request this list waits on.
        op: LinkOp,
    },
    /// Naming a link and choosing its chats; `link: None` creates a new
    /// link. The form's fields live and die with this view.
    Edit {
        link: Option<String>,
        form: LinkForm,
        /// The create / edit request this form waits on.
        op: LinkOp,
    },
}

/// The create / edit form of one invite link.
pub struct LinkForm {
    pub(crate) name_input: Entity<TextareaState>,
    /// Chats ticked in the form.
    pub(crate) selected: HashSet<i64>,
}

impl LinkForm {
    pub(crate) fn new(
        window: &mut Window,
        cx: &mut Context<QuillApp>,
        name: String,
        selected: HashSet<i64>,
    ) -> Self {
        let name_input = cx.new(|cx| {
            let mut state = TextareaState::new(window, cx)
                .placeholder(tr(keys::Q_OPS_LINK_NAME_OPTIONAL))
                .auto_grow(1, 1)
                .submit_on_enter(false);
            state.set_value(name, window, cx);
            state
        });
        Self {
            name_input,
            selected,
        }
    }
}

impl FolderShareView {
    /// The list with no delete awaiting confirmation.
    pub(crate) fn list() -> Self {
        FolderShareView::List {
            confirm_delete: None,
            copied: CopyMark::Idle,
            op: LinkOp::Idle,
        }
    }

    /// The form for `link` (`None` creates one), nothing sent yet.
    pub(crate) fn edit(link: Option<String>, form: LinkForm) -> Self {
        FolderShareView::Edit {
            link,
            form,
            op: LinkOp::Idle,
        }
    }

    /// The form's Cancel: back to the list. A request still out goes with
    /// it, so the list's busy state and its answer (the error line, or the
    /// quiet success) belong to the request that was sent.
    pub(crate) fn cancelled_form(&self) -> Self {
        FolderShareView::List {
            confirm_delete: None,
            copied: CopyMark::Idle,
            op: match self.op() {
                LinkOp::Saving(request) => LinkOp::Saving(*request),
                LinkOp::Idle | LinkOp::Failed(_) => LinkOp::Idle,
            },
        }
    }

    /// The request this view waits on, or the line it failed with.
    pub(crate) fn op(&self) -> &LinkOp {
        match self {
            FolderShareView::List { op, .. } | FolderShareView::Edit { op, .. } => op,
        }
    }

    /// A request went out from this view, or it ended: the answer settles
    /// this view's `op` only, and leaving the view drops it.
    pub(crate) fn set_op(&mut self, next: LinkOp) {
        match self {
            FolderShareView::List { op, .. } | FolderShareView::Edit { op, .. } => *op = next,
        }
    }

    /// A link request is out: the list's delete and edit buttons wait.
    pub(crate) fn is_busy(&self) -> bool {
        matches!(self.op(), LinkOp::Saving(_))
    }
}

/// The link request a Share Folder view waits on (it lives in the view, so
/// leaving the view lets go of it). Only the answer to this request settles
/// it. A failed request is `Failed`, the line the box shows; the next
/// request or leaving the view clears it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkOp {
    Idle,
    Saving(RequestId),
    Failed(String),
}

impl LinkOp {
    /// The line a failed request left, for the box to draw.
    pub(crate) fn error(&self) -> Option<&str> {
        match self {
            LinkOp::Failed(line) => Some(line),
            LinkOp::Idle | LinkOp::Saving(_) => None,
        }
    }
}

/// Share Folder (tdesktop `boxes/filters/edit_filter_links.cpp`): the
/// folder's invite links, and the create / edit form.
pub struct FolderShareDialog {
    pub(crate) folder_id: i32,
    pub(crate) view: FolderShareView,
}

impl FolderShareDialog {
    pub(crate) fn new(folder_id: i32) -> Self {
        Self {
            folder_id,
            view: FolderShareView::list(),
        }
    }

    /// The create / edit form, while it is the view.
    pub(crate) fn form(&self) -> Option<&LinkForm> {
        match &self.view {
            FolderShareView::Edit { form, .. } => Some(form),
            FolderShareView::List { .. } => None,
        }
    }

    pub(crate) fn form_mut(&mut self) -> Option<&mut LinkForm> {
        match &mut self.view {
            FolderShareView::Edit { form, .. } => Some(form),
            FolderShareView::List { .. } => None,
        }
    }

    /// The link list's check mark (none off the list).
    pub(crate) fn copy_mark(&self) -> &CopyMark {
        match &self.view {
            FolderShareView::List { copied, .. } => copied,
            FolderShareView::Edit { .. } => &CopyMark::Idle,
        }
    }

    /// Mark a link copied; the form has no copy buttons, so it keeps none
    /// (and the mark's timer is dropped with it).
    pub(crate) fn mark_copied(&mut self, mark: CopyMark) {
        if let FolderShareView::List { copied, .. } = &mut self.view {
            *copied = mark;
        }
    }
}

/// The copy button's check mark. The mark owns the timer that clears it:
/// a second copy, leaving the list or closing the box drops the mark and
/// with it the timer, so no timer is matched to its mark by time.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum CopyMark {
    #[default]
    Idle,
    Copied {
        link: String,
        timer: CopyTimer,
    },
}

/// The task that takes the check mark back; dropping it cancels the timer.
#[derive(Clone)]
pub struct CopyTimer(std::rc::Rc<Task<()>>);

impl CopyTimer {
    pub(crate) fn new(task: Task<()>) -> Self {
        Self(std::rc::Rc::new(task))
    }
}

impl std::fmt::Debug for CopyTimer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CopyTimer")
    }
}

impl PartialEq for CopyTimer {
    fn eq(&self, other: &Self) -> bool {
        std::rc::Rc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for CopyTimer {}

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
