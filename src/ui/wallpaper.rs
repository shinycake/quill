//! The chat wallpaper: a preset color, the account's Telegram wallpaper or
//! a chat's own (tdesktop `boxes/background_box.cpp`,
//! `window/section_widget.cpp`'s `PaintBackground`, `ui/chat/chat_theme.cpp`).
//!
//! Solid colors paint as a plain fill. Everything else (gradients,
//! freeform gradients, patterns, photos with Blur or Tile) is rendered once
//! per area size on the wallpaper thread (`wallpaper_cache`) and painted
//! as one image.

use super::app::QuillApp;
use super::bubble_theme::{OutgoingFill, note_history_area};
use super::wallpaper_cache::{Slot, wallpaper_layer};
use gpui_kit::component::*;
use gpui_kit::*;
use quill::telegram::envelope::{Background, BackgroundFill, BackgroundType};
use std::path::PathBuf;

/// What paints behind the message list.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) enum Wallpaper {
    Solid(u32),
    /// A two-colour or freeform gradient, and the freeform rotation (in
    /// tdesktop's doubled steps; see `wallpaper_raster::next_rotation`).
    Fill(BackgroundFill, u16),
    /// A downloaded photo; `blurred` is Telegram's Blur, `tiled` the local
    /// "Tile background" choice.
    Image {
        path: PathBuf,
        blurred: bool,
        tiled: bool,
    },
    /// A PNG / TGV pattern mixed into a fill.
    Pattern {
        fill: BackgroundFill,
        path: PathBuf,
        /// 0-100; negative also means inverted.
        intensity: i32,
        inverted: bool,
        /// The fill's freeform rotation.
        rotation: u16,
    },
}

impl Wallpaper {
    /// One color standing for the whole wallpaper (shown while the image
    /// renders; video masks blend into it).
    pub(super) fn backdrop(&self) -> u32 {
        match self {
            Wallpaper::Solid(c) => *c,
            Wallpaper::Fill(fill, _) | Wallpaper::Pattern { fill, .. } => fill.average(),
            Wallpaper::Image { .. } => 0x202020,
        }
    }

    /// Whether sending turns this wallpaper (a freeform gradient, alone or
    /// under a pattern; tdesktop rotates only three or four colours).
    pub(super) fn rotates(&self) -> bool {
        match self {
            Wallpaper::Fill(BackgroundFill::Freeform(colors), _)
            | Wallpaper::Pattern {
                fill: BackgroundFill::Freeform(colors),
                ..
            } => colors.len() >= 3,
            _ => false,
        }
    }

    /// The same wallpaper at `rotation`.
    pub(super) fn with_rotation(self, rotation: u16) -> Self {
        match self {
            Wallpaper::Fill(fill, _) => Wallpaper::Fill(fill, rotation),
            Wallpaper::Pattern {
                fill,
                path,
                intensity,
                inverted,
                ..
            } => Wallpaper::Pattern {
                fill,
                path,
                intensity,
                inverted,
                rotation,
            },
            other => other,
        }
    }

    /// The wallpaper's identity, whatever its rotation.
    pub(super) fn identity(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::hash::DefaultHasher::new();
        self.clone().with_rotation(0).hash(&mut hasher);
        hasher.finish()
    }

    /// The wallpaper at the rotation its sends have reached.
    pub(super) fn with_stored_rotation(self) -> Self {
        if !self.rotates() {
            return self;
        }
        let rotation = ROTATIONS.with(|r| r.borrow().get(&self.identity()).copied());
        self.with_rotation(rotation.unwrap_or(0))
    }

    /// The same wallpaper with Blur on or off (photos only).
    pub(super) fn with_blur(self, blur: bool) -> Self {
        match self {
            Wallpaper::Image { path, tiled, .. } => Wallpaper::Image {
                path,
                blurred: blur,
                tiled,
            },
            other => other,
        }
    }
}

