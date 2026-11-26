//! Settings › Appearance › Glass effects › the tuning dials: one slider per
//! knob of the glass shader the app draws (`ui/glass/recipes.rs`). Each
//! dial reads in the menu's own value (its bend in px, its blur, its
//! sheen…); every role scales its shipped value by the dial's share of the
//! default ([`GlassTuning::scale`]), so the defaults are the shipped look
//! exactly and a dial moves all roles together. Saved in
//! `appearance_prefs.json`; docs/decisions/glass-effects.md, "Tuning".
#![deny(clippy::indexing_slicing)]

use serde::{Deserialize, Serialize};

/// The dials, in whole stored steps so the appearance prefs stay `Eq`.
/// Each is clamped to its range when read ([`GlassTuning::get`]), so a
/// hand-edited file can't push the shader past what the sliders reach.
/// Missing fields take the default (the shipped look).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default)]
pub struct GlassTuning {
    /// How far the rim pulls the backdrop in, logical px.
    pub bend_px: u16,
    /// How deep into the slab the pull reaches, hundredths.
    pub bend_depth: u16,
    /// How sharply the pull gathers at the rim, hundredths.
    pub bend_curve: u16,
    /// The middle's frost, percent of the contrast floor (light theme).
    pub frost_pct: u16,
    /// The frost's blur, tenths of a logical px.
    pub blur_tenths: u16,
    /// Vibrancy of the bent backdrop, hundredths.
    pub saturation: u16,
    /// The top sheen, hundredths.
    pub sheen: u16,
    /// Rim light and fresnel, hundredths.
    pub rim_light: u16,
    /// The shade inside the rim, hundredths.
    pub inner_shade: u16,
    /// Alpha of the light line inside dark glass, percent.
    pub hairline_pct: u16,
    /// Strength of the drop shadow, percent of the shipped one.
    pub shadow_pct: u16,
    /// How far a button's rim pulls the backdrop in, units.
    pub button_refraction: u16,
    /// How much a button magnifies its middle, hundredths.
    pub button_magnify: u16,
    /// How far red and blue split at a button's rim, hundredths.
    pub color_split: u16,
}

/// One dial of the tuning panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GlassDial {
    Bend,
    BendDepth,
    BendCurve,
    Frost,
    Blur,
    Saturation,
    Sheen,
    RimLight,
    InnerShade,
    Hairline,
    Shadow,
    ButtonRefraction,
    ButtonMagnify,
    ColorSplit,
}

/// How a dial's value reads next to its slider.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialUnit {
    /// Logical pixels, whole.
    Pixels,
    /// Logical pixels with one decimal.
    FinePixels,
    /// A plain number with two decimals.
    Factor,
    /// Percent.
    Percent,
    /// A whole number (the shader's own units).
    Whole,
}

/// A dial's stored range, step, default and unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DialSpec {
    pub min: u16,
    pub max: u16,
    pub step: u16,
    /// The shipped look's value.
    pub default: u16,
    pub unit: DialUnit,
}

impl DialUnit {
    /// How many stored steps make one of the shown unit.
    fn per_unit(self) -> f32 {
        match self {
            DialUnit::Pixels | DialUnit::Percent | DialUnit::Whole => 1.0,
            DialUnit::FinePixels => 10.0,
            DialUnit::Factor => 100.0,
        }
    }
}

impl GlassDial {
    /// The panel's order: the slab's optics, its lighting, then buttons.
    pub const ALL: [GlassDial; 14] = [
        GlassDial::Bend,
        GlassDial::BendDepth,
        GlassDial::BendCurve,
        GlassDial::Frost,
        GlassDial::Blur,
        GlassDial::Saturation,
        GlassDial::Sheen,
        GlassDial::RimLight,
        GlassDial::InnerShade,
        GlassDial::Hairline,
        GlassDial::Shadow,
        GlassDial::ButtonRefraction,
        GlassDial::ButtonMagnify,
        GlassDial::ColorSplit,
    ];

