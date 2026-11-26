//! Settings › Appearance › Glass effects › the tuning dials: how far the
//! rim bends what is behind it, how deep the bevel is, the colour split,
//! the blur, the rim specular and the frost. Saved in
//! `appearance_prefs.json`; `ui/glass/recipes.rs` turns them into every
//! role's look. The dials follow the QuartzCore-style liquid glass demo
//! Idan shared (2026-11-26); docs/decisions/glass-effects.md, "Tuning".
#![deny(clippy::indexing_slicing)]

use serde::{Deserialize, Serialize};

/// The dials, in whole steps so the appearance prefs stay `Eq`. Each field
/// is clamped to its dial's range when it is read ([`GlassTuning::get`]),
/// so a hand-edited file can't push the shader past what the sliders
/// reach. Missing fields take the default (the Clear preset).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default)]
pub struct GlassTuning {
    /// How far past the rim the edge of the glass reaches, logical px.
    pub refraction_px: u8,
    /// How deep the rounded bevel is, logical px.
    pub bevel_px: u8,
    /// How far red and blue split apart at the rim, logical px.
    pub aberration_px: u8,
    /// The frost's blur, tenths of a logical px.
    pub blur_tenths: u8,
    /// The rim specular, hundredths.
    pub specular_pct: u8,
    /// How much of the contrast floor's tint the middle takes, percent.
    pub frost_pct: u8,
}

impl Default for GlassTuning {
    fn default() -> Self {
        GlassPreset::Clear.tuning()
    }
}

/// One dial of the tuning panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GlassDial {
    Refraction,
    Bevel,
    Aberration,
    Blur,
    Specular,
    Frost,
}

/// How a dial's value reads next to its slider.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialUnit {
    /// Logical pixels.
    Pixels,
    /// A plain multiplier with two decimals.
    Factor,
    /// Percent.
    Percent,
}

impl GlassDial {
    /// The panel's order.
    pub const ALL: [GlassDial; 6] = [
        GlassDial::Refraction,
        GlassDial::Bevel,
        GlassDial::Aberration,
        GlassDial::Blur,
        GlassDial::Specular,
        GlassDial::Frost,
    ];

    /// The stored steps the slider spans, and the step it moves by.
    pub fn steps(self) -> (u8, u8, u8) {
        match self {
            GlassDial::Refraction => (0, 80, 1),
            GlassDial::Bevel => (6, 90, 1),
            GlassDial::Aberration => (0, 36, 1),
            GlassDial::Blur => (0, 160, 5),
            GlassDial::Specular => (0, 160, 2),
            GlassDial::Frost => (0, 100, 1),
        }
    }

    pub fn unit(self) -> DialUnit {
        match self {
            GlassDial::Refraction | GlassDial::Bevel | GlassDial::Aberration | GlassDial::Blur => {
                DialUnit::Pixels
            }
            GlassDial::Specular => DialUnit::Factor,
            GlassDial::Frost => DialUnit::Percent,
        }
    }

    /// How many stored steps make one of the shown unit.
    fn per_unit(self) -> f32 {
        match self {
            GlassDial::Blur => 10.0,
            GlassDial::Specular => 100.0,
            _ => 1.0,
        }
    }

    /// The value next to the slider, e.g. "44 px", "1.15", "56%".
    pub fn label(self, tuning: &GlassTuning) -> String {
        let value = tuning.value(self);
        match self.unit() {
            DialUnit::Pixels if self == GlassDial::Blur => format!("{value:.1} px"),
            DialUnit::Pixels => format!("{value:.0} px"),
            DialUnit::Factor => format!("{value:.2}"),
            DialUnit::Percent => format!("{value:.0}%"),
        }
    }
}

impl GlassTuning {
    fn field(&self, dial: GlassDial) -> u8 {
        match dial {
            GlassDial::Refraction => self.refraction_px,
            GlassDial::Bevel => self.bevel_px,
            GlassDial::Aberration => self.aberration_px,
            GlassDial::Blur => self.blur_tenths,
            GlassDial::Specular => self.specular_pct,
            GlassDial::Frost => self.frost_pct,
        }
    }

    /// The dial's stored step, inside its range.
    pub fn get(&self, dial: GlassDial) -> u8 {
        let (min, max, _) = dial.steps();
        self.field(dial).clamp(min, max)
    }

    /// The dial in its shown unit (px, a factor, percent).
    pub fn value(&self, dial: GlassDial) -> f32 {
        f32::from(self.get(dial)) / dial.per_unit()
    }

