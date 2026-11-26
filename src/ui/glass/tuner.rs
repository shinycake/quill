//! Settings › Appearance › Glass effects › the tuning panel: a slider per
//! dial of the shader (`quill::glass_tuning`), a live preview and Reset.
//! The dials are `AppearancePrefs::glass_tuning`; the sliders are built
//! from them and rebuilt whenever the prefs move without them (Reset). A
//! drag redraws every window with each step and saves on release.
#![deny(clippy::indexing_slicing)]

use crate::ui::QuillApp;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::slider::{Slider, SliderEvent, SliderState};
use gpui_kit::component::theme::ActiveTheme;
use gpui_kit::component::{Disableable as _, Sizable as _};
use gpui_kit::*;
use quill::glass_tuning::{GlassDial, GlassTuning};
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
        GlassDial::Bend => keys::Q_PREFS_GLASS_BEND,
        GlassDial::BendDepth => keys::Q_PREFS_GLASS_BEND_DEPTH,
        GlassDial::BendCurve => keys::Q_PREFS_GLASS_BEND_CURVE,
        GlassDial::Frost => keys::Q_PREFS_GLASS_FROST,
        GlassDial::Blur => keys::Q_PREFS_GLASS_BLUR,
        GlassDial::Saturation => keys::Q_PREFS_GLASS_SATURATION,
        GlassDial::Sheen => keys::Q_PREFS_GLASS_SHEEN,
        GlassDial::RimLight => keys::Q_PREFS_GLASS_RIM_LIGHT,
        GlassDial::InnerShade => keys::Q_PREFS_GLASS_INNER_SHADE,
        GlassDial::Hairline => keys::Q_PREFS_GLASS_HAIRLINE,
        GlassDial::Shadow => keys::Q_PREFS_GLASS_SHADOW,
        GlassDial::ButtonRefraction => keys::Q_PREFS_GLASS_BUTTON_REFRACTION,
        GlassDial::ButtonMagnify => keys::Q_PREFS_GLASS_BUTTON_MAGNIFY,
        GlassDial::ColorSplit => keys::Q_PREFS_GLASS_COLOR_SPLIT,
    }
}

fn dial_id(dial: GlassDial) -> &'static str {
    match dial {
        GlassDial::Bend => "glass-dial-bend",
        GlassDial::BendDepth => "glass-dial-bend-depth",
        GlassDial::BendCurve => "glass-dial-bend-curve",
        GlassDial::Frost => "glass-dial-frost",
        GlassDial::Blur => "glass-dial-blur",
        GlassDial::Saturation => "glass-dial-saturation",
        GlassDial::Sheen => "glass-dial-sheen",
        GlassDial::RimLight => "glass-dial-rim-light",
        GlassDial::InnerShade => "glass-dial-inner-shade",
        GlassDial::Hairline => "glass-dial-hairline",
        GlassDial::Shadow => "glass-dial-shadow",
        GlassDial::ButtonRefraction => "glass-dial-button-refraction",
        GlassDial::ButtonMagnify => "glass-dial-button-magnify",
        GlassDial::ColorSplit => "glass-dial-color-split",
    }
}

/// The demo's dials: `id=step` pairs by comma (`bend=120,frost=20`), the
/// id as in [`dial_id`] without its prefix; anything else is skipped.
pub(crate) fn demo_dials(spec: &str) -> GlassTuning {
    spec.split(',')
        .filter_map(|pair| pair.split_once('='))
        .filter_map(|(name, step)| {
            let dial = GlassDial::ALL
                .into_iter()
                .find(|dial| dial_id(*dial).strip_prefix("glass-dial-") == Some(name.trim()))?;
            Some((dial, step.trim().parse::<f32>().ok()?))
        })
        .fold(GlassTuning::default(), |tuning, (dial, step)| {
            tuning.with(dial, step)
        })
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
                let spec = dial.spec();
                let slider = cx.new(|_| {
                    SliderState::new()
                        .min(f32::from(spec.min))
                        .max(f32::from(spec.max))
                        .step(f32::from(spec.step))
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
        let reset = Button::new("glass-tuning-reset")
            .label(tr(keys::Q_PREFS_RESET))
            .ghost()
            .small()
            .disabled(tuning.is_default())
            .on_click(cx.listener(|this, _, _, cx| {
                this.set_appearance(cx, |a| a.glass_tuning = GlassTuning::default());
            }));
        let preview = self.glass_tuner_preview(cx);
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
                .child(div().grid().grid_cols(2).gap_x_6().gap_y_3().children(rows))
                .child(div().flex().justify_end().child(reset))
                .into_any_element(),
        )
    }
}
