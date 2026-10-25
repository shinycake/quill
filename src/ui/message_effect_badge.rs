//! The effect badge on a message sent with an effect, and the notes the
//! effect overlay reads (tdesktop `BottomInfo::paintEffect`,
//! `replayEffectLink` and `HistoryItem::hasUnwatchedEffect`).
//!
//! The badge is a small picture at the left end of the message's footer:
//! the effect's static icon, or its emoji when it has none. A click replays
//! the effect over the chat. The footer is built without an app handle, so
//! the badge leaves its click in a queue (`REPLAYS`) and wakes the app,
//! whose overlay (`message_effect_play`) starts the effect. The badge also
//! notes where it was painted, when all of it is on screen: the effect is
//! centred on it, and a message that has not played its effect yet
//! (`quill::message_effect_watch`) starts it the first time its badge
//! shows.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::nested_click::SwallowPress;
use gpui_kit::TestSupportExt;
use gpui_kit::*;
use quill::lang::{keys, tr};
use quill::local_path::sandboxed_display_path;
use quill::message_effects::MessageEffect;
use quill::state::{HistoryMessage, Session};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;

/// The chat id and message id of a message.
pub(super) type Key = (i64, i64);

/// `st::reactionInfoSize`: the badge's cell.
pub(super) const CELL: f32 = 15.0;
/// `st::effectInfoImage`: the picture inside the cell.
pub(super) const IMAGE: f32 = 12.0;

/// What the cell shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum BadgeIcon {
    /// The effect's static icon, downloaded.
    Image(PathBuf),
    /// The effect has no icon: its emoji.
    Emoji(String),
    /// The icon is still on its way, or the effect is not known yet.
    Waiting,
}

/// The footer's effect badge for one message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct FooterEffect {
    pub key: Key,
    pub effect_id: i64,
    pub outgoing: bool,
    /// The effect has not played for this message yet (tdesktop
    /// `hasUnwatchedEffect`).
    pub unwatched: bool,
    pub icon: BadgeIcon,
}

/// What the badge cell shows for `effect`: its static icon once
/// downloaded, its emoji when it has no icon (tdesktop `generateImage`).
pub(super) fn effect_icon(
    effect: &MessageEffect,
    files: &quill::state::FileMap,
    media_roots: &[PathBuf],
) -> BadgeIcon {
    match effect.icon_file() {
        Some(file) => files
            .get(&file.0)
            .and_then(|file| file.usable_path())
            .and_then(|path| sandboxed_display_path(path, media_roots))
            .map_or(BadgeIcon::Waiting, BadgeIcon::Image),
        None if effect.static_icon.is_none() && !effect.emoji.is_empty() => {
            BadgeIcon::Emoji(effect.emoji.clone())
        }
        None => BadgeIcon::Waiting,
    }
}

/// The footer badge of `message`, if it was sent with an effect.
pub(super) fn footer_effect(
    message: &HistoryMessage,
    session: Option<&Session>,
    files: &quill::state::FileMap,
    media_roots: &[PathBuf],
) -> Option<FooterEffect> {
    let effect_id = message.extras.effect_id;
    if effect_id == 0 {
        return None;
    }
    let key = (message.chat_id.0, message.id.0);
    let icon = session
        .and_then(|session| session.stickers.effects.get(effect_id))
        .map_or(BadgeIcon::Waiting, |effect| {
            effect_icon(effect, files, media_roots)
        });
    Some(FooterEffect {
        key,
        effect_id,
        outgoing: message.is_outgoing,
        unwatched: session.is_some_and(|session| session.stickers.effects.watch.is_unwatched(key)),
        icon,
    })
}

/// A badge as it was painted.
#[derive(Debug, Clone, Copy)]
pub(super) struct Seen {
    /// The picture's box, in window coordinates.
    pub bounds: Bounds<Pixels>,
    pub effect_id: i64,
    pub outgoing: bool,
    pub unwatched: bool,
}

