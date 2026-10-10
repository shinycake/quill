//! The photo editor for a picked photo (Telegram Desktop's photo editor):
//! crop with handles, quarter turns, flip, and freehand drawing with a
//! color palette, brush sizes and undo, plus stickers and emoji placed on
//! the photo, moved and resized with the pointer. The pixels live in
//! `photo_edit`; this is the overlay, its canvas and the mouse handling.

use super::app::QuillApp;
use super::photo_edit::{CropRect, Placed, Stroke, flip, render, rotate_ccw};
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use image::RgbaImage;
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
const HANDLE_GRAB: f32 = 14.0;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum EditorMode {
    Crop,
    Draw,
    Stickers,
}

/// Emoji offered in the editor's Stickers mode (the common set Telegram
/// Desktop's panel opens with).
const EDITOR_EMOJI: [&str; 24] = [
    "😀", "😂", "😍", "🥰", "😎", "🤔", "😢", "😡", "👍", "👎", "❤️", "🔥", "🎉", "✨", "💯", "🙏",
    "👏", "🤝", "🎁", "⭐", "🌈", "☀️", "🌙", "⚡",
];

#[derive(Clone, Copy)]
enum ItemDrag {
    Move {
        from: (f32, f32),
        start: (f32, f32),
    },
    Resize {
        start_width: f32,
        start_distance: f32,
    },
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
    /// Index of the attachment being edited.
    index: usize,
    working: RgbaImage,
    preview: Arc<RenderImage>,
    crop: CropRect,
    strokes: Vec<Stroke>,
    current: Option<Stroke>,
    mode: EditorMode,
    color: usize,
    size: usize,
    drag: Option<CropDrag>,
    /// Stickers and emoji on the photo, each with its display copy.
    placed: Vec<(Placed, Arc<RenderImage>)>,
    selected: Option<usize>,
    item_drag: Option<ItemDrag>,
    /// Where the image was painted last frame, for mapping the pointer.
    image_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
}

/// A display copy of `image` (at most 1600 px), in GPUI's BGRA order.
fn preview_of(image: &RgbaImage) -> Arc<RenderImage> {
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
    fn new(index: usize, working: RgbaImage) -> Self {
        Self {
            index,
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
        }
    }

    fn rotate(&mut self) {
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

    fn flip(&mut self) {
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
            preview,
        ));
        self.selected = Some(self.placed.len() - 1);
        self.mode = EditorMode::Stickers;
    }

    fn delete_selected(&mut self) {
        if let Some(index) = self.selected.take()
            && index < self.placed.len()
        {
            self.placed.remove(index);
        }
    }

    /// An item's rectangle in normalized coordinates (left, top, w, h).
    fn item_rect(&self, item: &Placed) -> (f32, f32, f32, f32) {
        let (width, height) = self.working.dimensions();
        let w = item.width;
        let h = item.width * item.aspect() * width as f32 / height.max(1) as f32;
        (item.center.0 - w / 2.0, item.center.1 - h / 2.0, w, h)
    }

    fn press_item(&mut self, (x, y): (f32, f32)) {
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
        self.item_drag = hit.map(|index| ItemDrag::Move {
            from: (x, y),
            start: self.placed[index].0.center,
        });
    }

    fn drag_item(&mut self, (x, y): (f32, f32)) {
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
                item.width = (start_width * distance / start_distance).clamp(0.03, 2.0);
            }
        }
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
                self.viewer.photo_editor = Some(PhotoEditor::new(index, image.to_rgba8()));
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
        let placed: Vec<Placed> = editor.placed.iter().map(|(item, _)| item.clone()).collect();
        let edited = render(&editor.working, editor.crop, &editor.strokes, &placed);
        let dir = std::env::temp_dir().join("quill-edits");
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos());
        let path = dir.join(format!("edit-{stamp}.png"));
        let saved = std::fs::create_dir_all(&dir)
            .ok()
            .and_then(|()| edited.save(&path).ok())
            .and_then(|()| std::fs::canonicalize(&path).ok());
        match (
            saved,
            self.composer_ui.pending_attachments.get_mut(editor.index),
        ) {
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

    fn photo_editor_mut(&mut self, edit: impl FnOnce(&mut PhotoEditor), cx: &mut Context<Self>) {
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
                for (index, ((left, top, w, h), picture)) in items.iter().enumerate() {
                    let item_bounds =
                        Bounds::from_corners(at((*left, *top)), at((left + w, top + h)));
                    let _ = window.paint_image(
                        item_bounds,
                        item_bounds,
                        Corners::default(),
                        picture.clone(),
                        0,
                        false,
                    );
                    if selected_item == Some(index) {
                        window.paint_quad(outline(
                            item_bounds,
                            gpui_kit::white(),
                            BorderStyle::Dashed,
                        ));
                        window.paint_quad(fill(
                            Bounds::centered_at(item_bounds.bottom_right(), size(px(12.), px(12.))),
                            gpui_kit::white(),
                        ));
                    }
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
                    if mode == EditorMode::Crop {
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
                window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
                    if phase == DispatchPhase::Bubble && event.button == MouseButton::Left {
                        let _ = app.update(cx, |this, cx| {
                            this.photo_editor_mut(PhotoEditor::release, cx)
                        });
                    }
                });
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
                    if target == EditorMode::Stickers
                        && let Some(live) = this.live.as_mut()
                    {
                        let _ = live.driver.fetch_editor_stickers();
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

    /// Stickers mode: an emoji row, your recent and favorite stickers, and
    /// Delete for the selected one.
    fn photo_editor_sticker_tools(&self, cx: &mut Context<Self>) -> AnyElement {
        let has_selection = self
            .viewer
            .photo_editor
            .as_ref()
            .is_some_and(|editor| editor.selected.is_some());
        let roots = self.media_display_roots();
        let mut seen = std::collections::HashSet::new();
        // Thumbnails not on disk yet: fetch them; the strip fills in.
        let missing: Vec<quill::ids::FileId> = self
            .session()
            .map(|session| {
                session
                    .stickers
                    .recent
                    .iter()
                    .chain(&session.stickers.favorites)
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
                    .recent
                    .iter()
                    .chain(&session.stickers.favorites)
                    .filter(|item| seen.insert(item.file_id.0))
                    // One row under the emoji.
                    .take(16)
                    .filter_map(|item| {
                        let thumb = item
                            .thumb_file_id
                            .into_iter()
                            .chain([item.file_id])
                            .find_map(|id| session.files.get(&id.0)?.usable_path())
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
                                    this.connection.status_note =
                                        "Couldn't draw that emoji.".into();
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
            .when(has_selection, |this| {
                this.child(
                    Button::new("photo-editor-delete-item")
                        .icon(gpui_kit::assets::IconName::Trash)
                        .label("Delete")
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
    fn place_editor_sticker(
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
            self.connection.status_note = "Downloading the sticker…".into();
            cx.notify();
            return;
        };
        match super::editor_art::sticker_pixels(std::path::Path::new(&path), format) {
            Some(image) => self.photo_editor_mut(|editor| editor.place(image), cx),
            None => {
                self.connection.status_note = "Couldn't use that sticker.".into();
                cx.notify();
            }
        }
    }
}
