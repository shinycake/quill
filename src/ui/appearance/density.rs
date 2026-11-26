//! Display density in the UI (Appearance → Compact mode): the sizes render
//! code reads (`QuillApp::density`, from `quill::density`), the switch, and
//! the one step every appearance change that moves rows goes through.

use super::*;
use gpui_kit::component::Size;
use quill::density::{ControlSize, Density, DensityMetrics};

/// The appearance settings that decide how tall rows are. When one changes,
/// the open lists are laid out again in place
/// ([`QuillApp::appearance_layout_changed`]).
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct RowLayout {
    density: Density,
    preview_lines: u8,
    bubbles: bool,
    adaptive_for_wide: bool,
    layout: quill::layout_style::LayoutStyle,
}

impl RowLayout {
    pub(super) fn of(prefs: &AppearancePrefs) -> Self {
        Self {
            density: prefs.density,
            preview_lines: prefs.preview_lines,
            bubbles: prefs.bubbles,
            adaptive_for_wide: prefs.adaptive_for_wide,
            layout: prefs.layout,
        }
    }
}

impl QuillApp {
    /// The sizes the chosen density draws with (Quill's row scale; the
    /// chat list's rows read [`Self::row_density`]).
    pub(in crate::ui) fn density(&self) -> &'static DensityMetrics {
        self.appearance.density.metrics()
    }

    /// The chat list's row sizes in this window's layout: Web K's taller
    /// rows, larger photos and search field in Floating
    /// (`ChromeMetrics::rows`). Only these differ between the scales.
    pub(in crate::ui) fn row_density(&self) -> &'static DensityMetrics {
        self.appearance
            .density
            .metrics_in(self.chrome_metrics().rows)
    }

    /// The kit size of the density's controls (`DensityMetrics::control`).
    pub(in crate::ui) fn control_size(&self) -> Size {
        match self.density().control {
            ControlSize::Medium => Size::Medium,
            ControlSize::Small => Size::Small,
        }
    }

    /// The kit draws every window's title bar (and keeps dialogs, sheets and
    /// notifications clear of it) at the density's height.
    pub(super) fn apply_title_bar_height(&self, cx: &mut App) {
        let height = px(self.density().title_bar);
        if cx
            .try_global::<Theme>()
            .is_some_and(|theme| theme.title_bar_height != height)
        {
            Theme::global_mut(cx).set_title_bar_height(height);
        }
    }

    /// A [`RowLayout`] setting changed: lay the open lists out again where
    /// they are. The chat list keeps its top row (it scrolls by pixels), each
    /// window's history measures its rows again around its anchor, and what
    /// was placed in the old sizes (a pinned-row drag, the tab strips'
    /// scroll to the selected tab) starts over.
    pub(super) fn appearance_layout_changed(&mut self, cx: &mut Context<Self>) {
        self.chat_list.pin.end_drag();
        self.keep_chat_list_top();
        self.folders.tabs_revealed.set(None);
        for window in self.every_window() {
            self.in_window_scope(window, cx, |app, cx| {
                app.chat_ui.tabs_revealed.set(None);
                app.history
                    .scroller
                    .update(cx, |state, cx| state.remeasure(cx));
            });
        }
        cx.refresh_windows();
    }

    pub(in crate::ui) fn appearance_compact_mode_section(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let label = tr(keys::Q_PREFS_COMPACT_MODE);
        let control = Switch::new("appearance-compact-mode")
            .checked(self.appearance.density == Density::Compact)
            .accessibility_label(label)
            .on_click(cx.listener(|this, &on, _, cx| {
                let density = if on {
                    Density::Compact
                } else {
                    Density::Standard
                };
                this.set_appearance(cx, |appearance| appearance.density = density);
            }));
        let row = self.appearance_toggle_section(
            cx,
            label,
            tr(keys::Q_PREFS_COMPACT_MODE_ABOUT),
            control.into_any_element(),
        );
        self.setting_spot(&["appearance.compact_mode"], row, cx)
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "density_tests.rs"]
mod tests;
