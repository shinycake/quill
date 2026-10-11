//! A chat's palette (tdesktop `ChatTheme::adjustPalette`).
//!
//! - An emoji theme recolours the whole chat from its `accent_color`:
//!   tdesktop rebuilds the day or night palette through `ColorizerFrom`,
//!   so links, reply bars, poll bars, file icons and buttons follow the
//!   theme. Here the palette tokens (`chat_theme::pick`) pass through the
//!   same colorizer while the chat's rows are built.
//! - Inside outgoing bubbles every chromatic token moves to the outgoing
//!   accent (`outgoing_message_accent_color`, else the base accent toned
//!   by the outgoing fill: `bubblesAccentColorizer`). Text on the bubble is
//!   black on day themes and white on night ones; if that text or the
//!   accent cannot be read on one of the fill colours (contrast under
//!   1.14), every accent colour becomes the text colour.
//! - Service pills (dates, the unread bar) take the hue and saturation of
//!   the wallpaper's average colour (`msgServiceBg` adjusted by the
//!   background), for any wallpaper.
//!
//! The palette is scoped: [`with_palette`] sets it for the duration of a
//! closure, so only the chat's rows see it.

use super::bubble_theme::OutgoingFill;
use gpui_kit::{Hsla, Rgba};
use quill::telegram::envelope::ThemeSettings;
use quill::theme_colorize::{Colorizer, Hsv, MIN_ACCEPTABLE_CONTRAST, adjusted, contrast};
use std::cell::Cell;

/// How outgoing bubbles recolour accent tokens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OutgoingTone {
    /// Shift every chromatic colour to the outgoing accent.
    Shift(Colorizer),
    /// The accent is unreadable: chromatic colours become this one.
    Flat(u32),
}

/// One chat's palette changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) struct ChatPalette {
    accent: Option<Colorizer>,
    outgoing: Option<OutgoingTone>,
    /// Text (and on-fill marks) inside outgoing bubbles.
    outgoing_text: Option<u32>,
    /// The wallpaper's average colour, for service pills.
    service_tone: Option<u32>,
}

impl ChatPalette {
    /// The palette for a chat showing `theme` (the light or dark half) over
    /// a wallpaper whose average is `wallpaper_tone`. `base` is the
    /// palette's own accent. `None` when nothing changes.
    pub(crate) fn build(
        theme: Option<&ThemeSettings>,
        wallpaper_tone: Option<u32>,
        dark: bool,
        base: u32,
    ) -> Option<Self> {
        let mut palette = Self {
            service_tone: wallpaper_tone,
            ..Self::default()
        };
        if let Some(theme) = theme {
            palette.accent = Some(Colorizer::from_accent(base, theme.accent_color, dark));
            let fill = OutgoingFill::from_theme(theme, dark);
            let text = fill.text_rgb();
            let accent = theme
                .outgoing_accent_color
                .unwrap_or_else(|| adjusted(base, fill.solid()));
            let readable = fill.colors().iter().all(|&bg| {
                contrast(text, bg) >= MIN_ACCEPTABLE_CONTRAST
                    && contrast(accent, bg) >= MIN_ACCEPTABLE_CONTRAST
            });
            palette.outgoing = Some(if readable {
                OutgoingTone::Shift(Colorizer::for_bubbles(base, accent))
            } else {
                OutgoingTone::Flat(text)
            });
            palette.outgoing_text = Some(text);
        }
        (palette != Self::default()).then_some(palette)
    }

    /// `rgb` as this palette shows it, inside an outgoing bubble or not.
    pub(crate) fn apply(&self, rgb: u32, outgoing: bool) -> u32 {
        match (outgoing, self.outgoing) {
            (true, Some(OutgoingTone::Shift(colorizer))) => colorizer.apply(rgb),
            (true, Some(OutgoingTone::Flat(color))) => {
                let hsv = Hsv::of(rgb);
                if hsv.hue >= 0 && hsv.saturation >= 40 {
                    color
                } else {
                    rgb
                }
            }
            _ => self.accent.map_or(rgb, |c| c.apply(rgb)),
        }
    }
}

