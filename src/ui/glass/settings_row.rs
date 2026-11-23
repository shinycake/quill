//! Settings › Appearance › Glass effects: the switch, tagged Experimental,
//! and a caption that says why glass is paused when the system holds it
//! back.
#![deny(clippy::indexing_slicing)]

use crate::ui::QuillApp;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::tag::Tag;
use gpui_kit::component::theme::ActiveTheme;
use gpui_kit::component::{Sizable as _, h_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use quill::glass::{GlassLook, Material, SolidReason};
use quill::lang::{Key, keys, tr};

/// The caption under the switch for a look.
pub(crate) fn caption(look: GlassLook) -> Key {
    match look {
        GlassLook::Solid(SolidReason::ReduceTransparency) => {
            if cfg!(windows) {
                keys::Q_PREFS_GLASS_PAUSED_TRANSPARENCY_WINDOWS
            } else {
                keys::Q_PREFS_GLASS_PAUSED_TRANSPARENCY
            }
        }
        GlassLook::Solid(SolidReason::HighContrast) => keys::Q_PREFS_GLASS_PAUSED_CONTRAST,
        GlassLook::Solid(SolidReason::SavingPower) => keys::Q_PREFS_GLASS_PAUSED_POWER,
        GlassLook::Solid(SolidReason::Unsupported) => keys::Q_PREFS_GLASS_UNAVAILABLE,
        GlassLook::Solid(SolidReason::Off) | GlassLook::Glass(_) => {
            keys::Q_PREFS_GLASS_EFFECTS_ABOUT
        }
    }
}

impl QuillApp {
    pub(in crate::ui) fn appearance_glass_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let label = tr(keys::Q_PREFS_GLASS_EFFECTS);
        // The caption follows the switch: a pause only explains itself
        // when glass is chosen (the QUILL_GLASS override can draw glass
        // with the switch off).
        let look = match self.appearance.material {
            Material::Glass => super::look(cx),
            Material::Solid => GlassLook::Solid(SolidReason::Off),
        };
        let control = Switch::new("appearance-glass-effects")
            .checked(self.appearance.material == Material::Glass)
            .accessibility_label(label)
            .on_click(cx.listener(|this, &on, _, cx| {
                let material = if on {
                    Material::Glass
                } else {
                    Material::Solid
                };
                this.set_appearance(cx, |appearance| appearance.material = material);
            }));
        let paused = matches!(look, GlassLook::Solid(reason) if reason != SolidReason::Off);
        let row = div()
            .flex()
            .items_center()
            .gap_4()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .gap_0p5()
                    .child(
                        h_flex()
                            .gap_2()
                            .child(div().text_sm().child(label))
                            .child(
                                Tag::secondary()
                                    .outline()
                                    .small()
                                    .child(tr(keys::Q_PREFS_GLASS_EXPERIMENTAL)),
                            ),
                    )
                    .child(
                        div()
                            .debug_selector(|| "appearance-glass-caption".into())
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .when(paused, |this| {
                                this.text_color(cx.theme().foreground)
                            })
                            .child(tr(caption(look))),
                    ),
            )
            .child(div().flex_none().child(control))
            .into_any_element();
        self.setting_spot(&["appearance.glass_effects"], row, cx)
    }
}
