//! The drop zones shown while files are dragged over a chat
//! (`quill::drop_modes` holds the rules; this draws them and applies the
//! drop). One zone sends the files one way; a second one, below, offers
//! another way: quick media versus documents, a folder's files versus one
//! archive. GPUI delivers the same `ExternalPaths` drag on macOS, Windows
//! and Linux.

use super::app::{PaneMode, QuillApp};
use super::chat_theme::accent;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::composer::{AttachmentKind, ComposerAttachment};
use quill::drop_modes::{
    DragEntry, DragState, DropAction, DropPlan, Zone, classify, folder_files_for_sending,
    plan_drop, zones,
};
use quill::folder_archive::{ARCHIVE_SIZE_LIMIT, ArchiveError, build_archive};
use std::path::PathBuf;

impl QuillApp {
    /// Whether the open chat takes dropped files into its composer.
    pub(super) fn file_drop_enabled(&self) -> bool {
        matches!(self.pane_mode(), PaneMode::Ready)
            && self.composer_available(PaneMode::Ready)
            && self.composer_ui.pending_edit.is_none()
            && !self.composer_ui.rich_editor_open
            && !self.recording_active()
    }

    /// A file drag moved over the conversation: remember what it holds.
    pub(super) fn note_file_drag(&mut self, paths: &[PathBuf], cx: &mut Context<Self>) {
        if !self.file_drop_enabled() || self.composer_ui.drop_paths == paths {
            return;
        }
        self.composer_ui.drop_paths = paths.to_vec();
        let entries: Vec<DragEntry> = paths.iter().map(|path| DragEntry::probe(path)).collect();
        self.composer_ui.drop_state = classify(&entries);
        cx.notify();
    }

    /// The zones to draw now: a live file drag, or a demo's fixed state.
    pub(super) fn visible_drop_state(&mut self, cx: &App) -> Option<DragState> {
        if self.composer_ui.drop_preview.is_some() {
            return self.composer_ui.drop_preview;
        }
        if !cx.has_active_drag() {
            self.composer_ui.drop_state = None;
            self.composer_ui.drop_paths.clear();
        }
        self.composer_ui
            .drop_state
            .filter(|_| self.file_drop_enabled())
    }

    /// Files dropped on the conversation outside any zone, or while the
    /// zones are off (editing): the usual attach, which picks media or
    /// documents by what was dropped.
    pub(super) fn drop_files_plain(&mut self, paths: &[PathBuf], cx: &mut Context<Self>) {
        self.composer_ui.drop_state = None;
        self.composer_ui.drop_paths.clear();
        if self.file_drop_enabled() || self.composer_ui.pending_edit.is_some() {
            self.attach_dropped_files(paths, cx);
        }
    }

    /// Files dropped on a zone.
    pub(super) fn drop_on_zone(
        &mut self,
        action: DropAction,
        paths: &[PathBuf],
        cx: &mut Context<Self>,
    ) {
        self.composer_ui.drop_state = None;
        self.composer_ui.drop_paths.clear();
        if !self.file_drop_enabled() {
            return;
        }
        let folder_files = match (action, paths) {
            (DropAction::FolderFiles, [folder]) => folder_files_for_sending(folder),
            _ => Vec::new(),
        };
        match plan_drop(action, paths, &folder_files) {
            DropPlan::Attach(planned) => {
                self.connection.status_note = match ComposerAttachment::append_planned(
                    &mut self.composer_ui.pending_attachments,
                    &planned,
                ) {
                    Ok(count) => format!("Attached {count} files. Send to upload."),
                    Err(note) => note.into(),
                };
                cx.notify();
            }
            DropPlan::Reject(note) => {
                self.connection.status_note = note.into();
                cx.notify();
            }
            DropPlan::Archive(source) => {
                let Some(chat) = self.open_chat_id() else {
                    return;
                };
                self.connection.status_note = "Preparing the archive…".into();
                cx.notify();
                let dir = quill::local_path::media_cache_base().join("archives");
                quill::local_path::ensure_private_dir(&dir);
                cx.spawn(async move |this, cx| {
                    let built = cx
                        .background_spawn(async move {
                            build_archive(&source, &dir, ARCHIVE_SIZE_LIMIT)
                        })
                        .await;
                    let _ = this.update(cx, |this, cx| this.archive_ready(chat, built, cx));
                })
                .detach();
            }
        }
    }

    fn archive_ready(
        &mut self,
        chat: quill::ids::ChatId,
        built: Result<PathBuf, ArchiveError>,
        cx: &mut Context<Self>,
    ) {
        match built {
            Ok(path) => {
                // The person moved on to another chat while it was zipped.
                if self.open_chat_id() != Some(chat) {
                    if let Some(folder) = path.parent() {
                        let _ = std::fs::remove_dir_all(folder);
                    }
                    return;
                }
                self.connection.status_note = match ComposerAttachment::append_planned(
                    &mut self.composer_ui.pending_attachments,
                    &[(path, AttachmentKind::Document)],
                ) {
                    Ok(_) => "Attached the archive. Send to upload.".into(),
                    Err(note) => note.into(),
                };
            }
            Err(err) => self.connection.status_note = err.note(),
        }
        cx.notify();
    }

    /// The overlay: the zones stacked over the whole conversation.
    pub(super) fn drop_zone_overlay(&self, state: DragState, cx: &mut Context<Self>) -> AnyElement {
        let layout = zones(state);
        let zone = |id: &'static str, zone: Zone, cx: &mut Context<Self>| {
            let action = zone.action;
            div()
                .id(id)
                .role(Role::Group)
                .aria_label(format!("{}, {}", zone.title, zone.subtitle))
                .flex_1()
                .min_h_0()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap_1()
                .rounded_lg()
                .border_2()
                .border_dashed()
                .border_color(cx.theme().border)
                .bg(cx.theme().background.opacity(0.92))
                .drag_over::<ExternalPaths>(|style, _, _, cx| {
                    style
                        .border_color(accent())
                        .bg(cx.theme().accent.opacity(0.95))
                })
                .on_drop(cx.listener(move |this, paths: &ExternalPaths, _, cx| {
                    this.drop_on_zone(action, paths.paths(), cx);
                }))
                .child(div().text_lg().font_semibold().child(zone.title))
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(zone.subtitle),
                )
        };
        div()
            .id("file-drop-zones")
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .p_3()
            .flex()
            .flex_col()
            .gap_3()
            .child(zone("drop-zone-top", layout.top, cx))
            .when_some(layout.bottom, |this, bottom| {
                this.child(zone("drop-zone-bottom", bottom, cx))
            })
            .into_any_element()
    }
}
