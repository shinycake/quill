use super::super::app::QuillApp;
use gpui_kit::component::input::TextareaState;
use gpui_kit::*;
use quill::folders::FolderEditor;
use quill::telegram::envelope::ChatFolderSpec;
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

/// Parity slice: delete-folder confirmation. Optionally leaves suggested
/// chats with the folder (`getChatFolderChatsToLeave`).
pub struct FolderDeleteConfirm {
    pub(crate) folder_id: i32,
    pub(crate) name: String,
    pub(crate) leave_with_folder: bool,
}
