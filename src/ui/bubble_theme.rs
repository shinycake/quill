//! A chat theme's outgoing bubbles (tdesktop `ChatTheme::adjustPalette`
//! and `PrepareBubblesBackground`).
//!
//! An emoji theme's `outgoing_message_fill` gives one to four colors. One
//! color fills every outgoing bubble. Several make tdesktop's bubble
//! gradient: a vertical gradient laid over the whole message area, which
//! each bubble shows the slice of where it sits, so bubbles shift color as
//! they scroll. Here each bubble paints that slice as a two-stop gradient
//! from the area's color at its top edge to the one at its bottom edge.
//! Without a fill the theme's outgoing accent (else its accent) fills the
//! bubbles, as before.

use super::chat_theme::accent_strong;
use gpui_kit::component::theme::ActiveTheme;
use gpui_kit::*;
use quill::telegram::envelope::ThemeSettings;
use quill::wallpaper_raster::vertical_color_at;
use std::cell::Cell;

thread_local! {
    /// The message area of the history being painted (the wallpaper
    /// layer reports it before the rows paint).
    static HISTORY_AREA: Cell<Option<Bounds<Pixels>>> = const { Cell::new(None) };
}

/// Record the message area for this frame's bubbles.
pub(super) fn note_history_area(bounds: Bounds<Pixels>) {
    HISTORY_AREA.with(|area| area.set(Some(bounds)));
}

/// Fill of outgoing bubbles from a chat theme.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct OutgoingFill {
    colors: [u32; 4],
    count: u8,
    /// Night palette: white text on the bubbles; day: black.
    dark: bool,
}

impl OutgoingFill {
    /// From the theme half for the current light or dark mode.
    pub(crate) fn from_theme(settings: &ThemeSettings, dark: bool) -> Self {
        let list = settings
            .outgoing_fill
            .as_ref()
            .map(|fill| fill.colors())
            .filter(|c| !c.is_empty())
            .unwrap_or_else(|| {
                vec![
                    settings
                        .outgoing_accent_color
                        .unwrap_or(settings.accent_color),
                ]
            });
        Self::from_colors(&list, dark)
    }

    /// Up to four 0xRRGGBB colors, top to bottom.
    pub(crate) fn from_colors(list: &[u32], dark: bool) -> Self {
        let mut colors = [0u32; 4];
        let count = list.len().min(4);
        colors[..count].copy_from_slice(&list[..count]);
        Self {
            colors,
            count: count.max(1) as u8,
            dark,
        }
    }

    pub(crate) fn colors(&self) -> &[u32] {
        &self.colors[..self.count as usize]
    }

    /// One color for the whole fill (the mean).
    pub(crate) fn solid(&self) -> u32 {
        let colors = self.colors();
        let n = colors.len() as u32;
        let channel = |shift: u32| colors.iter().map(|c| (c >> shift) & 0xff).sum::<u32>() / n;
        (channel(16) << 16) | (channel(8) << 8) | channel(0)
    }

    pub(crate) fn is_gradient(&self) -> bool {
        self.count > 1
    }

    /// The fill's color `t` (0 top, 1 bottom) down the message area.
    pub(crate) fn color_at(&self, t: f32) -> u32 {
        vertical_color_at(self.colors(), t)
    }

    /// Text on these bubbles (tdesktop `historyTextOutFg`): black on day
    /// themes, white on night ones.
    pub(crate) fn text_rgb(&self) -> u32 {
        if self.dark { 0xffffff } else { 0x000000 }
    }

    pub(crate) fn text(&self) -> Hsla {
        rgb(self.text_rgb()).into()
    }
}

/// The background of outgoing bubbles: the theme's fill, else the accent.
pub(crate) fn outgoing_bubble_color(fill: Option<OutgoingFill>) -> Rgba {
    fill.map_or_else(accent_strong, |f| rgb(f.solid()))
}