    /// The tuning with `dial` on stored step `step` (a slider's value;
    /// rounded to the dial's step and clamped to its range).
    pub fn with(mut self, dial: GlassDial, step: f32) -> Self {
        let (min, max, by) = dial.steps();
        let by = f32::from(by.max(1));
        // The clamp keeps the cast in range; NaN lands on the minimum.
        let snapped = ((step / by).round() * by).clamp(f32::from(min), f32::from(max));
        let snapped = if snapped.is_nan() { min } else { snapped as u8 };
        let field = match dial {
            GlassDial::Refraction => &mut self.refraction_px,
            GlassDial::Bevel => &mut self.bevel_px,
            GlassDial::Aberration => &mut self.aberration_px,
            GlassDial::Blur => &mut self.blur_tenths,
            GlassDial::Specular => &mut self.specular_pct,
            GlassDial::Frost => &mut self.frost_pct,
        };
        *field = snapped;
        self
    }

    /// The preset these dials are on, if any.
    pub fn preset(&self) -> Option<GlassPreset> {
        GlassPreset::ALL
            .into_iter()
            .find(|preset| preset.tuning() == self.clamped())
    }

    /// Every dial inside its range.
    pub fn clamped(&self) -> Self {
        GlassDial::ALL.into_iter().fold(*self, |tuning, dial| {
            tuning.with(dial, f32::from(self.get(dial)))
        })
    }
}

/// The starting points of the panel, after the demo's variants. Its Dark
/// variant is left out: Quill's glass takes its tint from the theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GlassPreset {
    /// Thicker frost and blur: calm over busy content.
    Regular,
    /// The default: little blur, the backdrop shows through.
    Clear,
    /// Deep bend, a strong colour split and a bright rim.
    Prism,
}

impl GlassPreset {
    pub const ALL: [GlassPreset; 3] =
        [GlassPreset::Regular, GlassPreset::Clear, GlassPreset::Prism];

    pub fn tuning(self) -> GlassTuning {
        match self {
            GlassPreset::Regular => GlassTuning {
                refraction_px: 38,
                bevel_px: 30,
                aberration_px: 10,
                blur_tenths: 60,
                specular_pct: 76,
                frost_pct: 80,
            },
            GlassPreset::Clear => GlassTuning {
                refraction_px: 44,
                bevel_px: 34,
                aberration_px: 14,
                blur_tenths: 20,
                specular_pct: 86,
                frost_pct: 56,
            },
            GlassPreset::Prism => GlassTuning {
                refraction_px: 58,
                bevel_px: 42,
                aberration_px: 26,
                blur_tenths: 5,
                specular_pct: 116,
                frost_pct: 44,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_sit_on_their_dials_and_are_found_again() {
        for preset in GlassPreset::ALL {
            let tuning = preset.tuning();
            assert_eq!(tuning.clamped(), tuning, "{preset:?} is off its steps");
            assert_eq!(tuning.preset(), Some(preset));
        }
        assert_eq!(GlassTuning::default().preset(), Some(GlassPreset::Clear));
    }

    #[test]
    fn a_slider_value_snaps_to_the_dial() {
        let tuning = GlassTuning::default();
        assert_eq!(tuning.with(GlassDial::Refraction, 50.4).refraction_px, 50);
        assert_eq!(tuning.with(GlassDial::Blur, 23.0).blur_tenths, 25);
        assert_eq!(tuning.with(GlassDial::Bevel, 0.0).bevel_px, 6);
        assert_eq!(tuning.with(GlassDial::Frost, 400.0).frost_pct, 100);
        assert_eq!(tuning.with(GlassDial::Specular, f32::NAN).specular_pct, 0);
        let moved = tuning.with(GlassDial::Aberration, 20.0);
        assert_eq!(moved.preset(), None);
    }

    #[test]
    fn a_hand_edited_file_reads_inside_the_ranges() {
        let tuning: GlassTuning =
            serde_json::from_str(r#"{"refraction_px": 250, "bevel_px": 1}"#).unwrap_or_default();
        assert_eq!(tuning.get(GlassDial::Refraction), 80);
        assert_eq!(tuning.get(GlassDial::Bevel), 6);
        // Missing fields take the default's.
        assert_eq!(tuning.frost_pct, GlassTuning::default().frost_pct);
    }

    #[test]
    fn values_read_in_their_units() {
        let tuning = GlassPreset::Prism.tuning();
        assert_eq!(GlassDial::Refraction.label(&tuning), "58 px");
        assert_eq!(GlassDial::Blur.label(&tuning), "0.5 px");
        assert_eq!(GlassDial::Specular.label(&tuning), "1.16");
        assert_eq!(GlassDial::Frost.label(&tuning), "44%");
    }
}