thread_local! {
    static ACTIVE: Cell<Option<ChatPalette>> = const { Cell::new(None) };
    static OUTGOING: Cell<bool> = const { Cell::new(false) };
}

/// Restores the previous palette when dropped (also on unwind).
struct Restore(Option<ChatPalette>, bool);

impl Drop for Restore {
    fn drop(&mut self) {
        ACTIVE.with(|a| a.set(self.0));
        OUTGOING.with(|o| o.set(self.1));
    }
}

/// Run `f` with `palette` active; `outgoing` marks an outgoing message.
pub(crate) fn with_palette<R>(
    palette: Option<ChatPalette>,
    outgoing: bool,
    f: impl FnOnce() -> R,
) -> R {
    let _restore = Restore(
        ACTIVE.with(|a| a.replace(palette)),
        OUTGOING.with(|o| o.replace(outgoing)),
    );
    f()
}

/// Whether a history row is an outgoing message (or album).
pub(crate) fn row_is_outgoing(row: &super::history_row::HistoryRow) -> bool {
    use super::history_row::HistoryRow;
    match row {
        HistoryRow::Single(inputs) => inputs.message.is_outgoing,
        HistoryRow::Album { messages, .. } => messages.first().is_some_and(|m| m.is_outgoing),
    }
}

impl super::app::QuillApp {
    /// The open chat's palette: its emoji theme for the current light or
    /// dark mode, and its wallpaper's tone. A photo has no known average
    /// here, so it tones nothing.
    pub(super) fn history_palette(
        &self,
        look: &super::wallpaper::ChatLook,
        cx: &gpui_kit::App,
    ) -> Option<ChatPalette> {
        use gpui_kit::component::theme::ActiveTheme;
        if super::chat_theme::high_contrast() {
            return None;
        }
        let dark = cx.theme().is_dark();
        let theme = self
            .viewed_chat()
            .and_then(|chat| self.session()?.chat_theme_settings(chat.0, dark));
        let tone = look
            .wallpaper
            .as_ref()
            .filter(|w| !matches!(w, super::wallpaper::Wallpaper::Image { .. }))
            .map(super::wallpaper::Wallpaper::backdrop);
        ChatPalette::build(theme, tone, dark, super::chat_theme::base_accent_hex())
    }
}

fn to_rgb(color: Rgba) -> u32 {
    let c = |v: f32| (v.clamp(0., 1.) * 255.).round() as u32;
    (c(color.r) << 16) | (c(color.g) << 8) | c(color.b)
}

fn from_rgb(rgb: u32, a: f32) -> Rgba {
    Rgba {
        r: ((rgb >> 16) & 0xff) as f32 / 255.,
        g: ((rgb >> 8) & 0xff) as f32 / 255.,
        b: (rgb & 0xff) as f32 / 255.,
        a,
    }
}

/// A palette token as the active chat shows it.
#[inline]
pub(crate) fn recolor(color: Rgba) -> Rgba {
    let Some(palette) = ACTIVE.with(Cell::get) else {
        return color;
    };
    let outgoing = OUTGOING.with(Cell::get);
    let rgb = to_rgb(color);
    let out = palette.apply(rgb, outgoing);
    if out == rgb {
        color
    } else {
        from_rgb(out, color.a)
    }
}

/// [`recolor`] for kit colours used inside rows.
pub(crate) fn themed(color: Hsla) -> Hsla {
    recolor(Rgba::from(color)).into()
}

/// Text and marks on an outgoing bubble's fill: the theme's, else white.
pub(crate) fn on_fill_text() -> Option<Rgba> {
    if !OUTGOING.with(Cell::get) {
        return None;
    }
    ACTIVE
        .with(Cell::get)
        .and_then(|p| p.outgoing_text)
        .map(|rgb| from_rgb(rgb, 1.))
}

