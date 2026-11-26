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
use quill::layout_style::{ChromeMetrics, Look};

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

/// Extra room under the pinned bar at the foot of Floating's header card.
const PINNED_CARD_FOOT: f32 = 4.;

/// How far a tinted bar sits in from its card's edge.
const TINT_INSET: f32 = 6.;

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

impl QuillApp {
    /// A bar with a plain fill: Classic's line, or a row of its card that
    /// takes the card's fill. Call it after the bar's own `bg`.
    pub(in crate::ui) fn bar_frame<E: Styled>(&self, this: E, at: BarLine, cx: &App) -> E {
        match self.look() {
            Look::Lines => line(this, at, cx.theme()),
            Look::Cards => this.bg(transparent_black()),
        }
    }

    /// A bar that keeps its own colour (a call, a notice): Classic's line,
    /// or a rounded tile inset in its card, concentric with the card's
    /// corners, so its colour never reaches past the card's round ends.
    pub(in crate::ui) fn tinted_bar_frame<E: Styled>(&self, this: E, at: BarLine, cx: &App) -> E {
        let metrics = self.chrome_metrics();
        match metrics.look {
            Look::Lines => line(this, at, cx.theme()),
            Look::Cards => tile(this, metrics, cx.theme()),
        }
    }

    /// A coloured bar with no line in Classic (the call bars): as it is,
    /// or the rounded tile in its card.
    pub(in crate::ui) fn tinted_tile_frame<E: Styled>(&self, this: E, cx: &App) -> E {
        let metrics = self.chrome_metrics();
        match metrics.look {
            Look::Lines => this,
            Look::Cards => tile(this, metrics, cx.theme()),
        }
    }

    /// A strip over the composer (the bottom tab strip, the busy banner):
    /// Classic's line, or a row of the composer's card with the hairline
    /// under it, towards the composer.
    pub(in crate::ui) fn strip_frame<E: Styled + ParentElement + FluentBuilder>(
        &self,
        this: E,
        at: BarLine,
        cx: &App,
    ) -> E {
        match self.look() {
            Look::Lines => line(this, at, cx.theme()),
            Look::Cards => {
                let inset = px(self.density().header_inset_x);
                this.relative().child(
                    card_divider(inset, cx.theme())
                        .absolute()
                        .bottom_0()
                        .left_0()
                        .right_0(),
                )
            }
        }
    }

    /// The pinned bar, the header card's last row: in Floating its stripe
    /// starts where the card's hairline does, and it keeps clear of the
    /// card's round lower corners. As it is in Classic.
    pub(in crate::ui) fn pinned_bar_frame<E: Styled>(&self, this: E) -> E {
        let m = self.density();
        match self.look() {
            Look::Lines => this,
            Look::Cards => this
                .pl(px(m.header_inset_x))
                .pb(px(m.pinned_inset_y + PINNED_CARD_FOOT)),
        }
    }

    /// The card the bars gather in: a pill's fill, edge and shadow, or its
    /// glass. Classic has none, so it returns `this`.
    pub(in crate::ui) fn stack_card<E: Styled + FluentBuilder>(&self, this: E, cx: &App) -> E {
        let chrome = self.chrome();
        match chrome.metrics.look {
            Look::Lines => this,
            Look::Cards => pill(this, &chrome, cx.theme(), self.card_glass(cx).as_ref()),
        }
    }
}
