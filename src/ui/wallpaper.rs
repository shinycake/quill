//! The chat wallpaper: a preset color, or the account's Telegram wallpaper
//! (tdesktop `boxes/background_box.cpp`, `window/section_widget.cpp`'s
//! `ChatBackground`, `ui/chat/chat_theme.cpp`). Patterns are drawn
//! (`wallpaper_pattern`); blur and motion are not: a blurred photo shows
//! sharp.

use super::app::QuillApp;
use super::wallpaper_pattern::{PatternInk, pattern_ink, pattern_layer};
use gpui_kit::component::*;
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
    /// A pattern file drawn over `base` (a fill).
    Pattern {
        base: Box<Wallpaper>,
        path: PathBuf,
        ink: PatternInk,
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
            Wallpaper::Pattern { base, .. } => base.backdrop(),
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
        BackgroundType::Fill(fill) => Some(fill_wallpaper(fill)),
        // Until the pattern file is on disk the fill shows alone.
        BackgroundType::Pattern {
            fill,
            intensity,
            inverted,
            ..
        } => {
            let base = fill_wallpaper(fill);
            Some(match image_path {
                Some(path) => Wallpaper::Pattern {
                    ink: pattern_ink(&fill.colors(), *intensity, *inverted),
                    base: Box::new(base),
                    path: PathBuf::from(path),
                },
                None => base,
            })
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

/// What one chat shows: its wallpaper (with the dark-mode dimming) and the
/// theme's outgoing bubble color.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct ChatLook {
    pub wallpaper: Option<Wallpaper>,
    /// 0-100, applied only in dark mode.
    pub dimming: u8,
    /// 0xRRGGBB behind outgoing bubbles, from the chat's emoji theme.
    pub outgoing_fill: Option<u32>,
}

/// The wallpaper of a Telegram background, reading the photo / pattern file
/// from the session's file cache.
pub(super) fn session_wallpaper(
    session: &quill::state::Session,
    background: &Background,
) -> Option<Wallpaper> {
    let path = background
        .file
        .as_ref()
        .and_then(|f| session.files.get(&f.id.0))
        .and_then(|f| f.usable_path());
    background_wallpaper(background, path)
}

/// Paint `wallpaper` (and the dark-mode dimming) behind `this`'s children.
pub(super) fn paint_wallpaper<T: Styled + ParentElement>(
    this: T,
    wallpaper: Option<&Wallpaper>,
    dimming: u8,
) -> T {
    fn fill<T: Styled>(this: T, wallpaper: &Wallpaper) -> T {
        match wallpaper {
            Wallpaper::Solid(color) => this.bg(rgb(*color)),
            Wallpaper::Gradient { top, bottom, angle } => this.bg(linear_gradient(
                *angle as f32,
                linear_color_stop(rgb(*top), 0.),
                linear_color_stop(rgb(*bottom), 1.),
            )),
            Wallpaper::Image { backdrop, .. } => this.bg(rgb(*backdrop)),
            Wallpaper::Pattern { base, .. } => fill(this, base),
        }
    }
    let Some(wallpaper) = wallpaper else {
        return this;
    };
    let mut this = fill(this, wallpaper);
    match wallpaper {
        Wallpaper::Image { path, .. } => {
            this = this.child(
                img(path.clone())
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .object_fit(ObjectFit::Cover),
            );
        }
        Wallpaper::Pattern { path, ink, .. } => {
            this = this.child(pattern_layer(path.clone(), *ink));
        }
        _ => {}
    }
    if dimming > 0 {
        this = this.child(
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .bg(gpui_kit::black().opacity(f32::from(dimming.min(100)) / 100.0)),
        );
    }
    this
}

impl QuillApp {
    /// A 56 px tile showing an installed background (fill, gradient, pattern
    /// or photo); the caller adds the click handler.
    pub(super) fn wallpaper_tile(
        &self,
        id: (&'static str, u64),
        background: &Background,
        selected: bool,
        cx: &App,
    ) -> Stateful<Div> {
        use gpui_kit::component::theme::ActiveTheme;
        let paint = self
            .session()
            .and_then(|s| session_wallpaper(s, background));
        let muted = cx.theme().muted_foreground;
        let tile = div()
            .id(id)
            .relative()
            .size(px(56.))
            .rounded_md()
            .overflow_hidden()
            .border_2()
            .border_color(if selected {
                cx.theme().primary
            } else {
                cx.theme().border
            })
            .role(gpui_kit::Role::Button)
            .aria_label(format!("Wallpaper {}", background.name))
            .tab_index(0)
            .cursor_pointer()
            .bg(cx.theme().muted);
        match paint {
            Some(paint) => paint_wallpaper(tile, Some(&paint), 0),
            None => tile.child(
                div()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        Icon::new(gpui_kit::assets::IconName::Image)
                            .size(px(18.))
                            .text_color(muted),
                    ),
            ),
        }
    }

    /// The account-wide wallpaper to paint now.
    pub(super) fn current_wallpaper(&self, cx: &App) -> Option<Wallpaper> {
        use gpui_kit::component::theme::ActiveTheme;
        let dark = cx.theme().is_dark();
        let telegram = self.session().and_then(|s| {
            let background = s.chats_state.default_backgrounds.get(&dark)?;
            session_wallpaper(s, background)
        });
        resolve_wallpaper(
            self.appearance.wallpaper_rgb,
            self.appearance.telegram_wallpaper,
            telegram,
        )
    }

    /// What `chat_id` shows: its own wallpaper, else its theme's, else the
    /// account-wide one.
    pub(super) fn chat_look(&self, chat_id: Option<i64>, cx: &App) -> ChatLook {
        use gpui_kit::component::theme::ActiveTheme;
        let dark = cx.theme().is_dark();
        let own = chat_id.and_then(|id| {
            let session = self.session()?;
            let (background, dimming) = session.chat_wallpaper(id, dark)?;
            let wallpaper = session_wallpaper(session, background)?;
            Some((wallpaper, dimming.clamp(0, 100) as u8))
        });
        let outgoing_fill = chat_id.and_then(|id| {
            Some(
                self.session()?
                    .chat_theme_settings(id, dark)?
                    .outgoing_bubble_color(),
            )
        });
        match own {
            Some((wallpaper, dimming)) => ChatLook {
                wallpaper: Some(wallpaper),
                dimming: if dark { dimming } else { 0 },
                outgoing_fill,
            },
            None => ChatLook {
                wallpaper: self.current_wallpaper(cx),
                dimming: 0,
                outgoing_fill,
            },
        }
    }

    /// The solid color video masks should blend into.
    pub(super) fn wallpaper_backdrop(&self, cx: &App) -> Hsla {
        use gpui_kit::component::theme::ActiveTheme;
        let chat = self.open_chat_id().map(|c| c.0);
        self.chat_look(chat, cx)
            .wallpaper
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
            background_wallpaper(&bg(pattern.clone()), None),
            Some(Wallpaper::Solid(9))
        );
        let drawn = background_wallpaper(&bg(pattern), Some("/tmp/p.tgv")).unwrap();
        assert_eq!(drawn.backdrop(), 9);
        assert!(matches!(drawn, Wallpaper::Pattern { .. }));
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
