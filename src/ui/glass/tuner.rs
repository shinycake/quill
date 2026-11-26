//! Settings › Appearance › Glass effects › the tuning panel: presets, a
//! slider per dial (`quill::glass_tuning`), a live preview and Reset. The
//! dials are `AppearancePrefs::glass_tuning`; the sliders are built from
//! them and rebuilt whenever the prefs move without them (a preset, Reset).
//! A drag redraws every window with each step and saves on release.
#![deny(clippy::indexing_slicing)]

use crate::ui::QuillApp;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::slider::{Slider, SliderEvent, SliderState};
use gpui_kit::component::theme::ActiveTheme;
use gpui_kit::component::{Disableable as _, Sizable as _};
use gpui_kit::*;
use quill::glass_tuning::{GlassDial, GlassPreset, GlassTuning};
use quill::lang::{Key, keys, tr};

/// The panel's sliders, once Settings showed it.
#[derive(Default)]
pub(crate) struct GlassTuner {
    sliders: Option<TunerSliders>,
}

/// One slider per dial, and the dials they were built from.
struct TunerSliders {
    built_from: GlassTuning,
    dials: Vec<(GlassDial, Entity<SliderState>)>,
}

fn dial_name(dial: GlassDial) -> Key {
    match dial {
        GlassDial::Refraction => keys::Q_PREFS_GLASS_REFRACTION,
        GlassDial::Bevel => keys::Q_PREFS_GLASS_BEVEL,
        GlassDial::Aberration => keys::Q_PREFS_GLASS_ABERRATION,
        GlassDial::Blur => keys::Q_PREFS_GLASS_BLUR,
        GlassDial::Specular => keys::Q_PREFS_GLASS_SPECULAR,
        GlassDial::Frost => keys::Q_PREFS_GLASS_FROST,
    }
}

fn preset_name(preset: GlassPreset) -> Key {
    match preset {
        GlassPreset::Regular => keys::Q_PREFS_GLASS_PRESET_REGULAR,
        GlassPreset::Clear => keys::Q_PREFS_GLASS_PRESET_CLEAR,
        GlassPreset::Prism => keys::Q_PREFS_GLASS_PRESET_PRISM,
    }
}

fn dial_id(dial: GlassDial) -> &'static str {
    match dial {
        GlassDial::Refraction => "glass-dial-refraction",
        GlassDial::Bevel => "glass-dial-bevel",
        GlassDial::Aberration => "glass-dial-aberration",
        GlassDial::Blur => "glass-dial-blur",
        GlassDial::Specular => "glass-dial-specular",
        GlassDial::Frost => "glass-dial-frost",
    }
}

impl QuillApp {
    /// Build the sliders for the dials in the prefs, unless they already
    /// show them. Called as the Appearance page renders.
    pub(in crate::ui) fn sync_glass_tuner(&mut self, cx: &mut Context<Self>) {
        let tuning = self.appearance.glass_tuning;
        let tuner = &self.preferences.glass_tuner;
        if tuner
            .sliders
            .as_ref()
            .is_some_and(|sliders| sliders.built_from == tuning)
        {
            return;
        }
        let dials = GlassDial::ALL
            .into_iter()
            .map(|dial| {
                let (min, max, step) = dial.steps();
                let slider = cx.new(|_| {
                    SliderState::new()
                        .min(f32::from(min))
                        .max(f32::from(max))
                        .step(f32::from(step))
                        .default_value(f32::from(tuning.get(dial)))
                });
                cx.subscribe(&slider, move |this, _, event: &SliderEvent, cx| {
                    this.on_glass_dial(dial, event, cx);
                })
                .detach();
                (dial, slider)
            })
            .collect();
        self.preferences.glass_tuner.sliders = Some(TunerSliders {
            built_from: tuning,
            dials,
        });
    }

    /// A dial moved (`Change`, while dragging) or was let go (`Release`).
    fn on_glass_dial(&mut self, dial: GlassDial, event: &SliderEvent, cx: &mut Context<Self>) {
        let (SliderEvent::Change(value) | SliderEvent::Release(value)) = event;
        let tuning = self.appearance.glass_tuning.with(dial, value.end());
        if let Some(sliders) = self.preferences.glass_tuner.sliders.as_mut() {
            sliders.built_from = tuning;
        }
        match event {
            SliderEvent::Change(_) => {
                self.appearance.glass_tuning = tuning;
                super::set_tuning(tuning, cx);
                cx.notify();
            }
            SliderEvent::Release(_) => {
                self.set_appearance(cx, |appearance| appearance.glass_tuning = tuning);
            }
        }
    }

    /// The panel under the switch, while glass shows.
    pub(super) fn glass_tuner_panel(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let sliders = self.preferences.glass_tuner.sliders.as_ref()?;
        let tuning = self.appearance.glass_tuning;
        let current = tuning.preset();
        let mut presets = div().flex().flex_wrap().gap_2().items_center();
        for preset in GlassPreset::ALL {
            presets = presets.child(self.appearance_chip(
                format!("glass-preset-{preset:?}").to_lowercase(),
                tr(preset_name(preset)),
                current == Some(preset),
                cx,
                move |this, cx| {
                    this.set_appearance(cx, |a| a.glass_tuning = preset.tuning());
                },
            ));
        }
        let reset = Button::new("glass-tuning-reset")
            .label(tr(keys::Q_PREFS_RESET))
            .ghost()
            .small()
            .disabled(tuning == GlassTuning::default())
            .on_click(cx.listener(|this, _, _, cx| {
                this.set_appearance(cx, |a| a.glass_tuning = GlassTuning::default());
            }));
        let preview = super::tuner_preview::preview(cx);
        let theme = cx.theme();
        let rows = sliders.dials.iter().map(|(dial, slider)| {
            let name = tr(dial_name(*dial));
            div()
                .id(dial_id(*dial))
                .test_support()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap_2()
                        .child(div().text_sm().child(name))
                        .child(
                            div()
                                .text_sm()
                                .text_color(theme.muted_foreground)
                                .child(dial.label(&tuning)),
                        ),
                )
                .child(Slider::new(slider).accessibility_label(name))
        });
        Some(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(preview)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap_2()
                        .child(presets)
                        .child(reset),
                )
                .child(div().grid().grid_cols(2).gap_x_6().gap_y_3().children(rows))
                .into_any_element(),
        )
    }
}
