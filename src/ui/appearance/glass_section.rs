//! "Glass effects": the switch for liquid glass behind menus (macOS).
//! While Reduce transparency is on, the system overrides it: the switch
//! reads off, cannot be changed, and a note says why.
#![deny(clippy::indexing_slicing)]

use super::QuillApp;
use gpui_kit::component::Disableable;
use gpui_kit::component::switch::Switch;
use gpui_kit::*;
use quill::glass::{self, SwitchState};
use quill::lang::{keys, tr};

impl QuillApp {
    /// The row, or `None` where the platform has no glass. `supported` is a
    /// parameter so tests can show the row on any platform.
    pub(in crate::ui) fn glass_section(
        &self,
        supported: bool,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        supported.then(|| {
            let (checked, locked) = match glass::switch_state(
                self.appearance.glass_effects,
                glass::os_transparency(),
            ) {
                SwitchState::Free { on } => (on, false),
                SwitchState::OverriddenBySystem => (false, true),
            };
            let (selector, hint) = if locked {
                ("glass-effects-note", tr(keys::Q_PREFS_GLASS_EFFECTS_SYSTEM))
            } else {
                ("glass-effects-hint", tr(keys::Q_PREFS_GLASS_EFFECTS_HINT))
            };
            let switch = Switch::new("appearance-glass-effects")
                .checked(checked)
                .disabled(locked)
                .accessibility_label(tr(keys::Q_PREFS_GLASS_EFFECTS))
                .on_click(cx.listener(|this, &on, _, cx| {
                    this.set_appearance(cx, |a| a.glass_effects = on);
                }));
            let control = div()
                .flex()
                .child(
                    div()
                        .debug_selector(|| "glass-effects-switch".into())
                        .child(switch),
                )
                .into_any_element();
            div()
                .debug_selector(move || selector.into())
                .child(self.appearance_section(cx, tr(keys::Q_PREFS_GLASS_EFFECTS), hint, control))
                .into_any_element()
        })
    }
}
