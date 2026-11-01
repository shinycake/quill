//! The photo editor's Stickers mode: pictures placed on the photo, moved
//! and resized with the pointer, and the strip they are picked from
//! (split from `photo_editor.rs`).
#![deny(clippy::indexing_slicing)]
use super::app::QuillApp;
use super::photo_edit::Placed;
use super::photo_editor::{EditorMode, HANDLE_GRAB, PhotoEditor, preview_of};
use crate::ui::status_note::StatusNote;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use image::RgbaImage;
use quill::lang::{keys, tr};

/// Emoji offered in the editor's Stickers mode (the common set Telegram
/// Desktop's panel opens with).
const EDITOR_EMOJI: [&str; 24] = [
    "😀", "😂", "😍", "🥰", "😎", "🤔", "😢", "😡", "👍", "👎", "❤️", "🔥", "🎉", "✨", "💯", "🙏",
    "👏", "🤝", "🎁", "⭐", "🌈", "☀️", "🌙", "⚡",
];

#[derive(Clone, Copy)]
pub(super) enum ItemDrag {
    Move {
        from: (f32, f32),
        start: (f32, f32),
    },
    Resize {
        start_width: f32,
        start_distance: f32,
    },
}

impl PhotoEditor {
    /// Place a picture in the middle of the crop, a third of its width,
    /// and select it.
    fn place(&mut self, image: RgbaImage) {
        let crop = self.crop;
        let preview = preview_of(&image);
        self.placed.push((
            Placed {
                image,
                center: (crop.x + crop.w / 2.0, crop.y + crop.h / 2.0),
                width: crop.w / 3.0,
            },
            preview.into(),
        ));
        self.selected = Some(self.placed.len() - 1);
        self.mode = EditorMode::Stickers;
    }

    /// Remove the selected item; the Set mode's picked image stays.
    fn delete_selected(&mut self) {
        if let Some(index) = self.selected.take()
            && index < self.placed.len()
            && !self.is_base(index)
        {
            self.placed.remove(index);
        }
    }

    /// An item's rectangle in normalized coordinates (left, top, w, h).
    pub(super) fn item_rect(&self, item: &Placed) -> (f32, f32, f32, f32) {
        let (width, height) = self.dimensions();
        let w = item.width;
        let h = item.width * item.aspect() * width as f32 / height.max(1) as f32;
        (item.center.0 - w / 2.0, item.center.1 - h / 2.0, w, h)
    }

    pub(super) fn press_item(&mut self, (x, y): (f32, f32)) {
        let Some(bounds) = self.image_bounds.get() else {
            return;
        };
        let (scale_x, scale_y) = (bounds.size.width / px(1.), bounds.size.height / px(1.));
        // The selected item's resize handle (bottom-right corner) first.
        if let Some(index) = self.selected
            && let Some((item, _)) = self.placed.get(index)
        {
            let (left, top, w, h) = self.item_rect(item);
            let (hx, hy) = (left + w, top + h);
            if ((hx - x) * scale_x).abs() <= HANDLE_GRAB
                && ((hy - y) * scale_y).abs() <= HANDLE_GRAB
            {
                let distance = ((x - item.center.0) * scale_x).hypot((y - item.center.1) * scale_y);
                self.item_drag = Some(ItemDrag::Resize {
                    start_width: item.width,
                    start_distance: distance.max(1.0),
                });
                return;
            }
        }
        let hit = self.placed.iter().rposition(|(item, _)| {
            let (left, top, w, h) = self.item_rect(item);
            (left..left + w).contains(&x) && (top..top + h).contains(&y)
        });
        self.selected = hit;
        self.item_drag = hit.and_then(|index| {
            Some(ItemDrag::Move {
                from: (x, y),
                start: self.placed.get(index)?.0.center,
            })
        });
    }

    pub(super) fn drag_item(&mut self, (x, y): (f32, f32)) {
        let (Some(index), Some(drag), Some(bounds)) =
            (self.selected, self.item_drag, self.image_bounds.get())
        else {
            return;
        };
        let Some((item, _)) = self.placed.get_mut(index) else {
            return;
        };
        match drag {
            ItemDrag::Move { from, start } => {
                item.center = (
                    (start.0 + x - from.0).clamp(0.0, 1.0),
                    (start.1 + y - from.1).clamp(0.0, 1.0),
                );
            }
            ItemDrag::Resize {
                start_width,
                start_distance,
            } => {
                let (scale_x, scale_y) = (bounds.size.width / px(1.), bounds.size.height / px(1.));
                let distance = ((x - item.center.0) * scale_x).hypot((y - item.center.1) * scale_y);
                let largest = if self.base_item && index == 0 {
                    super::photo_editor_set::BASE_MAX_WIDTH
                } else {
                    2.0
                };
                item.width = (start_width * distance / start_distance).clamp(0.03, largest);
            }
        }
    }
}