/// Where a bubble from `top` to `bottom` sits in `area`, as 0-1 fractions.
pub(crate) fn slice_of(area: Bounds<Pixels>, top: Pixels, bottom: Pixels) -> (f32, f32) {
    let height = f32::from(area.size.height);
    if height <= 0.0 {
        return (0.0, 1.0);
    }
    let at = |y: Pixels| (f32::from(y - area.origin.y) / height).clamp(0.0, 1.0);
    (at(top), at(bottom))
}

/// The gradient a bubble paints over its solid fill when the theme has
/// several outgoing colors; `None` for a single color. `joined_above`
/// matches the bubble's small top corner.
pub(crate) fn gradient_layer(fill: OutgoingFill, joined_above: bool) -> Option<AnyElement> {
    if !fill.is_gradient() {
        return None;
    }
    Some(
        canvas(
            |_, _, _| {},
            move |bounds, _, window, cx| {
                let area = HISTORY_AREA.with(Cell::get).unwrap_or(bounds);
                let (top, bottom) = slice_of(area, bounds.top(), bounds.bottom());
                let big = cx.theme().radius_2xl();
                let small = px(5.);
                let radii = Corners {
                    top_left: big,
                    top_right: if joined_above { small } else { big },
                    bottom_right: small,
                    bottom_left: big,
                };
                window.paint_quad(
                    gpui_kit::fill(
                        bounds,
                        linear_gradient(
                            180.,
                            linear_color_stop(rgb(fill.color_at(top)), 0.),
                            linear_color_stop(rgb(fill.color_at(bottom)), 1.),
                        ),
                    )
                    .corner_radii(radii),
                );
            },
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .into_any_element(),
    )
}

#[cfg(test)]
mod tests {
    use super::{OutgoingFill, slice_of};
    use gpui_kit::{Bounds, point, px, size};
    use quill::telegram::envelope::{BackgroundFill, ThemeSettings};

    fn settings(fill: Option<BackgroundFill>, accent: Option<u32>) -> ThemeSettings {
        ThemeSettings {
            accent_color: 0x112233,
            background: None,
            outgoing_fill: fill,
            outgoing_accent_color: accent,
        }
    }

    #[test]
    fn theme_fills_keep_every_color_in_order() {
        let fill = OutgoingFill::from_theme(
            &settings(
                Some(BackgroundFill::Freeform(vec![
                    0x000000, 0x0000ff, 0x00ff00, 0xff0000,
                ])),
                None,
            ),
            false,
        );
        assert_eq!(fill.colors(), &[0x000000, 0x0000ff, 0x00ff00, 0xff0000]);
        assert!(fill.is_gradient());
        assert_eq!(fill.color_at(0.0), 0x000000);
        assert_eq!(fill.color_at(1.0), 0xff0000);
        let one =
            OutgoingFill::from_theme(&settings(Some(BackgroundFill::Solid(0xabcdef)), None), true);
        assert!(!one.is_gradient());
        assert_eq!(one.solid(), 0xabcdef);
        assert_eq!(one.text_rgb(), 0xffffff, "night: white");
        assert_eq!(fill.text_rgb(), 0x000000, "day: black");
    }

    #[test]
    fn without_a_fill_the_outgoing_accent_then_the_accent_colors_bubbles() {
        assert_eq!(
            OutgoingFill::from_theme(&settings(None, Some(0x445566)), false).solid(),
            0x445566
        );
        assert_eq!(
            OutgoingFill::from_theme(&settings(None, None), false).solid(),
            0x112233
        );
    }

    #[test]
    fn bubbles_take_the_slice_of_the_area_they_cover() {
        let area = Bounds::new(point(px(0.), px(100.)), size(px(300.), px(400.)));
        assert_eq!(slice_of(area, px(100.), px(300.)), (0.0, 0.5));
        assert_eq!(slice_of(area, px(50.), px(600.)), (0.0, 1.0), "clamped");
        let empty = Bounds::new(point(px(0.), px(0.)), size(px(10.), px(0.)));
        assert_eq!(slice_of(empty, px(0.), px(5.)), (0.0, 1.0));
    }
}
