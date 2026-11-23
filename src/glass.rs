//! Glass effects (Settings › Appearance › Glass effects): the stored choice
//! and the rule that turns it, with what the system reports, into the look
//! every window draws. Pure data and one pure function; the UI side is
//! `ui/glass` (docs/decisions/glass-effects.md).
#![deny(clippy::indexing_slicing)]

use serde::{Deserialize, Serialize};

/// What the person picked. An enum so a later look (a clearer glass) needs no
/// migration; `Solid` is the default, so files written before the switch
/// load unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Material {
    #[default]
    Solid,
    Glass,
}

/// Why a window draws the solid look.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SolidReason {
    /// The switch is off.
    Off,
    /// The system's Reduce Transparency (macOS) or Transparency effects off
    /// (Windows).
    ReduceTransparency,
    /// Quill's high-contrast theme, or the system's Increase Contrast.
    HighContrast,
    /// The battery saver or Low Power Mode with "Save Power on Low Battery"
    /// on, or the system running hot.
    SavingPower,
    /// The window renders on a software adapter.
    Unsupported,
}

/// Whether glass moves (rim light leaning to the pointer, the selection
/// sliding) or holds still for Reduce Motion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Motion {
    Full,
    Still,
}

/// What every window draws: today's solid surfaces, with the reason, or
/// glass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlassLook {
    Solid(SolidReason),
    Glass(Motion),
}

impl GlassLook {
    pub fn is_glass(self) -> bool {
        matches!(self, GlassLook::Glass(_))
    }
}

/// `QUILL_GLASS`, for demos and tests: `off` is the switch off, `on` the
/// switch on, `force` the switch on with the software-adapter row skipped
/// (so CI can capture glass on lavapipe and WARP).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlassOverride {
    Off,
    On,
    Force,
}

impl GlassOverride {
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "off" | "0" => Some(Self::Off),
            "on" | "1" => Some(Self::On),
            "force" => Some(Self::Force),
            _ => None,
        }
    }

    /// The override in the environment, if any.
    pub fn from_env() -> Option<Self> {
        std::env::var("QUILL_GLASS")
            .ok()
            .and_then(|value| Self::parse(&value))
    }
}

/// What the system reports. A probe that has not answered counts as "no
/// reduction", as the motion probe does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Readings {
    /// System Reduce Transparency / Transparency effects off.
    pub reduce_transparency: bool,
    /// System Increase Contrast.
    pub increase_contrast: bool,
    /// Quill's own high-contrast theme.
    pub high_contrast_theme: bool,
    /// Every power-saving flag is forced (`battery_saving::forcing`).
    pub saving_power: bool,
    /// The window's GPU is a software rasterizer.
    pub software_adapter: bool,
    /// System or Quill reduced motion.
    pub reduce_motion: bool,
}

