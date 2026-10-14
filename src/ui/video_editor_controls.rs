//! The video editor's layer and its controls under the picture (tdesktop
//! `VideoEditor::applyGeometry`): the strip, the quality dots
//! (`VideoQualitySlider`) and the rounded bar with Cancel, Rotate, Flip,
//! GIF and Done (`ControlsBar`, `BarTextButton`), each as wide as the
//! photo editor's bar at most.

use super::app::QuillApp;
use super::shell::DialogKind;
use super::video_editor::{EditorTarget, VideoEditorDialog, palette};
use super::video_editor_timeline::Fade;
use gpui_kit::TestSupportExt;
use gpui_kit::component::{Icon, Sizable};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::video_edit::{quality_label, quality_levels};
use quill::video_timeline::{
    QUALITY_DOT, QUALITY_DOT_ACTIVE, QUALITY_HEIGHT, QUALITY_LABEL_SKIP, QUALITY_LINE,
    QUALITY_LINE_GAP, quality_dot_x, quality_index_at,
};
use std::time::Instant;

/// `photoEditorButtonBarWidth` / `photoEditorButtonBarHeight`.
const BAR_WIDTH: f32 = 422.0;
const BAR_HEIGHT: f32 = 48.0;
/// `photoEditorTextButtonPadding`: Cancel and Done.
const TEXT_BUTTON_PADDING: f32 = 22.0;
/// `videoEditorContentSkip`, `videoEditorTimelineTop`,
/// `videoEditorQualitySkip`, `videoEditorBarSkip`, `videoEditorBarBottomSkip`.
const CONTENT_SKIP: f32 = 20.0;
const TIMELINE_TOP: f32 = 10.0;
const QUALITY_SKIP: f32 = 6.0;
const BAR_SKIP: f32 = 12.0;
const BAR_BOTTOM_SKIP: f32 = 16.0;
/// `videoEditorHintSkip`.
const HINT_SKIP: f32 = 8.0;
/// `kShiftDuration`: the active quality dot slides over.
const SHIFT_MS: u64 = 120;
/// `videoQualitySliderStyle`: 12 px semibold.
const QUALITY_FONT: f32 = 12.0;

