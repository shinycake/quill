//! The photo editor for a picked photo (Telegram Desktop's photo editor):
//! crop with handles, quarter turns, flip, and freehand drawing with a
//! color palette, brush sizes and undo, plus stickers and emoji placed on
//! the photo, moved and resized with the pointer. The pixels live in
//! `photo_edit`; this is the overlay, its canvas and the mouse handling.

use super::app::QuillApp;
use super::photo_edit::{CropRect, Placed, Stroke, flip, render, rotate_ccw};
use super::photo_editor_items::ItemDrag;
use quill::sticker_image::Corners as StickerCorners;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use image::RgbaImage;
use quill::sticker_set_box::SetKind;
use smallvec::SmallVec;
use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

/// Telegram Desktop's brush palette.
const COLORS: [[u8; 4]; 8] = [
    [255, 255, 255, 255],
    [0, 0, 0, 255],
    [255, 59, 48, 255],
    [255, 149, 0, 255],
    [255, 214, 10, 255],
    [52, 199, 89, 255],
    [10, 132, 255, 255],
    [175, 82, 222, 255],
];

/// Brush widths as a fraction of the image's shorter side.
const SIZES: [f32; 3] = [0.006, 0.014, 0.03];

/// Grab distance around a crop corner, in screen pixels.
pub(super) const HANDLE_GRAB: f32 = 14.0;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum EditorMode {
    Crop,
    Draw,
    Stickers,
}

/// What the edited picture is for.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum EditorPurpose {
    /// A pending composer attachment, by index.
    Attachment(usize),
    /// The media of a message being edited (`edit_media_tools.rs`).
    MessageEdit,
    /// A new sticker or emoji for a set the user owns: the image sits on a
    /// transparent square canvas, the crop is fixed, and the result is
    /// fitted to the set's side (tdesktop's `fixedCrop` editor from
    /// `StickerCreatorBox`; `photo_editor_set.rs`).
    Set { set_id: i64, kind: SetKind },
}

#[derive(Clone, Copy)]
enum CropDrag {
    /// Corner 0..4: top-left, top-right, bottom-right, bottom-left.
    Corner(usize),
    Move {
        from: (f32, f32),
        start: CropRect,
    },
}

pub(super) struct PhotoEditor {
    pub(super) purpose: EditorPurpose,
    pub(super) working: RgbaImage,
    preview: Arc<RenderImage>,
    pub(super) crop: CropRect,
    pub(super) strokes: Vec<Stroke>,
    current: Option<Stroke>,
    pub(super) mode: EditorMode,
    color: usize,
    size: usize,
    drag: Option<CropDrag>,
    /// Stickers and emoji on the photo, each with its display copy.
    pub(super) placed: Vec<(Placed, Arc<RenderImage>)>,
    pub(super) selected: Option<usize>,
    pub(super) item_drag: Option<ItemDrag>,
    /// Where the image was painted last frame, for mapping the pointer.
    pub(super) image_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// Set mode: `placed[0]` is the picked image on the transparent
    /// canvas, movable and zoomable, never deleted.
    pub(super) base_item: bool,
    /// Set mode: how round the sticker's corners come out.
    pub(super) corners: StickerCorners,
    /// Set mode: "Keep original aspect ratio" is on.
    pub(super) keep_ratio: bool,
    /// Set mode: the picked image's ratio when it may be kept, else 0.
    pub(super) original_ratio: f64,
}

/// A display copy of `image` (at most 1600 px), in GPUI's BGRA order.
pub(super) fn preview_of(image: &RgbaImage) -> Arc<RenderImage> {
    let (width, height) = image.dimensions();
    let scale = (1600.0 / width.max(height) as f32).min(1.0);
    let mut small = if scale < 1.0 {
        image::imageops::thumbnail(
            image,
            ((width as f32 * scale) as u32).max(1),
            ((height as f32 * scale) as u32).max(1),
        )
    } else {
        image.clone()
    };
    for pixel in small.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    Arc::new(RenderImage::new(SmallVec::from_buf([image::Frame::new(
        small,
    )])))
}

