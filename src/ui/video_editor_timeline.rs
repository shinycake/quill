//! The video editor's frame strip (tdesktop `Editor::VideoTimeline`): the
//! frames of the whole clip, the range between two handles, the playhead
//! that picks the cover frame, the cover dot above it and the duration and
//! size line; plus the cover bubble that follows the head and the flight of
//! the picture into it when the head is let go (`VideoEditor::updateBubble`,
//! `startCapture`). The rules are `quill::video_timeline`; the state shared
//! by the layer's parts is [`EditorMotion`].
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::video_editor::{EditorTarget, VideoEditorDialog, palette};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::video_timeline::{
    DOT_ACTIVE_SIZE, DOT_SIZE, GRIP_HEIGHT, GRIP_WIDTH, Grab, HEIGHT, LABEL_HEIGHT, LABEL_SKIP,
    Moved, PLAYHEAD_OUTLINE, PLAYHEAD_OVERFLOW, PLAYHEAD_WIDTH, RADIUS, SIZE_SKIP, STRIP_HEIGHT,
    Timeline, estimated_size,
};
use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// `kDotDuration`: the cover dot grows while the head is held.
const DOT_MS: u64 = 500;
/// `kSeekThrottle`: the player follows a drag at most this often.
const SEEK_THROTTLE: Duration = Duration::from_millis(120);
/// `kBubbleDuration`, `kBubbleMinVisible`, `kCaptureDuration`.
const BUBBLE_MS: u64 = 500;
const BUBBLE_MIN_VISIBLE: Duration = Duration::from_millis(600);
const CAPTURE_MS: u64 = 420;
/// `videoEditorBubbleSize`, `videoEditorBubbleSkip`, `videoEditorBubbleBorder`.
const BUBBLE_SIZE: f32 = 62.0;
const BUBBLE_SKIP: f32 = 8.0;
const BUBBLE_BORDER: f32 = 2.0;
/// `videoTimelineDurationStyle`: 12 px semibold.
const LABEL_FONT: f32 = 12.0;
/// Frames are scaled on paint, so a small width change keeps them
/// (`kFrameWidthTolerance`).
const FRAME_WIDTH_TOLERANCE: f32 = 0.25;

/// tdesktop's `anim::easeOutQuint`.
fn ease_out_quint(t: f32) -> f32 {
    1.0 - (1.0 - t.clamp(0.0, 1.0)).powi(5)
}

/// A value easing to a target on the frame clock (`Animations::Simple`).
#[derive(Clone, Copy, Default)]
pub(super) struct Fade {
    from: f32,
    to: f32,
    start: Option<Instant>,
    duration_ms: u64,
}

impl Fade {
    /// Ease from where the value is now to `to`; at once with reduced motion.
    pub(super) fn start(&mut self, to: f32, duration_ms: u64, reduce_motion: bool) {
        let now = Instant::now();
        self.from = self.value(now);
        self.to = to;
        self.start = (!reduce_motion && duration_ms > 0).then_some(now);
        self.duration_ms = duration_ms;
    }

    pub(super) fn value(&self, now: Instant) -> f32 {
        match self.start {
            Some(start) if self.duration_ms > 0 => {
                let t = now.duration_since(start).as_millis() as f32 / self.duration_ms as f32;
                self.from + (self.to - self.from) * ease_out_quint(t)
            }
            _ => self.to,
        }
    }

    pub(super) fn animating(&self, now: Instant) -> bool {
        self.start.is_some_and(|start| {
            now.duration_since(start).as_millis() < u128::from(self.duration_ms)
        })
    }

    pub(super) fn target(&self) -> f32 {
        self.to
    }
}

/// A press on the picture: a crop drag, or a tap that pauses.
#[derive(Clone, Copy)]
pub(super) struct StagePress {
    pub(super) at: Point<Pixels>,
    pub(super) grab: Option<quill::video_timeline::CropGrab>,
    pub(super) start: quill::video_timeline::CropArea,
    pub(super) moved: bool,
}

