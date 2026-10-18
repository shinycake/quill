//! The call windows' chrome tokens (tdesktop `callBg`, `callNameFg`,
//! `callStatusFg`, `callIconBg`, the scrim under video): the surface,
//! muted text, danger tint and scrim of the 1:1 call window, the group call
//! panel and their device menus.
//!
//! Like tdesktop's call palette, the stage is dark in every theme, light
//! included: the pictures sit on it and the chrome has to read over them.
//! Under high contrast ([`high_contrast`]) the same roles re-root for
//! maximum contrast: pure-black surfaces, white text and borders, heavier
//! scrims. The pure-black video letterbox is not a token; it stays
//! `black()`.
#![deny(clippy::indexing_slicing)]

use crate::ui::chat_theme::high_contrast;
use gpui_kit::{Hsla, hsla, rgb, rgba};

/// The call window ground in the normal themes (lib_ui `colors.palette`).
pub(crate) const SURFACE: u32 = 0x1b1f23;
/// The group call panel ground in the normal themes.
pub(crate) const GROUP_SURFACE: u32 = 0x1a2026;
/// Member rows and cards on the group panel, and their hover.
pub(crate) const GROUP_RAISED: u32 = 0x2c333d;
pub(crate) const GROUP_RAISED_OVER: u32 = 0x323a45;
/// Secondary text on the group panel as other screens still draw it.
pub(crate) const GROUP_MUTED: u32 = 0x91979e;
/// Secondary text on the group panel's own rows: `GROUP_MUTED` lifted to
/// WCAG AA (4.5:1) on the raised rows and their hover.
const GROUP_MUTED_AA: u32 = 0xa3a9b0;
/// Secondary text on the call window.
pub(crate) const MUTED: u32 = 0xaaabac;

fn hc_or(normal: Hsla, high: Hsla) -> Hsla {
    if high_contrast() { high } else { normal }
}

fn white_a(alpha: f32) -> Hsla {
    hsla(0., 0., 1., alpha)
}

fn black_a(alpha: f32) -> Hsla {
    hsla(0., 0., 0., alpha)
}

/// The 1:1 call window ground.
pub(crate) fn surface() -> Hsla {
    hc_or(rgb(SURFACE).into(), black_a(1.))
}

/// The group call panel ground.
pub(crate) fn group_surface() -> Hsla {
    hc_or(rgb(GROUP_SURFACE).into(), black_a(1.))
}

/// Member rows, search and cards on the group panel.
pub(crate) fn group_raised() -> Hsla {
    hc_or(rgb(GROUP_RAISED).into(), rgb(0x0a0a0a).into())
}

/// Hover on those rows.
pub(crate) fn group_raised_over() -> Hsla {
    hc_or(rgb(GROUP_RAISED_OVER).into(), rgb(0x262626).into())
}

/// A floating menu over the call (the device picker).
pub(crate) fn menu_surface() -> Hsla {
    hc_or(rgb(0x2a2f35).into(), black_a(1.))
}

/// The menu's outline.
pub(crate) fn border() -> Hsla {
    hc_or(white_a(0.12), white_a(0.9))
}

/// The outline of a glass button over video.
pub(crate) fn glass_border() -> Hsla {
    hc_or(white_a(0.25), white_a(0.9))
}

/// Primary text and icons on the stage.
pub(crate) fn text() -> Hsla {
    white_a(1.)
}

/// Secondary text on the call window.
pub(crate) fn muted() -> Hsla {
    hc_or(rgb(MUTED).into(), text())
}

/// Secondary text on the group panel.
pub(crate) fn group_muted() -> Hsla {
    hc_or(rgb(GROUP_MUTED_AA).into(), text())
}

/// A round button over video, and its hover.
pub(crate) fn glass() -> Hsla {
    hc_or(black_a(0.55), black_a(0.9))
}

pub(crate) fn glass_over() -> Hsla {
    hc_or(black_a(0.75), black_a(1.))
}

/// The darker chip on the group tiles (pin, full screen), and its hover.
pub(crate) fn chip() -> Hsla {
    hc_or(rgba(0x0000_0099).into(), black_a(0.9))
}

