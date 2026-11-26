//! Glass effects in the UI (docs/decisions/glass-effects.md). The look every
//! window draws is one app-wide value, `quill::glass::GlassLook`, kept in a
//! GPUI global and written only here: [`set_choice`] (the Appearance pref
//! and the theme's high contrast), [`set_system`] (the Reduce Transparency
//! and Increase Contrast probe) and [`refresh`] (power saving, the battery,
//! heat and reduced motion, called from the motion funnel) each recompute
//! it with `quill::glass::resolve` and redraw the windows when it changed.
//!
//! Surfaces read it through [`lens`] (or `GlassSurface`): `None` under the
//! solid look, so with the switch off no lens is ever painted and no
//! pipeline is built.
#![deny(clippy::indexing_slicing)]

pub(crate) mod probe;
pub(crate) mod recipes;
mod settings_row;
pub(crate) mod surface;

use gpui_kit::component::popover_glass::{PopoverGlass, PopoverGlassSurface};
use gpui_kit::component::theme::ActiveTheme;
use gpui_kit::*;
use quill::glass::{GlassLook, GlassOverride, Material, Readings, SolidReason, resolve};

pub(crate) use recipes::GlassRole;
pub(crate) use surface::{GlassFill, GlassSurface, glass_button, media_glass_button};

/// Everything the look is made of, and the look.
#[derive(Clone, Copy, Debug, PartialEq)]
struct GlassState {
    material: Material,
    high_contrast_theme: bool,
    system: probe::SystemGlass,
    /// Whether the GPU is a software rasterizer, once a window has said.
    software_adapter: Option<bool>,
    look: GlassLook,
}

impl Global for GlassState {}

impl GlassState {
    fn resolved(&self) -> GlassLook {
        let readings = Readings {
            reduce_transparency: self.system.reduce_transparency,
            increase_contrast: self.system.increase_contrast,
            high_contrast_theme: self.high_contrast_theme,
            saving_power: quill::battery_saving::forcing(),
            software_adapter: self.software_adapter.unwrap_or(false),
            reduce_motion: quill::power_saving::reduce_motion_now(),
        };
        resolve(self.material, readings, GlassOverride::from_env())
    }
}

/// The look every window draws now.
pub(crate) fn look(cx: &App) -> GlassLook {
    cx.try_global::<GlassState>()
        .map_or(GlassLook::Solid(SolidReason::Off), |state| state.look)
}

/// The glass `role` draws now, or `None` under the solid look.
pub(crate) fn lens(role: GlassRole, cx: &App) -> Option<LensStyle> {
    look(cx)
        .is_glass()
        .then(|| recipes::recipe(role, &cx.theme().colors))
}

/// The kit's popovers and toasts ask through this.
fn kit_glass(surface: PopoverGlassSurface, cx: &App) -> Option<LensStyle> {
    match surface {
        PopoverGlassSurface::Popover => lens(GlassRole::Menu, cx),
        PopoverGlassSurface::Toast => lens(GlassRole::Panel, cx),
        PopoverGlassSurface::Highlight => lens(GlassRole::Highlight, cx),
        PopoverGlassSurface::TabSelection { travel } => {
            lens(GlassRole::Selection, cx).map(|style| recipes::sliding(style, travel))
        }
    }
}

/// The single writer: change the inputs, resolve again, and redraw every
/// window when the look changed.
fn update(cx: &mut App, change: impl FnOnce(&mut GlassState)) {
    if !cx.has_global::<PopoverGlass>() {
        cx.set_global(PopoverGlass(kit_glass));
    }
    let before = cx.try_global::<GlassState>().copied();
    let mut state = before.unwrap_or(GlassState {
        material: Material::Solid,
        high_contrast_theme: false,
        system: probe::SystemGlass::default(),
        software_adapter: None,
        look: GlassLook::Solid(SolidReason::Off),
    });
    change(&mut state);
    if state.software_adapter.is_none() && state.material == Material::Glass {
        state.software_adapter = read_software_adapter(cx);
    }
    state.look = state.resolved();
    if before == Some(state) {
        return;
    }
    let redraw = before.is_none_or(|before| before.look != state.look);
    cx.set_global(state);
    if redraw {
        cx.refresh_windows();
    }
}

/// Whether the first window that answers renders on a software adapter.
/// `None` while no window can say (none open yet, or the one being drawn).
fn read_software_adapter(cx: &mut App) -> Option<bool> {
    let windows = cx.windows();
    windows.into_iter().find_map(|handle| {
        handle
            .update(cx, |_, window, _| window.gpu_specs())
            .ok()
            .map(|specs| specs.is_some_and(|specs| specs.is_software_emulated))
    })
}

/// The Appearance pref and the theme's high contrast
/// (`QuillApp::apply_appearance`).
pub(crate) fn set_choice(material: Material, high_contrast_theme: bool, cx: &mut App) {
    update(cx, |state| {
        state.material = material;
        state.high_contrast_theme = high_contrast_theme;
    });
}

/// The probe's answer.
pub(crate) fn set_system(system: probe::SystemGlass, cx: &mut App) {
    update(cx, |state| state.system = system);
}

/// Power saving, the battery, heat or reduced motion may have changed.
pub(crate) fn refresh(cx: &mut App) {
    if cx.has_global::<GlassState>() {
        update(cx, |_| {});
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "scene_tests.rs"]
mod scene_tests;
