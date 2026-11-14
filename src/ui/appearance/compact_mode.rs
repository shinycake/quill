//! The Compact mode switch in Appearance.

use super::*;

impl QuillApp {
    fn set_compact_mode(&mut self, compact: bool, cx: &mut Context<Self>) {
        if compact == self.appearance.compact_mode {
            return;
        }
        self.set_appearance(cx, |appearance| appearance.compact_mode = compact);
        self.refresh_compact_layout(cx);
    }

    pub(in crate::ui) fn refresh_compact_layout(&mut self, cx: &mut Context<Self>) {
        self.chat_list.pin.end_drag();
        self.folders.tabs_revealed.set(None);
        // The virtual lists cache measured row heights in their window slots.
        self.in_every_window(cx, |app, cx| {
            app.history
                .scroller
                .update(cx, |state, cx| state.remeasure(cx));
        });
        cx.refresh_windows();
    }

    pub(in crate::ui) fn appearance_compact_mode_section(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let label = tr(keys::Q_PREFS_COMPACT_MODE);
        let control = Switch::new("appearance-compact-mode")
            .checked(self.appearance.compact_mode)
            .accessibility_label(label)
            .on_click(cx.listener(|this, &on, _, cx| this.set_compact_mode(on, cx)));
        let row = self.appearance_toggle_section(
            cx,
            label,
            tr(keys::Q_PREFS_COMPACT_MODE_ABOUT),
            control.into_any_element(),
        );
        self.setting_spot(&["appearance.compact_mode"], row, cx)
    }
}