/// Everything the layer's parts track between frames: drags, animations,
/// the strip's frames and where things were painted last.
#[derive(Default)]
pub(super) struct EditorMotion {
    /// The strip's frames, `None` until each is cut.
    pub(super) frames: Vec<Option<super::owned_image::OwnedImage>>,
    /// The strip width the frames were cut for.
    frames_width: f32,
    frames_cancel: Option<Arc<AtomicBool>>,
    /// Where the strip control was painted.
    pub(super) timeline_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// The held part of the strip and the pointer's offset from it.
    pub(super) grab: Option<(Grab, f32)>,
    /// The head was dragged in this hold (the bubble flies on release).
    head_dragged: bool,
    pub(super) dot: Fade,
    seek_pending: Option<i64>,
    seek_last: Option<Instant>,
    pub(super) bubble: Fade,
    bubble_shown_at: Option<Instant>,
    /// The picture flying into the bubble since then.
    pub(super) capture: Option<Instant>,
    /// Where the picture and the crop were painted.
    pub(super) picture_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    pub(super) crop_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    pub(super) press: Option<StagePress>,
    /// The thirds grid over the crop while it is dragged.
    pub(super) grid: Fade,
    pub(super) quality_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// The active dot sliding from this index (`_activeShift`).
    pub(super) quality_shift: Fade,
    pub(super) quality_pressed: bool,
}

impl EditorMotion {
    /// Stop cutting frames; the editor is gone.
    pub(super) fn release_frames(&self) {
        if let Some(cancel) = &self.frames_cancel {
            cancel.store(true, Ordering::Relaxed);
        }
    }

    /// Something still moves on its own and wants the next frame.
    pub(super) fn animating(&self, now: Instant) -> bool {
        self.dot.animating(now)
            || self.bubble.animating(now)
            || self.grid.animating(now)
            || self.quality_shift.animating(now)
            || self.capture.is_some()
            || self.bubble_shown_at.is_some()
    }
}

/// The timeline with the strip width it was painted at.
fn measured(editor: &VideoEditorDialog) -> Option<Timeline> {
    let mut timeline = editor.timeline?;
    timeline.width = editor
        .motion
        .timeline_bounds
        .get()
        .map_or(0.0, |bounds| f32::from(bounds.size.width));
    Some(timeline)
}

/// The size label: `~4.2 MB` for an estimate, the file's size when it is
/// sent as it is.
fn size_label(editor: &VideoEditorDialog) -> Option<String> {
    let (bytes, estimate) = match editor.target {
        // A sticker's encoder fits the set's byte cap whatever the clip.
        EditorTarget::Set { .. } => return None,
        EditorTarget::Avatar => (
            quill::profile_photo_video::estimated_size(&editor.edits),
            true,
        ),
        EditorTarget::Tray(_) | EditorTarget::MessageEdit => {
            estimated_size(&editor.edits, &editor.info?)?
        }
    };
    let text = quill::telegram::envelope::format_size_text(i64::try_from(bytes).ok()?);
    Some(if estimate { format!("~{text}") } else { text })
}

