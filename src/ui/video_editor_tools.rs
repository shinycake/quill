//! The video editor's picture (tdesktop `VideoEditor` with its `Crop`): the
//! kept range playing in a loop, muted, turned and mirrored like the edit;
//! the crop drawn on it with corner brackets, dragged by its edges or moved
//! whole, with the thirds grid while it moves; a click pauses with the play
//! badge in the middle (`setupTapToPause`, `paintPlayBadge`). Rotate and
//! Flip live here too. The strip is `video_editor_timeline.rs`.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::video_editor::{VideoEditorDialog, palette};
use super::video_editor_timeline::StagePress;
use super::video_player::{PlayState, Purpose, VideoPlayer, supported, video_frame};
use gpui_kit::*;
use quill::lang::{keys, tr};
use quill::sticker_video::square_crop;
use quill::video_edit::{Crop, quality_levels, turned_size};
use quill::video_timeline::{CropArea, crop_dragged, crop_grab_at};
use std::time::Instant;

/// `photoEditorCropPointSize`: the corner brackets are twice this long.
const BRACKET: f32 = 20.0;
/// `st::lineWidth * 2` for the frame, `* 4` for the brackets.
const FRAME_LINE: f32 = 2.0;
const BRACKET_LINE: f32 = 4.0;
/// `videoEditorPlayBadgeSize`.
const BADGE: f32 = 68.0;
/// How far a press may travel and still be a click
/// (`QApplication::startDragDistance`).
const DRAG_DISTANCE: f32 = 10.0;
/// The grid fades in and out over this long.
const GRID_MS: u64 = 200;
/// Decoded pictures are kept at most this large on their longer side.
const PLAYER_EDGE: u32 = 2048;

impl QuillApp {
    /// Start the muted loop of the kept range (tdesktop `setupStreaming`):
    /// it plays from the start of the range as soon as the video is read.
    pub(super) fn start_video_editor_player(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = self.composer_ui.send_box.video_editor.as_mut() else {
            return;
        };
        if !supported() {
            return;
        }
        let duration = editor.info.map(|info| info.duration_ms as f64 / 1000.0);
        match VideoPlayer::open(
            &editor.source,
            Purpose::Inline {
                max_edge: PLAYER_EDGE,
            },
            duration,
            cx,
        ) {
            Ok(video) => {
                video.set_volume(0.0);
                video.set_orientation(editor.edits.turns, editor.edits.flipped);
                video.seek(editor.edits.from_ms as f64 / 1000.0);
                if !editor.user_paused {
                    video.play();
                }
                video.watch(cx.entity_id());
                editor.playback_ms = Some(editor.edits.from_ms);
                editor.player = Some(video);
            }
            Err(_) => editor.note = Some(tr(keys::Q_AV_QUILL_CANT_PLAY_THIS_VIDEO).into()),
        }
        cx.notify();
    }

    /// Keep the loop inside the range and the playhead on the player; asks
    /// for the next frame while something moves.
    pub(super) fn pump_video_editor(&mut self, cx: &mut Context<Self>) {
        self.video_editor_tick_bubble(cx);
        let Some(editor) = self.composer_ui.send_box.video_editor.as_mut() else {
            return;
        };
        let held = editor.user_paused || editor.motion.grab.is_some() || editor.busy;
        let (from, till) = (editor.edits.from_ms, editor.edits.till_ms);
        let mut playing = false;
        // A clip that fails after opening: the player goes (the still
        // shows, as when it can't open) and the note says so.
        if editor
            .player
            .as_ref()
            .is_some_and(|video| video.state() == PlayState::Failed)
        {
            editor.player = None;
            editor.note = Some(tr(keys::Q_AV_QUILL_CANT_PLAY_THIS_VIDEO).into());
        }
        if !held && let Some(video) = editor.player.as_ref() {
            match video.state() {
                PlayState::Failed => {}
                // Still opening: it was told where to start and to play.
                PlayState::Opening => playing = true,
                // Ran to the file's end (the range ends there): back to
                // the range's start, then play.
                PlayState::Ended => {
                    video.seek(from as f64 / 1000.0);
                    editor.playback_ms = Some(from);
                    video.play();
                    playing = true;
                }
                _ => {
                    let at = (video.transport().position * 1000.0).round() as i64;
                    if at >= till || at < from.saturating_sub(250) {
                        video.seek(from as f64 / 1000.0);
                        editor.playback_ms = Some(from);
                    } else {
                        editor.playback_ms = Some(at.clamp(from, till));
                    }
                    if !video.wants_play() {
                        video.play();
                    }
                    playing = true;
                }
            }
        }
        if playing || editor.motion.animating(Instant::now()) {
            self.request_animation_tick(60, cx);
        }
    }

