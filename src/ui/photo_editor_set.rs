//! The photo editor's Set mode for a new sticker or emoji (tdesktop
//! `boxes/sticker_creator_box.cpp`, `PrepareEditorState`, and the corners
//! button of `PhotoEditorControls`): the picked image is a movable,
//! zoomable item on a transparent square canvas, the crop is the whole
//! canvas (or the image's own ratio with "Keep original aspect ratio"), and
//! a menu picks how round the corners come out. The pixels are in
//! `quill::sticker_image`; this is the editor state and its tools.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::photo_edit::{CropRect, Placed, Stroke, overlay_item, render};
use super::photo_editor::{EditorPurpose, HANDLE_GRAB, PhotoEditor, preview_of};
use super::photo_editor_items::ItemDrag;
use gpui_kit::component::button::*;
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::*;
use gpui_kit::*;
use image::RgbaImage;
use quill::lang::{keys, tr, tr_with};
use quill::sticker_image::{
    CANVAS_SIDE, CORNERS_ABOUT, Corners, KEEP_RATIO_LABEL, accepts_image, fitted_width,
    keep_ratio_crop, original_ratio,
};
use quill::sticker_set_box::SetKind;

/// How much one wheel pixel scales the picked image.
const ZOOM_PER_PX: f32 = 0.002;
/// The picked image's width over the canvas, at the smallest.
const BASE_MIN_WIDTH: f32 = 0.1;
/// The picked image's width over the canvas, at the largest.
pub(super) const BASE_MAX_WIDTH: f32 = 4.0;

impl PhotoEditor {
    /// An editor for an owned set: a transparent canvas with `image`
    /// fitted and centered on it as the base item. `None` for an image the
    /// editor refuses (empty, or longer than ten times its shorter side).
    pub(super) fn for_set(image: RgbaImage, set_id: i64, kind: SetKind) -> Option<Self> {
        let (width, height) = image.dimensions();
        if !accepts_image(width, height) {
            return None;
        }
        let canvas = RgbaImage::new(CANVAS_SIDE, CANVAS_SIDE);
        let mut editor = Self::on_canvas(EditorPurpose::Set { set_id, kind }, canvas);
        editor.base_item = true;
        // Emoji keep no ratio (`RunImageEditorAndCreate` zeroes it).
        editor.original_ratio = match kind {
            SetKind::Stickers => original_ratio(width, height),
            SetKind::Emoji => 0.0,
        };
        let preview = preview_of(&image);
        editor.placed.push((
            Placed {
                width: fitted_width(width, height),
                image,
                center: (0.5, 0.5),
            },
            preview.into(),
        ));
        Some(editor)
    }

    /// The crop can't be dragged: it is the canvas, or the kept ratio.
    pub(super) fn fixed_crop(&self) -> bool {
        self.base_item
    }

    /// `index` is the picked image of the Set mode.
    pub(super) fn is_base(&self, index: usize) -> bool {
        self.base_item && index == 0
    }

    /// The menu offers "Keep original aspect ratio".
    pub(super) fn offers_keep_ratio(&self) -> bool {
        self.base_item && self.original_ratio > 0.0
    }

    pub(super) fn set_corners(&mut self, corners: Corners) {
        self.corners = corners;
    }

    /// "Keep original aspect ratio": the crop becomes the largest centered
    /// rectangle of the image's ratio, or the whole canvas again.
    pub(super) fn toggle_keep_ratio(&mut self) {
        if !self.offers_keep_ratio() {
            return;
        }
        self.keep_ratio = !self.keep_ratio;
        self.crop = if self.keep_ratio {
            let (x, y, w, h) = keep_ratio_crop(self.original_ratio);
            CropRect { x, y, w, h }
        } else {
            CropRect::FULL
        };
    }

    /// A wheel turn over the canvas: scale the picked image about its
    /// center (up is bigger).
    pub(super) fn zoom_base(&mut self, delta_px: f32) {
        if !self.base_item || !delta_px.is_finite() {
            return;
        }
        if let Some((item, _)) = self.placed.first_mut() {
            let factor = (1.0 + delta_px * ZOOM_PER_PX).clamp(0.5, 2.0);
            item.width = (item.width * factor).clamp(BASE_MIN_WIDTH, BASE_MAX_WIDTH);
        }
    }