thread_local! {
    static BADGES: RefCell<HashMap<Key, Seen>> = RefCell::new(HashMap::new());
    static REPLAYS: RefCell<Vec<Key>> = const { RefCell::new(Vec::new()) };
    static APP: RefCell<Option<WeakEntity<QuillApp>>> = const { RefCell::new(None) };
    /// The chat open in the main window, the only one effects play over.
    static MAIN_CHAT: std::cell::Cell<Option<i64>> = const { std::cell::Cell::new(None) };
}

/// Remember the app, so a click on a badge can wake it, and the chat open
/// in its main window: badges painted for other chats (a separate chat
/// window) are not noted.
pub(super) fn set_app(app: WeakEntity<QuillApp>, main_chat: Option<i64>) {
    APP.with(|slot| *slot.borrow_mut() = Some(app));
    MAIN_CHAT.with(|slot| slot.set(main_chat));
}

/// The history list is being rebuilt: forget where the badges were.
pub(super) fn begin_pass() {
    BADGES.with(|badges| badges.borrow_mut().clear());
}

pub(super) fn seen(key: Key) -> Option<Seen> {
    BADGES.with(|badges| badges.borrow().get(&key).copied())
}

/// The badge nearest to `near` (within `radius`) that shows `effect_id` on
/// the same side of `chat`, for an effect whose message got its final id
/// while it played.
pub(super) fn nearest(
    chat: i64,
    effect_id: i64,
    outgoing: bool,
    near: Point<Pixels>,
    radius: Pixels,
) -> Option<(Key, Seen)> {
    BADGES.with(|badges| {
        badges
            .borrow()
            .iter()
            .filter(|(key, badge)| {
                key.0 == chat && badge.effect_id == effect_id && badge.outgoing == outgoing
            })
            .map(|(key, badge)| {
                let dx = f32::from(badge.bounds.origin.x - near.x);
                let dy = f32::from(badge.bounds.origin.y - near.y);
                (*key, *badge, dx * dx + dy * dy)
            })
            .filter(|(_, _, distance)| *distance <= f32::from(radius).powi(2))
            .min_by(|a, b| a.2.total_cmp(&b.2))
            .map(|(key, badge, _)| (key, badge))
    })
}

/// Badges clicked since the last call.
pub(super) fn take_replays() -> Vec<Key> {
    REPLAYS.with(|replays| std::mem::take(&mut *replays.borrow_mut()))
}

/// Badges on screen whose message had not played its effect when they were
/// painted (tdesktop `startEffectOnRead`). Nothing counts as shown while
/// the window is not active (tdesktop `markingContentsRead`). The caller
/// marks each watched in the session, which decides whether it plays.
pub(super) fn shown_unwatched(window_active: bool) -> Vec<(Key, Seen)> {
    if !window_active {
        return Vec::new();
    }
    BADGES.with(|badges| {
        badges
            .borrow()
            .iter()
            .filter(|(_, badge)| badge.unwatched)
            .map(|(key, badge)| (*key, *badge))
            .collect()
    })
}

/// The badge's picture: the icon or the emoji, nothing while waiting.
pub(super) fn picture(icon: &BadgeIcon) -> Option<AnyElement> {
    match icon {
        BadgeIcon::Image(path) => Some(
            img(super::image_budget::sized_media(
                path,
                (px(IMAGE), px(IMAGE)),
                None,
                super::image_budget::Fit::Contain,
            ))
            .size(px(IMAGE))
            .object_fit(ObjectFit::Contain)
            .with_fallback(|| div().size(px(IMAGE)).into_any_element())
            .into_any_element(),
        ),
        BadgeIcon::Emoji(emoji) => Some(
            div()
                .text_size(px(IMAGE - 1.0))
                .line_height(px(IMAGE))
                .child(emoji.clone())
                .into_any_element(),
        ),
        BadgeIcon::Waiting => None,
    }
}