impl QuillApp {
    /// Cut the strip's frames for the width it was painted at, when that
    /// changed (`reloadFrames`).
    pub(super) fn video_editor_reload_frames(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = self.composer_ui.send_box.video_editor.as_mut() else {
            return;
        };
        let (Some(timeline), Some(info)) = (measured(editor), editor.info) else {
            return;
        };
        let (_, strip) = timeline.strip();
        let times = timeline.frame_times(info.width, info.height);
        if times.is_empty() {
            return;
        }
        let motion = &mut editor.motion;
        let kept = motion.frames.len() == times.len()
            && (strip - motion.frames_width).abs() <= motion.frames_width * FRAME_WIDTH_TOLERANCE;
        if kept {
            return;
        }
        motion.release_frames();
        let cancel = Arc::new(AtomicBool::new(false));
        motion.frames_cancel = Some(cancel.clone());
        motion.frames_width = strip;
        motion.frames = times.iter().map(|_| None).collect();
        let serial = editor.serial;
        let source = editor.source.clone();
        // Twice the strip's height, for HiDPI screens.
        let height = (STRIP_HEIGHT * 2.0) as u32;
        // One frame at a time, each shown as it arrives.
        self.spawn_scoped(cx, async move |this, cx| {
            for (index, at) in times.into_iter().enumerate() {
                if cancel.load(Ordering::Relaxed) {
                    return;
                }
                let source = source.clone();
                let frame = cx
                    .background_spawn(async move {
                        let path =
                            quill::video_edit::render_strip_frame(&source, at, height).ok()?;
                        let still = super::video_editor::load_still(&path);
                        let _ = std::fs::remove_file(path);
                        still
                    })
                    .await;
                let alive = this.update(cx, |this, cx| {
                    let Some(editor) = this
                        .composer_ui
                        .send_box
                        .video_editor
                        .as_mut()
                        .filter(|editor| editor.serial == serial)
                    else {
                        return false;
                    };
                    if let Some(slot) = editor.motion.frames.get_mut(index) {
                        *slot = frame.map(Into::into);
                    }
                    cx.notify();
                    true
                });
                if !matches!(alive, Ok(true)) {
                    return;
                }
            }
        })
        .detach();
    }

    /// A press on the strip (`mousePressEvent`).
    fn video_timeline_press(&mut self, x: f32, cx: &mut Context<Self>) {
        let reduce = cx.reduce_motion();
        let Some(editor) = self.composer_ui.send_box.video_editor.as_mut() else {
            return;
        };
        let Some(timeline) = measured(editor) else {
            return;
        };
        let grab = timeline.grab_at(x);
        editor.motion.grab = Some((grab, timeline.grab_shift(grab, x)));
        editor.motion.head_dragged = grab == Grab::Head;
        if grab == Grab::Head {
            editor.motion.dot.start(1.0, DOT_MS, reduce);
        }
        if let Some(player) = editor.player.as_ref() {
            player.pause();
        }
        self.video_timeline_drag(x, cx);
    }

    /// The held part follows the pointer (`applyGrab`).
    fn video_timeline_drag(&mut self, x: f32, cx: &mut Context<Self>) {
        let Some(editor) = self.composer_ui.send_box.video_editor.as_mut() else {
            return;
        };
        let (Some(mut timeline), Some((grab, shift))) = (measured(editor), editor.motion.grab)
        else {
            return;
        };
        let moved = timeline.drag(grab, x - shift);
        let width = editor.timeline.map_or(0.0, |kept| kept.width);
        editor.timeline = Some(Timeline { width, ..timeline });
        editor.edits.from_ms = timeline.from;
        editor.edits.till_ms = timeline.till;
        editor.edits.cover_ms = timeline.cover;
        let at = match moved {
            Moved::Nothing => {
                cx.notify();
                return;
            }
            Moved::Trim(at) => at,
            Moved::Cover(at) => {
                editor.motion.bubble_shown_at = None;
                if editor.motion.bubble.target() < 1.0 {
                    editor
                        .motion
                        .bubble
                        .start(1.0, BUBBLE_MS, cx.reduce_motion());
                }
                at
            }
        };
        editor.playback_ms = Some(at);
        // The player follows at most every `SEEK_THROTTLE`; the last wish
        // is taken on release.
        let now = Instant::now();
        let due = editor
            .motion
            .seek_last
            .is_none_or(|last| now.duration_since(last) >= SEEK_THROTTLE);
        if due {
            editor.motion.seek_last = Some(now);
            editor.motion.seek_pending = None;
            if let Some(player) = editor.player.as_ref() {
                player.seek(at as f64 / 1000.0);
            }
        } else {
            editor.motion.seek_pending = Some(at);
        }
        let cover_moved = matches!(moved, Moved::Cover(_));
        let no_player = editor.player.is_none();
        if cover_moved || no_player {
            self.refresh_video_preview(at, cx);
        }
        cx.notify();
    }