impl PhotoEditor {
    fn new(purpose: EditorPurpose, working: RgbaImage) -> Self {
        Self {
            purpose,
            preview: preview_of(&working),
            working,
            crop: CropRect::FULL,
            strokes: Vec::new(),
            current: None,
            mode: EditorMode::Crop,
            color: 2,
            size: 1,
            drag: None,
            placed: Vec::new(),
            selected: None,
            item_drag: None,
            image_bounds: Rc::new(Cell::new(None)),
            base_item: false,
            corners: StickerCorners::default(),
            keep_ratio: false,
            original_ratio: 0.0,
        }
    }

    /// An editor on the photo of a message being edited.
    pub(super) fn for_message_edit(working: RgbaImage) -> Self {
        Self::new(EditorPurpose::MessageEdit, working)
    }

    /// An editor on a transparent canvas with `working` as its base item
    /// (`photo_editor_set.rs` fills the rest in).
    pub(super) fn on_canvas(purpose: EditorPurpose, canvas: RgbaImage) -> Self {
        Self::new(purpose, canvas)
    }

    pub(super) fn rotate(&mut self) {
        let (width, height) = self.working.dimensions();
        self.placed = self
            .placed
            .iter()
            .map(|(item, _)| {
                let item = item.rotated_ccw(width, height);
                let preview = preview_of(&item.image);
                (item, preview)
            })
            .collect();
        self.working = rotate_ccw(&self.working);
        self.preview = preview_of(&self.working);
        self.crop = self.crop.rotated_ccw();
        self.strokes = self.strokes.iter().map(Stroke::rotated_ccw).collect();
    }

    pub(super) fn flip(&mut self) {
        self.placed = self
            .placed
            .iter()
            .map(|(item, _)| {
                let item = item.flipped();
                let preview = preview_of(&item.image);
                (item, preview)
            })
            .collect();
        self.working = flip(&self.working);
        self.preview = preview_of(&self.working);
        self.crop = self.crop.flipped();
        self.strokes = self.strokes.iter().map(Stroke::flipped).collect();
    }

    /// The pointer in normalized image coordinates (may fall outside 0..1).
    fn normalized(&self, position: Point<Pixels>) -> Option<(f32, f32)> {
        let bounds = self.image_bounds.get()?;
        Some((
            (position.x - bounds.origin.x) / bounds.size.width,
            (position.y - bounds.origin.y) / bounds.size.height,
        ))
    }

    fn press(&mut self, position: Point<Pixels>) {
        let Some((x, y)) = self.normalized(position) else {
            return;
        };
        match self.mode {
            EditorMode::Stickers => self.press_item((x, y)),
            EditorMode::Draw => {
                self.current = Some(Stroke {
                    color: COLORS[self.color],
                    width: SIZES[self.size],
                    points: vec![(x, y)],
                });
            }
            EditorMode::Crop if self.fixed_crop() => self.press_base((x, y)),
            EditorMode::Crop => {
                let Some(bounds) = self.image_bounds.get() else {
                    return;
                };
                let crop = self.crop;
                let corners = [
                    (crop.x, crop.y),
                    (crop.x + crop.w, crop.y),
                    (crop.x + crop.w, crop.y + crop.h),
                    (crop.x, crop.y + crop.h),
                ];
                let near = corners.iter().position(|&(cx, cy)| {
                    let dx = (cx - x) * (bounds.size.width / px(1.));
                    let dy = (cy - y) * (bounds.size.height / px(1.));
                    dx.abs() <= HANDLE_GRAB && dy.abs() <= HANDLE_GRAB
                });
                self.drag = match near {
                    Some(corner) => Some(CropDrag::Corner(corner)),
                    None if (crop.x..crop.x + crop.w).contains(&x)
                        && (crop.y..crop.y + crop.h).contains(&y) =>
                    {
                        Some(CropDrag::Move {
                            from: (x, y),
                            start: crop,
                        })
                    }
                    None => None,
                };
            }
        }
    }

