//! The chat wallpaper: a preset color, or the account's Telegram wallpaper
//! (tdesktop `boxes/background_box.cpp`, `window/section_widget.cpp`'s
//! `ChatBackground`). Patterns, blur and motion are not drawn: a pattern
//! wallpaper shows its fill, a blurred photo shows sharp.

use super::app::QuillApp;
use gpui_kit::*;
use quill::telegram::envelope::{Background, BackgroundFill, BackgroundType};
use std::path::PathBuf;

/// What paints behind the message list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Wallpaper {
    Solid(u32),
    /// `angle` is a CSS-style angle (180 = top to bottom).
    Gradient {
        top: u32,
        bottom: u32,
        angle: i32,
    },
    /// A downloaded photo, with the color to show around it.
    Image {
        path: PathBuf,
        backdrop: u32,
    },
}

impl Wallpaper {
    /// One color standing for the whole wallpaper (video masks blend into
    /// it).
    pub(super) fn backdrop(&self) -> u32 {
        match self {
            Wallpaper::Solid(c) => *c,
            Wallpaper::Gradient { top, bottom, .. } => BackgroundFill::Gradient {
                top: *top,
                bottom: *bottom,
                angle: 0,
            }
            .average(),
            Wallpaper::Image { backdrop, .. } => *backdrop,
        }
    }
}

/// Telegram's `rotation_angle` (0 = top to bottom, clockwise) as a CSS-style
/// angle for `linear_gradient`.
fn gradient_angle(rotation: i32) -> i32 {
    (180 + rotation).rem_euclid(360)
}

fn fill_wallpaper(fill: &BackgroundFill) -> Wallpaper {
    match fill {
        BackgroundFill::Solid(color) => Wallpaper::Solid(*color),
        BackgroundFill::Gradient { top, bottom, angle } => Wallpaper::Gradient {
            top: *top,
            bottom: *bottom,
            angle: gradient_angle(*angle),
        },
        // Freeform gradients blend up to four colors; two stops are the
        // closest this paints.
        BackgroundFill::Freeform(colors) => match (colors.first(), colors.last()) {
            (Some(first), Some(last)) if first != last => Wallpaper::Gradient {
                top: *first,
                bottom: *last,
                angle: 180,
            },
            (Some(first), _) => Wallpaper::Solid(*first),
            _ => Wallpaper::Solid(0),
        },
    }
}

/// What a Telegram background paints, given the local path of its photo
/// (`None` while it downloads — the wallpaper is skipped until then).
pub(super) fn background_wallpaper(
    background: &Background,
    image_path: Option<&str>,
) -> Option<Wallpaper> {
    match &background.kind {
        BackgroundType::Fill(fill) | BackgroundType::Pattern { fill, .. } => {
            Some(fill_wallpaper(fill))
        }
        BackgroundType::Wallpaper { .. } => image_path.map(|path| Wallpaper::Image {
            path: PathBuf::from(path),
            backdrop: 0x202020,
        }),
        BackgroundType::ChatTheme { .. } => None,
    }
}

/// Pick the wallpaper: the Telegram one when chosen and available, else the
/// preset color.
pub(super) fn resolve_wallpaper(
    preset_rgb: Option<u32>,
    use_telegram: bool,
    telegram: Option<Wallpaper>,
) -> Option<Wallpaper> {
    if use_telegram && let Some(wallpaper) = telegram {
        return Some(wallpaper);
    }
    preset_rgb.map(Wallpaper::Solid)
}

impl QuillApp {
    /// The wallpaper to paint now.
    pub(super) fn current_wallpaper(&self, cx: &App) -> Option<Wallpaper> {
        use gpui_kit::component::theme::ActiveTheme;
        let dark = cx.theme().is_dark();
        let telegram = self.session().and_then(|s| {
            let background = s.default_backgrounds.get(&dark)?;
            let path = background
                .file
                .as_ref()
                .and_then(|f| s.files.get(&f.id.0))
                .and_then(|f| f.usable_path());
            background_wallpaper(background, path)
        });
        resolve_wallpaper(
            self.appearance.wallpaper_rgb,
            self.appearance.telegram_wallpaper,
            telegram,
        )
    }

    /// The solid color video masks should blend into.
    pub(super) fn wallpaper_backdrop(&self, cx: &App) -> Hsla {
        use gpui_kit::component::theme::ActiveTheme;
        self.current_wallpaper(cx)
            .map_or(cx.theme().background, |w| rgb(w.backdrop()).into())
    }
}

