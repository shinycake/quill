//! The bars above and below a chat's history (docs/decisions/floating-layout.md).
//! Classic stacks them, each with its own hairline. Floating gathers them
//! into two cards: the header and every bar over the history in one, the
//! composer and the strips over it in the other. Inside a card a bar
//! drops its line and its fill and takes the card's; an inset hairline
//! parts it from its neighbour.

use super::frames::pill;
use super::radius::card_in;
use crate::ui::app::QuillApp;
use gpui_kit::component::theme::{ActiveTheme, Theme};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::layout_style::{Chrome, ChromeMetrics, Look};

/// Where Classic draws a bar's hairline: under it (a bar over the history)
/// or over it (a bar under the history).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) enum BarLine {
    Under,
    Over,
}

fn line<E: Styled>(this: E, line: BarLine, theme: &Theme) -> E {
    match line {
        BarLine::Under => this.border_b_1().border_color(theme.border),
        BarLine::Over => this.border_t_1().border_color(theme.border),
    }
}

/// A bar with a plain fill: Classic's line, or a row of its card that
/// takes the card's fill. Call it after the bar's own `bg`.
pub(in crate::ui) fn bar<E: Styled>(
    this: E,
    at: BarLine,
    metrics: &ChromeMetrics,
    theme: &Theme,
) -> E {
    match metrics.look {
        Look::Lines => line(this, at, theme),
        Look::Cards => this.bg(transparent_black()),
    }
}

/// How far a tinted bar sits in from its card's edge.
const TINT_INSET: f32 = 6.;

/// A bar that keeps its own colour (a call, a notice): Classic's line, or
/// a rounded tile inset in its card, concentric with the card's corners,
/// so its colour never reaches past the card's round ends.
pub(in crate::ui) fn tinted_bar<E: Styled>(
    this: E,
    at: BarLine,
    metrics: &ChromeMetrics,
    theme: &Theme,
) -> E {
    match metrics.look {
        Look::Lines => line(this, at, theme),
        Look::Cards => tile(this, metrics, theme),
    }
}

/// A coloured bar with no line in Classic (the call bars): as it is, or
/// the rounded tile in its card.
pub(in crate::ui) fn tinted_tile<E: Styled>(this: E, metrics: &ChromeMetrics, theme: &Theme) -> E {
    match metrics.look {
        Look::Lines => this,
        Look::Cards => tile(this, metrics, theme),
    }
}

fn tile<E: Styled>(this: E, metrics: &ChromeMetrics, theme: &Theme) -> E {
    this.m(px(TINT_INSET))
        .rounded((card_in(metrics, theme) - px(TINT_INSET)).max(px(0.)))
}

/// The hairline between two rows of a card, inset by the card's padding.
pub(in crate::ui) fn card_divider(inset: Pixels, theme: &Theme) -> Div {
    div()
        .flex_none()
        .h(px(1.))
        .mx(inset)
        .bg(theme.border.opacity(0.5))
}

/// A strip over the composer (the bottom tab strip, the busy banner):
/// Classic's line, or a row of the composer's card with the hairline
/// under it, towards the composer.
pub(in crate::ui) fn strip<E: Styled + ParentElement + FluentBuilder>(
    this: E,
    at: BarLine,
    inset: Pixels,
    metrics: &ChromeMetrics,
    theme: &Theme,
) -> E {
    match metrics.look {
        Look::Lines => line(this, at, theme),
        Look::Cards => this.relative().child(
            card_divider(inset, theme)
                .absolute()
                .bottom_0()
                .left_0()
                .right_0(),
        ),
    }
}

/// The card the bars gather in: a pill's fill, edge and shadow, or its
/// glass. Classic
/// has none, so it returns `this`.
pub(in crate::ui) fn stack_card<E: Styled + FluentBuilder>(
    this: E,
    chrome: &Chrome,
    theme: &Theme,
    glass: Option<&LensStyle>,
) -> E {
    match chrome.metrics.look {
        Look::Lines => this,
        Look::Cards => pill(this, chrome, theme, glass),
    }
}

impl QuillApp {
    /// [`bar`] in this window.
    pub(in crate::ui) fn bar_frame<E: Styled>(&self, this: E, at: BarLine, cx: &App) -> E {
        bar(this, at, self.chrome_metrics(), cx.theme())
    }

    /// [`tinted_bar`] in this window.
    pub(in crate::ui) fn tinted_bar_frame<E: Styled>(&self, this: E, at: BarLine, cx: &App) -> E {
        tinted_bar(this, at, self.chrome_metrics(), cx.theme())
    }

    /// [`tinted_tile`] in this window.
    pub(in crate::ui) fn tinted_tile_frame<E: Styled>(&self, this: E, cx: &App) -> E {
        tinted_tile(this, self.chrome_metrics(), cx.theme())
    }

    /// [`strip`] in this window.
    pub(in crate::ui) fn strip_frame<E: Styled + ParentElement + FluentBuilder>(
        &self,
        this: E,
        at: BarLine,
        cx: &App,
    ) -> E {
        let inset = px(self.density().header_inset_x);
        strip(this, at, inset, self.chrome_metrics(), cx.theme())
    }

    /// [`stack_card`] in this window.
    pub(in crate::ui) fn stack_card<E: Styled + FluentBuilder>(&self, this: E, cx: &App) -> E {
        stack_card(
            this,
            &self.chrome(cx),
            cx.theme(),
            self.card_glass(cx).as_ref(),
        )
    }
}