    fn drag_to(&mut self, position: Point<Pixels>) {
        let Some((x, y)) = self.normalized(position) else {
            return;
        };
        if let Some(stroke) = self.current.as_mut() {
            stroke.points.push((x.clamp(0.0, 1.0), y.clamp(0.0, 1.0)));
            return;
        }
        if self.item_drag.is_some() {
            self.drag_item((x, y));
            return;
        }
        let (x, y) = (x.clamp(0.0, 1.0), y.clamp(0.0, 1.0));
        match self.drag {
            Some(CropDrag::Move { from, start }) => {
                self.crop = CropRect {
                    x: start.x + x - from.0,
                    y: start.y + y - from.1,
                    ..start
                }
                .clamped();
            }
            Some(CropDrag::Corner(corner)) => {
                let crop = self.crop;
                let (mut left, mut top) = (crop.x, crop.y);
                let (mut right, mut bottom) = (crop.x + crop.w, crop.y + crop.h);
                let min = CropRect::MIN_SIDE;
                match corner {
                    0 => (left, top) = (x.min(right - min), y.min(bottom - min)),
                    1 => (right, top) = (x.max(left + min), y.min(bottom - min)),
                    2 => (right, bottom) = (x.max(left + min), y.max(top + min)),
                    _ => (left, bottom) = (x.min(right - min), y.max(top + min)),
                }
                self.crop = CropRect {
                    x: left,
                    y: top,
                    w: right - left,
                    h: bottom - top,
                }
                .clamped();
            }
            None => {}
        }
    }

    fn release(&mut self) {
        if let Some(stroke) = self.current.take() {
            self.strokes.push(stroke);
        }
        self.drag = None;
        self.item_drag = None;
    }
}

impl QuillApp {
    /// Open the editor on a pending photo attachment.
    pub(super) fn open_photo_editor(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(attachment) = self.composer_ui.pending_attachments.get(index) else {
            return;
        };
        match image::open(&attachment.path) {
            Ok(image) => {
                self.viewer.photo_editor = Some(PhotoEditor::new(
                    EditorPurpose::Attachment(index),
                    image.to_rgba8(),
                ));
            }
            Err(_) => self.connection.status_note = "Couldn't open this image for editing.".into(),
        }
        cx.notify();
    }

    pub(super) fn close_photo_editor(&mut self, cx: &mut Context<Self>) {
        self.viewer.photo_editor = None;
        cx.notify();
    }

    /// Render the edit at full resolution into a new PNG and send that
    /// instead of the original.
    fn save_photo_editor(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = self.viewer.photo_editor.take() else {
            return;
        };
        if let EditorPurpose::Set { set_id, kind } = editor.purpose {
            let edited = editor.render_set_image();
            self.finish_set_image(set_id, kind, &edited, editor.corners, cx);
            return;
        }
        let placed: Vec<Placed> = editor.placed.iter().map(|(item, _)| item.clone()).collect();
        let edited = render(&editor.working, editor.crop, &editor.strokes, &placed);
        let index = match editor.purpose {
            EditorPurpose::Attachment(index) => index,
            EditorPurpose::MessageEdit => {
                let saved = super::edit_media_tools::write_edited_png(&edited);
                self.finish_edit_image(saved, cx);
                return;
            }
            EditorPurpose::Set { .. } => return,
        };
        let saved = super::edit_media_tools::write_edited_png(&edited);
        match (saved, self.composer_ui.pending_attachments.get_mut(index)) {
            (Some(path), Some(attachment)) => {
                let stem = attachment
                    .path
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .unwrap_or("photo")
                    .to_string();
                attachment.path = path;
                attachment.file_name = format!("{stem}.png");
                attachment.kind = quill::composer::AttachmentKind::Photo;
            }
            _ => self.connection.status_note = "Couldn't save the edited photo.".into(),
        }
        cx.notify();
    }

    pub(super) fn photo_editor_mut(
        &mut self,
        edit: impl FnOnce(&mut PhotoEditor),
        cx: &mut Context<Self>,
    ) {
        if let Some(editor) = self.viewer.photo_editor.as_mut() {
            edit(editor);
            cx.notify();
        }
    }