/// The look, from the stored choice, the readings and the override. The
/// first matching row wins (the table in docs/decisions/glass-effects.md).
pub fn resolve(
    material: Material,
    readings: Readings,
    overridden: Option<GlassOverride>,
) -> GlassLook {
    let switch_on = match overridden {
        Some(GlassOverride::Off) => false,
        Some(GlassOverride::On | GlassOverride::Force) => true,
        None => material == Material::Glass,
    };
    if !switch_on {
        return GlassLook::Solid(SolidReason::Off);
    }
    if readings.reduce_transparency {
        return GlassLook::Solid(SolidReason::ReduceTransparency);
    }
    if readings.high_contrast_theme || readings.increase_contrast {
        return GlassLook::Solid(SolidReason::HighContrast);
    }
    if readings.saving_power {
        return GlassLook::Solid(SolidReason::SavingPower);
    }
    if readings.software_adapter && overridden != Some(GlassOverride::Force) {
        return GlassLook::Solid(SolidReason::Unsupported);
    }
    if readings.reduce_motion {
        GlassLook::Glass(Motion::Still)
    } else {
        GlassLook::Glass(Motion::Full)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLEAR: Readings = Readings {
        reduce_transparency: false,
        increase_contrast: false,
        high_contrast_theme: false,
        saving_power: false,
        software_adapter: false,
        reduce_motion: false,
    };

    fn on(readings: Readings) -> GlassLook {
        resolve(Material::Glass, readings, None)
    }

    #[test]
    fn the_switch_off_is_solid_whatever_the_system_says() {
        assert_eq!(
            resolve(Material::Solid, CLEAR, None),
            GlassLook::Solid(SolidReason::Off)
        );
        let everything = Readings {
            reduce_transparency: true,
            saving_power: true,
            ..CLEAR
        };
        assert_eq!(
            resolve(Material::Solid, everything, None),
            GlassLook::Solid(SolidReason::Off)
        );
    }

    #[test]
    fn each_reading_pauses_glass_with_its_own_reason() {
        let cases = [
            (
                Readings {
                    reduce_transparency: true,
                    ..CLEAR
                },
                SolidReason::ReduceTransparency,
            ),
            (
                Readings {
                    high_contrast_theme: true,
                    ..CLEAR
                },
                SolidReason::HighContrast,
            ),
            (
                Readings {
                    increase_contrast: true,
                    ..CLEAR
                },
                SolidReason::HighContrast,
            ),
            (
                Readings {
                    saving_power: true,
                    ..CLEAR
                },
                SolidReason::SavingPower,
            ),
            (
                Readings {
                    software_adapter: true,
                    ..CLEAR
                },
                SolidReason::Unsupported,
            ),
        ];
        for (readings, reason) in cases {
            assert_eq!(on(readings), GlassLook::Solid(reason), "{readings:?}");
        }
    }

    #[test]
    fn the_first_matching_row_wins() {
        let all = Readings {
            reduce_transparency: true,
            increase_contrast: true,
            saving_power: true,
            software_adapter: true,
            reduce_motion: true,
            ..CLEAR
        };
        assert_eq!(on(all), GlassLook::Solid(SolidReason::ReduceTransparency));
        let contrast_and_power = Readings {
            high_contrast_theme: true,
            saving_power: true,
            ..CLEAR
        };
        assert_eq!(
            on(contrast_and_power),
            GlassLook::Solid(SolidReason::HighContrast)
        );
    }

    #[test]
    fn reduce_motion_keeps_glass_but_holds_it_still() {
        assert_eq!(on(CLEAR), GlassLook::Glass(Motion::Full));
        let still = Readings {
            reduce_motion: true,
            ..CLEAR
        };
        assert_eq!(on(still), GlassLook::Glass(Motion::Still));
    }

    #[test]
    fn the_override_sets_the_switch_and_force_skips_only_the_adapter_row() {
        assert_eq!(
            resolve(Material::Glass, CLEAR, Some(GlassOverride::Off)),
            GlassLook::Solid(SolidReason::Off)
        );
        assert_eq!(
            resolve(Material::Solid, CLEAR, Some(GlassOverride::On)),
            GlassLook::Glass(Motion::Full)
        );
        let software = Readings {
            software_adapter: true,
            ..CLEAR
        };
        assert_eq!(
            resolve(Material::Solid, software, Some(GlassOverride::On)),
            GlassLook::Solid(SolidReason::Unsupported)
        );
        assert_eq!(
            resolve(Material::Solid, software, Some(GlassOverride::Force)),
            GlassLook::Glass(Motion::Full)
        );
        let saving = Readings {
            saving_power: true,
            ..CLEAR
        };
        assert_eq!(
            resolve(Material::Solid, saving, Some(GlassOverride::Force)),
            GlassLook::Solid(SolidReason::SavingPower)
        );
    }

    #[test]
    fn the_override_parses_its_three_words() {
        assert_eq!(GlassOverride::parse("off"), Some(GlassOverride::Off));
        assert_eq!(GlassOverride::parse(" ON "), Some(GlassOverride::On));
        assert_eq!(GlassOverride::parse("force"), Some(GlassOverride::Force));
        assert_eq!(GlassOverride::parse("maybe"), None);
    }

    #[test]
    fn the_stored_choice_defaults_to_solid_and_round_trips() {
        assert_eq!(Material::default(), Material::Solid);
        let json = serde_json::to_string(&Material::Glass).unwrap_or_default();
        assert_eq!(json, "\"glass\"");
        let back: Material = serde_json::from_str(&json).unwrap_or_default();
        assert_eq!(back, Material::Glass);
    }
}