thread_local! {
    /// Freeform rotation per wallpaper identity (tdesktop keeps it on the
    /// chat theme, which the chats showing the same wallpaper share).
    static ROTATIONS: std::cell::RefCell<std::collections::HashMap<u64, u16>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

/// Turn `wallpaper` one step, as sending a message does
/// (`ChatTheme::rotateComplexGradientBackground`). False when it does not
/// rotate.
pub(super) fn rotate(wallpaper: &Wallpaper) -> bool {
    if !wallpaper.rotates() {
        return false;
    }
    ROTATIONS.with(|r| {
        let mut map = r.borrow_mut();
        let entry = map.entry(wallpaper.identity()).or_insert(0);
        *entry = quill::wallpaper_raster::next_rotation(*entry);
    });
    true
}

fn fill_wallpaper(fill: &BackgroundFill) -> Wallpaper {
    match fill {
        BackgroundFill::Solid(color) => Wallpaper::Solid(*color),
        BackgroundFill::Freeform(colors) if colors.len() == 1 => Wallpaper::Solid(colors[0]),
        other => Wallpaper::Fill(other.clone(), 0),
    }
}

/// What a Telegram background paints, given the local path of its photo or
/// pattern (`None` while it downloads: a photo is skipped until then, a
/// pattern shows its fill).
pub(super) fn background_wallpaper(
    background: &Background,
    image_path: Option<&str>,
) -> Option<Wallpaper> {
    match &background.kind {
        BackgroundType::Fill(fill) => Some(fill_wallpaper(fill)),
        BackgroundType::Pattern {
            fill,
            intensity,
            inverted,
            ..
        } => Some(match image_path {
            Some(path) => Wallpaper::Pattern {
                fill: fill.clone(),
                path: PathBuf::from(path),
                intensity: *intensity,
                inverted: *inverted,
                rotation: 0,
            },
            None => fill_wallpaper(fill),
        }),
        // `is_moving` is not drawn: tdesktop has no motion on desktop.
        BackgroundType::Wallpaper { blurred, .. } => image_path.map(|path| Wallpaper::Image {
            path: PathBuf::from(path),
            blurred: *blurred,
            tiled: false,
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
/// theme's outgoing bubble colors.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct ChatLook {
    pub wallpaper: Option<Wallpaper>,
    /// 0-100, applied only in dark mode and only to photos.
    pub dimming: u8,
    /// The chat's emoji theme: fill and accent of outgoing bubbles.
    pub outgoing: Option<OutgoingFill>,
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
        .and_then(|f| session.media.files.get(&f.id.0))
        .and_then(|f| f.usable_path());
    background_wallpaper(background, path)
}

/// What a gallery thumbnail shows: a photo from the document's small
/// thumbnail (never blurred, as in tdesktop's `BackgroundBox`), anything
/// else as it paints in the chat.
pub(super) fn thumbnail_wallpaper(
    session: &quill::state::Session,
    background: &Background,
) -> Option<Wallpaper> {
    if background.needs_image()
        && let Some(path) = background
            .thumbnail
            .as_ref()
            .and_then(|f| session.media.files.get(&f.id.0))
            .and_then(|f| f.usable_path())
    {
        return Some(Wallpaper::Image {
            path: PathBuf::from(path),
            blurred: false,
            tiled: false,
        });
    }
    session_wallpaper(session, background)
}

/// Paint `wallpaper` (and, for photos, the dark-mode dimming) behind
/// `this`'s children. `slot` names the area for the render cache.
pub(super) fn paint_wallpaper<T: Styled + ParentElement>(
    this: T,
    wallpaper: Option<&Wallpaper>,
    dimming: u8,
    slot: Slot,
) -> T {
    let history = matches!(slot, Slot::History(_));
    // The history reports its area every frame: gradient bubbles take
    // their colors from where they sit in it.
    let area_probe = || {
        canvas(|bounds, _, _| note_history_area(bounds), |_, _, _, _| {})
            .absolute()
            .top_0()
            .left_0()
            .size_full()
    };
    let Some(wallpaper) = wallpaper else {
        return if history {
            this.child(area_probe())
        } else {
            this
        };
    };
    let mut this = this.bg(rgb(wallpaper.backdrop()));
    this = match wallpaper {
        Wallpaper::Solid(_) if history => this.child(area_probe()),
        Wallpaper::Solid(_) => this,
        _ => this.child(wallpaper_layer(
            wallpaper.clone(),
            slot,
            history.then_some(note_history_area as fn(Bounds<Pixels>)),
        )),
    };
    if dimming > 0 && matches!(wallpaper, Wallpaper::Image { .. }) {
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
            .and_then(|s| thumbnail_wallpaper(s, background));
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
            Some(paint) => paint_wallpaper(tile, Some(&paint), 0, Slot::Thumb),
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

    /// The account's Telegram wallpaper for the current theme, with the
    /// local Tile choice applied to photos.
    pub(super) fn account_telegram_wallpaper(&self, dark: bool) -> Option<Wallpaper> {
        let wallpaper = self.session().and_then(|s| {
            let background = s.chats_state.default_backgrounds.get(&dark)?;
            session_wallpaper(s, background)
        })?;
        Some(match wallpaper {
            Wallpaper::Image { path, blurred, .. } => Wallpaper::Image {
                path,
                blurred,
                tiled: self.appearance.wallpaper_tile(dark),
            },
            other => other,
        })
    }

    /// The account-wide wallpaper to paint now.
    pub(super) fn current_wallpaper(&self, cx: &App) -> Option<Wallpaper> {
        use gpui_kit::component::theme::ActiveTheme;
        let dark = cx.theme().is_dark();
        resolve_wallpaper(
            self.appearance.wallpaper_rgb,
            self.appearance.telegram_wallpaper,
            self.account_telegram_wallpaper(dark),
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
        let outgoing = chat_id.and_then(|id| {
            Some(OutgoingFill::from_theme(
                self.session()?.chat_theme_settings(id, dark)?,
                dark,
            ))
        });
        match own {
            Some((wallpaper, dimming)) => ChatLook {
                wallpaper: Some(wallpaper.with_stored_rotation()),
                dimming: if dark { dimming } else { 0 },
                outgoing,
            },
            None => ChatLook {
                wallpaper: self
                    .current_wallpaper(cx)
                    .map(Wallpaper::with_stored_rotation),
                dimming: 0,
                outgoing,
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
    use super::{Wallpaper, background_wallpaper, resolve_wallpaper};
    use quill::telegram::envelope::{Background, BackgroundFill, BackgroundType};

    fn bg(kind: BackgroundType) -> Background {
        Background {
            id: 1,
            is_default: true,
            is_dark: false,
            name: "x".into(),
            file: None,
            thumbnail: None,
            kind,
        }
    }

    #[test]
    fn fills_map_to_paintable_wallpapers() {
        assert_eq!(
            background_wallpaper(&bg(BackgroundType::Fill(BackgroundFill::Solid(5))), None),
            Some(Wallpaper::Solid(5))
        );
        let gradient = BackgroundFill::Gradient {
            top: 1,
            bottom: 2,
            angle: 45,
        };
        assert_eq!(
            background_wallpaper(&bg(BackgroundType::Fill(gradient.clone())), None),
            Some(Wallpaper::Fill(gradient, 0))
        );
        let freeform = BackgroundFill::Freeform(vec![1, 2, 3, 4]);
        assert_eq!(
            background_wallpaper(&bg(BackgroundType::Fill(freeform.clone())), None),
            Some(Wallpaper::Fill(freeform, 0)),
            "all four colours reach the renderer"
        );
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
        assert!(matches!(drawn, Wallpaper::Pattern { intensity: 50, .. }));
        let photo = bg(BackgroundType::Wallpaper {
            blurred: true,
            moving: true,
        });
        assert_eq!(background_wallpaper(&photo, None), None);
        assert_eq!(
            background_wallpaper(&photo, Some("/tmp/a.jpg")),
            Some(Wallpaper::Image {
                path: "/tmp/a.jpg".into(),
                blurred: true,
                tiled: false,
            })
        );
        let sharp = background_wallpaper(&photo, Some("/tmp/a.jpg"))
            .unwrap()
            .with_blur(false);
        assert!(matches!(sharp, Wallpaper::Image { blurred: false, .. }));
    }

    #[test]
    fn freeform_wallpapers_rotate_per_identity() {
        use super::rotate;
        let four = Wallpaper::Fill(BackgroundFill::Freeform(vec![1, 2, 3, 4]), 0);
        let two = Wallpaper::Fill(
            BackgroundFill::Gradient {
                top: 1,
                bottom: 2,
                angle: 0,
            },
            0,
        );
        assert!(four.rotates() && !two.rotates());
        assert!(!rotate(&two));
        assert_eq!(four.clone().with_stored_rotation(), four);
        assert!(rotate(&four));
        let turned = four.clone().with_stored_rotation();
        assert_eq!(turned, four.clone().with_rotation(675));
        assert_eq!(turned.identity(), four.identity(), "same wallpaper");
        assert!(rotate(&turned), "the identity ignores the rotation");
        assert_eq!(four.clone().with_stored_rotation(), four.with_rotation(630));
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
}
