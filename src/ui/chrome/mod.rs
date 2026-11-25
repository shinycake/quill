//! The window layout in the UI (docs/decisions/floating-layout.md): the
//! chrome a window draws this frame, read through [`QuillApp::chrome`], and
//! the helpers render sites pass their elements through. Render sites never
//! ask which layout is on (`scripts/check-layout-style.py`).
#![deny(clippy::indexing_slicing)]

pub(in crate::ui) mod frame;
pub(in crate::ui) mod frames;
pub(in crate::ui) mod miniature;
pub(in crate::ui) mod radius;

use crate::ui::app::QuillApp;
use gpui_kit::component::theme::ActiveTheme;
use gpui_kit::prelude::FluentBuilder;
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

    /// The window is in the drawer band (Floating, 601 to 925 wide).
    pub(in crate::ui) fn in_drawer_band(&self) -> bool {
        let width = f32::from(self.slices.window.width.get());
        self.chrome_metrics()
            .drawer_to
            .is_some_and(|to| width <= to)
    }

    /// The wallpaper is the window's, under the cards (Floating), not the
    /// history's.
    pub(in crate::ui) fn window_backdrop(&self, cx: &App) -> bool {
        self.chrome(cx).metrics.backdrop == quill::layout_style::Backdrop::Window
    }

    /// The chat header's frame in this window (`frames::header_frame`).
    pub(in crate::ui) fn header_frame<E: Styled + FluentBuilder>(&self, this: E, cx: &App) -> E {
        frames::header_frame(this, &self.chrome(cx), cx.theme())
    }

    /// The composer's frame in this window (`frames::composer_frame`).
    pub(in crate::ui) fn composer_frame<E: Styled + FluentBuilder>(&self, this: E, cx: &App) -> E {
        frames::composer_frame(this, &self.chrome(cx), cx.theme())
    }

    /// How far the history's rows keep clear of the pills over them: none
    /// while the bars are stacked.
    pub(in crate::ui) fn history_insets(&self) -> frame::HistoryInsets {
        if !self.bars_float() {
            return frame::HistoryInsets::ZERO;
        }
        let metrics = self.chrome_metrics();
        let (top, composer) = self.slices.window.pill_heights();
        let frame = frame::floating_frame(frame::FrameInput {
            pane: 0.,
            left_reserved: 0.,
            gutter: metrics.gutter,
            bar_gap: metrics.bar_gap,
            chat_column: metrics.chat_column.unwrap_or(0.),
            top: f32::from(top),
            composer: f32::from(composer),
        });
        frame::HistoryInsets {
            left: px(0.),
            right: px(0.),
            ..frame.insets
        }
    }

    /// The header and the bars float over the history in their own slice.
    pub(in crate::ui) fn bars_float(&self) -> bool {
        self.chrome_metrics().placement == quill::layout_style::Placement::Overlaid
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
        let chat = self.viewed_chat().map(|chat| chat.0);
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

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "cards_tests.rs"]
mod cards_tests;
