//! Liquid glass on macOS: when it is on, and what it looks like.
//!
//! Glass is a material for floating menus (the functional layer), never for
//! content. This module decides two things from plain data. [`glass_mode`]
//! says whether glass is drawn at all, from the platform, the "Glass
//! effects" switch and the two macOS accessibility options. [`regular`]
//! says how it looks, from a const table of [`Recipe`]s whose numbers were
//! fitted to the system material and then raised until text on them is
//! readable over any backdrop (the tests below check every grey and a few
//! saturated colours against the real theme colours).
//!
//! The state lives in process-wide slots like `power_saving`'s: the switch
//! is pushed by the appearance code, the OS flags by the macOS watcher.
//! Nothing mirrors [`glass_mode_now`] into a UI flag; every surface asks
//! on each render.
#![deny(clippy::indexing_slicing)]

use std::sync::atomic::{AtomicBool, Ordering};

/// The two macOS accessibility options that change how glass looks or
/// whether it is allowed at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct OsTransparency {
    /// System Settings > Accessibility > Display > Reduce transparency.
    pub reduce_transparency: bool,
    /// System Settings > Accessibility > Display > Increase contrast.
    pub increase_contrast: bool,
}

impl OsTransparency {
    /// The screenshot demos' overrides on top of what the system reports:
    /// `QUILL_REDUCE_TRANSPARENCY=1` and `QUILL_INCREASE_CONTRAST=1` turn
    /// the option on. They never turn a system option off.
    pub fn with_env_overrides(self) -> Self {
        Self {
            reduce_transparency: self.reduce_transparency
                || env_is_set("QUILL_REDUCE_TRANSPARENCY"),
            increase_contrast: self.increase_contrast || env_is_set("QUILL_INCREASE_CONTRAST"),
        }
    }
}

fn env_is_set(name: &str) -> bool {
    std::env::var(name).is_ok_and(|v| v == "1" || v.eq_ignore_ascii_case("true"))
}

/// Whether menus are drawn as glass, and with which accessibility variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlassMode {
    /// Solid surfaces, exactly the look before glass existed.
    Off,
    /// Glass; `increase_contrast` asks for the heavier-tinted variant with
    /// a full-contrast border.
    On { increase_contrast: bool },
}

/// Glass needs a platform that has it, the switch on, and Reduce
/// transparency off. Increase contrast only changes the variant.
pub const fn glass_mode(
    platform_supports: bool,
    setting_on: bool,
    os: OsTransparency,
) -> GlassMode {
    if platform_supports && setting_on && !os.reduce_transparency {
        GlassMode::On {
            increase_contrast: os.increase_contrast,
        }
    } else {
        GlassMode::Off
    }
}

/// How the "Glass effects" switch is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwitchState {
    /// The user's choice, changeable.
    Free { on: bool },
    /// Reduce transparency is on: shown off and locked, with a note that
    /// the system setting decides.
    OverriddenBySystem,
}

pub const fn switch_state(setting_on: bool, os: OsTransparency) -> SwitchState {
    if os.reduce_transparency {
        SwitchState::OverriddenBySystem
    } else {
        SwitchState::Free { on: setting_on }
    }
}

/// The light or dark look of the theme the glass sits in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Appearance {
    Light,
    Dark,
}

/// Every number of one glass material. Lengths are in points, colours in
/// sRGB 0..1. The backdrop goes through
/// `out = gain * (luma + (c - luma) * saturation) + tint_level * tint_alpha`
/// per channel, so the material is a blur of what lies behind it, pulled
/// toward a grey.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Recipe {
    pub gain: f32,
    pub saturation: f32,
    pub tint_level: f32,
    pub tint_alpha: f32,
    pub blur_pt: f32,
    /// Width of the bevel where the backdrop bends at the rim.
    pub lens_pt: f32,
    /// Farthest the bevel pulls a sample from.
    pub reach_pt: f32,
    pub magnify: f32,
    pub dispersion: f32,
    /// Brightness added to the rim line.
    pub edge: f32,
    pub edge_width_pt: f32,
    pub edge_aa_pt: f32,
    /// A full-contrast border around the surface (Increase contrast).
    pub border: bool,
}

const fn grey(level_255: u32) -> f32 {
    level_255 as f32 / 255.
}

/// Fitted to the system's Regular material: dark is `0.139 * b + 41/255`
/// with a 3.5 pt blur, light is `0.142 * b + 212/255` with a 6 pt blur.
/// The dark gain is halved (0.07): at the fitted 0.139 a bright backdrop
/// lifted the interior until the secondary text of the Tinted theme fell to
/// 2.4:1. Over a dark backdrop (a 0.2 grey) the interior differs by under
/// 4/255.
const REGULAR_DARK: Recipe = Recipe {
    gain: 0.07,
    saturation: 1.4,
    tint_level: grey(41),
    tint_alpha: 1.,
    blur_pt: 3.5,
    lens_pt: 19.,
    reach_pt: 19.,
    magnify: 1.,
    dispersion: 0.03,
    edge: 0.12,
    edge_width_pt: 1.5,
    edge_aa_pt: 1.,
    border: false,
};

