//! "Edit Image" and "Edit Cover" while editing an own photo or video
//! message (tdesktop `EditCaptionBox::showMenu`). The menu sits in the
//! edit banner next to "Replace"; both tools stage their result and the
//! edit sends it as one `editMessageMedia` when saved.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::photo_editor::PhotoEditor;
use crate::ui::status_note::StatusNote;
use gpui_kit::component::button::*;
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::*;
use gpui_kit::*;
use quill::composer::{
    CLEAR_COVER_LABEL, ComposerEdit, EDIT_MEDIA_INVALID_FILE, EditImageFetch, EditMediaTool,
    cover_send_path, session_allows_cover,
};
use quill::lang::{keys, tr, tr_with};
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
    let dir = quill::scratch::temp_work_dir("quill-edits");
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
            .label(tr(keys::Q_MISC_EDIT_MEDIA))
            .ghost()
            .small()
            .tooltip(tr(keys::Q_MISC_EDIT_THE_PHOTO_OR_VIDEO))
            .accessibility_label(tr(keys::Q_MISC_EDIT_MEDIA))
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
                    menu = menu.item(PopupMenuItem::new(label).on_click(move |_, window, cx| {
                        let _ = owner.update(cx, |this, cx| {
                            this.run_edit_media_action(action, window, cx)
                        });
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

    fn run_edit_media_action(
        &mut self,
        action: MenuAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match action {
            MenuAction::Tool(EditMediaTool::EditImage) => self.begin_edit_image(cx),
            MenuAction::Tool(EditMediaTool::EditVideo) => self.open_edit_video_editor(window, cx),
            MenuAction::Tool(EditMediaTool::EditCover) => self.pick_edit_cover(cx),
            MenuAction::ClearCover => {
                if let Some(edit) = self.chat_ui.pending_edit_mut() {
                    match edit.media_edit.replacement.as_mut() {
                        Some(replacement) => replacement.cover = None,
                        None => edit.media_edit.cover = None,
                    }
                }
                self.set_note(StatusNote::silent(tr(keys::Q_MISC_COVER_CLEARED)));
                cx.notify();
            }
        }
    }

    /// "Edit Image": open the photo editor on the staged replacement, or
    /// on the downloaded original of the message's photo.
    fn begin_edit_image(&mut self, cx: &mut Context<Self>) {
        let Some(edit) = self.chat_ui.pending_edit() else {
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
        let asked_for = (edit.chat_id, edit.message_id);
        let Some(edit_id) = self.chat_ui.edit_id() else {
            return;
        };
        let decoding = self.decode_image_task(
            source,
            move |this, result, cx| {
                // The edit it was started for is gone or another one now.
                if !this.chat_ui.end_image_decode(edit_id) {
                    return;
                }
                match result {
                    Ok(pixels) => {
                        if this.viewer.photo_editor.is_none() {
                            this.viewer.photo_editor.open(PhotoEditor::for_message_edit(
                                asked_for.0,
                                asked_for.1,
                                pixels,
                            ));
                        }
                    }
                    Err(_) => this.set_note(StatusNote::error(tr(
                        keys::Q_MISC_COULDNT_OPEN_THIS_IMAGE_FOR,
                    ))),
                }
                cx.notify();
            },
            cx,
        );
        self.chat_ui.begin_image_decode(decoding);
    }

    /// The photo isn't on disk yet: ask TDLib for it; the editor opens
    /// when it lands (`edit_image_upkeep`), or a note says it failed.
    fn request_edit_image_download(&mut self, cx: &mut Context<Self>) {
        let file_id = self.chat_ui.pending_edit().and_then(|edit| {
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
        let asked = match (file_id, self.live.as_mut()) {
            (Some(file_id), Some(live)) => {
                live.driver.download_file(file_id, 8).map(|_| file_id).ok()
            }
            _ => None,
        };
        match asked {
            Some(file_id) => {
                if let Some(edit) = self.chat_ui.pending_edit_mut() {
                    edit.media_edit.image = EditImageFetch::Downloading { file_id };
                }
                self.set_note(StatusNote::silent(tr(keys::Q_MISC_DOWNLOADING)));
            }
            None => self.set_note(StatusNote::error(tr(
                keys::Q_OPS_COULD_NOT_DOWNLOAD_PLEASE_TRY,
            ))),
        }
        cx.notify();
    }

    /// Each window's frame: the photo "Edit Image" waited for has landed
    /// (open the editor on it) or its download ended without a file (say
    /// so). The wait is part of the edit, so ending the edit drops it.
    pub(in crate::ui) fn edit_image_upkeep(&mut self, cx: &mut Context<Self>) {
        let Some(edit) = self.chat_ui.pending_edit() else {
            return;
        };
        let EditImageFetch::Downloading { file_id } = edit.media_edit.image else {
            return;
        };
        let landed = self
            .menu_media_local(edit.chat_id, edit.message_id)
            .is_some();
        let failed = !landed
            && self.session().is_some_and(|session| {
                session
                    .media
                    .downloads
                    .ended_without_file(file_id.0, &session.media.files)
            });
        if !landed && !failed {
            return;
        }
        if let Some(edit) = self.chat_ui.pending_edit_mut() {
            edit.media_edit.image = EditImageFetch::Idle;
        }
        if landed {
            self.begin_edit_image(cx);
        } else {
            self.set_note(StatusNote::error(tr(
                keys::Q_OPS_COULD_NOT_DOWNLOAD_PLEASE_TRY,
            )));
            cx.notify();
        }
    }

    /// The photo editor's "Done" for a message edit: stage the edited PNG
    /// as the replacement, keeping the spoiler the photo had.
    pub(super) fn finish_edit_image(&mut self, saved: Option<PathBuf>, cx: &mut Context<Self>) {
        let Some(path) = saved else {
            self.set_note(StatusNote::error(tr(
                keys::Q_MISC_COULDNT_SAVE_THE_EDITED_PHOTO,
            )));
            cx.notify();
            return;
        };
        let Some(edit) = self.chat_ui.pending_edit() else {
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
            .chat_ui
            .pending_edit_mut()
            .and_then(|edit| edit.media_edit.replacement.as_mut())
        {
            replacement.spoiler = spoiler;
        }
        cx.notify();
    }

    /// "Edit Cover": choose an image (tdesktop `VideoCoverUploader::choose`).
    fn pick_edit_cover(&mut self, cx: &mut Context<Self>) {
        // The edit the cover is for: its dialog's wait lives in that edit, so
        // a picker that returns after the edit ended, or after another edit
        // of the same message began, drops its answer.
        let Some(asked_for) = self.chat_ui.edit_id() else {
            return;
        };
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(tr(keys::Q_MISC_CHOOSE_VIDEO_COVER).into()),
        });
        let waiting = self.spawn_scoped(cx, async move |this, cx| {
            let picked = picker.await;
            let _ = this.update(cx, |this, cx| {
                if !this.chat_ui.end_cover_pick(asked_for) {
                    return;
                }
                if let Ok(Ok(Some(paths))) = picked
                    && let Some(path) = paths.first()
                {
                    this.set_edit_cover(path, cx);
                }
            });
        });
        self.chat_ui.begin_cover_pick(waiting);
    }

    /// Stage a picked cover; it goes out with the edit.
    pub(super) fn set_edit_cover(&mut self, path: &Path, cx: &mut Context<Self>) {
        let Some(edit) = self.screen.chat_ui.pending_edit_mut() else {
            return;
        };
        match cover_send_path(path) {
            Some(cover) => {
                self.screen.connection.set_note_for(
                    &self.screen.windows,
                    StatusNote::silent(tr_with(
                        keys::Q_MISC_COVER_WILL_BE_SAVED_WITH,
                        &[("shown_name", &(shown_name(&cover)).to_string())],
                    )),
                );
                // A staged replacement video carries its own cover.
                match edit.media_edit.replacement.as_mut() {
                    Some(replacement) => replacement.cover = Some(cover),
                    None => edit.media_edit.cover = Some(cover),
                }
            }
            None => self.set_note(StatusNote::silent(EDIT_MEDIA_INVALID_FILE)),
        }
        cx.notify();
    }
}