    /// A press on the canvas in Crop mode moves or resizes the picked
    /// image only (its handle is the bottom-right corner); stickers over
    /// it are left to the Stickers mode.
    pub(super) fn press_base(&mut self, (x, y): (f32, f32)) {
        let (Some(bounds), Some((base, _))) = (self.image_bounds.get(), self.placed.first()) else {
            return;
        };
        let (scale_x, scale_y) = (bounds.size.width / px(1.), bounds.size.height / px(1.));
        let (left, top, w, h) = self.item_rect(base);
        let on_handle = ((left + w - x) * scale_x).abs() <= HANDLE_GRAB
            && ((top + h - y) * scale_y).abs() <= HANDLE_GRAB;
        let inside = (left..left + w).contains(&x) && (top..top + h).contains(&y);
        self.item_drag = if on_handle {
            let distance = ((x - base.center.0) * scale_x).hypot((y - base.center.1) * scale_y);
            Some(ItemDrag::Resize {
                start_width: base.width,
                start_distance: distance.max(1.0),
            })
        } else if inside {
            Some(ItemDrag::Move {
                from: (x, y),
                start: base.center,
            })
        } else {
            None
        };
        self.selected = self.item_drag.is_some().then_some(0);
    }

    /// What "Done" renders, without the editor's UI handles, so it can
    /// run off the UI thread.
    pub(super) fn set_render_job(&self) -> SetRenderJob {
        let mut items = self.placed.iter().map(|(item, _)| item.clone());
        let base = self.base_item.then(|| items.next()).flatten();
        SetRenderJob {
            canvas: self.orientation.apply(self.working.clone()),
            base,
            rest: items.collect(),
            strokes: self.strokes.clone(),
            crop: self.crop,
        }
    }

    /// The result: the picked image drawn on the canvas first, then the
    /// strokes and the other items, cropped.
    #[cfg(test)]
    pub(super) fn render_set_image(&self) -> RgbaImage {
        self.set_render_job().render()
    }
}

/// The Set mode's canvas, items and strokes, ready to render anywhere.
pub(super) struct SetRenderJob {
    canvas: RgbaImage,
    base: Option<Placed>,
    rest: Vec<Placed>,
    strokes: Vec<Stroke>,
    crop: CropRect,
}

impl SetRenderJob {
    pub(super) fn render(&self) -> RgbaImage {
        let mut canvas = self.canvas.clone();
        if let Some(base) = &self.base {
            overlay_item(&mut canvas, base);
        }
        render(&canvas, self.crop, &self.strokes, &self.rest)
    }
}

impl QuillApp {
    /// Crop-mode tools of the Set mode: Rotate, Flip and the corners menu
    /// (`lng_photo_editor_corners_*`, plus "Keep original aspect ratio"
    /// when the image isn't square).
    pub(super) fn photo_editor_set_tools(
        &self,
        editor: &PhotoEditor,
        cx: &mut Context<Self>,
    ) -> Div {
        let owner = cx.entity().downgrade();
        let current = editor.corners;
        let keep_ratio = editor.keep_ratio;
        let offers_keep_ratio = editor.offers_keep_ratio();
        let corners_button = Button::new("photo-editor-corners")
            .label(tr_with(
                keys::Q_MISC_CORNERS,
                &[("label", (current.label()))],
            ))
            .small()
            .ghost()
            .tooltip(CORNERS_ABOUT)
            .dropdown_menu(move |mut menu, _, _| {
                menu = menu
                    .item(PopupMenuItem::label(CORNERS_ABOUT))
                    .item(PopupMenuItem::separator());
                for corners in Corners::ALL {
                    let owner = owner.clone();
                    menu = menu.item(
                        PopupMenuItem::new(corners.label())
                            .checked(corners == current)
                            .on_click(move |_, _, cx| {
                                let _ = owner.update(cx, |this, cx| {
                                    this.photo_editor_mut(|editor| editor.set_corners(corners), cx);
                                });
                            }),
                    );
                }
                if offers_keep_ratio {
                    let owner = owner.clone();
                    menu = menu.item(PopupMenuItem::separator()).item(
                        PopupMenuItem::new(KEEP_RATIO_LABEL)
                            .checked(keep_ratio)
                            .on_click(move |_, _, cx| {
                                let _ = owner.update(cx, |this, cx| {
                                    this.photo_editor_mut(PhotoEditor::toggle_keep_ratio, cx);
                                });
                            }),
                    );
                }
                menu
            });
        div()
            .flex()
            .items_center()
            .gap_2()
            .child(
                Button::new("photo-editor-rotate")
                    .icon(gpui_kit::assets::IconName::RotateCcw)
                    .label(tr(keys::Q_MISC_ROTATE))
                    .small()
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.photo_editor_mut(PhotoEditor::rotate, cx);
                    })),
            )
            .child(
                Button::new("photo-editor-flip")
                    .label(tr(keys::PHOTO_EDITOR_MENU_FLIP))
                    .small()
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.photo_editor_mut(PhotoEditor::flip, cx);
                    })),
            )
            .child(corners_button)
    }
}

