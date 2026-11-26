//! The colours of a chat row, chosen once from whether it is the open
//! chat's row and how the layout draws that row (`SelectedRow`): Classic's
//! tint, or Floating's solid accent row (docs/decisions/floating-layout.md).

use gpui_kit::component::theme::ThemeColor;
use gpui_kit::{Hsla, Rgba};
use quill::layout_style::SelectedRow;

use super::selected_secondary_fg;
use crate::ui::chat_theme::accent;

/// The open chat's row under the pointer (or the keyboard selection).
pub(in crate::ui) fn open_row_hover_fill(selected: SelectedRow, theme: &ThemeColor) -> Hsla {
    match selected {
        SelectedRow::Tint => theme.primary.opacity(0.3),
        SelectedRow::Solid => solid_fill(theme),
    }
}

/// The solid row's fill: the accent, darkened or lightened (hue kept)
/// until its text and secondary text keep WCAG AA on it. Some accents
/// (a light blue under white text) are 3.7:1 as they are.
fn solid_fill(theme: &ThemeColor) -> Hsla {
    let fill = rgb_of(theme.primary);
    let text = rgb_of(theme.primary_foreground);
    let moved = quill::theme_colorize::readable(fill, text, SOLID_CONTRAST);
    if moved == fill {
        return theme.primary;
    }
    let rgba = Rgba {
        r: ((moved >> 16) & 0xff) as f32 / 255.,
        g: ((moved >> 8) & 0xff) as f32 / 255.,
        b: (moved & 0xff) as f32 / 255.,
        a: 1.,
    };
    Hsla::from(rgba)
}

/// An opaque colour as 0xRRGGBB.
fn rgb_of(color: Hsla) -> u32 {
    let c = Rgba::from(color);
    let byte = |v: f32| (v.clamp(0., 1.) * 255.).round() as u32;
    (byte(c.r) << 16) | (byte(c.g) << 8) | byte(c.b)
}

/// The open row's hover fill on a tinted row (a forum's topic rows keep
/// the tint in every layout).
pub(in crate::ui) fn tinted_open_row_hover_fill(theme: &ThemeColor) -> Hsla {
    open_row_hover_fill(SelectedRow::Tint, theme)
}

/// How a row's counters are coloured: their own colours, or inverted on a
/// solid accent row.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::ui) enum BadgeInk {
    Own,
    /// On a fill: the pill takes `text` as its fill and `fill` as its text.
    OnFill {
        fill: Hsla,
        text: Hsla,
    },
}

/// A chat row's colours.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::ui) struct RowInk {
    pub fill: Hsla,
    /// The open row's hover and pressed fills; a row that is not open
    /// takes the list's own (`pressable`).
    pub open_hover: Option<(Hsla, Hsla)>,
    /// The title's colour, where the row sets one.
    pub title: Option<Hsla>,
    /// Time, preview and glyphs.
    pub secondary: Hsla,
    /// The preview's sender name.
    pub sender: Hsla,
    /// The typing line.
    pub activity: Hsla,
    /// The ring that parts the online dot from the photo.
    pub ring: Hsla,
    pub badges: BadgeInk,
}

impl RowInk {
    /// `rest_fill` is a row's fill when it is not open (the list's, or
    /// clear on a glass card); `muted_fg` its secondary text.
    pub(in crate::ui) fn of(
        open: bool,
        selected: SelectedRow,
        rest_fill: Hsla,
        muted_fg: Hsla,
        theme: &ThemeColor,
    ) -> Self {
        match (open, selected) {
            (false, _) => Self {
                fill: rest_fill,
                open_hover: None,
                title: None,
                secondary: muted_fg,
                sender: accent().into(),
                activity: theme.primary,
                ring: theme.sidebar,
                badges: BadgeInk::Own,
            },
            (true, SelectedRow::Tint) => {
                let secondary = selected_secondary_fg(theme.foreground);
                Self {
                    fill: theme.primary.opacity(0.25),
                    open_hover: Some((
                        open_row_hover_fill(selected, theme),
                        theme.primary.opacity(0.4),
                    )),
                    title: None,
                    secondary,
                    sender: secondary,
                    activity: theme.primary,
                    ring: theme.sidebar,
                    badges: BadgeInk::Own,
                }
            }
            (true, SelectedRow::Solid) => {
                let text = theme.primary_foreground;
                let fill = solid_fill(theme);
                Self {
                    fill,
                    open_hover: Some((fill, fill)),
                    title: Some(text),
                    secondary: text.opacity(SOLID_SECONDARY_ALPHA),
                    sender: text,
                    activity: text,
                    ring: fill,
                    badges: BadgeInk::OnFill { fill, text },
                }
            }
        }
    }
}

impl RowInk {
    /// The ink of a row on a glass card, whose open row is a glass
    /// highlight: Classic's tint colours, but the lens is the open row's
    /// fill, so the row keeps the list's own (`rest_fill`) and no hover fill
    /// of its own.
    pub(in crate::ui) fn on_glass(
        open: bool,
        rest_fill: Hsla,
        muted_fg: Hsla,
        theme: &ThemeColor,
    ) -> Self {
        Self {
            fill: rest_fill,
            open_hover: None,
            ..Self::of(open, SelectedRow::Tint, rest_fill, muted_fg, theme)
        }
    }
}

/// Secondary text on the solid row: the row's text colour, softened.
const SOLID_SECONDARY_ALPHA: f32 = 0.9;

/// How far the solid fill keeps from its text, so that the softened
/// secondary text still keeps 4.5:1.
const SOLID_CONTRAST: f64 = 5.4;

#[cfg(test)]
#[path = "row_ink_tests.rs"]
mod tests;