const REGULAR_LIGHT: Recipe = Recipe {
    gain: 0.142,
    tint_level: grey(212),
    blur_pt: 6.,
    ..REGULAR_DARK
};

/// Increase contrast: the backdrop barely shows through and the surface
/// gets a border.
const REGULAR_DARK_HIGH_CONTRAST: Recipe = Recipe {
    gain: 0.05,
    tint_level: grey(30),
    border: true,
    ..REGULAR_DARK
};

const REGULAR_LIGHT_HIGH_CONTRAST: Recipe = Recipe {
    gain: 0.05,
    tint_level: grey(238),
    border: true,
    ..REGULAR_LIGHT
};

/// Every Regular recipe, in the order the readability test walks them.
pub const REGULAR_RECIPES: [(Appearance, bool, Recipe); 4] = [
    (Appearance::Light, false, REGULAR_LIGHT),
    (Appearance::Dark, false, REGULAR_DARK),
    (Appearance::Light, true, REGULAR_LIGHT_HIGH_CONTRAST),
    (Appearance::Dark, true, REGULAR_DARK_HIGH_CONTRAST),
];

/// The Regular material, for text-heavy floating surfaces such as menus.
pub const fn regular(appearance: Appearance, increase_contrast: bool) -> Recipe {
    match (appearance, increase_contrast) {
        (Appearance::Light, false) => REGULAR_LIGHT,
        (Appearance::Dark, false) => REGULAR_DARK,
        (Appearance::Light, true) => REGULAR_LIGHT_HIGH_CONTRAST,
        (Appearance::Dark, true) => REGULAR_DARK_HIGH_CONTRAST,
    }
}

/// The colour of the glass interior over a grey backdrop `b` (sRGB 0..1).
/// Saturation does not move a grey.
pub fn interior(recipe: &Recipe, backdrop: f32) -> f32 {
    (recipe.gain * backdrop + recipe.tint_level * recipe.tint_alpha).clamp(0., 1.)
}

/// The interior over a coloured backdrop, as the shader computes it.
pub fn shade(recipe: &Recipe, backdrop: [f32; 3]) -> [f32; 3] {
    let [r, g, b] = backdrop;
    let luma = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    let tint = recipe.tint_level * recipe.tint_alpha;
    [r, g, b].map(|c| (recipe.gain * (luma + (c - luma) * recipe.saturation) + tint).clamp(0., 1.))
}

const SETTING: usize = 0;
const OS_REDUCE_TRANSPARENCY: usize = 1;
const OS_INCREASE_CONTRAST: usize = 2;
const SLOTS: usize = 3;

static GLOBAL: [AtomicBool; SLOTS] = [const { AtomicBool::new(false) }; SLOTS];
static ISOLATED: AtomicBool = AtomicBool::new(false);

thread_local! {
    static LOCAL: std::cell::Cell<[bool; SLOTS]> = const { std::cell::Cell::new([false; SLOTS]) };
}

/// Keep this state per thread from now on. The app keeps one process-wide
/// copy; UI tests run in parallel, so each gets its own (the same split as
/// `power_saving`).
#[doc(hidden)]
pub fn isolate_for_tests() {
    ISOLATED.store(true, Ordering::Relaxed);
}

fn slot_get(slot: usize) -> bool {
    if ISOLATED.load(Ordering::Relaxed) {
        LOCAL.with(|c| c.get().get(slot).copied().unwrap_or(false))
    } else {
        GLOBAL
            .get(slot)
            .is_some_and(|value| value.load(Ordering::Relaxed))
    }
}

fn slot_set(slot: usize, value: bool) {
    if ISOLATED.load(Ordering::Relaxed) {
        LOCAL.with(|c| {
            let mut all = c.get();
            if let Some(entry) = all.get_mut(slot) {
                *entry = value;
            }
            c.set(all);
        });
    } else if let Some(entry) = GLOBAL.get(slot) {
        entry.store(value, Ordering::Relaxed);
    }
}

/// Push the "Glass effects" switch. `QUILL_GLASS=0` (screenshot demos)
/// keeps glass off whatever the saved setting says.
pub fn set_setting(on: bool) {
    slot_set(SETTING, on && !env_forces_off());
}

fn env_forces_off() -> bool {
    std::env::var("QUILL_GLASS").is_ok_and(|v| v == "0")
}

/// Remember what the system reports. Callable from any thread.
pub fn set_os(os: OsTransparency) {
    slot_set(OS_REDUCE_TRANSPARENCY, os.reduce_transparency);
    slot_set(OS_INCREASE_CONTRAST, os.increase_contrast);
}

/// The system options as last reported.
pub fn os_transparency() -> OsTransparency {
    OsTransparency {
        reduce_transparency: slot_get(OS_REDUCE_TRANSPARENCY),
        increase_contrast: slot_get(OS_INCREASE_CONTRAST),
    }
}

/// Whether the switch is on, as last pushed.
pub fn setting_on() -> bool {
    slot_get(SETTING)
}

/// Whether glass is drawn right now. Always [`GlassMode::Off`] off macOS.
pub fn glass_mode_now() -> GlassMode {
    glass_mode(cfg!(target_os = "macos"), setting_on(), os_transparency())
}

