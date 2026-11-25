//! The window layout in the UI (docs/decisions/floating-layout.md): the
//! chrome a window draws this frame, read through [`QuillApp::chrome`], and
//! the helpers render sites pass their elements through. Render sites never
//! ask which layout is on (`scripts/check-layout-style.py`).
#![deny(clippy::indexing_slicing)]

pub(in crate::ui) mod miniature;
pub(in crate::ui) mod radius;

use crate::ui::app::QuillApp;
use gpui_kit::component::theme::ActiveTheme;
use gpui_kit::*;
use quill::layout_style::{
    BackdropKind, Chrome, ChromeMetrics, Contrast, FLOATING_FROM, LayoutOverride, LayoutStyle,
    Power, Readings, contrast_ratio, metrics_for, resolve,
};

/// WCAG 2.1 relative luminance of a colour.
pub(in crate::ui) fn luminance(color: Hsla) -> f32 {
    let rgb = Rgba::from(color);
    let channel = |c: f32| {
        let c = c.clamp(0., 1.);
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(rgb.r) + 0.7152 * channel(rgb.g) + 0.0722 * channel(rgb.b)
}

impl QuillApp {
    /// The layout in force: the `QUILL_DEMO_LAYOUT` override when set, else
    /// the Appearance choice. The Settings card and the onboarding card
    /// show this one, so a demo run with the override shows it selected.
    pub(in crate::ui) fn layout_choice(&self) -> LayoutStyle {
        quill::layout_style::choice(self.appearance.layout, LayoutOverride::from_env())
    }

    /// The metrics of the window whose state is in place, which need no
    /// theme: the shell's columns read them.
    pub(in crate::ui) fn chrome_metrics(&self) -> &'static ChromeMetrics {
        metrics_for(
            self.appearance.layout,
            LayoutOverride::from_env(),
            f32::from(self.slices.window.width.get()),
            FLOATING_FROM,
        )
    }

    /// What the window whose state is in place draws this frame.
    pub(in crate::ui) fn chrome(&self, cx: &App) -> Chrome {
        resolve(
            self.appearance.layout,
            LayoutOverride::from_env(),
            self.chrome_readings(cx),
        )
    }

    fn chrome_readings(&self, cx: &App) -> Readings {
        let theme = cx.theme();
        let backdrop = match self.window_wallpaper(cx) {
            Some(_) => BackdropKind::Picture,
            None => BackdropKind::Plain {
                card_contrast: contrast_ratio(
                    luminance(theme.sidebar),
                    luminance(theme.background),
                ),
            },
        };
        Readings {
            width: f32::from(self.slices.window.width.get()),
            floating_from: FLOATING_FROM,
            contrast: if super::chat_theme::high_contrast() {
                Contrast::High
            } else {
                Contrast::Normal
            },
            power: if quill::battery_saving::forcing() {
                Power::Saver
            } else {
                Power::Normal
            },
            backdrop,
        }
    }

    /// The wallpaper behind the window's chat: the open chat's own, else
    /// the account's.
    fn window_wallpaper(&self, cx: &App) -> Option<super::wallpaper::Wallpaper> {
        let chat = self.open_chat_id().map(|chat| chat.0);
        self.chat_look(chat, cx).wallpaper
    }

    /// A window's render measured its width: store it, and when the chrome
    /// changed with it (a resize across 600 px), render the cached slices
    /// again, as their own bounds may not have changed.
    pub(in crate::ui) fn note_window_width(&self, width: Pixels, cx: &mut Context<Self>) {
        let before = self.chrome(cx);
        self.slices.window.width.set(width);
        if self.chrome(cx) != before {
            self.notify_slices(cx);
        }
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "chrome_tests.rs"]
mod tests;