impl QuillApp {
    /// The whole layer: the picture over the controls, and the cover bubble
    /// above everything.
    pub(super) fn video_editor_layer(&mut self, cx: &mut Context<Self>) -> AnyElement {
        self.pump_video_editor(cx);
        let Some(editor) = self.composer_ui.send_box.video_editor.as_ref() else {
            return div().into_any_element();
        };
        let ready = editor.timeline.is_some();
        let stage = self.video_editor_stage(editor, cx);
        let mut controls = div()
            .w_full()
            .max_w(px(BAR_WIDTH))
            .flex()
            .flex_col()
            .pt(px(TIMELINE_TOP))
            .pb(px(BAR_BOTTOM_SKIP));
        if ready {
            controls = controls.child(self.video_editor_timeline(editor, cx));
        }
        if let Some(note) = &editor.note {
            controls = controls.child(
                div()
                    .pt(px(HINT_SKIP))
                    .w_full()
                    .text_center()
                    .text_sm()
                    .text_color(palette::muted())
                    .child(note.clone()),
            );
        }
        if let Some(quality) = self.video_editor_quality(editor, cx) {
            controls = controls.child(div().pt(px(QUALITY_SKIP)).child(quality));
        }
        controls = controls.child(
            div()
                .pt(px(BAR_SKIP))
                .child(self.video_editor_bar(editor, cx)),
        );
        div()
            .id("video-editor")
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .text_color(palette::text())
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .p(px(CONTENT_SKIP))
                    .pb_0()
                    .child(stage),
            )
            .child(
                div()
                    .flex()
                    .justify_center()
                    .px(px(CONTENT_SKIP))
                    .child(controls),
            )
            .children(self.video_editor_bubble(editor))
            .into_any_element()
    }

    fn set_video_editor_quality(&mut self, index: usize, cx: &mut Context<Self>) {
        let reduce = cx.reduce_motion();
        let Some(editor) = self.composer_ui.send_box.video_editor.as_mut() else {
            return;
        };
        let Some(info) = editor.info else {
            return;
        };
        let (width, height) = editor.edits.edited_size(&info);
        let levels = quality_levels(width, height);
        let Some(level) = levels.get(index).copied() else {
            return;
        };
        if editor.edits.quality == level {
            return;
        }
        // Slide from where the dot is now (`setIndex`).
        let was = levels
            .iter()
            .position(|known| *known == editor.edits.quality)
            .unwrap_or(levels.len().saturating_sub(1));
        editor.motion.quality_shift = Fade::default();
        editor.motion.quality_shift.start(was as f32, 0, true);
        editor
            .motion
            .quality_shift
            .start(index as f32, SHIFT_MS, reduce);
        editor.edits.quality = level;
        cx.notify();
    }

    fn video_quality_pointer(&mut self, x: Pixels, pressed: Option<bool>, cx: &mut Context<Self>) {
        let Some(editor) = self.composer_ui.send_box.video_editor.as_mut() else {
            return;
        };
        if let Some(pressed) = pressed {
            editor.motion.quality_pressed = pressed;
        } else if !editor.motion.quality_pressed {
            return;
        }
        let (Some(bounds), Some(info)) = (editor.motion.quality_bounds.get(), editor.info) else {
            return;
        };
        let (width, height) = editor.edits.edited_size(&info);
        let count = quality_levels(width, height).len();
        let index = quality_index_at(
            f32::from(x - bounds.origin.x),
            count,
            f32::from(bounds.size.width),
        );
        self.set_video_editor_quality(index, cx);
    }

    /// The quality dots, when the clip offers a choice and the target takes
    /// one (a sticker is always encoded at the set's side).
    fn video_editor_quality(
        &self,
        editor: &VideoEditorDialog,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if matches!(editor.target, EditorTarget::Set { .. }) {
            return None;
        }
        let (width, height) = editor.edits.edited_size(&editor.info?);
        let levels = quality_levels(width, height);
        if levels.len() < 2 {
            return None;
        }
        let index = levels
            .iter()
            .position(|level| *level == editor.edits.quality)
            .unwrap_or(levels.len() - 1);
        let motion = &editor.motion.quality_shift;
        let active = if motion.animating(Instant::now()) {
            motion.value(Instant::now())
        } else {
            index as f32
        };
        let labels: Vec<String> = levels.iter().map(|level| quality_label(*level)).collect();
        let cell = editor.motion.quality_bounds.clone();
        let app = cx.entity().downgrade();
        Some(
            canvas(
                move |bounds, window, _| window.insert_hitbox(bounds, HitboxBehavior::Normal),
                move |bounds, hitbox, window, cx| {
                    cell.set(Some(bounds));
                    paint_quality(bounds, &labels, index, active, window, cx);
                    window.set_cursor_style(CursorStyle::PointingHand, &hitbox);
                    let down_app = app.clone();
                    let down_hitbox = hitbox.clone();
                    window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
                        if phase == DispatchPhase::Bubble
                            && event.button == MouseButton::Left
                            && down_hitbox.is_hovered(window)
                        {
                            let x = event.position.x;
                            let _ = down_app.update(cx, |this, cx| {
                                this.video_quality_pointer(x, Some(true), cx)
                            });
                        }
                    });
                    let move_app = app.clone();
                    window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                        if phase == DispatchPhase::Bubble
                            && event.pressed_button == Some(MouseButton::Left)
                        {
                            let x = event.position.x;
                            let _ = move_app
                                .update(cx, |this, cx| this.video_quality_pointer(x, None, cx));
                        }
                    });
                    let up_app = app.clone();
                    window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
                        if phase == DispatchPhase::Bubble && event.button == MouseButton::Left {
                            let _ = up_app.update(cx, |this, _| {
                                if let Some(editor) =
                                    this.composer_ui.send_box.video_editor.as_mut()
                                {
                                    editor.motion.quality_pressed = false;
                                }
                            });
                        }
                    });
                },
            )
            .w_full()
            .h(px(QUALITY_HEIGHT))
            .into_any_element(),
        )
    }

    /// The rounded bar: Cancel at the left, Done at the right, the tools
    /// spread between them.
    fn video_editor_bar(&self, editor: &VideoEditorDialog, cx: &mut Context<Self>) -> AnyElement {
        let ready = editor.info.is_some() && !editor.busy;
        // The sound is dropped anyway for a sticker or a replacement, so
        // there is nothing to choose (`removeAudio`).
        let gif_allowed = matches!(editor.target, EditorTarget::Tray(_));
        let gif = editor.edits.gif;
        let flipped = editor.edits.flipped;
        let text_button = |id: &'static str, label: &'static str, color: Hsla| {
            div()
                .id(id)
                .test_support()
                .h_full()
                .px(px(TEXT_BUTTON_PADDING))
                .flex()
                .items_center()
                .rounded_full()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(color)
                .cursor_pointer()
                .hover(|style| style.bg(palette::hover()))
                .child(label)
        };
        let icon_button = |id: &'static str, icon: Icon, active: bool| {
            div()
                .id(id)
                .test_support()
                .size(px(BAR_HEIGHT))
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .cursor_pointer()
                .hover(|style| style.bg(palette::hover()))
                .child(icon.with_size(px(22.)).text_color(if active {
                    palette::accent()
                } else {
                    palette::text().opacity(0.85)
                }))
        };
        div()
            .w_full()
            .h(px(BAR_HEIGHT))
            .flex()
            .items_center()
            .justify_between()
            .rounded_full()
            .bg(palette::bar())
            .child(
                text_button("video-editor-cancel", "Cancel", palette::text()).on_click(
                    cx.listener(|this, _, window, cx| {
                        this.close_video_editor(cx);
                        this.close_kit_dialog_if_done(DialogKind::VideoEditor, window, cx);
                    }),
                ),
            )
            .child(
                icon_button(
                    "video-editor-rotate",
                    Icon::new(gpui_kit::assets::IconName::RotateCwSquare),
                    false,
                )
                .tooltip(|window, cx| Tooltip::new("Rotate").build(window, cx))
                .on_click(cx.listener(|this, _, _, cx| this.rotate_video_editor(cx))),
            )
            .child(
                icon_button(
                    "video-editor-flip",
                    Icon::new(gpui_kit::assets::IconName::FlipHorizontal2),
                    flipped,
                )
                .tooltip(|window, cx| Tooltip::new("Flip").build(window, cx))
                .on_click(cx.listener(|this, _, _, cx| this.flip_video_editor(cx))),
            )
            .when(gif_allowed, |bar| {
                bar.child(
                    div()
                        .id("video-editor-gif")
                        .test_support()
                        .size(px(BAR_HEIGHT))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_full()
                        .cursor_pointer()
                        .hover(|style| style.bg(palette::hover()))
                        .tooltip(|window, cx| {
                            Tooltip::new("Send as GIF, without sound").build(window, cx)
                        })
                        .child(gif_glyph(gif))
                        .on_click(cx.listener(|this, _, _, cx| this.toggle_video_editor_gif(cx))),
                )
            })
            .child(
                text_button(
                    "video-editor-done",
                    "Done",
                    if ready {
                        palette::link()
                    } else {
                        palette::link().opacity(0.4)
                    },
                )
                .on_click(cx.listener(|this, _, window, cx| {
                    if this.finish_video_editor(window, cx) {
                        this.close_kit_dialog_if_done(DialogKind::VideoEditor, window, cx);
                    }
                })),
            )
            .into_any_element()
    }

    fn toggle_video_editor_gif(&mut self, cx: &mut Context<Self>) {
        if let Some(editor) = self.composer_ui.send_box.video_editor.as_mut() {
            editor.edits.gif = !editor.edits.gif;
            cx.notify();
        }
    }
}