/// A service pill background, toned by the wallpaper like tdesktop's
/// `msgServiceBg`.
pub(crate) fn service_bg(color: Hsla) -> Hsla {
    let Some(tone) = ACTIVE.with(Cell::get).and_then(|p| p.service_tone) else {
        return color;
    };
    let rgba = Rgba::from(color);
    from_rgb(adjusted(to_rgb(rgba), tone), rgba.a).into()
}

#[cfg(test)]
mod tests {
    use super::{ChatPalette, OutgoingTone, on_fill_text, recolor, service_bg, with_palette};
    use gpui_kit::{Hsla, Rgba};
    use quill::telegram::envelope::{BackgroundFill, ThemeSettings};

    fn theme(fill: u32, accent: u32) -> ThemeSettings {
        ThemeSettings {
            accent_color: accent,
            background: None,
            outgoing_fill: Some(BackgroundFill::Solid(fill)),
            outgoing_accent_color: None,
        }
    }

    fn rgba(rgb: u32) -> Rgba {
        Rgba {
            r: ((rgb >> 16) & 0xff) as f32 / 255.,
            g: ((rgb >> 8) & 0xff) as f32 / 255.,
            b: (rgb & 0xff) as f32 / 255.,
            a: 1.,
        }
    }

    #[test]
    fn no_theme_and_no_wallpaper_change_nothing() {
        assert_eq!(ChatPalette::build(None, None, false, 0x0969da), None);
        let only_tone = ChatPalette::build(None, Some(0x336699), false, 0x0969da).unwrap();
        assert_eq!(only_tone.apply(0x0969da, false), 0x0969da);
    }

    #[test]
    fn the_theme_accent_recolours_tokens_only_inside_the_scope() {
        let palette = ChatPalette::build(Some(&theme(0xffd6ec, 0xe0569b)), None, false, 0x0969da);
        let blue = rgba(0x0969da);
        let inside = with_palette(palette, false, || recolor(blue));
        assert_ne!(inside, blue);
        assert_eq!(recolor(blue), blue, "restored after the scope");
        // Greys are never touched.
        let grey = rgba(0x777777);
        assert_eq!(with_palette(palette, false, || recolor(grey)), grey);
    }

    #[test]
    fn light_theme_bubbles_get_black_text_and_dark_ones_white() {
        let light = ChatPalette::build(Some(&theme(0xffd6ec, 0xe0569b)), None, false, 0x0969da);
        assert_eq!(
            with_palette(light, true, on_fill_text),
            Some(rgba(0x000000))
        );
        assert_eq!(with_palette(light, false, on_fill_text), None, "incoming");
        let dark = ChatPalette::build(Some(&theme(0x5b2a86, 0xb98cff)), None, true, 0x1f6feb);
        assert_eq!(with_palette(dark, true, on_fill_text), Some(rgba(0xffffff)));
    }

    #[test]
    fn an_unreadable_outgoing_accent_falls_back_to_the_text_colour() {
        // A white outgoing accent on a white fill in a light theme.
        let mut settings = theme(0xffffff, 0xe0569b);
        settings.outgoing_accent_color = Some(0xfefefe);
        let palette = ChatPalette::build(Some(&settings), None, false, 0x0969da).unwrap();
        assert_eq!(palette.outgoing, Some(OutgoingTone::Flat(0x000000)));
        assert_eq!(palette.apply(0x0969da, true), 0x000000);
        assert_eq!(palette.apply(0x777777, true), 0x777777, "greys stay");
        // A readable accent shifts instead.
        let fine =
            ChatPalette::build(Some(&theme(0xffd6ec, 0xe0569b)), None, false, 0x0969da).unwrap();
        assert!(matches!(fine.outgoing, Some(OutgoingTone::Shift(_))));
    }

    #[test]
    fn service_pills_take_the_wallpaper_tone() {
        let palette = ChatPalette::build(None, Some(0x2e7d32), false, 0x0969da);
        let pill: Hsla = rgba(0xdddddd).into();
        let toned = with_palette(palette, false, || service_bg(pill));
        assert_ne!(toned, pill);
        assert!((toned.l - pill.l).abs() < 0.02, "keeps its lightness");
        assert_eq!(service_bg(pill), pill);
    }
}