#[cfg(test)]
mod tests {
    use super::super::photo_editor::PhotoEditor;
    use image::RgbaImage;
    use quill::sticker_image::Corners;
    use quill::sticker_set_box::SetKind;

    fn editor(width: u32, height: u32, kind: SetKind) -> PhotoEditor {
        let image = RgbaImage::from_pixel(width, height, image::Rgba([10, 20, 30, 255]));
        PhotoEditor::for_set(image, 78, kind).expect("accepted")
    }

    #[test]
    fn a_set_image_sits_fitted_on_a_transparent_canvas() {
        let wide = editor(400, 200, SetKind::Stickers);
        assert_eq!(wide.working.dimensions(), (512, 512));
        assert_eq!(wide.working.get_pixel(0, 0)[3], 0);
        let (base, _) = wide.placed.first().expect("base item");
        assert_eq!(base.center, (0.5, 0.5));
        assert_eq!(base.width, 1.0);
        assert!(wide.fixed_crop() && wide.is_base(0) && !wide.is_base(1));
        assert!(wide.offers_keep_ratio());

        let tall = editor(200, 400, SetKind::Stickers);
        assert_eq!(tall.placed.first().map(|(item, _)| item.width), Some(0.5));

        let emoji = editor(400, 200, SetKind::Emoji);
        assert!(!emoji.offers_keep_ratio(), "emoji keep no ratio");
        let square = editor(300, 300, SetKind::Stickers);
        assert!(!square.offers_keep_ratio());

        let long = RgbaImage::from_pixel(10, 200, image::Rgba([0, 0, 0, 255]));
        assert!(PhotoEditor::for_set(long, 78, SetKind::Stickers).is_none());
    }

    #[test]
    fn the_result_is_padded_then_masked_and_keep_ratio_cuts_the_padding() {
        let mut wide = editor(400, 200, SetKind::Stickers);
        let padded = wide.render_set_image();
        assert_eq!(padded.dimensions(), (512, 512));
        assert_eq!(padded.get_pixel(256, 10)[3], 0, "transparent band above");
        assert_eq!(
            padded.get_pixel(256, 256)[3],
            255,
            "the image in the middle"
        );

        wide.toggle_keep_ratio();
        assert!(wide.keep_ratio);
        let kept = wide.render_set_image();
        assert_eq!(kept.dimensions(), (512, 256));
        assert_eq!(kept.get_pixel(256, 128)[3], 255);
        wide.toggle_keep_ratio();
        assert_eq!(wide.crop, super::CropRect::FULL);

        let prepared = quill::sticker_image::prepared_sticker_image(&kept, 512, Corners::Large);
        assert_eq!(prepared.get_pixel(0, 0)[3], 0, "rounded corner");
        assert_eq!(prepared.get_pixel(256, 128)[3], 255);
    }

    #[test]
    fn the_wheel_zooms_the_picked_image_within_limits() {
        let mut square = editor(300, 300, SetKind::Stickers);
        square.zoom_base(100.0);
        let width = square
            .placed
            .first()
            .map(|(item, _)| item.width)
            .unwrap_or(0.0);
        assert!(width > 1.0 && width <= 1.2);
        for _ in 0..200 {
            square.zoom_base(400.0);
        }
        assert_eq!(
            square.placed.first().map(|(item, _)| item.width),
            Some(super::BASE_MAX_WIDTH)
        );
        for _ in 0..400 {
            square.zoom_base(-400.0);
        }
        assert_eq!(
            square.placed.first().map(|(item, _)| item.width),
            Some(super::BASE_MIN_WIDTH)
        );
        square.set_corners(Corners::None);
        assert_eq!(square.corners, Corners::None);
    }
}
