//! The drop zones shown while files are dragged over a chat
//! (`quill::drop_modes` holds the rules; this draws them and applies the
//! drop). One zone sends the files one way; a second one, below, offers
//! another way: quick media versus documents, a folder's files versus one
//! archive. GPUI delivers the same `ExternalPaths` drag on macOS, Windows
//! and Linux.
#![deny(clippy::indexing_slicing)]

use super::app::{PaneMode, QuillApp};
use super::chat_theme::accent;
use super::composer_sends::SendSlot;
use crate::ui::status_note::StatusNote;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::composer::{AttachmentKind, ComposerAttachment, is_gif_path};
use quill::drop_modes::{
    DragEntry, DragState, DropAction, DropPlan, Zone, classify, folder_files_for_sending,
    plan_drop, zones,
};
use quill::folder_archive::{ARCHIVE_SIZE_LIMIT, ArchiveError, build_archive};
use quill::lang::{keys, tr, trn};
use std::path::PathBuf;

/// The folder zip behind "Send as archive": at most one per composer, for
/// the chat it was dropped in. It ends with the answer (landed or dropped)
/// or with the screen, whose task and composer go together.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) enum ArchiveJob {
    #[default]
    Idle,
    Building(SendSlot),
}

/// What a file drag over the conversation is known to hold: nothing, the
/// paths being looked at on disk, or the zones they classify to. A demo's
/// fixed state is its own variant, so a screenshot needs no real drag.
#[derive(Default)]
pub(super) enum DragOver {
    #[default]
    None,
    /// Each path is looked at off the UI thread; the answer applies only
    /// while the drag still holds these paths.
    Probing(Vec<PathBuf>),
    Ready(Vec<PathBuf>, DragState),
    Fixed(DragState),
}

impl DragOver {
    /// The drag already holds exactly `paths`.
    fn is_over(&self, paths: &[PathBuf]) -> bool {
        match self {
            Self::Probing(held) | Self::Ready(held, _) => held == paths,
            Self::None | Self::Fixed(_) => false,
        }
    }

    /// The classification of `probed` arrived. True when the drag still
    /// holds those paths (a state of `None`: no zone applies).
    fn probed(&mut self, probed: Vec<PathBuf>, state: Option<DragState>) -> bool {
        if !self.is_over(&probed) {
            return false;
        }
        *self = match state {
            Some(state) => Self::Ready(probed, state),
            None => Self::Probing(probed),
        };
        true
    }

    fn state(&self) -> Option<DragState> {
        match self {
            Self::Ready(_, state) | Self::Fixed(state) => Some(*state),
            Self::None | Self::Probing(_) => None,
        }
    }

    /// The drag ended (dropped or left): a demo's fixed state stays.
    fn end_drag(&mut self) {
        if !matches!(self, Self::Fixed(_)) {
            *self = Self::None;
        }
    }
}

impl QuillApp {
    /// Whether the open chat takes dropped files into its composer.
    pub(super) fn file_drop_enabled(&self) -> bool {
        matches!(self.pane_mode(), PaneMode::Ready)
            && self.composer_available(PaneMode::Ready)
            && !self.chat_ui.is_editing()
            && !self.chat_ui.rich_editor_open
            && !self.recording_active()
    }

    /// A file drag moved over the conversation: remember what it holds.
    pub(super) fn note_file_drag(&mut self, paths: &[PathBuf], cx: &mut Context<Self>) {
        if !self.file_drop_enabled() || self.composer_ui.drag_over.is_over(paths) {
            return;
        }
        self.composer_ui.drag_over = DragOver::Probing(paths.to_vec());
        // Each path is looked at on disk, which can stall on a network drive.
        let probed = paths.to_vec();
        self.run_off_ui_scoped_then(
            move || {
                let entries: Vec<DragEntry> = probed.iter().map(|p| DragEntry::probe(p)).collect();
                (probed, classify(&entries))
            },
            |this, (probed, state), cx| {
                if this.composer_ui.drag_over.probed(probed, state) {
                    cx.notify();
                }
            },
            cx,
        );
    }

    /// The zones to draw now: a live file drag, or a demo's fixed state.
    pub(super) fn visible_drop_state(&mut self, cx: &App) -> Option<DragState> {
        if let DragOver::Fixed(state) = self.composer_ui.drag_over {
            return Some(state);
        }
        if !cx.has_active_drag() {
            self.composer_ui.drag_over = DragOver::None;
        }
        self.composer_ui
            .drag_over
            .state()
            .filter(|_| self.file_drop_enabled())
    }