use gpui_kit::component::tooltip::Tooltip;

/// tdesktop's `photo_editor/gif` icon: "GIF" in a rounded frame, blue when
/// the clip goes out as a GIF.
fn gif_glyph(active: bool) -> Div {
    let color = if active {
        palette::accent()
    } else {
        palette::text().opacity(0.85)
    };
    div()
        .px(px(3.))
        .h(px(18.))
        .flex()
        .items_center()
        .rounded(px(4.))
        .border_2()
        .border_color(color)
        .text_color(color)
        .text_size(px(10.))
        .font_weight(FontWeight::BOLD)
        .child("GIF")
}

/// `VideoQualitySlider::paintEvent`: lines between the dots, the active dot
/// larger, the labels under them (or only the active one when they don't
/// fit).
fn paint_quality(
    bounds: Bounds<Pixels>,
    labels: &[String],
    index: usize,
    active: f32,
    window: &mut Window,
    cx: &mut App,
) {
    let count = labels.len();
    let width = f32::from(bounds.size.width);
    let at = |x: f32, y: f32| point(bounds.origin.x + px(x), bounds.origin.y + px(y));
    let line_y = QUALITY_DOT_ACTIVE / 2.0;
    for i in 0..count.saturating_sub(1) {
        let from = quality_dot_x(i, count, width) + QUALITY_DOT / 2.0 + QUALITY_LINE_GAP;
        let till = quality_dot_x(i + 1, count, width) - QUALITY_DOT / 2.0 - QUALITY_LINE_GAP;
        if till > from {
            window.paint_quad(
                fill(
                    Bounds::new(
                        at(from, line_y - QUALITY_LINE / 2.0),
                        size(px(till - from), px(QUALITY_LINE)),
                    ),
                    palette::inactive(),
                )
                .corner_radii(px(QUALITY_LINE / 2.0)),
            );
        }
    }
    for i in 0..count {
        let distance = (active - i as f32).abs().min(1.0);
        let side = QUALITY_DOT_ACTIVE + (QUALITY_DOT - QUALITY_DOT_ACTIVE) * distance;
        let color = palette::text().opacity(if distance < 1.0 { 1.0 } else { 0.6 });
        window.paint_quad(
            fill(
                Bounds::centered_at(
                    at(quality_dot_x(i, count, width), line_y),
                    size(px(side), px(side)),
                ),
                color,
            )
            .corner_radii(px(side / 2.0)),
        );
    }
    let shape = |text: &str, color: Hsla, window: &mut Window| {
        let mut font = window.text_style().font();
        font.weight = FontWeight::SEMIBOLD;
        let run = TextRun {
            len: text.len(),
            font,
            color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        window
            .text_system()
            .shape_line(text.to_string().into(), px(QUALITY_FONT), &[run], None)
    };
    let top = QUALITY_DOT_ACTIVE + QUALITY_LABEL_SKIP;
    let line_height = px(QUALITY_HEIGHT - top);
    let lines: Vec<_> = labels
        .iter()
        .enumerate()
        .map(|(i, label)| {
            let color = if i == index {
                palette::text()
            } else {
                palette::muted()
            };
            shape(label, color, window)
        })
        .collect();
    let total: f32 = lines.iter().map(|line| f32::from(line.width)).sum();
    // `LabelsMinSkip`: a font height between labels at least.
    let fit = total + count.saturating_sub(1) as f32 * QUALITY_FONT * 1.25 <= width;
    if fit {
        for (i, line) in lines.iter().enumerate() {
            let w = f32::from(line.width);
            let x = (quality_dot_x(i, count, width) - w / 2.0).clamp(0.0, (width - w).max(0.0));
            let _ = line.paint(at(x, top), line_height, TextAlign::Left, None, window, cx);
        }
    } else if let Some(line) = lines.get(index) {
        let x = (width - f32::from(line.width)) / 2.0;
        let _ = line.paint(at(x, top), line_height, TextAlign::Left, None, window, cx);
    }
}
