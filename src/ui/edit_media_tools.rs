//! "Edit Image" and "Edit Cover" while editing an own photo or video
//! message (tdesktop `EditCaptionBox::showMenu`). The menu sits in the
//! edit banner next to "Replace"; both tools stage their result and the
//! edit sends it as one `editMessageMedia` when saved.

use super::app::QuillApp;
use super::photo_editor::PhotoEditor;
use gpui_kit::component::button::*;
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::*;
use gpui_kit::*;
use quill::composer::{
    CLEAR_COVER_LABEL, ComposerEdit, EDIT_MEDIA_INVALID_FILE, EditMediaTool, cover_send_path,
    session_allows_cover,
};
use std::path::{Path, PathBuf};

/// The edit-media actions run from the menu.
#[derive(Clone, Copy)]
enum MenuAction {
    Tool(EditMediaTool),
    ClearCover,
}

/// Render the edited photo at full size into a new PNG in the temp
/// folder; shared by the attachment editor and the message editor.
pub(super) fn write_edited_png(edited: &image::RgbaImage) -> Option<PathBuf> {
    let dir = std::env::temp_dir().join("quill-edits");
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    let path = dir.join(format!("edit-{stamp}.png"));
    std::fs::create_dir_all(&dir)
        .ok()
        .and_then(|()| edited.save(&path).ok())
        .and_then(|()| std::fs::canonicalize(&path).ok())
}

/// Short file name for status notes.
fn shown_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("image")
        .to_string()
}

impl QuillApp {
    /// The tools for the edit in progress, honouring the chat's peer rule.
    fn edit_media_tools_for(&self, edit: &ComposerEdit) -> Vec<EditMediaTool> {
        let cover_ok = self
            .session()
            .is_some_and(|session| session_allows_cover(session, edit.chat_id));
        edit.edit_media_tools(cover_ok)
    }

    /// The banner's trailing buttons: the edit-media menu (when the media
    /// has a tool) followed by "Replace".
    pub(super) fn edit_media_trailing(
        &self,
        edit: &ComposerEdit,
        replace: Option<AnyElement>,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let tools = self.edit_media_tools_for(edit);
        if tools.is_empty() {
            return replace;
        }
        let owner = cx.entity().downgrade();
        let has_cover = edit.effective_cover().is_some();
        let menu_button = Button::new("composer-edit-media-tools")
            .icon(gpui_kit::assets::IconName::Pencil)
            .label("Edit media")
            .ghost()
            .small()
            .tooltip("Edit the photo or video")
            .accessibility_label("Edit media")
            .dropdown_menu(move |mut menu, _, _| {
                let mut actions: Vec<(&'static str, MenuAction)> = tools
                    .iter()
                    .map(|tool| (tool.label(), MenuAction::Tool(*tool)))
                    .collect();
                if has_cover {
                    actions.push((CLEAR_COVER_LABEL, MenuAction::ClearCover));
                }
                for (label, action) in actions {
                    let owner = owner.clone();
                    menu = menu.item(PopupMenuItem::new(label).on_click(move |_, _, cx| {
                        let _ = owner.update(cx, |this, cx| this.run_edit_media_action(action, cx));
                    }));
                }
                menu
            });
        Some(
            div()
                .flex()
                .items_center()
                .gap_1()
                .child(menu_button)
                .children(replace)
                .into_any_element(),
        )
    }

    fn run_edit_media_action(&mut self, action: MenuAction, cx: &mut Context<Self>) {
        match action {
            MenuAction::Tool(EditMediaTool::EditImage) => self.begin_edit_image(cx),
            MenuAction::Tool(EditMediaTool::EditCover) => self.pick_edit_cover(cx),
            MenuAction::ClearCover => {
                if let Some(edit) = self.composer_ui.pending_edit.as_mut() {
                    edit.media_edit.cover = None;
                }
                self.connection.status_note = "cover cleared".into();
                cx.notify();
            }
        }
    }

    /// "Edit Image": open the photo editor on the staged replacement, or
    /// on the downloaded original of the message's photo.
    fn begin_edit_image(&mut self, cx: &mut Context<Self>) {
        let Some(edit) = self.composer_ui.pending_edit.as_ref() else {
            return;
        };
        let source = match edit.media_edit.replacement.as_ref() {
            Some(replacement) => Some(replacement.path.clone()),
            None => self
                .menu_media_local(edit.chat_id, edit.message_id)
                .map(|(_, path)| path),
        };
        let Some(source) = source else {
            self.request_edit_image_download(cx);
            return;
        };
        match image::open(&source) {
            Ok(image) => {
                self.viewer.photo_editor = Some(PhotoEditor::for_message_edit(image.to_rgba8()));
            }
            Err(_) => self.connection.status_note = "Couldn't open this image for editing.".into(),
        }
        cx.notify();
    }

    /// The photo isn't on disk yet: ask TDLib for it, then try again.
    fn request_edit_image_download(&mut self, cx: &mut Context<Self>) {
        let file_id = self.composer_ui.pending_edit.as_ref().and_then(|edit| {
            let message = self
                .session()?
                .histories
                .get(&edit.chat_id.0)?
                .messages
                .get(&edit.message_id.0)?;
            match &message.content {
                quill::telegram::envelope::MessageContent::Photo(photo) => photo.open_file_id(),
                _ => None,
            }
        });
        if let (Some(file_id), Some(live)) = (file_id, self.live.as_mut()) {
            let _ = live.driver.ensure_media_files(&[file_id]);
        }
        self.connection.status_note =
            "downloading… choose Edit Image again when it finishes".into();
        cx.notify();
    }

    /// The photo editor's "Done" for a message edit: stage the edited PNG
    /// as the replacement, keeping the spoiler the photo had.
    pub(super) fn finish_edit_image(&mut self, saved: Option<PathBuf>, cx: &mut Context<Self>) {
        let Some(path) = saved else {
            self.connection.status_note = "Couldn't save the edited photo.".into();
            cx.notify();
            return;
        };
        let Some(edit) = self.composer_ui.pending_edit.as_ref() else {
            return;
        };
        let spoiler = edit
            .media_edit
            .replacement
            .as_ref()
            .map_or(edit.media_edit.had_spoiler, |replacement| {
                replacement.spoiler
            });
        self.set_edit_replacement(&path, cx);
        if let Some(replacement) = self
            .composer_ui
            .pending_edit
            .as_mut()
            .and_then(|edit| edit.media_edit.replacement.as_mut())
        {
            replacement.spoiler = spoiler;
        }
        cx.notify();
    }

    /// "Edit Cover": choose an image (tdesktop `VideoCoverUploader::choose`).
    fn pick_edit_cover(&mut self, cx: &mut Context<Self>) {
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Choose video cover".into()),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = picker.await
                && let Some(path) = paths.first().cloned()
            {
                let _ = this.update(cx, |this, cx| this.set_edit_cover(&path, cx));
            }
        })
        .detach();
    }

    /// Stage a picked cover; it goes out with the edit.
    pub(super) fn set_edit_cover(&mut self, path: &Path, cx: &mut Context<Self>) {
        let Some(edit) = self.composer_ui.pending_edit.as_mut() else {
            return;
        };
        match cover_send_path(path) {
            Some(cover) => {
                self.connection.status_note =
                    format!("Cover {} will be saved with the edit", shown_name(&cover));
                edit.media_edit.cover = Some(cover);
            }
            None => self.connection.status_note = EDIT_MEDIA_INVALID_FILE.into(),
        }
        cx.notify();
    }
}