    /// The strip is let go: playback resumes from where the drag left it,
    /// and a moved head sends the picture into the bubble.
    fn video_timeline_release(&mut self, cx: &mut Context<Self>) {
        let reduce = cx.reduce_motion();
        let Some(editor) = self.composer_ui.send_box.video_editor.as_mut() else {
            return;
        };
        let Some((grab, _)) = editor.motion.grab.take() else {
            return;
        };
        if grab == Grab::Head {
            editor.motion.dot.start(0.0, DOT_MS, reduce);
        }
        let (from, till) = (editor.edits.from_ms, editor.edits.till_ms);
        let latest = editor
            .motion
            .seek_pending
            .take()
            .or(editor.playback_ms)
            .unwrap_or(from)
            .clamp(from, till);
        let resume = if latest >= till { from } else { latest };
        editor.motion.seek_last = None;
        editor.playback_ms = Some(resume);
        if let Some(player) = editor.player.as_ref() {
            player.seek(resume as f64 / 1000.0);
            if !editor.user_paused {
                player.play();
            }
        }
        if std::mem::take(&mut editor.motion.head_dragged) {
            // `startCapture`: the bubble hides while the picture flies in.
            if reduce {
                editor.motion.bubble_shown_at = Some(Instant::now());
            } else {
                editor.motion.capture = Some(Instant::now());
                editor.motion.bubble = Fade::default();
            }
        }
        cx.notify();
    }

    /// Advance the bubble and the flight; called each frame while the
    /// layer animates.
    pub(super) fn video_editor_tick_bubble(&mut self, cx: &mut Context<Self>) {
        let reduce = cx.reduce_motion();
        let Some(editor) = self.composer_ui.send_box.video_editor.as_mut() else {
            return;
        };
        let now = Instant::now();
        let motion = &mut editor.motion;
        if let Some(start) = motion.capture
            && now.duration_since(start) >= Duration::from_millis(CAPTURE_MS)
        {
            motion.capture = None;
            motion.bubble = Fade {
                from: 1.0,
                to: 1.0,
                start: None,
                duration_ms: 0,
            };
            motion.bubble_shown_at = Some(now);
        }
        if let Some(shown) = motion.bubble_shown_at
            && now.duration_since(shown) >= BUBBLE_MIN_VISIBLE
        {
            motion.bubble_shown_at = None;
            motion.bubble.start(0.0, BUBBLE_MS, reduce);
        }
        // The head was let go without a flight: the bubble fades out.
        if motion.grab.is_none()
            && motion.capture.is_none()
            && motion.bubble_shown_at.is_none()
            && motion.bubble.target() > 0.0
        {
            motion.bubble.start(0.0, BUBBLE_MS, reduce);
        }
    }