#[cfg(test)]
mod tests {
    use super::{Wallpaper, background_wallpaper, gradient_angle, resolve_wallpaper};
    use quill::telegram::envelope::{Background, BackgroundFill, BackgroundType};

    fn bg(kind: BackgroundType) -> Background {
        Background {
            id: 1,
            is_default: true,
            is_dark: false,
            name: "x".into(),
            file: None,
            kind,
        }
    }

    #[test]
    fn fills_map_to_paintable_wallpapers() {
        assert_eq!(
            background_wallpaper(&bg(BackgroundType::Fill(BackgroundFill::Solid(5))), None),
            Some(Wallpaper::Solid(5))
        );
        let gradient = BackgroundType::Fill(BackgroundFill::Gradient {
            top: 1,
            bottom: 2,
            angle: 0,
        });
        assert_eq!(
            background_wallpaper(&bg(gradient), None),
            Some(Wallpaper::Gradient {
                top: 1,
                bottom: 2,
                angle: 180
            })
        );
        let freeform = BackgroundType::Fill(BackgroundFill::Freeform(vec![1, 2, 3, 4]));
        assert!(matches!(
            background_wallpaper(&bg(freeform), None),
            Some(Wallpaper::Gradient {
                top: 1,
                bottom: 4,
                ..
            })
        ));
    }

    #[test]
    fn patterns_show_their_fill_and_photos_wait_for_the_file() {
        let pattern = BackgroundType::Pattern {
            fill: BackgroundFill::Solid(9),
            intensity: 50,
            inverted: false,
            moving: false,
        };
        assert_eq!(
            background_wallpaper(&bg(pattern), None),
            Some(Wallpaper::Solid(9))
        );
        let photo = bg(BackgroundType::Wallpaper {
            blurred: false,
            moving: false,
        });
        assert_eq!(background_wallpaper(&photo, None), None);
        assert!(matches!(
            background_wallpaper(&photo, Some("/tmp/a.jpg")),
            Some(Wallpaper::Image { .. })
        ));
    }

    #[test]
    fn telegram_wallpaper_wins_only_when_chosen_and_ready() {
        let tg = Some(Wallpaper::Solid(7));
        assert_eq!(
            resolve_wallpaper(Some(1), true, tg.clone()),
            Some(Wallpaper::Solid(7))
        );
        assert_eq!(
            resolve_wallpaper(Some(1), false, tg.clone()),
            Some(Wallpaper::Solid(1))
        );
        // Chosen but not downloaded yet: the preset (or the theme) shows.
        assert_eq!(
            resolve_wallpaper(Some(1), true, None),
            Some(Wallpaper::Solid(1))
        );
        assert_eq!(resolve_wallpaper(None, true, None), None);
    }

    #[test]
    fn gradient_angles_wrap() {
        assert_eq!(gradient_angle(0), 180);
        assert_eq!(gradient_angle(180), 0);
        assert_eq!(gradient_angle(225), 45);
    }
}

/// Screenshot fixture: five installed wallpapers (fills only, no photos),
/// the second one the account default for both themes.
pub(super) fn apply_demo_wallpapers(session: &mut quill::state::Session) {
    let fill = |id: i64, name: &str, fill: BackgroundFill| Background {
        id,
        is_default: false,
        is_dark: false,
        name: name.into(),
        file: None,
        kind: BackgroundType::Fill(fill),
    };
    let list = vec![
        fill(1, "Mint", BackgroundFill::Solid(0xb8e0c9)),
        fill(
            2,
            "Dusk",
            BackgroundFill::Gradient {
                top: 0x2b3a67,
                bottom: 0xb56576,
                angle: 0,
            },
        ),
        fill(
            3,
            "Sunrise",
            BackgroundFill::Gradient {
                top: 0xffd89b,
                bottom: 0x19547b,
                angle: 45,
            },
        ),
        fill(4, "Sand", BackgroundFill::Solid(0xe9dcc3)),
        fill(
            5,
            "Aurora",
            BackgroundFill::Freeform(vec![0x0f2027, 0x2c5364, 0x42a5a5]),
        ),
    ];
    let default = list[1].clone();
    session.installed_backgrounds = Some(list);
    session.default_backgrounds.insert(false, default.clone());
    session.default_backgrounds.insert(true, default);
}
