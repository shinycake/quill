//! The window layout in the UI (docs/decisions/floating-layout.md): the
//! chrome a window draws this frame, read through [`QuillApp::chrome`], and
//! the helpers render sites pass their elements through. Render sites never
//! ask which layout is on (`scripts/check-layout-style.py`).
#![deny(clippy::indexing_slicing)]

pub(in crate::ui) mod bars;
pub(in crate::ui) mod frame;
pub(in crate::ui) mod frames;
pub(in crate::ui) mod list_parts;
pub(in crate::ui) mod menu_pop;
pub(in crate::ui) mod menu_shape;
pub(in crate::ui) mod miniature;
pub(in crate::ui) mod radius;

use crate::ui::app::QuillApp;
use gpui_kit::component::theme::ActiveTheme;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::layout_style::{
    BackdropKind, Chrome, ChromeMetrics, Contrast, FLOATING_FROM, LayoutOverride, LayoutStyle,
    Power, Readings, TitleStrip, contrast_ratio, metrics_for, resolve,
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

    /// The window's title strip: lined in Classic, bare in Floating.
    pub(in crate::ui) fn title_strip(&self) -> TitleStrip {
        self.chrome_metrics().title
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
            pane: f32::from(self.history.measure.get().pane()),
            left_reserved: 0.,
            gutter: metrics.gutter,
            bar_gap: metrics.bar_gap,
            chat_column: metrics.chat_column.unwrap_or(0.),
            top: f32::from(top),
            composer: f32::from(composer),
        });
        frame.insets
    }

    /// The frame a body that is not the list sits in, or `None` when the
    /// bars stack (Classic), which leaves the body as it is.
    pub(in crate::ui) fn pane_frame(&self) -> Option<frame::PaneFrame> {
        if !self.bars_float() {
            return None;
        }
        let insets = self.history_insets();
        let metrics = self.chrome_metrics();
        Some(frame::PaneFrame {
            top: insets.top,
            bottom: insets.bottom,
            gutter: px(metrics.gutter),
            column: metrics.chat_column.map(px),
        })
    }

    /// How the history's rows are placed: across the pane, or in the
    /// centred column when the bars float.
    pub(in crate::ui) fn row_column(&self) -> frame::RowColumn {
        match (self.bars_float(), self.chrome_metrics().chat_column) {
            (true, Some(column)) => frame::RowColumn::Centred {
                gutter: self.chrome_metrics().gutter,
                column,
            },
            _ => frame::RowColumn::Pane,
        }
    }

    /// A chat's left tab column's frame (`frames::tab_column_frame`).
    pub(in crate::ui) fn tab_column_frame<E: Styled + FluentBuilder>(
        &self,
        this: E,
        cx: &App,
    ) -> E {
        frames::tab_column_frame(
            this,
            &self.chrome(cx),
            cx.theme(),
            self.card_glass(cx).as_ref(),
        )
    }

    /// The glass the Floating cards draw while Glass effects is on and
    /// resolves to glass (the `Panel` recipe), else `None`: the opaque
    /// card. Classic has no cards, so never any.
    pub(in crate::ui) fn card_glass(&self, cx: &App) -> Option<LensStyle> {
        match self.chrome_metrics().clip {
            quill::layout_style::Clip::Square => None,
            quill::layout_style::Clip::Rounded => {
                super::glass::lens(super::glass::GlassRole::Panel, cx)
            }
        }
    }

    /// A chat row's resting fill: the list's own, or none on a glass card,
    /// where an opaque row would hide the glass.
    pub(in crate::ui) fn list_row_fill(&self, cx: &App) -> Hsla {
        match self.card_glass(cx) {
            Some(_) => transparent_black(),
            None => cx.theme().sidebar,
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
        menu_shape::note_drawn_window(self.chrome_metrics(), cx);
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