#[cfg(test)]
mod tests {
    use super::*;

    const NONE: OsTransparency = OsTransparency {
        reduce_transparency: false,
        increase_contrast: false,
    };

    #[test]
    fn mode_covers_every_combination() {
        for platform in [false, true] {
            for setting in [false, true] {
                for reduce in [false, true] {
                    for contrast in [false, true] {
                        let os = OsTransparency {
                            reduce_transparency: reduce,
                            increase_contrast: contrast,
                        };
                        let expected = if platform && setting && !reduce {
                            GlassMode::On {
                                increase_contrast: contrast,
                            }
                        } else {
                            GlassMode::Off
                        };
                        assert_eq!(
                            glass_mode(platform, setting, os),
                            expected,
                            "platform {platform} setting {setting} reduce {reduce} contrast {contrast}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn reduce_transparency_locks_the_switch_off() {
        let reduce = OsTransparency {
            reduce_transparency: true,
            ..NONE
        };
        assert_eq!(switch_state(true, reduce), SwitchState::OverriddenBySystem);
        assert_eq!(switch_state(false, reduce), SwitchState::OverriddenBySystem);
        assert_eq!(switch_state(true, NONE), SwitchState::Free { on: true });
        assert_eq!(switch_state(false, NONE), SwitchState::Free { on: false });
        let contrast = OsTransparency {
            increase_contrast: true,
            ..NONE
        };
        assert_eq!(switch_state(true, contrast), SwitchState::Free { on: true });
    }

    #[test]
    fn slots_drive_the_mode_and_are_off_the_mac() {
        isolate_for_tests();
        assert_eq!(glass_mode_now(), GlassMode::Off, "nothing pushed yet");
        set_setting(true);
        let expected = if cfg!(target_os = "macos") {
            GlassMode::On {
                increase_contrast: false,
            }
        } else {
            GlassMode::Off
        };
        assert_eq!(glass_mode_now(), expected);
        set_os(OsTransparency {
            reduce_transparency: true,
            increase_contrast: true,
        });
        assert_eq!(glass_mode_now(), GlassMode::Off);
        assert!(os_transparency().increase_contrast);
        set_os(NONE);
        set_setting(false);
        assert_eq!(glass_mode_now(), GlassMode::Off);
    }

    #[test]
    fn regular_table_matches_the_system_fit() {
        let dark = regular(Appearance::Dark, false);
        assert_eq!((dark.gain, dark.blur_pt), (0.07, 3.5));
        assert!((interior(&dark, 0.) - 41. / 255.).abs() < 1e-6);
        // Over a dark backdrop the readable gain stays within 4/255 of the
        // measured material, `0.139 * b + 41/255`.
        let fitted = 0.139 * 0.2 + 41. / 255.;
        assert!((interior(&dark, 0.2) - fitted).abs() < 4. / 255.);
        let light = regular(Appearance::Light, false);
        assert_eq!((light.gain, light.blur_pt), (0.142, 6.));
        assert!((interior(&light, 0.) - 212. / 255.).abs() < 1e-6);
        for (appearance, contrast, recipe) in REGULAR_RECIPES {
            assert_eq!(regular(appearance, contrast), recipe);
            assert_eq!(
                recipe.border, contrast,
                "only Increase contrast gets a border"
            );
        }
    }

    #[test]
    fn increase_contrast_lets_less_of_the_backdrop_through() {
        for appearance in [Appearance::Light, Appearance::Dark] {
            let normal = regular(appearance, false);
            let heavy = regular(appearance, true);
            assert!(heavy.gain < normal.gain);
            let spread = |r: &Recipe| interior(r, 1.) - interior(r, 0.);
            assert!(spread(&heavy) < spread(&normal));
        }
    }

    #[test]
    fn interior_stays_in_range_for_every_backdrop() {
        for (_, _, recipe) in REGULAR_RECIPES {
            for k in 0..=255u32 {
                let value = interior(&recipe, k as f32 / 255.);
                assert!((0. ..=1.).contains(&value));
            }
            for backdrop in [
                [1., 0., 0.],
                [0., 1., 0.],
                [0., 0., 1.],
                [1., 1., 0.],
                [0., 1., 1.],
            ] {
                for c in shade(&recipe, backdrop) {
                    assert!((0. ..=1.).contains(&c));
                }
            }
        }
    }

    #[test]
    fn shade_leaves_a_grey_where_interior_puts_it() {
        for (_, _, recipe) in REGULAR_RECIPES {
            for k in [0u32, 64, 128, 200, 255] {
                let b = k as f32 / 255.;
                for c in shade(&recipe, [b, b, b]) {
                    assert!((c - interior(&recipe, b)).abs() < 1e-5);
                }
            }
        }
    }

    #[test]
    fn saturation_pushes_colours_apart_around_their_grey() {
        let recipe = regular(Appearance::Dark, false);
        let [r, g, b] = shade(&recipe, [1., 0., 0.]);
        assert!(r > g && (g - b).abs() < 1e-6);
    }
}
