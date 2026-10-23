//! The video story cover picker: a preview of the chosen frame over a strip
//! of frames cut across the clip, with a handle to drag (Telegram's mobile
//! cover picker; tdesktop's `Editor::VideoTimeline` is its frame-strip
//! cousin, see docs/decisions/story-album-picker-cover-strip.md). Frames
//! come from the same ffmpeg path as the video editor's strip.
#![deny(clippy::indexing_slicing)]

use super::*;
use crate::ui::story_viewer::chrome;
use quill::story_cover::{
    STRIP_FRAMES, fraction_of, frame_times, time_at, time_label, timestamp_secs,
};
use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

const PREVIEW_HEIGHT: f32 = 180.0;
const STRIP_HEIGHT: f32 = 56.0;
const HANDLE_WIDTH: f32 = 10.0;
const PANEL_WIDTH: f32 = 360.0;
/// A key press moves the handle by this share of the clip, Shift by more.
const KEY_STEP: f32 = 1.0 / 40.0;
const KEY_STEP_BIG: f32 = 1.0 / 8.0;

static NEXT_SERIAL: AtomicU64 = AtomicU64::new(1);

/// The picker's state while it is open on a story.
pub(in crate::ui) struct CoverPicker {
    chat_id: i64,
    story_id: i32,
    source: Option<PathBuf>,
    /// The clip's length, once read.
    duration_ms: Option<i64>,
    /// The chosen time.
    at_ms: i64,
    frames: Vec<Option<Arc<RenderImage>>>,
    preview: Option<Arc<RenderImage>>,
    /// The preview is being cut for this time.
    preview_job: Option<i64>,
    dragging: bool,
    unreadable: bool,
    bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    focus: FocusHandle,
    serial: u64,
}

