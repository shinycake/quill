//! The custom background editor (tdesktop `info_userpic_colors_editor`):
//! one to four colors, added and removed with + and -, each set by hex or
//! by hue, saturation and lightness.

use super::{QuillApp, render_rgba, replace_image};
use gpui_kit::component::button::*;
use gpui_kit::component::input::{InputEvent, TextareaState};
use gpui_kit::component::slider::{Slider, SliderEvent, SliderState};
use gpui_kit::component::*;
use gpui_kit::*;
use quill::profile_photo_gradient::{
    MAX_COLORS, format_hex, from_hsl, next_color, parse_hex, render_background, to_hsl,
};
use quill::profile_photo_sources::PRESET_COUNT;
use std::sync::Arc;

const PREVIEW_PX: f32 = 120.0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Axis {
    Hue,
    Saturation,
    Lightness,
}

pub(crate) struct GradientEditor {
    colors: Vec<u32>,
    selected: usize,
    /// What the three sliders show for the selected color.
    hsl: (f32, f32, f32),
    hex: Entity<TextareaState>,
    hue: Entity<SliderState>,
    saturation: Entity<SliderState>,
    lightness: Entity<SliderState>,
    preview: Option<Arc<RenderImage>>,
}

impl Drop for GradientEditor {
    fn drop(&mut self) {
        if let Some(image) = self.preview.take() {
            super::super::image_budget::retire_all([image]);
        }
    }
}

impl GradientEditor {
    fn redraw(&mut self) {
        let square = render_background(&self.colors, (PREVIEW_PX * 2.0) as u32);
        replace_image(
            &mut self.preview,
            render_rgba(square.side, square.side, square.rgba),
        );
    }
}

impl QuillApp {
    fn editor_mut(&mut self) -> Option<&mut GradientEditor> {
        self.sources_mut()?.emoji.editor.as_mut()
    }