    pub const fn spec(self) -> DialSpec {
        const fn d(min: u16, max: u16, step: u16, default: u16, unit: DialUnit) -> DialSpec {
            DialSpec {
                min,
                max,
                step,
                default,
                unit,
            }
        }
        match self {
            GlassDial::Bend => d(0, 160, 1, 80, DialUnit::Pixels),
            GlassDial::BendDepth => d(50, 300, 5, 170, DialUnit::Factor),
            GlassDial::BendCurve => d(100, 1200, 25, 400, DialUnit::Factor),
            GlassDial::Frost => d(0, 100, 1, 56, DialUnit::Percent),
            GlassDial::Blur => d(0, 120, 5, 20, DialUnit::FinePixels),
            GlassDial::Saturation => d(0, 250, 5, 130, DialUnit::Factor),
            GlassDial::Sheen => d(0, 400, 10, 180, DialUnit::Factor),
            GlassDial::RimLight => d(0, 300, 5, 100, DialUnit::Factor),
            GlassDial::InnerShade => d(0, 100, 5, 25, DialUnit::Factor),
            GlassDial::Hairline => d(0, 100, 1, 24, DialUnit::Percent),
            GlassDial::Shadow => d(0, 300, 5, 100, DialUnit::Percent),
            GlassDial::ButtonRefraction => d(0, 128, 1, 64, DialUnit::Whole),
            GlassDial::ButtonMagnify => d(0, 60, 1, 18, DialUnit::Factor),
            GlassDial::ColorSplit => d(0, 100, 1, 25, DialUnit::Factor),
        }
    }

    /// The value next to the slider, e.g. "80 px", "2.0 px", "1.30", "56%".
    pub fn label(self, tuning: &GlassTuning) -> String {
        let value = tuning.value(self);
        match self.spec().unit {
            DialUnit::Pixels => format!("{value:.0} px"),
            DialUnit::FinePixels => format!("{value:.1} px"),
            DialUnit::Factor => format!("{value:.2}"),
            DialUnit::Percent => format!("{value:.0}%"),
            DialUnit::Whole => format!("{value:.0}"),
        }
    }
}

impl Default for GlassTuning {
    fn default() -> Self {
        GlassDial::ALL
            .into_iter()
            .fold(Self::zero(), |mut tuning, dial| {
                *tuning.field_mut(dial) = dial.spec().default;
                tuning
            })
    }
}

impl GlassTuning {
    const fn zero() -> Self {
        Self {
            bend_px: 0,
            bend_depth: 0,
            bend_curve: 0,
            frost_pct: 0,
            blur_tenths: 0,
            saturation: 0,
            sheen: 0,
            rim_light: 0,
            inner_shade: 0,
            hairline_pct: 0,
            shadow_pct: 0,
            button_refraction: 0,
            button_magnify: 0,
            color_split: 0,
        }
    }

    fn field_mut(&mut self, dial: GlassDial) -> &mut u16 {
        match dial {
            GlassDial::Bend => &mut self.bend_px,
            GlassDial::BendDepth => &mut self.bend_depth,
            GlassDial::BendCurve => &mut self.bend_curve,
            GlassDial::Frost => &mut self.frost_pct,
            GlassDial::Blur => &mut self.blur_tenths,
            GlassDial::Saturation => &mut self.saturation,
            GlassDial::Sheen => &mut self.sheen,
            GlassDial::RimLight => &mut self.rim_light,
            GlassDial::InnerShade => &mut self.inner_shade,
            GlassDial::Hairline => &mut self.hairline_pct,
            GlassDial::Shadow => &mut self.shadow_pct,
            GlassDial::ButtonRefraction => &mut self.button_refraction,
            GlassDial::ButtonMagnify => &mut self.button_magnify,
            GlassDial::ColorSplit => &mut self.color_split,
        }
    }

