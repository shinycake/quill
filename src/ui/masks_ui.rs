//! Mask sticker sets: the Masks strip in the photo editor and the entry to
//! its manage box (`ui/stickers_manager`, Masks and Archived tabs).
//!
//! tdesktop only offers masks where an image is edited: the story editor's
//! sticker panel has a Masks tab, and its gear opens the same box as the
//! sticker manager with just Masks and Archived tabs. Quill mirrors that in
//! the photo editor's Stickers mode. A mask sticker is placed as a plain
//! picture; neither app fits it to a face.
#![deny(clippy::indexing_slicing)]
use super::app::QuillApp;
use crate::ui::status_note::StatusNote;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::lang::{keys, tr};
use quill::local_path::sandboxed_display_path;

/// Mask thumbnails shown in the editor strip.
const STRIP_LIMIT: usize = 24;

impl QuillApp {
    pub(super) fn masks_panel_mut(&mut self) -> Option<&mut quill::state::MasksPanel> {
        if let Some(live) = self.live.as_mut() {
            Some(&mut live.driver.session.stickers.masks)
        } else {
            self.demo_session
                .as_mut()
                .map(|session| &mut session.stickers.masks)
        }
    }

    pub(super) fn open_mask_sets(&mut self, cx: &mut Context<Self>) {
        self.pickers.sticker_manager.masks.clear();
        self.pickers
            .sticker_manager
            .scroll
            .set_offset(point(px(0.), px(0.)));
        if let Some(live) = self.live.as_mut() {
            live.driver.set_sticker_manager_open(true);
            if live.driver.open_mask_sets().is_err() {
                self.connection
                    .set_note(StatusNote::error(tr(keys::Q_MISC_COULD_NOT_LOAD_MASKS)));
            }
        } else if let Some(session) = self.demo_session.as_mut() {
            session.stickers.masks.open = true;
            session.stickers.manager.open = true;
        }
        cx.notify();
    }

    pub(super) fn change_mask_set(
        &mut self,
        set_id: i64,
        installed: bool,
        archived: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(masks) = self.masks_panel_mut() {
            masks.confirm_remove = None;
        }
        if let Some(live) = self.live.as_mut() {
            self.connection.set_note(
                match live.driver.change_mask_set(set_id, installed, archived) {
                    Ok(Some(_)) => StatusNote::silent(tr(keys::Q_MISC_UPDATING_MASKS)),
                    Ok(None) => StatusNote::silent(tr(keys::Q_MISC_A_MASK_UPDATE_IS_ALREADY)),
                    Err(_) => StatusNote::error(tr(keys::Q_MISC_COULD_NOT_UPDATE_MASKS)),
                },
            );
        }
        cx.notify();
    }

    /// The Masks row of the photo editor's Stickers mode: pictures from
    /// your installed mask sets, and a button for the manage dialog.
    pub(super) fn photo_editor_mask_strip(&self, cx: &mut Context<Self>) -> AnyElement {
        let roots = self.media_display_roots();
        let session = self.session();
        let panel = session
            .map(|s| s.stickers.masks.clone())
            .unwrap_or_default();
        let thumbs: Vec<_> = panel
            .installed
            .iter()
            .filter_map(|set| panel.set_stickers.get(&set.id))
            .flatten()
            .filter_map(|item| {
                let session = session?;
                let path = item
                    .display_file_id()
                    .into_iter()
                    .chain(item.thumb_file_id)
                    .find_map(|id| session.media.files.get(&id.0)?.usable_path())
                    .and_then(|path| sandboxed_display_path(path, &roots))?;
                Some((item.file_id, item.format, path))
            })
            .take(STRIP_LIMIT)
            .collect();
        let empty_note = if panel.failed {
            Some(tr(keys::Q_MISC_COULD_NOT_LOAD_MASKS))
        } else if panel.installed_loaded && panel.installed.is_empty() {
            Some(tr(keys::Q_MISC_NO_MASKS_INSTALLED))
        } else {
            None
        };
        div()
            .flex()
            .flex_col()
            .items_center()
            .gap_1()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_xs()
                            .font_semibold()
                            .child(tr(keys::SWITCH_MASKS)),
                    )
                    .child(
                        Button::new("photo-editor-manage-masks")
                            .label(tr(keys::Q_MISC_MANAGE_MASKS))
                            .small()
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| this.open_mask_sets(cx))),
                    ),
            )
            .children(empty_note.map(|text| div().text_xs().child(text)))
            .when(!thumbs.is_empty(), |this| {
                this.child(div().flex().flex_wrap().justify_center().gap_1().children(
                    thumbs.into_iter().map(|(file_id, format, thumb)| {
                        div()
                            .id(("photo-editor-mask", file_id.0 as u64))
                            .size(px(44.))
                            .rounded_md()
                            .cursor_pointer()
                            .hover(|style| style.bg(gpui_kit::white().opacity(0.12)))
                            .child(img(thumb).size_full().object_fit(ObjectFit::Contain))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.place_editor_sticker(file_id, format, cx);
                            }))
                    }),
                ))
            })
            .into_any_element()
    }
}