    /// The strip control: frames, selection, head, cover dot and labels.
    pub(super) fn video_editor_timeline(
        &self,
        editor: &VideoEditorDialog,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let app = cx.entity().downgrade();
        // A reload asked for by the paint below is for this window's editor.
        let scope = self.windows.scope;
        let bounds_cell = editor.motion.timeline_bounds.clone();
        let timeline = editor.timeline;
        let frames: Vec<Option<Arc<RenderImage>>> = editor
            .motion
            .frames
            .iter()
            .map(|frame| frame.as_deref().cloned())
            .collect();
        let playback = editor.playback_ms;
        let dot = editor.motion.dot.value(Instant::now());
        let size_text = size_label(editor);
        let wanted_reload = editor.info.is_some();
        let painted_width = editor.motion.frames_width;
        canvas(
            move |bounds, window, _| window.insert_hitbox(bounds, HitboxBehavior::Normal),
            move |bounds, hitbox, window, cx| {
                let previous = bounds_cell.replace(Some(bounds));
                let Some(mut timeline) = timeline else {
                    return;
                };
                timeline.width = f32::from(bounds.size.width);
                let (_, strip_w) = timeline.strip();
                if wanted_reload
                    && (previous.map(|old| old.size.width) != Some(bounds.size.width)
                        || painted_width == 0.0)
                    && (strip_w - painted_width).abs() > painted_width * FRAME_WIDTH_TOLERANCE
                {
                    let app = app.clone();
                    cx.defer(move |cx| {
                        let _ = app.update(cx, |this, cx| {
                            this.in_window_scope(scope, cx, |this, cx| {
                                this.video_editor_reload_frames(cx)
                            })
                        });
                    });
                }
                paint_timeline(
                    &timeline,
                    bounds,
                    &frames,
                    playback,
                    dot,
                    size_text.as_deref(),
                    window,
                    cx,
                );
                let cursor = if hitbox.is_hovered(window) {
                    CursorStyle::ResizeLeftRight
                } else {
                    CursorStyle::Arrow
                };
                window.set_cursor_style(cursor, &hitbox);
                let origin = bounds.origin.x;
                let down_app = app.clone();
                let down_hitbox = hitbox.clone();
                window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
                    if phase == DispatchPhase::Bubble
                        && event.button == MouseButton::Left
                        && down_hitbox.is_hovered(window)
                    {
                        let x = f32::from(event.position.x - origin);
                        let _ = down_app.update(cx, |this, cx| this.video_timeline_press(x, cx));
                    }
                });
                let move_app = app.clone();
                window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                    if phase == DispatchPhase::Bubble
                        && event.pressed_button == Some(MouseButton::Left)
                    {
                        let x = f32::from(event.position.x - origin);
                        let _ = move_app.update(cx, |this, cx| this.video_timeline_drag(x, cx));
                    }
                });
                let up_app = app.clone();
                window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
                    if phase == DispatchPhase::Bubble && event.button == MouseButton::Left {
                        let _ = up_app.update(cx, |this, cx| this.video_timeline_release(cx));
                    }
                });
            },
        )
        .w_full()
        .h(px(HEIGHT))
        .into_any_element()
    }

    /// The cover bubble over the strip and the picture flying into it, on
    /// top of the whole layer.
    pub(super) fn video_editor_bubble(&self, editor: &VideoEditorDialog) -> Option<AnyElement> {
        let now = Instant::now();
        let still = editor.still.as_deref().cloned()?;
        let shown = editor.motion.bubble.value(now);
        let capture = editor.motion.capture.map(|start| {
            ease_out_quint(now.duration_since(start).as_millis() as f32 / CAPTURE_MS as f32)
        });
        if shown <= 0.0 && capture.is_none() {
            return None;
        }
        let mut timeline = editor.timeline?;
        let strip = editor.motion.timeline_bounds.get()?;
        timeline.width = f32::from(strip.size.width);
        let crop = editor.motion.crop_bounds.get();
        let crop_area = quill::video_timeline::CropArea::of(editor.edits.crop);
        let round = editor.target.round();
        Some(
            canvas(
                |_, _, _| (),
                move |_, _, window, _| {
                    let (dot_x, dot_y) = timeline.cover_dot();
                    let target = bubble_rect(&still, crop_area, strip, dot_x, window);
                    let dot = point(strip.origin.x + px(dot_x), strip.origin.y + px(dot_y));
                    match (capture, crop) {
                        (Some(value), Some(from)) => {
                            paint_capture(&still, crop_area, (from, target), value, round, window)
                        }
                        _ => paint_bubble(&still, crop_area, target, dot, shown, round, window),
                    }
                },
            )
            .absolute()
            .inset_0()
            .when(capture.is_none(), |layer| layer.opacity(shown))
            .into_any_element(),
        )
    }
}