pub(crate) fn chip_over() -> Hsla {
    hc_or(rgba(0x0000_00cc).into(), black_a(1.))
}

/// A tile caption over the picture.
pub(crate) fn caption() -> Hsla {
    hc_or(rgba(0xffff_ffe0).into(), text())
}

/// The dim behind a dialog on the panel.
pub(crate) fn scrim() -> Hsla {
    hc_or(black_a(0.5), black_a(0.8))
}

/// A pill over video (the emoji fingerprint).
pub(crate) fn pill() -> Hsla {
    hc_or(black_a(0.3), black_a(0.85))
}

/// The shade under the title and under the buttons when video fills the
/// window.
pub(crate) fn video_shade_top() -> Hsla {
    hc_or(black_a(0.45), black_a(0.7))
}

pub(crate) fn video_shade_bottom() -> Hsla {
    hc_or(black_a(0.5), black_a(0.8))
}

/// The transparent end of a shade.
pub(crate) fn clear() -> Hsla {
    black_a(0.)
}

/// An idle round button (white wash) and a row's hover.
pub(crate) fn wash() -> Hsla {
    hc_or(white_a(0.12), white_a(0.25))
}

pub(crate) fn wash_over() -> Hsla {
    hc_or(white_a(0.1), white_a(0.25))
}

/// A switched-on round button and the icon on it.
pub(crate) fn active() -> Hsla {
    hc_or(white_a(0.9), white_a(1.))
}

pub(crate) fn on_active() -> Hsla {
    rgb(0x222222).into()
}

/// A muted microphone button.
pub(crate) fn muted_fill() -> Hsla {
    hc_or(rgb(0x8f8f8f).into(), rgb(0x595959).into())
}

/// Error and reconnect banners on the group panel (white text on them).
pub(crate) fn danger_tint() -> Hsla {
    hc_or(rgba(0xeb53_5333).into(), rgb(0x9c1c1c).into())
}

/// The group panel's leave button.
pub(crate) fn danger_fill() -> Hsla {
    hc_or(rgba(0xf75c_5c7f).into(), rgb(0xb3261e).into())
}

/// The answer and hang-up buttons of the call window. The normal colours
/// are tdesktop's and carry white at under 4.5:1; high contrast darkens
/// them so the white glyph reads.
pub(crate) fn answer_fill() -> Hsla {
    hc_or(rgb(0x66c95b).into(), rgb(0x1a6b2a).into())
}

pub(crate) fn hangup_fill() -> Hsla {
    hc_or(rgb(0xd75a5a).into(), rgb(0xb3261e).into())
}

/// The ring around the answer button.
pub(crate) fn answer_ring() -> Hsla {
    rgba(0x50eb_4140).into()
}

#[cfg(test)]
mod tests {
    use super::{
        GROUP_MUTED, MUTED, SURFACE, border, caption, chip, danger_fill, group_muted, group_raised,
        group_raised_over, group_surface, high_contrast, menu_surface, muted, surface, text,
    };
    use crate::ui::chat_theme::set_high_contrast;
    use gpui_kit::{Hsla, Rgba, hsla, rgb, rgba};