    /// A click on the picture pauses or resumes (`togglePause`).
    fn toggle_video_editor_pause(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = self.composer_ui.send_box.video_editor.as_mut() else {
            return;
        };
        let Some(video) = editor.player.as_ref() else {
            return;
        };
        editor.user_paused = !editor.user_paused;
        if editor.user_paused {
            video.pause();
        } else {
            // Paused at the file's end: resume from the range's start.
            if video.state() == PlayState::Ended {
                video.seek(editor.edits.from_ms as f64 / 1000.0);
                editor.playback_ms = Some(editor.edits.from_ms);
            }
            video.play();
        }
        cx.notify();
    }

    /// A quality the cropped clip no longer offers goes back to Original.
    fn fit_editor_quality(editor: &mut VideoEditorDialog) {
        if let Some(info) = editor.info {
            let (width, height) = editor.edits.edited_size(&info);
            if !quality_levels(width, height).contains(&editor.edits.quality) {
                editor.edits.quality = 0;
            }
        }
    }

    /// Turn the picture a quarter turn clockwise; the crop turns with it.
    pub(super) fn rotate_video_editor(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = self.composer_ui.send_box.video_editor.as_mut() else {
            return;
        };
        editor.edits.turns = (editor.edits.turns + 1) % 4;
        editor.edits.crop = match (&editor.target, editor.info, editor.edits.crop) {
            (_, _, Some(crop)) => Some(crop.turned_cw()),
            // A sticker's or an avatar's crop is always a square.
            (target, Some(info), None) if target.square() => {
                let (width, height) = turned_size(info.width, info.height, editor.edits.turns);
                square_crop(width, height)
            }
            _ => None,
        };
        Self::fit_editor_quality(editor);
        self.video_editor_geometry_changed(cx);
    }

    /// Mirror the picture left to right; the crop keeps what it held.
    pub(super) fn flip_video_editor(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = self.composer_ui.send_box.video_editor.as_mut() else {
            return;
        };
        editor.edits.flipped = !editor.edits.flipped;
        editor.edits.crop = editor.edits.crop.map(Crop::mirrored);
        self.video_editor_geometry_changed(cx);
    }

    fn video_editor_geometry_changed(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = self.composer_ui.send_box.video_editor.as_mut() else {
            return;
        };
        if let Some(video) = editor.player.as_ref() {
            video.set_orientation(editor.edits.turns, editor.edits.flipped);
        }
        let at = editor.edits.cover_ms;
        self.refresh_video_preview(at, cx);
        cx.notify();
    }

    fn video_stage_press(&mut self, at: Point<Pixels>, cx: &mut Context<Self>) {
        let reduce = cx.reduce_motion();
        let Some(editor) = self.composer_ui.send_box.video_editor.as_mut() else {
            return;
        };
        let Some(picture) = editor.motion.picture_bounds.get() else {
            return;
        };
        let start = CropArea::of(editor.edits.crop);
        let local = (
            f32::from(at.x - picture.origin.x),
            f32::from(at.y - picture.origin.y),
        );
        let shown = (
            f32::from(picture.size.width),
            f32::from(picture.size.height),
        );
        let grab = crop_grab_at(start, local, shown);
        if grab.is_some() {
            editor.motion.grid.start(1.0, GRID_MS, reduce);
        }
        editor.motion.press = Some(StagePress {
            at,
            grab,
            start,
            moved: false,
        });
        cx.notify();
    }

