//! The frames of the chat list column, the side panels, the chat header
//! and the composer: Classic's lines, or Floating's cards and pills
//! (docs/decisions/floating-layout.md).
//! Every call site passes its element through here, so the two layouts
//! cannot drift apart at one site.
//!
//! Rounded clipping of a card's content waits for the renderer's
//! `clip_rounded` (plan step 1). Until then a card clips to its rectangle
//! and the surface inside it rounds its own fill: content that paints to
//! the card's edge (a hovered row at the bottom corner, a profile cover)
//! still shows square corners. The seam is [`card_clip`].

use super::radius::card_in;
use gpui_kit::component::theme::Theme;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::layout_style::{Chrome, Clip, Edge, Elevation, Placement};

/// The chat list column's own frame, inside its slot: Classic's right
/// border, or a card's rounded fill.
pub(in crate::ui) fn list_frame<E: Styled + FluentBuilder>(
    this: E,
    chrome: &Chrome,
    theme: &Theme,
) -> E {
    match chrome.metrics.clip {
        Clip::Square => this
            .border_r_1()
            .border_color(theme.border)
            .bg(theme.sidebar),
        Clip::Rounded => surface(this, chrome, theme),
    }
}

/// A side panel's frame: Classic's left border, or a card's rounded fill.
pub(in crate::ui) fn panel_frame<E: Styled + FluentBuilder>(
    this: E,
    chrome: &Chrome,
    theme: &Theme,
) -> E {
    match chrome.metrics.clip {
        Clip::Square => this
            .border_l_1()
            .border_color(theme.border)
            .bg(theme.sidebar),
        Clip::Rounded => surface(this, chrome, theme),
    }
}

/// The chat header's frame: Classic's line under it, or a pill.
pub(in crate::ui) fn header_frame<E: Styled + FluentBuilder>(
    this: E,
    chrome: &Chrome,
    theme: &Theme,
) -> E {
    match chrome.metrics.placement {
        Placement::Stacked => this.border_b_1().border_color(theme.border),
        Placement::Overlaid => pill(this, chrome, theme),
    }
}

/// The composer's frame: Classic's line over it, or a pill.
pub(in crate::ui) fn composer_frame<E: Styled + FluentBuilder>(
    this: E,
    chrome: &Chrome,
    theme: &Theme,
) -> E {
    match chrome.metrics.placement {
        Placement::Stacked => this.border_t_1().border_color(theme.border),
        Placement::Overlaid => pill(this, chrome, theme),
    }
}

/// A pill over the history: a card's fill, edge and shadow. It does not
/// clip, as the composer's popups hang outside it.
fn pill<E: Styled + FluentBuilder>(this: E, chrome: &Chrome, theme: &Theme) -> E {
    surface(this, chrome, theme).when(chrome.elevation == Elevation::Shadowed, |this| {
        this.shadow(card_shadow(theme))
    })
}

/// A card's fill and edge, rounded.
fn surface<E: Styled + FluentBuilder>(this: E, chrome: &Chrome, theme: &Theme) -> E {
    this.bg(theme.sidebar)
        .rounded(card_in(chrome.metrics, theme))
        .when(chrome.edge == Edge::Hairline, |this| {
            this.border_1().border_color(theme.border)
        })
}

/// The card around a slot's content: its shadow, its corners and its
/// clip. Classic has no card, so it returns `this` unchanged.
pub(in crate::ui) fn card<E: Styled + FluentBuilder>(this: E, chrome: &Chrome, theme: &Theme) -> E {
    match chrome.metrics.clip {
        Clip::Square => this,
        Clip::Rounded => card_clip(
            this.rounded(card_in(chrome.metrics, theme))
                .when(chrome.elevation == Elevation::Shadowed, |this| {
                    this.shadow(card_shadow(theme))
                }),
        ),
    }
}

/// The seam for the renderer's rounded clip: today a rectangular clip.
fn card_clip<E: Styled>(this: E) -> E {
    this.overflow_hidden()
}

/// A card's shadow over the wallpaper: a soft, close one and a wider,
/// fainter one (Web K's `--menu-box-shadow` family, in the theme's ink).
pub(in crate::ui) fn card_shadow(theme: &Theme) -> Vec<BoxShadow> {
    let ink = if theme.is_dark() { 0.32 } else { 0.12 };
    vec![
        BoxShadow {
            color: hsla(0., 0., 0., ink),
            offset: point(px(0.), px(1.)),
            blur_radius: px(2.),
            spread_radius: px(0.),
            inset: false,
        },
        BoxShadow {
            color: hsla(0., 0., 0., ink * 0.6),
            offset: point(px(0.), px(4.)),
            blur_radius: px(16.),
            spread_radius: px(0.),
            inset: false,
        },
    ]
}

/// The surface the hidden column peeks in on: kit's sheet surface (the
/// sidebar fill with the overlay shadow), or nothing of its own where the
/// slot inside draws the list card.
pub(in crate::ui) fn peek_surface<E: Styled + FluentBuilder>(
    this: E,
    chrome: &Chrome,
    theme: &Theme,
) -> E {
    match chrome.metrics.clip {
        Clip::Square => this.bg(theme.sidebar).shadow_xl(),
        Clip::Rounded => this,
    }
}