/// The badge, to go first in the footer row.
pub(super) fn badge(effect: &FooterEffect) -> AnyElement {
    let key = effect.key;
    let (effect_id, outgoing, unwatched) = (effect.effect_id, effect.outgoing, effect.unwatched);
    let tracker = canvas(
        |_, _, _| {},
        move |cell, _, window, _| {
            let image = Bounds::new(
                point(
                    cell.origin.x + (cell.size.width - px(IMAGE)) / 2.0,
                    cell.origin.y + (cell.size.height - px(IMAGE)) / 2.0,
                ),
                size(px(IMAGE), px(IMAGE)),
            );
            // Only a badge that shows whole counts as shown.
            if MAIN_CHAT.with(std::cell::Cell::get) != Some(key.0)
                || image.intersect(&window.content_mask().bounds) != image
            {
                return;
            }
            BADGES.with(|badges| {
                badges.borrow_mut().insert(
                    key,
                    Seen {
                        bounds: image,
                        effect_id,
                        outgoing,
                        unwatched,
                    },
                );
            });
            // The overlay already ran for this frame: ask for another to
            // start an effect that has not played.
            if unwatched {
                window.request_animation_frame();
            }
        },
    )
    .absolute()
    .inset_0()
    .size_full();
    div()
        .id(("message-effect", key.1 as u64))
        .test_support()
        .relative()
        .flex_none()
        .w(px(CELL))
        .h(px(CELL))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .role(Role::Button)
        .aria_label(tr(keys::Q_REST_REPLAY_THE_MESSAGE_EFFECT))
        .swallow_press()
        .on_click(move |_, _, cx| {
            REPLAYS.with(|replays| replays.borrow_mut().push(key));
            if let Some(app) = APP.with(|app| app.borrow().clone()) {
                let _ = app.update(cx, |_, cx| cx.notify());
            }
        })
        .children(picture(&effect.icon))
        .child(tracker)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::{BADGES, Seen, begin_pass, nearest, seen, shown_unwatched};
    use gpui_kit::{Bounds, point, px, size};

    #[test]
    fn nothing_counts_as_shown_while_the_window_is_inactive() {
        let badge = |unwatched| Seen {
            bounds: Bounds::default(),
            effect_id: 1,
            outgoing: false,
            unwatched,
        };
        BADGES.with(|badges| {
            let mut badges = badges.borrow_mut();
            badges.clear();
            badges.insert((5, 6), badge(true));
            badges.insert((5, 7), badge(false));
        });
        assert!(shown_unwatched(false).is_empty());
        let shown = shown_unwatched(true);
        assert_eq!(shown.len(), 1);
        assert_eq!(shown[0].0, (5, 6));
        begin_pass();
        assert!(seen((5, 6)).is_none());
    }

    #[test]
    fn a_message_that_got_its_final_id_is_found_by_position() {
        let at = |x: f32, y: f32| Bounds::new(point(px(x), px(y)), size(px(12.), px(12.)));
        let badge = |bounds, effect_id, outgoing| Seen {
            bounds,
            effect_id,
            outgoing,
            unwatched: false,
        };
        BADGES.with(|badges| {
            let mut badges = badges.borrow_mut();
            badges.clear();
            badges.insert((7, 100), badge(at(300., 400.), 55, true));
            badges.insert((7, 101), badge(at(300., 300.), 55, true));
            badges.insert((7, 102), badge(at(300., 405.), 56, true));
            badges.insert((8, 103), badge(at(300., 401.), 55, true));
            badges.insert((7, 104), badge(at(300., 402.), 55, false));
        });
        let found = nearest(7, 55, true, point(px(300.), px(398.)), px(80.));
        assert_eq!(found.map(|(key, _)| key), Some((7, 100)));
        assert!(nearest(7, 55, true, point(px(10.), px(10.)), px(80.)).is_none());
        begin_pass();
    }
}