    fn video_stage_drag(&mut self, at: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(editor) = self.composer_ui.send_box.video_editor.as_mut() else {
            return;
        };
        let (Some(mut press), Some(picture)) =
            (editor.motion.press, editor.motion.picture_bounds.get())
        else {
            return;
        };
        let (dx, dy) = (f32::from(at.x - press.at.x), f32::from(at.y - press.at.y));
        if !press.moved && dx.abs() + dy.abs() >= DRAG_DISTANCE {
            press.moved = true;
        }
        editor.motion.press = Some(press);
        let Some(grab) = press.grab.filter(|_| press.moved) else {
            return;
        };
        let shown = (
            f32::from(picture.size.width),
            f32::from(picture.size.height),
        );
        let square = match (&editor.target, editor.info) {
            (target, Some(info)) if target.square() => {
                let (width, height) = turned_size(info.width, info.height, editor.edits.turns);
                Some(width as f32 / height.max(1) as f32)
            }
            _ => None,
        };
        let area = crop_dragged(press.start, grab, (dx, dy), shown, square);
        editor.edits.crop = area.to_crop();
        cx.notify();
    }

    fn video_stage_release(&mut self, cx: &mut Context<Self>) {
        let reduce = cx.reduce_motion();
        let Some(editor) = self.composer_ui.send_box.video_editor.as_mut() else {
            return;
        };
        let Some(press) = editor.motion.press.take() else {
            return;
        };
        if press.grab.is_some() {
            editor.motion.grid.start(0.0, GRID_MS, reduce);
        }
        if press.moved {
            Self::fit_editor_quality(editor);
            cx.notify();
        } else {
            self.toggle_video_editor_pause(cx);
        }
    }

