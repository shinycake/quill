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
use quill::layout_style::{Chrome, Edge, Elevation, Look};

/// The chat list column's own frame, inside its slot: Classic's right
/// border, or a card's rounded fill (none over glass: the slot's card
/// draws the lens, `glass`).
pub(in crate::ui) fn list_frame<E: Styled + FluentBuilder>(
    this: E,
    chrome: &Chrome,
    theme: &Theme,
    glass: Option<&LensStyle>,
) -> E {
    match chrome.metrics.look {
        Look::Lines => this
            .border_r_1()
            .border_color(theme.border)
            .bg(theme.sidebar),
        Look::Cards => surface(this, chrome, theme, glass),
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
    match chrome.metrics.look {
        Look::Lines => this
            .border_l_1()
            .border_color(theme.border)
            .bg(theme.sidebar),
        Look::Cards => surface(this, chrome, theme, glass),
    }
}

/// The column resize handle: Classic's lights its line on hover; between
/// cards there is no line to light.
pub(in crate::ui) fn resize_handle<E: InteractiveElement>(
    this: E,
    chrome: &Chrome,
    theme: &Theme,
) -> E {
    match chrome.metrics.look {
        Look::Lines => {
            let line = theme.border;
            this.hover(move |style| style.bg(line))
        }
        Look::Cards => this,
    }
}

/// The settings window: Classic's page fill; in Floating no fill of its
/// own, so the window's wallpaper shows in the gutter around its two cards.
pub(in crate::ui) fn settings_frame<E: Styled>(this: E, chrome: &Chrome, theme: &Theme) -> E {
    match chrome.metrics.look {
        Look::Lines => this.bg(theme.background),
        Look::Cards => {
            let gutter = px(chrome.metrics.gutter);
            this.p(gutter).gap(gutter)
        }
    }
}

/// The settings list inside its card: no line at its edge, and clear on a
/// glass card. Classic keeps the kit sidebar as it is.
pub(in crate::ui) fn settings_list<E: Styled>(this: E, chrome: &Chrome, fill: Hsla) -> E {
    match chrome.metrics.look {
        Look::Lines => this,
        Look::Cards => this.border_r_0().border_l_0().bg(fill),
    }
}

/// A settings page: as it is in Classic; a card (fill, edge, shadow and
/// clip) in Floating.
pub(in crate::ui) fn page_card<E: Styled + FluentBuilder>(
    this: E,
    chrome: &Chrome,
    theme: &Theme,
    glass: Option<&LensStyle>,
) -> E {
    match chrome.metrics.look {
        Look::Lines => this,
        Look::Cards => card(surface(this, chrome, theme, glass), chrome, theme, glass),
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
pub(super) fn surface<E: Styled + FluentBuilder>(
    this: E,
    chrome: &Chrome,
    theme: &Theme,
    glass: Option<&LensStyle>,
) -> E {
    let this = this.rounded(card_in(chrome.metrics, theme));
    match glass {
        Some(_) => this,
        None => this.bg(theme.sidebar).map(|this| match chrome.edge {
            Edge::Hairline => this.border_1().border_color(theme.border),
            // A near-black card on a dark wallpaper has no outline of its
            // own and its shadow does not show: a faint light edge keeps it.
            Edge::None if theme.is_dark() => this.border_1().border_color(DARK_CARD_EDGE),
            Edge::None => this,
        }),
    }
}

/// How wide a card's edge is drawn (`surface`): a hairline, or none.
pub(in crate::ui) fn edge_width(chrome: &Chrome, theme: &Theme) -> Pixels {
    match chrome.edge {
        Edge::Hairline => px(1.),
        Edge::None if theme.is_dark() => px(1.),
        Edge::None => px(0.),
    }
}

/// The edge of a dark card over the wallpaper.
pub(super) const DARK_CARD_EDGE: Hsla = Hsla {
    h: 0.,
    s: 0.,
    l: 1.,
    a: 0.12,
};

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
    match chrome.metrics.look {
        Look::Lines => this,
        Look::Cards => card_clip(match glass {
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
/// sidebar fill with the overlay shadow), or, where the slot inside draws
/// the list card, a deeper shadow behind that card: the drawer stands over
/// the chat, not beside it (Web K's sliding left column).
pub(in crate::ui) fn peek_surface<E: Styled + ParentElement + FluentBuilder>(
    this: E,
    chrome: &Chrome,
    theme: &Theme,
) -> E {
    match chrome.metrics.look {
        Look::Lines => this.bg(theme.sidebar).shadow_xl(),
        Look::Cards => {
            let gutter = px(chrome.metrics.gutter);
            this.child(
                div()
                    .absolute()
                    .top(gutter)
                    .bottom(gutter)
                    .left(gutter)
                    .right_0()
                    .rounded(card_in(chrome.metrics, theme))
                    .shadow(drawer_shadow(theme)),
            )
        }
    }
}

/// The held drawer's shadow: wider and darker than a card's.
fn drawer_shadow(theme: &Theme) -> Vec<BoxShadow> {
    let ink = if theme.is_dark() { 0.6 } else { 0.28 };
    vec![BoxShadow {
        color: hsla(0., 0., 0., ink),
        offset: point(px(4.), px(8.)),
        blur_radius: px(32.),
        spread_radius: px(0.),
        inset: false,
    }]
}

/// The scrim over the chat behind the held drawer at `shown` of its
/// slide: Floating's (Web K's dimmed chat behind the left column), none in
/// Classic, whose sheet keeps its own shadow.
pub(in crate::ui) fn drawer_scrim(chrome: &Chrome, shown: f32) -> Option<Hsla> {
    match chrome.metrics.look {
        Look::Lines => None,
        Look::Cards => Some(black().opacity(DRAWER_SCRIM * shown.clamp(0., 1.))),
    }
}

/// How dark the drawer's scrim gets once the drawer is in.
const DRAWER_SCRIM: f32 = 0.32;

/// The title strip's fill and lower edge: the frame's in Classic; in
/// Floating neither, so the window's wallpaper runs under the strip as it
/// does under the cards. Applied after the frame's own, so it only ever
/// takes them away.
pub(in crate::ui) fn title_strip<E: Styled>(this: E, strip: Look) -> E {
    match strip {
        Look::Lines => this,
        Look::Cards => this.border_b_0().bg(transparent_black()),
    }
}