impl QuillApp {
    /// Phase 9.5: toggle the cover picker for a video story.
    pub(in crate::ui) fn toggle_story_cover_edit(
        &mut self,
        chat_id: i64,
        story_id: i32,
        cx: &mut Context<Self>,
    ) {
        if self.stories.cover.is_some() {
            self.stories.cover = None;
            cx.notify();
            return;
        }
        self.stories.cover_sent = false;
        let source = self.story_video_path();
        let fallback_ms = self
            .stories
            .viewer
            .current()
            .and_then(|item| item.duration_secs)
            .map(|secs| i64::from(secs) * 1000);
        let serial = NEXT_SERIAL.fetch_add(1, Ordering::Relaxed);
        self.stories.cover = Some(CoverPicker {
            chat_id,
            story_id,
            unreadable: source.is_none(),
            source: source.clone(),
            duration_ms: None,
            at_ms: 0,
            frames: vec![None; STRIP_FRAMES],
            preview: None,
            preview_job: None,
            dragging: false,
            bounds: Rc::new(Cell::new(None)),
            focus: cx.focus_handle(),
            serial,
        });
        cx.notify();
        let Some(source) = source else { return };
        self.spawn_scoped(cx, async move |this, cx| {
            let probe_source = source.clone();
            let probed = cx
                .background_spawn(async move {
                    quill::video_edit::probe_source(&probe_source).map(|info| info.duration_ms)
                })
                .await;
            let duration_ms = probed.filter(|ms| *ms > 0).or(fallback_ms);
            let Some(duration_ms) = duration_ms else {
                let _ = this.update(cx, |this, cx| {
                    if let Some(cover) = this.stories.cover.as_mut().filter(|c| c.serial == serial)
                    {
                        cover.unreadable = true;
                    }
                    cx.notify();
                });
                return;
            };
            let opened = this.update(cx, |this, cx| {
                let Some(cover) = this.stories.cover.as_mut().filter(|c| c.serial == serial) else {
                    return false;
                };
                cover.duration_ms = Some(duration_ms);
                this.cover_refresh_preview(cx);
                cx.notify();
                true
            });
            if !matches!(opened, Ok(true)) {
                return;
            }
            let height = (STRIP_HEIGHT * 2.0) as u32;
            for (index, at) in frame_times(duration_ms, STRIP_FRAMES)
                .into_iter()
                .enumerate()
            {
                let source = source.clone();
                let still = cx
                    .background_spawn(async move {
                        let path =
                            quill::video_edit::render_strip_frame(&source, at, height).ok()?;
                        let still = crate::ui::video_editor::load_still(&path);
                        let _ = std::fs::remove_file(path);
                        still
                    })
                    .await;
                let alive = this.update(cx, |this, cx| {
                    let Some(cover) = this.stories.cover.as_mut().filter(|c| c.serial == serial)
                    else {
                        return false;
                    };
                    if let Some(slot) = cover.frames.get_mut(index) {
                        *slot = still;
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

    /// Cut the preview for the chosen time; one cut runs at a time and the
    /// last wish is cut when it ends.
    fn cover_refresh_preview(&mut self, cx: &mut Context<Self>) {
        let Some(cover) = self.stories.cover.as_mut() else {
            return;
        };
        let (Some(source), Some(_)) = (cover.source.clone(), cover.duration_ms) else {
            return;
        };
        if cover.preview_job.is_some() {
            return;
        }
        let at = cover.at_ms;
        cover.preview_job = Some(at);
        let serial = cover.serial;
        self.spawn_scoped(cx, async move |this, cx| {
            let still = cx
                .background_spawn(async move {
                    let path = quill::video_edit::render_frame(&source, at, 0, false).ok()?;
                    let still = crate::ui::video_editor::load_still(&path);
                    let _ = std::fs::remove_file(path);
                    still
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                let Some(cover) = this.stories.cover.as_mut().filter(|c| c.serial == serial) else {
                    return;
                };
                cover.preview_job = None;
                if still.is_some() {
                    cover.preview = still;
                }
                let wanted = cover.at_ms;
                if wanted != at {
                    this.cover_refresh_preview(cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// The handle goes to a position along the strip (0 to 1).
    fn cover_move_to(&mut self, fraction: f32, cx: &mut Context<Self>) {
        let Some(cover) = self.stories.cover.as_mut() else {
            return;
        };
        let Some(duration) = cover.duration_ms else {
            return;
        };
        let at = time_at(fraction, duration);
        if at != cover.at_ms {
            cover.at_ms = at;
            self.cover_refresh_preview(cx);
        }
        cx.notify();
    }

    fn cover_press(&mut self, x: f32, window: &mut Window, cx: &mut Context<Self>) {
        let Some(cover) = self.stories.cover.as_mut() else {
            return;
        };
        let Some(bounds) = cover.bounds.get() else {
            return;
        };
        cover.dragging = true;
        window.focus(&cover.focus, cx);
        let width = f32::from(bounds.size.width).max(1.0);
        self.cover_move_to((x - f32::from(bounds.origin.x)) / width, cx);
    }

    fn cover_drag(&mut self, x: f32, cx: &mut Context<Self>) {
        let Some(cover) = self.stories.cover.as_ref().filter(|c| c.dragging) else {
            return;
        };
        let Some(bounds) = cover.bounds.get() else {
            return;
        };
        let width = f32::from(bounds.size.width).max(1.0);
        self.cover_move_to((x - f32::from(bounds.origin.x)) / width, cx);
    }

    fn cover_release(&mut self) {
        if let Some(cover) = self.stories.cover.as_mut() {
            cover.dragging = false;
        }
    }

    fn cover_key(&mut self, key: &str, shift: bool, cx: &mut Context<Self>) {
        let Some(cover) = self.stories.cover.as_ref() else {
            return;
        };
        let Some(duration) = cover.duration_ms else {
            return;
        };
        let step = if shift { KEY_STEP_BIG } else { KEY_STEP };
        let here = fraction_of(cover.at_ms, duration);
        let to = match key {
            "left" => here - step,
            "right" => here + step,
            "home" => 0.0,
            "end" => 1.0,
            _ => return,
        };
        self.cover_move_to(to, cx);
    }

    /// Phase 9.5: the cover picker panel — preview, frame strip, time and
    /// the Set / Cancel buttons.
    pub(in crate::ui) fn story_cover_editor(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(cover) = self.stories.cover.as_ref() else {
            return div().into_any_element();
        };
        // Review fix-up: shared `story_manage.pending` slot — no second
        // op while one is in flight.
        let manage_busy = self.session().is_some_and(|s| s.stories.manage.pending);
        let chrome_text = || chrome::secondary();
        let ready = cover.duration_ms.is_some();
        let fraction = cover
            .duration_ms
            .map_or(0.0, |duration| fraction_of(cover.at_ms, duration));
        let time = time_label(cover.at_ms);
        let preview: AnyElement = match (&cover.preview, cover.unreadable) {
            (_, true) => div()
                .px_3()
                .text_sm()
                .text_color(chrome_text())
                .child(tr(keys::Q_MEDIA_COVER_UNREADABLE))
                .into_any_element(),
            (Some(image), false) => img(ImageSource::Render(image.clone()))
                .h_full()
                .object_fit(ObjectFit::Contain)
                .into_any_element(),
            (None, false) => div()
                .text_sm()
                .text_color(chrome_text())
                .child(tr(keys::Q_MEDIA_COVER_PREVIEW_LOADING))
                .into_any_element(),
        };
        let mut frames = div().flex().size_full().rounded_md().overflow_hidden();
        for (index, frame) in cover.frames.iter().enumerate() {
            let tile = div()
                .id(("story-cover-frame", index))
                .h_full()
                .flex_1()
                .overflow_hidden()
                .bg(chrome::frame_bg());
            frames = frames.child(match frame {
                Some(image) => tile.child(
                    img(ImageSource::Render(image.clone()))
                        .size_full()
                        .object_fit(ObjectFit::Cover),
                ),
                None => tile,
            });
        }
        let app = cx.entity().downgrade();
        let bounds_cell = cover.bounds.clone();
        let hit = canvas(
            move |bounds, window, _| window.insert_hitbox(bounds, HitboxBehavior::Normal),
            move |bounds, hitbox, window, _| {
                bounds_cell.set(Some(bounds));
                window.set_cursor_style(CursorStyle::ResizeLeftRight, &hitbox);
                let down_app = app.clone();
                window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
                    if phase == DispatchPhase::Bubble
                        && event.button == MouseButton::Left
                        && bounds.contains(&event.position)
                    {
                        let x = f32::from(event.position.x);
                        let _ = down_app.update(cx, |this, cx| this.cover_press(x, window, cx));
                    }
                });
                let move_app = app.clone();
                window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                    if phase == DispatchPhase::Bubble
                        && event.pressed_button == Some(MouseButton::Left)
                    {
                        let x = f32::from(event.position.x);
                        let _ = move_app.update(cx, |this, cx| this.cover_drag(x, cx));
                    }
                });
                let up_app = app.clone();
                window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
                    if phase == DispatchPhase::Bubble && event.button == MouseButton::Left {
                        let _ = up_app.update(cx, |this, _| this.cover_release());
                    }
                });
            },
        )
        .absolute()
        .size_full();
        let handle = div()
            .absolute()
            .top(px(-3.))
            .bottom(px(-3.))
            .left(relative(fraction))
            .ml(px(-HANDLE_WIDTH / 2.0))
            .w(px(HANDLE_WIDTH))
            .rounded_md()
            .border_2()
            .border_color(cx.theme().primary)
            .bg(gpui_kit::white().opacity(0.35));
        let strip = div()
            .id("story-cover-strip")
            .track_focus(&cover.focus)
            .focus_visible(|style| style.border_color(cx.theme().ring))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                this.cover_key(&event.keystroke.key, event.keystroke.modifiers.shift, cx);
            }))
            .relative()
            .w_full()
            .h(px(STRIP_HEIGHT))
            .child(frames)
            .when(ready, |this| this.child(handle))
            .child(hit);
        div()
            .id("story-cover-editor")
            .occlude()
            .flex()
            .flex_col()
            .gap_2()
            .w(px(PANEL_WIDTH))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_sm()
                            .font_semibold()
                            .text_color(chrome::fg())
                            .child(tr(keys::Q_MEDIA_COVER_CHOOSE)),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(chrome_text())
                            .child(time.clone()),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .h(px(PREVIEW_HEIGHT))
                    .w_full()
                    .rounded_md()
                    .overflow_hidden()
                    .bg(chrome::frame_bg())
                    .child(preview),
            )
            .child(strip)
            .child(
                div()
                    .flex()
                    .gap_2()
                    .justify_end()
                    .child(
                        Button::new("story-cover-cancel")
                            .label(tr(keys::CANCEL))
                            .custom(chrome::ghost(cx))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.stories.cover = None;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("story-cover-set")
                            .label(if self.stories.cover_sent {
                                tr(keys::Q_MEDIA_SAVING)
                            } else {
                                tr(keys::Q_MEDIA_SET_2)
                            })
                            .disabled(manage_busy || !ready)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.story_cover_save(cx);
                            })),
                    ),
            )
            .into_any_element()
    }

    /// Phase 9.5: send `editStoryCover` (`td_api.tl:13738`) for the chosen
    /// frame. The driver gates on `can_be_edited`; the tick closes the
    /// picker on success.
    pub(super) fn story_cover_save(&mut self, cx: &mut Context<Self>) {
        if self.stories.cover_sent {
            return;
        }
        let Some((chat_id, story_id, at_ms)) = self
            .stories
            .cover
            .as_ref()
            .map(|cover| (cover.chat_id, cover.story_id, cover.at_ms))
        else {
            return;
        };
        // Demo mode: surface the same notice as `story_composer_save_edit`
        // — management calls need live TDLib.
        if self.live.is_none() {
            if let Some(demo) = self.demo_session.as_mut() {
                demo.stories.manage.error =
                    Some(tr(keys::Q_MEDIA_DEMO_EDITING_RUNS_WITH_LIVE).into());
            }
            cx.notify();
            return;
        }
        if let Some(live) = self.live.as_mut() {
            match live
                .driver
                .edit_story_cover(ChatId(chat_id), story_id, timestamp_secs(at_ms))
            {
                Ok(_) => self.stories.cover_sent = true,
                Err(_) => {
                    live.driver.session.stories.manage.error =
                        Some(tr(keys::Q_MEDIA_COULD_NOT_SEND_THE_COVER).into());
                }
            }
        }
        cx.notify();
    }
}