/// The bubble's place above the strip, centered on the cover dot and kept
/// inside the window, with the cropped still's aspect.
fn bubble_rect(
    still: &Arc<RenderImage>,
    crop: quill::video_timeline::CropArea,
    strip: Bounds<Pixels>,
    dot_x: f32,
    window: &Window,
) -> Bounds<Pixels> {
    let picture = still.size(0);
    let (w, h) = (
        picture.width.0 as f32 * crop.w,
        (picture.height.0 as f32 * crop.h).max(1.0),
    );
    let scale = (BUBBLE_SIZE / w.max(1.0)).min(BUBBLE_SIZE / h);
    let (w, h) = ((w * scale).max(1.0), (h * scale).max(1.0));
    let viewport = f32::from(window.viewport_size().width);
    let x = (f32::from(strip.origin.x) + dot_x - w / 2.0).clamp(0.0, (viewport - w).max(0.0));
    let y = f32::from(strip.origin.y) - BUBBLE_SKIP - h;
    Bounds::new(point(px(x), px(y)), size(px(w), px(h)))
}

/// Paint the cropped part of `still` into `rect`.
fn paint_cropped(
    still: &Arc<RenderImage>,
    crop: quill::video_timeline::CropArea,
    rect: Bounds<Pixels>,
    radius: Pixels,
    window: &mut Window,
) {
    let full = Bounds::new(
        point(
            rect.origin.x - rect.size.width * (crop.x / crop.w.max(0.001)),
            rect.origin.y - rect.size.height * (crop.y / crop.h.max(0.001)),
        ),
        size(
            rect.size.width / crop.w.max(0.001),
            rect.size.height / crop.h.max(0.001),
        ),
    );
    let _ = window.paint_image(rect, full, Corners::all(radius), still.clone(), 0, false);
}

/// The bubble growing out of the cover dot (`_bubble` paint).
fn paint_bubble(
    still: &Arc<RenderImage>,
    crop: quill::video_timeline::CropArea,
    target: Bounds<Pixels>,
    dot: Point<Pixels>,
    shown: f32,
    round: bool,
    window: &mut Window,
) {
    let center = point(
        target.center().x,
        dot.y + (target.center().y - dot.y) * shown,
    );
    let grown = size(target.size.width * shown, target.size.height * shown);
    let rect = Bounds::centered_at(center, grown);
    let radius = shape_radius(rect, round);
    paint_cropped(still, crop, rect, radius, window);
    window.paint_quad(quad(
        rect,
        radius,
        transparent_black(),
        px(BUBBLE_BORDER),
        palette::text(),
        BorderStyle::Solid,
    ));
}

/// `ShapeRadius`: an avatar's bubble is a circle, any other a rounded
/// rectangle.
fn shape_radius(rect: Bounds<Pixels>, round: bool) -> Pixels {
    let shorter = rect.size.width.min(rect.size.height);
    if round { shorter / 2. } else { shorter * 0.1 }
}

/// The picture flying from the crop into the bubble, with a short flash
/// where it left (`_capture` paint).
fn paint_capture(
    still: &Arc<RenderImage>,
    crop: quill::video_timeline::CropArea,
    (from, to): (Bounds<Pixels>, Bounds<Pixels>),
    value: f32,
    round: bool,
    window: &mut Window,
) {
    let lerp = |a: Pixels, b: Pixels| a + (b - a) * value;
    let rect = Bounds::new(
        point(
            lerp(from.origin.x, to.origin.x),
            lerp(from.origin.y, to.origin.y),
        ),
        size(
            lerp(from.size.width, to.size.width),
            lerp(from.size.height, to.size.height),
        ),
    );
    let radius = shape_radius(rect, round) * value;
    paint_cropped(still, crop, rect, radius, window);
    window.paint_quad(quad(
        rect,
        radius,
        transparent_black(),
        px(BUBBLE_BORDER),
        palette::text().opacity(value * value),
        BorderStyle::Solid,
    ));
    let flash = (1.0 - value * 3.0).clamp(0.0, 1.0);
    if flash > 0.0 {
        window.paint_quad(fill(from, palette::text().opacity(flash)));
    }
}

