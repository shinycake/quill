//! The photo editor for a picked photo (Telegram Desktop's photo editor):
//! crop with handles, quarter turns, flip, and freehand drawing with a
//! color palette, brush sizes and undo. The pixels live in
//! `photo_edit`; this is the overlay, its canvas and the mouse handling.

use super::app::QuillApp;
use super::photo_edit::{CropRect, Stroke, flip, render, rotate_ccw};
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
    for pixel in small.chunks_exact_mut(4) {
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
            image_bounds: Rc::new(Cell::new(None)),
        }
    }

    fn rotate(&mut self) {
        self.working = rotate_ccw(&self.working);
        self.preview = preview_of(&self.working);
        self.crop = self.crop.rotated_ccw();
        self.strokes = self.strokes.iter().map(Stroke::rotated_ccw).collect();
    }

    fn flip(&mut self) {
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
    }
}

impl QuillApp {
    /// Open the editor on a pending photo attachment.
    pub(super) fn open_photo_editor(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(attachment) = self.pending_attachments.get(index) else {
            return;
        };
        match image::open(&attachment.path) {
            Ok(image) => {
                self.photo_editor = Some(PhotoEditor::new(index, image.to_rgba8()));
            }
            Err(_) => self.status_note = "Couldn't open this image for editing.".into(),
        }
        cx.notify();
    }

    fn close_photo_editor(&mut self, cx: &mut Context<Self>) {
        self.photo_editor = None;
        cx.notify();
    }

    /// Render the edit at full resolution into a new PNG and send that
    /// instead of the original.
    fn save_photo_editor(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = self.photo_editor.take() else {
            return;
        };
        let edited = render(&editor.working, editor.crop, &editor.strokes);
        let dir = std::env::temp_dir().join("quill-edits");
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos());
        let path = dir.join(format!("edit-{stamp}.png"));
        let saved = std::fs::create_dir_all(&dir)
            .ok()
            .and_then(|()| edited.save(&path).ok())
            .and_then(|()| std::fs::canonicalize(&path).ok());
        match (saved, self.pending_attachments.get_mut(editor.index)) {
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
            _ => self.status_note = "Couldn't save the edited photo.".into(),
        }
        cx.notify();
    }

    fn photo_editor_mut(&mut self, edit: impl FnOnce(&mut PhotoEditor), cx: &mut Context<Self>) {
        if let Some(editor) = self.photo_editor.as_mut() {
            edit(editor);
            cx.notify();
        }
    }

    /// The editor overlay: canvas in the middle, tools below.
    pub(super) fn photo_editor_overlay(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let editor = self.photo_editor.as_ref()?;
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
                // Outside the crop is shaded in both modes, so drawing
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
                    this.photo_editor_mut(|editor| editor.mode = target, cx);
                }))
        };
        let tools = match mode {
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
                                )),
                        ),
                )
                .into_any_element(),
        )
    }
}