    /// The dial's stored step, inside its range.
    pub fn get(&self, dial: GlassDial) -> u16 {
        let spec = dial.spec();
        let mut copy = *self;
        (*copy.field_mut(dial)).clamp(spec.min, spec.max)
    }

    /// The dial in its shown unit (px, a factor, percent).
    pub fn value(&self, dial: GlassDial) -> f32 {
        f32::from(self.get(dial)) / dial.spec().unit.per_unit()
    }

    /// The dial's share of its default: exactly 1 on the shipped look.
    /// Recipes multiply each role's shipped value by it.
    pub fn scale(&self, dial: GlassDial) -> f32 {
        let default = dial.spec().default;
        if default == 0 {
            return 1.0;
        }
        f32::from(self.get(dial)) / f32::from(default)
    }

    /// The tuning with `dial` on stored step `step` (a slider's value;
    /// rounded to the dial's step and clamped to its range).
    pub fn with(mut self, dial: GlassDial, step: f32) -> Self {
        let spec = dial.spec();
        let by = f32::from(spec.step.max(1));
        // The clamp keeps the cast in range; NaN lands on the minimum.
        let snapped = ((step / by).round() * by).clamp(f32::from(spec.min), f32::from(spec.max));
        *self.field_mut(dial) = if snapped.is_nan() {
            spec.min
        } else {
            snapped as u16
        };
        self
    }

    /// Whether every dial is on the shipped look.
    pub fn is_default(&self) -> bool {
        GlassDial::ALL
            .into_iter()
            .all(|dial| self.get(dial) == dial.spec().default)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_sit_on_their_steps_and_scale_by_exactly_one() {
        let tuning = GlassTuning::default();
        assert!(tuning.is_default());
        for dial in GlassDial::ALL {
            let spec = dial.spec();
            assert!(
                spec.min <= spec.default && spec.default <= spec.max,
                "{dial:?}"
            );
            assert_eq!((spec.default - spec.min) % spec.step, 0, "{dial:?}");
            assert_eq!((spec.max - spec.min) % spec.step, 0, "{dial:?}");
            assert_eq!(tuning.scale(dial), 1.0, "{dial:?}");
        }
    }

    #[test]
    fn a_slider_value_snaps_to_the_dial() {
        let tuning = GlassTuning::default();
        assert_eq!(tuning.with(GlassDial::Bend, 50.4).bend_px, 50);
        assert_eq!(tuning.with(GlassDial::Blur, 23.0).blur_tenths, 25);
        assert_eq!(tuning.with(GlassDial::BendDepth, 0.0).bend_depth, 50);
        assert_eq!(tuning.with(GlassDial::Frost, 400.0).frost_pct, 100);
        assert_eq!(tuning.with(GlassDial::Sheen, f32::NAN).sheen, 0);
        assert!(!tuning.with(GlassDial::Bend, 120.0).is_default());
        assert_eq!(
            tuning.with(GlassDial::Bend, 120.0).scale(GlassDial::Bend),
            1.5
        );
    }

    #[test]
    fn a_hand_edited_file_reads_inside_the_ranges() {
        let tuning: GlassTuning =
            serde_json::from_str(r#"{"bend_px": 900, "bend_depth": 1}"#).unwrap_or_default();
        assert_eq!(tuning.get(GlassDial::Bend), 160);
        assert_eq!(tuning.get(GlassDial::BendDepth), 50);
        // Missing fields take the default's.
        assert_eq!(tuning.frost_pct, GlassTuning::default().frost_pct);
    }

    #[test]
    fn values_read_in_their_units() {
        let tuning = GlassTuning::default();
        assert_eq!(GlassDial::Bend.label(&tuning), "80 px");
        assert_eq!(GlassDial::Blur.label(&tuning), "2.0 px");
        assert_eq!(GlassDial::Saturation.label(&tuning), "1.30");
        assert_eq!(GlassDial::Frost.label(&tuning), "56%");
        assert_eq!(GlassDial::ButtonRefraction.label(&tuning), "64");
    }
}