impl QuillApp {
    /// Stickers mode: an emoji row, your recent and favorite stickers, and
    /// Delete for the selected one.
    pub(super) fn photo_editor_sticker_tools(&self, cx: &mut Context<Self>) -> AnyElement {
        let has_selection = self
            .viewer
            .photo_editor
            .as_ref()
            .is_some_and(|editor| editor.selected.is_some_and(|index| !editor.is_base(index)));
        let roots = self.media_display_roots();
        let mut seen = std::collections::HashSet::new();
        // Thumbnails not on disk yet: fetch them; the strip fills in.
        let missing: Vec<quill::ids::FileId> = self
            .session()
            .map(|session| {
                session
                    .stickers
                    .stickers
                    .recent
                    .iter()
                    .chain(&session.stickers.stickers.favorites)
                    .take(16)
                    .map(|item| item.thumb_file_id.unwrap_or(item.file_id))
                    .filter(|id| session.should_download(*id))
                    .collect()
            })
            .unwrap_or_default();
        if !missing.is_empty() {
            let app = cx.entity().downgrade();
            cx.defer(move |cx| {
                let _ = app.update(cx, |this, _| {
                    if let Some(live) = this.live.as_mut() {
                        for id in missing {
                            let _ = live.driver.download_file(id, 8);
                        }
                    }
                });
            });
        }
        let stickers: Vec<_> = self
            .session()
            .map(|session| {
                session
                    .stickers
                    .stickers
                    .recent
                    .iter()
                    .chain(&session.stickers.stickers.favorites)
                    .filter(|item| seen.insert(item.file_id.0))
                    // One row under the emoji.
                    .take(16)
                    .filter_map(|item| {
                        let thumb = item
                            .thumb_file_id
                            .into_iter()
                            .chain([item.file_id])
                            .find_map(|id| session.media.files.get(&id.0)?.usable_path())
                            .and_then(|path| {
                                quill::local_path::sandboxed_display_path(path, &roots)
                            })?;
                        Some((item.file_id, item.format, thumb))
                    })
                    .collect()
            })
            .unwrap_or_default();
        div()
            .flex()
            .flex_col()
            .items_center()
            .gap_2()
            .max_w(px(760.))
            .child(div().flex().flex_wrap().justify_center().gap_1().children(
                EDITOR_EMOJI.iter().enumerate().map(|(index, emoji)| {
                    let emoji = *emoji;
                    div()
                        .id(("photo-editor-emoji", index as u64))
                        .size(px(32.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_md()
                        .text_xl()
                        .cursor_pointer()
                        .hover(|style| style.bg(gpui_kit::white().opacity(0.12)))
                        .child(emoji)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            match super::editor_art::rasterize_emoji(emoji, 256.0) {
                                Some(image) => {
                                    this.photo_editor_mut(|editor| editor.place(image), cx)
                                }
                                None => {
                                    this.connection.set_note(StatusNote::error(tr(
                                        keys::Q_MISC_COULDNT_DRAW_THAT_EMOJI,
                                    )));
                                    cx.notify();
                                }
                            }
                        }))
                }),
            ))
            .child(div().flex().flex_wrap().justify_center().gap_1().children(
                stickers.into_iter().map(|(file_id, format, thumb)| {
                    div()
                        .id(("photo-editor-sticker", file_id.0 as u64))
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
            .child(self.photo_editor_mask_strip(cx))
            .when(has_selection, |this| {
                this.child(
                    Button::new("photo-editor-delete-item")
                        .icon(gpui_kit::assets::IconName::Trash)
                        .label(tr(keys::PHOTO_EDITOR_MENU_DELETE))
                        .small()
                        .ghost()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.photo_editor_mut(PhotoEditor::delete_selected, cx);
                        })),
                )
            })
            .into_any_element()
    }

    /// Place a sticker's still picture, downloading the sticker first if
    /// needed (click again once it has arrived).
    pub(super) fn place_editor_sticker(
        &mut self,
        file_id: quill::ids::FileId,
        format: quill::telegram::envelope::StickerFormat,
        cx: &mut Context<Self>,
    ) {
        let roots = self.media_display_roots();
        let path = self
            .session()
            .and_then(|session| {
                session
                    .media
                    .files
                    .get(&file_id.0)?
                    .usable_path()
                    .map(str::to_string)
            })
            .and_then(|path| quill::local_path::sandboxed_display_path(&path, &roots));
        let Some(path) = path else {
            if let Some(live) = self.live.as_mut() {
                let _ = live.driver.download_file(file_id, 16);
            }
            self.connection
                .set_note(StatusNote::silent(tr(keys::Q_MISC_DOWNLOADING_THE_STICKER)));
            cx.notify();
            return;
        };
        self.run_off_ui_scoped_then(
            move || {
                quill::thread_guard::ffi_boundary(None, || {
                    super::editor_art::sticker_pixels(std::path::Path::new(&path), format)
                })
            },
            |this, pixels, cx| match pixels {
                Some(image) => this.photo_editor_mut(|editor| editor.place(image), cx),
                None => {
                    this.connection
                        .set_note(StatusNote::error(tr(keys::Q_MISC_COULDNT_USE_THAT_STICKER)));
                    cx.notify();
                }
            },
            cx,
        );
    }
}