    /// The picture with the crop over it, fitted into the space above the
    /// controls.
    pub(super) fn video_editor_stage(
        &self,
        editor: &VideoEditorDialog,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let app = cx.entity().downgrade();
        let (width, height) = editor.info.map_or((16, 9), |info| {
            turned_size(info.width, info.height, editor.edits.turns)
        });
        let picture = (width.max(1) as f32, height.max(1) as f32);
        // The player's picture, else the still (cut turned and mirrored).
        let player = editor.player.clone();
        let still = editor.still.as_deref().cloned();
        let crop = CropArea::of(editor.edits.crop);
        let picture_cell = editor.motion.picture_bounds.clone();
        let crop_cell = editor.motion.crop_bounds.clone();
        let grid = editor.motion.grid.value(Instant::now());
        let round = editor.target.round();
        let paused = editor.user_paused && editor.player.is_some();
        let overlay = canvas(
            move |bounds, window, _| window.insert_hitbox(bounds, HitboxBehavior::Normal),
            move |bounds, hitbox, window, _| {
                let fit = picture_fit(bounds, picture);
                picture_cell.set(Some(fit));
                // The picture is the player's element below; the still
                // shows until the player has one.
                let playing = player
                    .as_ref()
                    .is_some_and(|player| player.current_picture().is_some());
                if !playing && let Some(still) = still.clone() {
                    let _ = window.paint_image(fit, fit, Corners::default(), still, 0, false);
                }
                let at = |x: f32, y: f32| {
                    point(
                        fit.origin.x + fit.size.width * x,
                        fit.origin.y + fit.size.height * y,
                    )
                };
                let cut =
                    Bounds::from_corners(at(crop.x, crop.y), at(crop.x + crop.w, crop.y + crop.h));
                crop_cell.set(Some(cut));
                paint_crop(fit, cut, grid, round, window);
                if paused {
                    paint_play_badge(cut.center(), window);
                }
                let cursor = match crop_grab_at(
                    crop,
                    (
                        f32::from(window.mouse_position().x - fit.origin.x),
                        f32::from(window.mouse_position().y - fit.origin.y),
                    ),
                    (f32::from(fit.size.width), f32::from(fit.size.height)),
                ) {
                    Some(grab) if grab.moves() => CursorStyle::OpenHand,
                    Some(grab) if (grab.left || grab.right) && !(grab.top || grab.bottom) => {
                        CursorStyle::ResizeLeftRight
                    }
                    Some(grab) if (grab.top || grab.bottom) && !(grab.left || grab.right) => {
                        CursorStyle::ResizeUpDown
                    }
                    Some(grab) if (grab.left && grab.top) || (grab.right && grab.bottom) => {
                        CursorStyle::ResizeUpLeftDownRight
                    }
                    Some(_) => CursorStyle::ResizeUpRightDownLeft,
                    None => CursorStyle::Arrow,
                };
                window.set_cursor_style(cursor, &hitbox);
                let down_app = app.clone();
                let down_hitbox = hitbox.clone();
                window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
                    if phase == DispatchPhase::Bubble
                        && event.button == MouseButton::Left
                        && down_hitbox.is_hovered(window)
                    {
                        let at = event.position;
                        let _ = down_app.update(cx, |this, cx| this.video_stage_press(at, cx));
                    }
                });
                let move_app = app.clone();
                window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                    if phase == DispatchPhase::Bubble
                        && event.pressed_button == Some(MouseButton::Left)
                    {
                        let at = event.position;
                        let _ = move_app.update(cx, |this, cx| this.video_stage_drag(at, cx));
                    }
                });
                let up_app = app.clone();
                window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
                    if phase == DispatchPhase::Bubble && event.button == MouseButton::Left {
                        let _ = up_app.update(cx, |this, cx| this.video_stage_release(cx));
                    }
                });
            },
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full();
        // The player's picture in the same fitted box as `fit` above.
        div()
            .relative()
            .size_full()
            .children(editor.player.clone().map(|player| {
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .child(FittedPlayer { player, picture })
            }))
            .child(overlay)
            .into_any_element()
    }
}

/// Where a `picture` (width, height) sits in `bounds`: fitted, small
/// pictures scaled up, centered (tdesktop's editor picture).
fn picture_fit(bounds: Bounds<Pixels>, (width, height): (f32, f32)) -> Bounds<Pixels> {
    let scale = (f32::from(bounds.size.width) / width).min(f32::from(bounds.size.height) / height);
    let scale = if scale.is_finite() { scale.max(0.) } else { 0. };
    Bounds::centered_at(bounds.center(), size(px(width * scale), px(height * scale)))
}

/// The player's picture boxed to [`picture_fit`] of its bounds, so it
/// lies exactly under the crop the canvas paints over it.
struct FittedPlayer {
    player: VideoPlayer,
    picture: (f32, f32),
}

impl IntoElement for FittedPlayer {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for FittedPlayer {
    type RequestLayoutState = ();
    type PrepaintState = AnyElement;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let style = Style {
            size: size(relative(1.).into(), relative(1.).into()),
            ..Style::default()
        };
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let fit = picture_fit(bounds, self.picture);
        let mut element = div()
            .w(fit.size.width)
            .h(fit.size.height)
            .child(video_frame(&self.player))
            .into_any_element();
        element.layout_as_root(fit.size.into(), window, cx);
        element.prepaint_at(fit.origin, window, cx);
        element
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        element: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        element.paint(window, cx);
    }
}