    pub(super) fn open_gradient_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(emoji) = self.sources().map(|s| &s.emoji) else {
            return;
        };
        let mut colors = emoji.colors();
        colors.truncate(MAX_COLORS);
        if colors.is_empty() {
            colors.push(0x4D8DFF);
        }
        let hsl = to_hsl(colors[0]);
        let hex = cx.new(|cx| {
            let mut state = TextareaState::new(window, cx)
                .placeholder("#RRGGBB")
                .auto_grow(1, 1)
                .submit_on_enter(false);
            state.set_value(format_hex(colors[0]), window, cx);
            state
        });
        cx.subscribe_in(
            &hex,
            window,
            |this, entity, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::Change) {
                    let text = entity.read(cx).value().to_string();
                    this.editor_hex_changed(&text, window, cx);
                }
            },
        )
        .detach();
        let slider =
            |axis: Axis, max: f32, value: f32, window: &mut Window, cx: &mut Context<Self>| {
                let entity = cx.new(|_| {
                    SliderState::new()
                        .min(0.0)
                        .max(max)
                        .step(max / 360.0)
                        .default_value(value)
                });
                cx.subscribe_in(
                    &entity,
                    window,
                    move |this, _, event: &SliderEvent, window, cx| {
                        let (SliderEvent::Change(v) | SliderEvent::Release(v)) = event;
                        this.editor_slider_moved(axis, v.end(), window, cx);
                    },
                )
                .detach();
                entity
            };
        let hue = slider(Axis::Hue, 360.0, hsl.0, window, cx);
        let saturation = slider(Axis::Saturation, 1.0, hsl.1, window, cx);
        let lightness = slider(Axis::Lightness, 1.0, hsl.2, window, cx);
        let mut editor = GradientEditor {
            colors,
            selected: 0,
            hsl,
            hex,
            hue,
            saturation,
            lightness,
            preview: None,
        };
        editor.redraw();
        if let Some(emoji) = self.sources_mut().map(|s| &mut s.emoji) {
            emoji.editor = Some(editor);
        }
        cx.notify();
    }

    /// Show the selected color in the hex field and on the sliders.
    fn editor_sync_controls(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = self.editor_mut() else {
            return;
        };
        let color = editor.colors[editor.selected];
        editor.hsl = to_hsl(color);
        let (h, s, l) = editor.hsl;
        let hex_text = format_hex(color);
        let (hex, hue, sat, light) = (
            editor.hex.clone(),
            editor.hue.clone(),
            editor.saturation.clone(),
            editor.lightness.clone(),
        );
        hue.update(cx, |state, cx| state.set_value(h, window, cx));
        sat.update(cx, |state, cx| state.set_value(s, window, cx));
        light.update(cx, |state, cx| state.set_value(l, window, cx));
        // Typing "#fff" must not be rewritten to "#FFFFFF" under the caret.
        if parse_hex(hex.read(cx).value().as_ref()) != Some(color) {
            hex.update(cx, |state, cx| state.set_value(hex_text, window, cx));
        }
    }

    fn editor_set_color(&mut self, color: u32, cx: &mut Context<Self>) {
        let Some(editor) = self.editor_mut() else {
            return;
        };
        let at = editor.selected;
        editor.colors[at] = color;
        editor.redraw();
        cx.notify();
    }

    fn editor_hex_changed(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(color) = parse_hex(text) else {
            return;
        };
        let Some(editor) = self.editor_mut() else {
            return;
        };
        if editor.colors[editor.selected] == color {
            return;
        }
        self.editor_set_color(color, cx);
        self.editor_sync_controls(window, cx);
    }

    fn editor_slider_moved(
        &mut self,
        axis: Axis,
        value: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(editor) = self.editor_mut() else {
            return;
        };
        let (mut h, mut s, mut l) = editor.hsl;
        match axis {
            Axis::Hue => h = value,
            Axis::Saturation => s = value,
            Axis::Lightness => l = value,
        }
        if (h, s, l) == editor.hsl {
            return;
        }
        editor.hsl = (h, s, l);
        let color = from_hsl(h, s, l);
        let hex = editor.hex.clone();
        self.editor_set_color(color, cx);
        // The hex field follows; its own change finds the color equal.
        hex.update(cx, |state, cx| {
            state.set_value(format_hex(color), window, cx);
        });
    }

    fn editor_select(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(editor) = self.editor_mut()
            && index < editor.colors.len()
        {
            editor.selected = index;
        }
        self.editor_sync_controls(window, cx);
        cx.notify();
    }

    fn editor_add(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(editor) = self.editor_mut()
            && editor.colors.len() < MAX_COLORS
        {
            let color = next_color(*editor.colors.last().unwrap_or(&0x4D8DFF));
            editor.colors.push(color);
            editor.selected = editor.colors.len() - 1;
            editor.redraw();
        }
        self.editor_sync_controls(window, cx);
        cx.notify();
    }

    fn editor_remove(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(editor) = self.editor_mut()
            && editor.colors.len() > 1
        {
            editor.colors.pop();
            editor.selected = editor.selected.min(editor.colors.len() - 1);
            editor.redraw();
        }
        self.editor_sync_controls(window, cx);
        cx.notify();
    }

    fn editor_apply(&mut self, cx: &mut Context<Self>) {
        if let Some(emoji) = self.sources_mut().map(|s| &mut s.emoji)
            && let Some(editor) = emoji.editor.take()
        {
            emoji.custom = editor.colors.clone();
            emoji.preset = PRESET_COUNT;
            emoji.refresh();
        }
        cx.notify();
    }

    fn editor_cancel(&mut self, cx: &mut Context<Self>) {
        if let Some(emoji) = self.sources_mut().map(|s| &mut s.emoji) {
            emoji.close_editor();
        }
        cx.notify();
    }

    pub(super) fn gradient_editor_panel(
        &self,
        editor: &GradientEditor,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let primary = theme.primary;
        let transparent = theme.transparent;
        let mut swatches = div().flex().gap_2().items_center().justify_center();
        for (index, color) in editor.colors.iter().enumerate() {
            swatches = swatches.child(
                div()
                    .id(("profile-photo-editor-color", index))
                    .size(px(32.))
                    .rounded_full()
                    .border_2()
                    .border_color(if index == editor.selected {
                        primary
                    } else {
                        transparent
                    })
                    .bg(rgb(*color))
                    .cursor_pointer()
                    .role(Role::Button)
                    .aria_label(format!("Color {}", index + 1))
                    .tab_index(0)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.editor_select(index, window, cx);
                    })),
            );
        }
        let can_add = editor.colors.len() < MAX_COLORS;
        let can_remove = editor.colors.len() > 1;
        swatches = swatches
            .child(
                Button::new("profile-photo-editor-add")
                    .label("+")
                    .ghost()
                    .disabled(!can_add)
                    .on_click(cx.listener(|this, _, window, cx| this.editor_add(window, cx))),
            )
            .child(
                Button::new("profile-photo-editor-remove")
                    .label("−")
                    .ghost()
                    .disabled(!can_remove)
                    .on_click(cx.listener(|this, _, window, cx| this.editor_remove(window, cx))),
            );
        let row = |caption: &'static str, slider: &Entity<SliderState>| {
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(div().text_xs().text_color(muted).child(caption))
                .child(Slider::new(slider))
        };
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(div().font_semibold().child("Custom background"))
            .child(
                div().flex().justify_center().child(
                    div()
                        .size(px(PREVIEW_PX))
                        .flex_none()
                        .rounded_full()
                        .children(editor.preview.clone().map(|image| {
                            img(ImageSource::Render(image))
                                .size(px(PREVIEW_PX))
                                .rounded_full()
                        })),
                ),
            )
            .child(swatches)
            .child(
                input::Textarea::new(&editor.hex)
                    .aria_label("Color in hex")
                    .h(px(40.)),
            )
            .child(row("Hue", &editor.hue))
            .child(row("Saturation", &editor.saturation))
            .child(row("Lightness", &editor.lightness))
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("profile-photo-editor-cancel")
                            .label("Cancel")
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| this.editor_cancel(cx))),
                    )
                    .child(
                        Button::new("profile-photo-editor-save")
                            .label("Save")
                            .on_click(cx.listener(|this, _, _, cx| this.editor_apply(cx))),
                    ),
            )
            .into_any_element()
    }
}