    fn lum(c: Hsla) -> f32 {
        let c = Rgba::from(c);
        let lin = |v: f32| {
            if v <= 0.03928 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * lin(c.r) + 0.7152 * lin(c.g) + 0.0722 * lin(c.b)
    }

    fn ratio(fg: Hsla, bg: Hsla) -> f32 {
        let (a, b) = (lum(fg), lum(bg));
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    /// `over` painted on `under`, ignoring `over`'s alpha channel's source.
    fn over(top: Hsla, under: Hsla) -> Hsla {
        let (t, u) = (Rgba::from(top), Rgba::from(under));
        let mix = |a: f32, b: f32| a * t.a + b * (1. - t.a);
        Rgba {
            r: mix(t.r, u.r),
            g: mix(t.g, u.g),
            b: mix(t.b, u.b),
            a: 1.,
        }
        .into()
    }

    fn same(a: Hsla, b: Hsla) -> bool {
        let (a, b) = (Rgba::from(a), Rgba::from(b));
        [(a.r, b.r), (a.g, b.g), (a.b, b.b), (a.a, b.a)]
            .iter()
            .all(|(x, y)| (x - y).abs() < 0.002)
    }

    #[test]
    fn muted_text_is_aa_on_its_surfaces_in_every_mode() {
        for high in [false, true] {
            set_high_contrast(high);
            assert!(high_contrast() == high);
            assert!(ratio(muted(), surface()) >= 4.5, "call window, hc {high}");
            assert!(
                ratio(group_muted(), group_raised()) >= 4.5,
                "group panel, hc {high}"
            );
            assert!(
                ratio(group_muted(), group_raised_over()) >= 4.5,
                "group panel hover, hc {high}"
            );
            assert!(ratio(muted(), menu_surface()) >= 4.5, "menu, hc {high}");
        }
        set_high_contrast(false);
    }

    #[test]
    fn high_contrast_never_lowers_the_contrast() {
        set_high_contrast(false);
        let normal = (ratio(text(), surface()), ratio(border(), menu_surface()));
        set_high_contrast(true);
        assert!(ratio(text(), surface()) >= normal.0);
        assert!(ratio(border(), menu_surface()) >= normal.1);
        set_high_contrast(false);
    }

    #[test]
    fn normal_themes_keep_the_old_literals() {
        set_high_contrast(false);
        let pairs: [(Hsla, Hsla); 22] = [
            (surface(), rgb(0x1b1f23).into()),
            (group_surface(), rgb(0x1a2026).into()),
            (group_raised(), rgb(0x2c333d).into()),
            (group_raised_over(), rgb(0x323a45).into()),
            (menu_surface(), rgb(0x2a2f35).into()),
            (muted(), rgb(0xaaabac).into()),
            (group_muted(), rgb(0xa3a9b0).into()),
            (border(), hsla(0., 0., 1., 0.12)),
            (super::glass_border(), hsla(0., 0., 1., 0.25)),
            (text(), hsla(0., 0., 1., 1.)),
            (super::glass(), hsla(0., 0., 0., 0.55)),
            (super::glass_over(), hsla(0., 0., 0., 0.75)),
            (chip(), rgba(0x0000_0099).into()),
            (super::chip_over(), rgba(0x0000_00cc).into()),
            (caption(), rgba(0xffff_ffe0).into()),
            (super::scrim(), hsla(0., 0., 0., 0.5)),
            (super::pill(), hsla(0., 0., 0., 0.3)),
            (super::video_shade_top(), hsla(0., 0., 0., 0.45)),
            (super::video_shade_bottom(), hsla(0., 0., 0., 0.5)),
            (super::muted_fill(), rgb(0x8f8f8f).into()),
            (super::danger_tint(), rgba(0xeb53_5333).into()),
            (danger_fill(), rgba(0xf75c_5c7f).into()),
        ];
        for (index, (token, literal)) in pairs.iter().enumerate() {
            assert!(same(*token, *literal), "token {index} drifted");
        }
        assert_eq!(
            (SURFACE, MUTED, GROUP_MUTED),
            (0x1b1f23, 0xaaabac, 0x91979e)
        );
    }

    #[test]
    fn caption_chip_and_danger_text_stay_readable() {
        for high in [false, true] {
            set_high_contrast(high);
            // A caption sits on a dark scrim over the picture: judge it on
            // the darkest the tile can be under the chip, and on the panel.
            let ground = over(chip(), group_surface());
            assert!(
                ratio(over(caption(), ground), ground) >= 4.5,
                "caption on chip, hc {high}"
            );
            assert!(
                ratio(text(), over(chip(), rgb(0x808080).into())) >= 3.0,
                "chip icon on mid grey, hc {high}"
            );
            assert!(
                ratio(text(), over(danger_fill(), group_surface())) >= 3.0,
                "white on the danger fill, hc {high}"
            );
        }
        set_high_contrast(true);
        assert!(ratio(text(), danger_fill()) >= 4.5);
        assert!(ratio(text(), super::answer_fill()) >= 4.5);
        assert!(ratio(text(), super::hangup_fill()) >= 4.5);
        set_high_contrast(false);
        assert!(same(super::answer_fill(), rgb(0x66c95b).into()));
        assert!(same(super::hangup_fill(), rgb(0xd75a5a).into()));
        set_high_contrast(false);
    }
}