/// `VideoTimeline::paintEvent`.
#[allow(clippy::too_many_arguments)]
fn paint_timeline(
    timeline: &Timeline,
    bounds: Bounds<Pixels>,
    frames: &[Option<Arc<RenderImage>>],
    playback: Option<i64>,
    dot: f32,
    size_text: Option<&str>,
    window: &mut Window,
    cx: &mut App,
) {
    let at = |x: f32, y: f32| point(bounds.origin.x + px(x), bounds.origin.y + px(y));
    let (strip_x, strip_w) = timeline.strip();
    let top = LABEL_HEIGHT + LABEL_SKIP;
    let strip = Bounds::new(at(strip_x, top), size(px(strip_w), px(STRIP_HEIGHT)));
    // The frames, rounded at the strip's ends.
    window.paint_quad(fill(strip, palette::inactive()).corner_radii(px(RADIUS)));
    let count = frames.len();
    for (index, frame) in frames.iter().enumerate() {
        let Some(frame) = frame else {
            continue;
        };
        let left = strip_x + index as f32 * strip_w / count as f32;
        let right = strip_x + (index + 1) as f32 * strip_w / count as f32;
        let cell = Bounds::new(at(left, top), size(px(right - left), px(STRIP_HEIGHT)));
        // Cover the cell: the frame keeps its aspect and is cut to it.
        let picture = frame.size(0);
        let (pw, ph) = (picture.width.0 as f32, picture.height.0.max(1) as f32);
        let scale = ((right - left) / pw.max(1.0)).max(STRIP_HEIGHT / ph);
        let drawn = Bounds::centered_at(cell.center(), size(px(pw * scale), px(ph * scale)));
        let corners = Corners {
            top_left: px(if index == 0 { RADIUS } else { 0.0 }),
            bottom_left: px(if index == 0 { RADIUS } else { 0.0 }),
            top_right: px(if index + 1 == count { RADIUS } else { 0.0 }),
            bottom_right: px(if index + 1 == count { RADIUS } else { 0.0 }),
        };
        let _ = window.paint_image(cell, drawn, corners, frame.clone(), 0, false);
    }
    // The dim outside the kept range.
    let (left, right) = (timeline.x_at(timeline.from), timeline.x_at(timeline.till));
    if left > strip_x {
        window.paint_quad(
            fill(
                Bounds::new(at(strip_x, top), size(px(left - strip_x), px(STRIP_HEIGHT))),
                palette::dim(),
            )
            .corner_radii(Corners {
                top_left: px(RADIUS),
                bottom_left: px(RADIUS),
                ..Corners::default()
            }),
        );
    }
    if right < strip_x + strip_w {
        window.paint_quad(
            fill(
                Bounds::new(
                    at(right, top),
                    size(px(strip_x + strip_w - right), px(STRIP_HEIGHT)),
                ),
                palette::dim(),
            )
            .corner_radii(Corners {
                top_right: px(RADIUS),
                bottom_right: px(RADIUS),
                ..Corners::default()
            }),
        );
    }
    // The selection frame: thick sides with grips, thin top and bottom.
    let frame = Bounds::new(
        at(left - quill::video_timeline::HANDLE_WIDTH, top),
        size(
            px(right - left + quill::video_timeline::HANDLE_WIDTH * 2.0),
            px(STRIP_HEIGHT),
        ),
    );
    window.paint_quad(quad(
        frame,
        px(RADIUS),
        transparent_black(),
        Edges {
            top: px(GRIP_WIDTH),
            bottom: px(GRIP_WIDTH),
            left: px(quill::video_timeline::HANDLE_WIDTH),
            right: px(quill::video_timeline::HANDLE_WIDTH),
        },
        palette::text(),
        BorderStyle::Solid,
    ));
    let grip_h = GRIP_HEIGHT.min(STRIP_HEIGHT / 2.0);
    let grip_y = top + (STRIP_HEIGHT - grip_h) / 2.0;
    let handle = quill::video_timeline::HANDLE_WIDTH;
    for grip_x in [
        left - handle + (handle - GRIP_WIDTH) / 2.0,
        right + (handle - GRIP_WIDTH) / 2.0,
    ] {
        window.paint_quad(
            fill(
                Bounds::new(at(grip_x, grip_y), size(px(GRIP_WIDTH), px(grip_h))),
                palette::dim(),
            )
            .corner_radii(px(GRIP_WIDTH / 2.0)),
        );
    }
    // The playhead, outlined so it shows on any frame.
    let head_x = timeline.head_x(playback);
    let head = Bounds::new(
        at(head_x, top - PLAYHEAD_OVERFLOW),
        size(
            px(PLAYHEAD_WIDTH),
            px(STRIP_HEIGHT + PLAYHEAD_OVERFLOW * 2.0),
        ),
    );
    let outlined = head.dilate(px(PLAYHEAD_OUTLINE));
    window.paint_quad(
        fill(outlined, palette::dim()).corner_radii(px(PLAYHEAD_WIDTH / 2.0 + PLAYHEAD_OUTLINE)),
    );
    window.paint_quad(fill(head, palette::text()).corner_radii(px(PLAYHEAD_WIDTH / 2.0)));
    // The cover dot, larger while the head is held.
    let (dot_x, dot_y) = timeline.cover_dot();
    let side = DOT_SIZE + (DOT_ACTIVE_SIZE - DOT_SIZE) * dot;
    window.paint_quad(
        fill(
            Bounds::centered_at(at(dot_x, dot_y), size(px(side), px(side))),
            palette::accent(),
        )
        .corner_radii(px(side / 2.0)),
    );
    paint_labels(timeline, bounds, size_text, window, cx);
}

