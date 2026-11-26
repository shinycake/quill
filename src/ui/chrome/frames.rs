//! The frames of the chat list column, the side panels, the chat header
//! and the composer: Classic's lines, or Floating's cards and pills
//! (docs/decisions/floating-layout.md).
//! Every call site passes its element through here, so the two layouts
//! cannot drift apart at one site.
//!
//! A card clips its content to its rounded shape ([`card_clip`],
//! docs/decisions/rounded-content-masks.md), so a hovered row at the
//! bottom corner or a profile cover is cut by the card's corners.

use super::radius::card_in;
use gpui_kit::component::theme::Theme;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::layout_style::{Chrome, Clip, Edge, Elevation, Placement, TitleStrip};

/// The chat list column's own frame, inside its slot: Classic's right
/// border, or a card's rounded fill (none over glass: the slot's card
/// draws the lens, `glass`).
pub(in crate::ui) fn list_frame<E: Styled + FluentBuilder>(
    this: E,
    chrome: &Chrome,
    theme: &Theme,
    glass: Option<&LensStyle>,
) -> E {
    match chrome.metrics.clip {
        Clip::Square => this
            .border_r_1()
            .border_color(theme.border)
            .bg(theme.sidebar),
        Clip::Rounded => surface(this, chrome, theme, glass),
    }
}

/// A side panel's frame: Classic's left border, or a card's rounded fill
/// (none over glass: the panels' card draws the lens).
pub(in crate::ui) fn panel_frame<E: Styled + FluentBuilder>(
    this: E,
    chrome: &Chrome,
    theme: &Theme,
    glass: Option<&LensStyle>,
) -> E {
    match chrome.metrics.clip {
        Clip::Square => this
            .border_l_1()
            .border_color(theme.border)
            .bg(theme.sidebar),
        Clip::Rounded => surface(this, chrome, theme, glass),
    }
}

/// The chat header's frame: Classic's line under it, or nothing of its
/// own: it is the first row of the bars' card (`bars::stack_card`).
pub(in crate::ui) fn header_frame<E: Styled + FluentBuilder>(
    this: E,
    chrome: &Chrome,
    theme: &Theme,
) -> E {
    match chrome.metrics.placement {
        Placement::Stacked => this.border_b_1().border_color(theme.border),
        Placement::Overlaid => this,
    }
}

/// The composer's frame: Classic's line over it, or nothing of its own: it
/// is a row of the composer's card (`bars::stack_card`).
pub(in crate::ui) fn composer_frame<E: Styled + FluentBuilder>(
    this: E,
    chrome: &Chrome,
    theme: &Theme,
) -> E {
    match chrome.metrics.placement {
        Placement::Stacked => this.border_t_1().border_color(theme.border),
        Placement::Overlaid => this,
    }
}

/// A chat's left tab column: Classic's lined column, or a card.
pub(in crate::ui) fn tab_column_frame<E: Styled + FluentBuilder>(
    this: E,
    chrome: &Chrome,
    theme: &Theme,
    glass: Option<&LensStyle>,
) -> E {
    match chrome.metrics.clip {
        Clip::Square => this
            .bg(theme.background)
            .border_r_1()
            .border_color(theme.border),
        Clip::Rounded => card(surface(this, chrome, theme, glass), chrome, theme, glass),
    }
}

/// A pill over the history: a card's fill, edge and shadow, or its glass.
/// It does not clip, as the composer's popups hang outside it.
pub(super) fn pill<E: Styled + FluentBuilder>(
    this: E,
    chrome: &Chrome,
    theme: &Theme,
    glass: Option<&LensStyle>,
) -> E {
    match glass {
        Some(lens) => lens_on(this, chrome, theme, lens),
        None => surface(this, chrome, theme, None)
            .when(chrome.elevation == Elevation::Shadowed, |this| {
                this.shadow(card_shadow(theme))
            }),
    }
}

/// A card's fill and edge, rounded. Over glass it draws neither: the lens
/// is the fill, the rim and the shadow (docs/decisions/glass-effects.md).
fn surface<E: Styled + FluentBuilder>(
    this: E,
    chrome: &Chrome,
    theme: &Theme,
    glass: Option<&LensStyle>,
) -> E {
    let this = this.rounded(card_in(chrome.metrics, theme));
    match glass {
        Some(_) => this,
        None => this
            .bg(theme.sidebar)
            .when(chrome.edge == Edge::Hairline, |this| {
                this.border_1().border_color(theme.border)
            }),
    }
}

/// The glass of a card, rounded as the card.
fn lens_on<E: Styled>(this: E, chrome: &Chrome, theme: &Theme, lens: &LensStyle) -> E {
    this.rounded(card_in(chrome.metrics, theme))
        .backdrop_glass(Some(*lens))
}

/// The card around a slot's content: its shadow (or its glass), its
/// corners and its clip. Classic has no card, so it returns `this`
/// unchanged. The lens sits here, outside the slot's cached view, so it
/// reads the window's backdrop.
pub(in crate::ui) fn card<E: Styled + FluentBuilder>(
    this: E,
    chrome: &Chrome,
    theme: &Theme,
    glass: Option<&LensStyle>,
) -> E {
    match chrome.metrics.clip {
        Clip::Square => this,
        Clip::Rounded => card_clip(match glass {
            Some(lens) => lens_on(this, chrome, theme, lens),
            None => this
                .rounded(card_in(chrome.metrics, theme))
                .when(chrome.elevation == Elevation::Shadowed, |this| {
                    this.shadow(card_shadow(theme))
                }),
        }),
    }
}

/// A card's clip: its content is cut by its rounded corners.
pub(in crate::ui) fn card_clip<E: Styled>(this: E) -> E {
    this.overflow_hidden().clip_rounded()
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

/// The title strip's lower edge: the frame's line in Classic, none in
/// Floating, where the cards sit under the strip on the wallpaper. Applied
/// after the line, so it only ever takes it away; the strip keeps its fill.
pub(in crate::ui) fn title_strip<E: Styled>(this: E, strip: TitleStrip) -> E {
    match strip {
        TitleStrip::Lined => this,
        TitleStrip::Bare => this.border_b_0(),
    }
}