    /// The editor overlay: canvas in the middle, tools below.
    pub(super) fn photo_editor_overlay(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let editor = self.viewer.photo_editor.as_ref()?;
        let app = cx.entity().downgrade();
        let preview = editor.preview.clone();
        let preview_size = preview.size(0);
        let (preview_w, preview_h) = (preview_size.width.0 as f32, preview_size.height.0 as f32);
        let crop = editor.crop;
        let mode = editor.mode;
        let strokes: Vec<Stroke> = editor
            .strokes
            .iter()
            .chain(editor.current.as_ref())
            .cloned()
            .collect();
        let image_bounds = editor.image_bounds.clone();
        let items: Vec<((f32, f32, f32, f32), Arc<RenderImage>)> = editor
            .placed
            .iter()
            .map(|(item, preview)| (editor.item_rect(item), preview.clone()))
            .collect();
        let selected_item = editor.selected.filter(|_| mode == EditorMode::Stickers);
        let base_item = editor.base_item;
        let corner_multiplier = editor.corners.multiplier();
        let surface = canvas(
            move |bounds, window, _| window.insert_hitbox(bounds, HitboxBehavior::Normal),
            move |bounds, hitbox, window, _| {
                // Fit the picture into the canvas.
                // Fit (and scale small pictures up), like Telegram Desktop.
                let scale =
                    (bounds.size.width / px(preview_w)).min(bounds.size.height / px(preview_h));
                let fit_size = size(px(preview_w * scale), px(preview_h * scale));
                let origin = point(
                    bounds.origin.x + (bounds.size.width - fit_size.width) / 2.,
                    bounds.origin.y + (bounds.size.height - fit_size.height) / 2.,
                );
                let fit = Bounds {
                    origin,
                    size: fit_size,
                };
                image_bounds.set(Some(fit));
                let _ = window.paint_image(fit, fit, Corners::default(), preview, 0, false);
                let at = |(x, y): (f32, f32)| {
                    point(
                        origin.x + fit_size.width * x,
                        origin.y + fit_size.height * y,
                    )
                };
                let side = fit_size.width.min(fit_size.height) / px(1.);
                // Set mode: the picked image is the ground the drawing and
                // the other items go on.
                if base_item && let Some(((left, top, w, h), picture)) = items.first() {
                    let item_bounds =
                        Bounds::from_corners(at((*left, *top)), at((left + w, top + h)));
                    paint_editor_item(window, item_bounds, picture.clone(), selected_item == Some(0));
                }
                for stroke in &strokes {
                    let color = Rgba {
                        r: f32::from(stroke.color[0]) / 255.,
                        g: f32::from(stroke.color[1]) / 255.,
                        b: f32::from(stroke.color[2]) / 255.,
                        a: 1.,
                    };
                    let mut path = PathBuilder::stroke(px((stroke.width * side).max(1.)));
                    let mut points = stroke.points.iter().copied().map(at);
                    if let Some(first) = points.next() {
                        path.move_to(first);
                        path.line_to(point(first.x + px(0.1), first.y));
                        for point in points {
                            path.line_to(point);
                        }
                    }
                    if let Ok(path) = path.build() {
                        window.paint_path(path, color);
                    }
                }
                // Stickers and emoji over the drawing; the selected one gets
                // a frame and a resize handle.
                for (index, ((left, top, w, h), picture)) in items
                    .iter()
                    .enumerate()
                    .skip(usize::from(base_item))
                {
                    let item_bounds =
                        Bounds::from_corners(at((*left, *top)), at((left + w, top + h)));
                    paint_editor_item(window, item_bounds, picture.clone(), selected_item == Some(index));
                }
                // Outside the crop is shaded in all modes, so editing
                // shows what will be sent; handles only while cropping.
                {
                    let shade = gpui_kit::black().opacity(if mode == EditorMode::Crop {
                        0.55
                    } else {
                        0.75
                    });
                    let crop_bounds = Bounds::from_corners(
                        at((crop.x, crop.y)),
                        at((crop.x + crop.w, crop.y + crop.h)),
                    );
                    for band in [
                        Bounds::from_corners(fit.origin, point(fit.right(), crop_bounds.top())),
                        Bounds::from_corners(
                            point(fit.left(), crop_bounds.bottom()),
                            fit.bottom_right(),
                        ),
                        Bounds::from_corners(
                            point(fit.left(), crop_bounds.top()),
                            point(crop_bounds.left(), crop_bounds.bottom()),
                        ),
                        Bounds::from_corners(
                            point(crop_bounds.right(), crop_bounds.top()),
                            point(fit.right(), crop_bounds.bottom()),
                        ),
                    ] {
                        window.paint_quad(fill(band, shade));
                    }
                    if base_item {
                        // The fixed sticker frame, rounded like the corners
                        // choice; no handles, as it can't be dragged.
                        let radius = crop_bounds.size.width.min(crop_bounds.size.height)
                            * corner_multiplier;
                        window.paint_quad(quad(
                            crop_bounds,
                            Corners::all(radius),
                            gpui_kit::transparent_black(),
                            px(1.),
                            gpui_kit::white().opacity(0.8),
                            BorderStyle::Solid,
                        ));
                    } else if mode == EditorMode::Crop {
                        window.paint_quad(outline(
                            crop_bounds,
                            gpui_kit::white(),
                            BorderStyle::Solid,
                        ));
                        for corner in [
                            crop_bounds.origin,
                            crop_bounds.top_right(),
                            crop_bounds.bottom_right(),
                            crop_bounds.bottom_left(),
                        ] {
                            window.paint_quad(fill(
                                Bounds::centered_at(corner, size(px(10.), px(10.))),
                                gpui_kit::white(),
                            ));
                        }
                    }
                }
                let cursor = if mode == EditorMode::Draw {
                    CursorStyle::Crosshair
                } else {
                    CursorStyle::PointingHand
                };
                window.set_cursor_style(cursor, &hitbox);
                let down_app = app.clone();
                let down_hitbox = hitbox.clone();
                window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
                    if phase == DispatchPhase::Bubble
                        && event.button == MouseButton::Left
                        && down_hitbox.is_hovered(window)
                    {
                        let position = event.position;
                        let _ = down_app.update(cx, |this, cx| {
                            this.photo_editor_mut(|editor| editor.press(position), cx)
                        });
                    }
                });
                let move_app = app.clone();
                window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                    if phase == DispatchPhase::Bubble
                        && event.pressed_button == Some(MouseButton::Left)
                    {
                        let position = event.position;
                        let _ = move_app.update(cx, |this, cx| {
                            this.photo_editor_mut(|editor| editor.drag_to(position), cx)
                        });
                    }
                });
                let up_app = app.clone();
                window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
                    if phase == DispatchPhase::Bubble && event.button == MouseButton::Left {
                        let _ = up_app.update(cx, |this, cx| {
                            this.photo_editor_mut(PhotoEditor::release, cx)
                        });
                    }
                });
                if base_item {
                    // The wheel zooms the picked image (tdesktop's scene
                    // zoom in the sticker editor).
                    let wheel_hitbox = hitbox.clone();
                    window.on_mouse_event(move |event: &ScrollWheelEvent, phase, window, cx| {
                        if phase == DispatchPhase::Bubble && wheel_hitbox.is_hovered(window) {
                            let delta = event.delta.pixel_delta(px(20.)).y / px(1.);
                            let _ = app.update(cx, |this, cx| {
                                this.photo_editor_mut(|editor| editor.zoom_base(delta), cx)
                            });
                        }
                    });
                }
            },
        )
        .size_full();

        let stickers_tools =
            (mode == EditorMode::Stickers).then(|| self.photo_editor_sticker_tools(cx));
        let mode_button = |id: &'static str,
                           label: &'static str,
                           icon: gpui_kit::assets::IconName,
                           target: EditorMode| {
            Button::new(id)
                .icon(icon)
                .label(label)
                .small()
                .ghost()
                .selected(mode == target)
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(live) = this.live.as_mut() {
                        if target == EditorMode::Stickers {
                            let _ = live.driver.fetch_editor_stickers();
                            let _ = live.driver.load_editor_masks();
                        } else {
                            live.driver.release_editor_masks();
                        }
                    }
                    this.photo_editor_mut(|editor| editor.mode = target, cx);
                }))
        };
        let tools = match mode {
            EditorMode::Stickers => div()
                .flex()
                .flex_col()
                .items_center()
                .gap_2()
                .children(stickers_tools),
            EditorMode::Crop if base_item => self.photo_editor_set_tools(editor, cx),
            EditorMode::Crop => div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    Button::new("photo-editor-rotate")
                        .icon(gpui_kit::assets::IconName::RotateCcw)
                        .label("Rotate")
                        .small()
                        .ghost()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.photo_editor_mut(PhotoEditor::rotate, cx);
                        })),
                )
                .child(
                    Button::new("photo-editor-flip")
                        .label("Flip")
                        .small()
                        .ghost()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.photo_editor_mut(PhotoEditor::flip, cx);
                        })),
                )
                .child(
                    Button::new("photo-editor-reset-crop")
                        .label("Reset")
                        .small()
                        .ghost()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.photo_editor_mut(|editor| editor.crop = CropRect::FULL, cx);
                        })),
                ),
            EditorMode::Draw => {
                let (current_color, current_size) = (editor.color, editor.size);
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .children(COLORS.iter().enumerate().map(|(index, rgba)| {
                        let color = Rgba {
                            r: f32::from(rgba[0]) / 255.,
                            g: f32::from(rgba[1]) / 255.,
                            b: f32::from(rgba[2]) / 255.,
                            a: 1.,
                        };
                        div()
                            .id(("photo-editor-color", index as u64))
                            .size(px(22.))
                            .rounded_full()
                            .bg(color)
                            .border_2()
                            .border_color(if index == current_color {
                                gpui_kit::white()
                            } else {
                                gpui_kit::white().opacity(0.25)
                            })
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.photo_editor_mut(|editor| editor.color = index, cx);
                            }))
                    }))
                    .child(div().w(px(8.)))
                    .children(SIZES.iter().enumerate().map(|(index, _)| {
                        let dot = 4. + index as f32 * 5.;
                        div()
                            .id(("photo-editor-size", index as u64))
                            .size(px(26.))
                            .rounded_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .when(index == current_size, |this| {
                                this.bg(gpui_kit::white().opacity(0.15))
                            })
                            .cursor_pointer()
                            .child(div().size(px(dot)).rounded_full().bg(gpui_kit::white()))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.photo_editor_mut(|editor| editor.size = index, cx);
                            }))
                    }))
                    .child(
                        Button::new("photo-editor-undo")
                            .icon(gpui_kit::assets::IconName::Undo2)
                            .label("Undo")
                            .small()
                            .ghost()
                            .disabled(editor.strokes.is_empty())
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.photo_editor_mut(
                                    |editor| {
                                        editor.strokes.pop();
                                    },
                                    cx,
                                );
                            })),
                    )
            }
        };
        Some(
            div()
                .id("photo-editor")
                .absolute()
                .inset_0()
                .occlude()
                .bg(gpui_kit::black().opacity(0.97))
                // Clear of the window's title-bar buttons.
                .pt_6()
                .text_color(gpui_kit::white())
                .flex()
                .flex_col()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .px_4()
                        .py_2()
                        .child(
                            Button::new("photo-editor-cancel")
                                .label("Cancel")
                                .ghost()
                                .on_click(
                                    cx.listener(|this, _, _, cx| this.close_photo_editor(cx)),
                                ),
                        )
                        .child(div().font_semibold().child("Edit photo"))
                        .child(
                            Button::new("photo-editor-done")
                                .label("Done")
                                .primary()
                                .on_click(cx.listener(|this, _, _, cx| this.save_photo_editor(cx))),
                        ),
                )
                .child(div().flex_1().min_h_0().p_6().child(surface))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap_2()
                        .pb_4()
                        .child(tools)
                        .child(
                            div()
                                .flex()
                                .gap_2()
                                .child(mode_button(
                                    "photo-editor-mode-crop",
                                    "Crop",
                                    gpui_kit::assets::IconName::Crop,
                                    EditorMode::Crop,
                                ))
                                .child(mode_button(
                                    "photo-editor-mode-draw",
                                    "Draw",
                                    gpui_kit::assets::IconName::Pencil,
                                    EditorMode::Draw,
                                ))
                                .child(mode_button(
                                    "photo-editor-mode-stickers",
                                    "Stickers",
                                    gpui_kit::assets::IconName::FaceSlightlySmiling,
                                    EditorMode::Stickers,
                                )),
                        ),
                )
                .into_any_element(),
        )
    }
}

/// A placed picture, with a dashed frame and a resize handle when selected.
fn paint_editor_item(
    window: &mut Window,
    item_bounds: Bounds<Pixels>,
    picture: Arc<RenderImage>,
    selected: bool,
) {
    let _ = window.paint_image(item_bounds, item_bounds, Corners::default(), picture, 0, false);
    if selected {
        window.paint_quad(outline(item_bounds, gpui_kit::white(), BorderStyle::Dashed));
        window.paint_quad(fill(
            Bounds::centered_at(item_bounds.bottom_right(), size(px(12.), px(12.))),
            gpui_kit::white(),
        ));
    }
}