    /// Files dropped on the conversation outside any zone, or while the
    /// zones are off (editing): the usual attach, which picks media or
    /// documents by what was dropped.
    pub(super) fn drop_files_plain(&mut self, paths: &[PathBuf], cx: &mut Context<Self>) {
        self.composer_ui.drag_over.end_drag();
        if self.file_drop_enabled() || self.chat_ui.is_editing() {
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
        self.composer_ui.drag_over.end_drag();
        if !self.file_drop_enabled() {
            return;
        }
        // A folder on a slow share is walked off the UI thread; its plan
        // applies where it was dropped, if the person is still there.
        if let (DropAction::FolderFiles, [folder]) = (action, paths) {
            let Some(slot) = self.send_slot() else {
                return;
            };
            let folder = folder.clone();
            let kept = paths.to_vec();
            self.run_off_ui_scoped_then(
                move || folder_files_for_sending(&folder),
                move |this, folder_files, cx| {
                    if this.send_slot().as_ref() == Some(&slot) && this.file_drop_enabled() {
                        this.apply_zone_drop(action, &kept, &folder_files, cx);
                    }
                },
                cx,
            );
            return;
        }
        self.apply_zone_drop(action, paths, &[], cx);
    }

    fn apply_zone_drop(
        &mut self,
        action: DropAction,
        paths: &[PathBuf],
        folder_files: &[PathBuf],
        cx: &mut Context<Self>,
    ) {
        match plan_drop(action, paths, folder_files) {
            // The usual attach makes a lone GIF a GIF.
            DropPlan::Attach(_) if matches!(paths, [only] if is_gif_path(only)) => {
                self.attach_dropped_files(paths, cx);
            }
            DropPlan::Attach(planned) => {
                let Some(appended) = self.update_pending_attachments(|tray| {
                    ComposerAttachment::append_planned(tray, &planned)
                }) else {
                    return;
                };
                self.set_note(match appended {
                    Ok(count) => {
                        StatusNote::silent(trn(keys::Q_MISC_ATTACHED_FILES_SEND_TO_UPLOAD, count))
                    }
                    Err(note) => StatusNote::error(note),
                });
                self.check_tray_videos(cx);
                cx.notify();
            }
            DropPlan::Reject(note) => {
                self.set_note(StatusNote::error(note));
                cx.notify();
            }
            DropPlan::Archive(source) => {
                let Some(slot) = self.send_slot() else {
                    return;
                };
                // One zip at a time; its note is still showing.
                if self.composer_ui.archive != ArchiveJob::Idle {
                    return;
                }
                self.composer_ui.archive = ArchiveJob::Building(slot.clone());
                self.set_note(StatusNote::silent(tr(keys::Q_MISC_PREPARING_THE_ARCHIVE)));
                cx.notify();
                let dir = quill::local_path::media_cache_base().join("archives");
                quill::local_path::ensure_private_dir(&dir);
                self.spawn_scoped(cx, async move |this, cx| {
                    let built = cx
                        .background_spawn(async move {
                            build_archive(&source, &dir, ARCHIVE_SIZE_LIMIT)
                        })
                        .await
                        // Removed with its folder when the screen is gone.
                        .map(quill::scratch::OwnedFile::with_folder);
                    let _ = this.update(cx, |this, cx| this.archive_ready(slot, built, cx));
                })
                .detach();
            }
        }
    }

    fn archive_ready(
        &mut self,
        slot: SendSlot,
        built: Result<quill::scratch::OwnedFile, ArchiveError>,
        cx: &mut Context<Self>,
    ) {
        self.composer_ui.archive = ArchiveJob::Idle;
        // The person moved on to another chat (or account) while it was
        // zipped: the result is dropped (an archive removes itself), and
        // so is the note that said it was being prepared.
        if self.send_slot().as_ref() != Some(&slot) {
            if self.connection.note().text() == tr(keys::Q_MISC_PREPARING_THE_ARCHIVE) {
                self.set_note(StatusNote::default());
            }
            cx.notify();
            return;
        }
        match built {
            Ok(archive) => {
                let path = archive.path().to_path_buf();
                let Some(appended) = self.update_pending_attachments(|tray| {
                    ComposerAttachment::append_planned(tray, &[(path, AttachmentKind::Document)])
                }) else {
                    return;
                };
                self.set_note(match appended {
                    Ok(_) => {
                        archive.keep();
                        StatusNote::silent(tr(keys::Q_MISC_ATTACHED_THE_ARCHIVE_SEND_TO))
                    }
                    Err(note) => StatusNote::error(note),
                });
            }
            Err(err) => self.set_note(StatusNote::error(err.note())),
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

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "drop_zones_tests.rs"]
mod tests;

#[cfg(test)]
mod drag_over_tests {
    use super::*;

    fn paths(names: &[&str]) -> Vec<PathBuf> {
        names.iter().map(PathBuf::from).collect()
    }

    #[core::prelude::v1::test]
    fn a_classification_applies_only_to_the_paths_still_dragged() {
        let state = quill::drop_modes::classify(&[quill::drop_modes::DragEntry::probe(
            std::path::Path::new("a.heic"),
        )]);
        let mut over = DragOver::None;
        over = match state {
            Some(_) => DragOver::Probing(paths(&["a.heic"])),
            None => over,
        };
        assert!(over.is_over(&paths(&["a.heic"])));
        assert!(
            !over.probed(paths(&["b.heic"]), state),
            "another drag's answer"
        );
        assert!(over.state().is_none());
        assert!(over.probed(paths(&["a.heic"]), state));
        assert_eq!(over.state().is_some(), state.is_some());
        over.end_drag();
        assert!(over.state().is_none());
    }

    #[core::prelude::v1::test]
    fn a_demos_fixed_state_outlives_the_end_of_a_drag() {
        let state = quill::drop_modes::classify(&[quill::drop_modes::DragEntry::probe(
            std::path::Path::new("a.heic"),
        )]);
        let Some(state) = state else {
            return;
        };
        let mut over = DragOver::Fixed(state);
        over.end_drag();
        assert!(over.state().is_some());
    }
}