/// `Crop::paintFrame` and `paintGrid`: the fade outside, a thin frame,
/// thick corner brackets and the thirds grid at `grid` opacity.
fn paint_crop(
    fit: Bounds<Pixels>,
    cut: Bounds<Pixels>,
    grid: f32,
    round: bool,
    window: &mut Window,
) {
    for band in [
        Bounds::from_corners(fit.origin, point(fit.right(), cut.top())),
        Bounds::from_corners(point(fit.left(), cut.bottom()), fit.bottom_right()),
        Bounds::from_corners(
            point(fit.left(), cut.top()),
            point(cut.left(), cut.bottom()),
        ),
        Bounds::from_corners(
            point(cut.right(), cut.top()),
            point(fit.right(), cut.bottom()),
        ),
    ] {
        if band.size.width > px(0.) && band.size.height > px(0.) {
            window.paint_quad(fill(band, palette::crop_fade()));
        }
    }
    if round {
        paint_round_fade(cut, window);
    }
    window.paint_quad(outline(
        cut.dilate(px(FRAME_LINE / 2.0)),
        palette::text().opacity(0.5),
        BorderStyle::Solid,
    ));
    if grid > 0.0 {
        let line = palette::text().opacity(0.5 * grid);
        for third in [1.0 / 3.0, 2.0 / 3.0] {
            let x = cut.left() + cut.size.width * third;
            let y = cut.top() + cut.size.height * third;
            window.paint_quad(fill(
                Bounds::new(point(x, cut.top()), size(px(1.), cut.size.height)),
                line,
            ));
            window.paint_quad(fill(
                Bounds::new(point(cut.left(), y), size(cut.size.width, px(1.))),
                line,
            ));
        }
    }
    let length = px(BRACKET).min(cut.size.width.min(cut.size.height) / 2.);
    let thick = px(BRACKET_LINE);
    let half = thick / 2.;
    let white = palette::text();
    for (corner, right, bottom) in [
        (cut.origin, false, false),
        (cut.top_right(), true, false),
        (cut.bottom_left(), false, true),
        (cut.bottom_right(), true, true),
    ] {
        let x = if right {
            corner.x - length + half
        } else {
            corner.x - half
        };
        // Each arm is centered on the frame line.
        let y = corner.y - half;
        window.paint_quad(fill(Bounds::new(point(x, y), size(length, thick)), white));
        let x = corner.x - half;
        let y = if bottom {
            corner.y - length + half
        } else {
            corner.y - half
        };
        window.paint_quad(fill(Bounds::new(point(x, y), size(thick, length)), white));
    }
}

/// `paintPlayBadge`: a dim circle with a play triangle.
fn paint_play_badge(center: Point<Pixels>, window: &mut Window) {
    let badge = Bounds::centered_at(center, size(px(BADGE), px(BADGE)));
    window.paint_quad(fill(badge, palette::dim()).corner_radii(px(BADGE / 2.0)));
    let side = BADGE * 0.36;
    let mut path = PathBuilder::fill();
    // Nudged right so the triangle looks centered.
    let left = center.x - px(side * 0.38);
    path.move_to(point(left, center.y - px(side / 2.0)));
    path.line_to(point(left + px(side * 0.9), center.y));
    path.line_to(point(left, center.y + px(side / 2.0)));
    path.close();
    if let Ok(path) = path.build() {
        window.paint_path(path, palette::text());
    }
}

/// An avatar's crop fades outside its circle too (`CropType::Ellipse`):
/// each corner of the square, between its sides and the arc.
fn paint_round_fade(cut: Bounds<Pixels>, window: &mut Window) {
    let radius = cut.size.width.min(cut.size.height) / 2.;
    if radius <= px(0.) {
        return;
    }
    let center = cut.center();
    let (left, right) = (point(cut.left(), center.y), point(cut.right(), center.y));
    let (top, bottom) = (point(center.x, cut.top()), point(center.x, cut.bottom()));
    // Corner, then the arc counter-clockwise through that quarter.
    for (corner, from, to) in [
        (cut.origin, top, left),
        (cut.top_right(), right, top),
        (cut.bottom_right(), bottom, right),
        (cut.bottom_left(), left, bottom),
    ] {
        let mut path = PathBuilder::fill();
        path.move_to(corner);
        path.line_to(from);
        path.arc_to(point(radius, radius), px(0.), false, false, to);
        path.close();
        if let Ok(path) = path.build() {
            window.paint_path(path, palette::crop_fade());
        }
    }
}