/// `paintDuration`: the size at the right when both fit, the duration
/// centered over the selection.
fn paint_labels(
    timeline: &Timeline,
    bounds: Bounds<Pixels>,
    size_text: Option<&str>,
    window: &mut Window,
    cx: &mut App,
) {
    let width = f32::from(bounds.size.width);
    let shape = |text: String, window: &mut Window| {
        let mut font = window.text_style().font();
        font.weight = FontWeight::SEMIBOLD;
        let run = TextRun {
            len: text.len(),
            font,
            color: palette::text(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        window
            .text_system()
            .shape_line(text.into(), px(LABEL_FONT), &[run], None)
    };
    let duration = shape(timeline.duration_label(), window);
    let duration_w = f32::from(duration.width);
    let size_line = size_text.map(|text| shape(text.to_string(), window));
    let size_w = size_line.as_ref().map_or(0.0, |line| f32::from(line.width));
    let size_shown = size_w > 0.0 && duration_w + SIZE_SKIP + size_w <= width;
    let line_height = px(LABEL_HEIGHT);
    if size_shown && let Some(line) = &size_line {
        let origin = point(bounds.origin.x + px(width - size_w), bounds.origin.y);
        let _ = line.paint(origin, line_height, TextAlign::Left, None, window, cx);
    }
    let available = if size_shown {
        width - size_w - SIZE_SKIP
    } else {
        width
    };
    let x = timeline.duration_x(duration_w.min(available), available);
    let origin = point(bounds.origin.x + px(x), bounds.origin.y);
    window.with_content_mask(
        Some(ContentMask {
            bounds: Bounds::new(bounds.origin, size(px(available), line_height)),
        }),
        |window| {
            let _ = duration.paint(origin, line_height, TextAlign::Left, None, window, cx);
        },
    );
}
