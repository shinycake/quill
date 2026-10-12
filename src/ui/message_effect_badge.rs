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
//! centred on it, and a message that has not played its effect yet starts
//! it the first time its badge shows.

use super::app::QuillApp;
use super::nested_click::SwallowPress;
use gpui_kit::*;
use quill::local_path::sandboxed_display_path;
use quill::state::{HistoryMessage, Session};
use quill::telegram::envelope::ParsedFile;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

/// The chat id and message id of a message.
pub(super) type Key = (i64, i64);

/// `st::reactionInfoSize`: the badge's cell.
pub(super) const CELL: f32 = 15.0;
/// `st::effectInfoImage`: the picture inside the cell.
const IMAGE: f32 = 12.0;
/// Messages remembered as having played, at most.
const MAX_WATCHED: usize = 16_384;

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
    /// The effect has not played for this message yet: an incoming message
    /// that is still unread, or one just sent from here (tdesktop
    /// `hasUnwatchedEffect`).
    pub unwatched: bool,
    pub icon: BadgeIcon,
}

/// tdesktop `hasUnwatchedEffect` for a message with an effect: an incoming
/// message that is still unread, or one just sent from here. A message that
/// came from the server already read or sent (also from another device) has
/// played.
fn unwatched(outgoing: bool, pending: bool, id: i64, last_read_inbox: i64) -> bool {
    if outgoing {
        pending
    } else {
        id > last_read_inbox
    }
}

/// The footer badge of `message`, if it was sent with an effect.
pub(super) fn footer_effect(
    message: &HistoryMessage,
    session: Option<&Session>,
    files: &HashMap<i32, ParsedFile>,
    media_roots: &[PathBuf],
) -> Option<FooterEffect> {
    let effect_id = message.extras.effect_id;
    if effect_id == 0 {
        return None;
    }
    let last_read = session
        .and_then(|session| session.chats.get(&message.chat_id.0))
        .map_or(0, |chat| chat.last_read_inbox_message_id.0);
    let icon = session
        .and_then(|session| session.stickers.effects.get(effect_id))
        .map_or(BadgeIcon::Waiting, |effect| match effect.icon_file() {
            Some(file) => files
                .get(&file.0)
                .and_then(|file| file.usable_path())
                .and_then(|path| sandboxed_display_path(path, media_roots))
                .map_or(BadgeIcon::Waiting, BadgeIcon::Image),
            None if effect.static_icon.is_none() && !effect.emoji.is_empty() => {
                BadgeIcon::Emoji(effect.emoji.clone())
            }
            None => BadgeIcon::Waiting,
        });
    Some(FooterEffect {
        key: (message.chat_id.0, message.id.0),
        effect_id,
        outgoing: message.is_outgoing,
        unwatched: unwatched(
            message.is_outgoing,
            message.pending,
            message.id.0,
            last_read,
        ),
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

#[derive(Default)]
struct Watched {
    keys: HashSet<Key>,
}

impl Watched {
    fn contains(&self, key: Key) -> bool {
        self.keys.contains(&key)
    }

    fn insert(&mut self, key: Key) {
        if self.keys.len() >= MAX_WATCHED {
            self.keys.clear();
        }
        self.keys.insert(key);
    }
}

thread_local! {
    static BADGES: RefCell<HashMap<Key, Seen>> = RefCell::new(HashMap::new());
    static WATCHED: RefCell<Watched> = RefCell::new(Watched::default());
    static REPLAYS: RefCell<Vec<Key>> = const { RefCell::new(Vec::new()) };
    static APP: RefCell<Option<WeakEntity<QuillApp>>> = const { RefCell::new(None) };
}

/// Remember the app, so a click on a badge can wake it.
pub(super) fn set_app(app: WeakEntity<QuillApp>) {
    APP.with(|slot| *slot.borrow_mut() = Some(app));
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

/// Messages whose effect should play now because their badge showed for
/// the first time while unwatched (tdesktop `playEffectOnRead`). Each is
/// marked watched whether or not it plays: with effects off in power saving
/// it never plays by itself afterwards. Nothing starts while the window is
/// not active (tdesktop `markingContentsRead`).
pub(super) fn take_unwatched(window_active: bool) -> Vec<(Key, Seen)> {
    if !window_active {
        return Vec::new();
    }
    let candidates: Vec<(Key, Seen)> = BADGES.with(|badges| {
        badges
            .borrow()
            .iter()
            .filter(|(key, badge)| badge.unwatched && !WATCHED.with(|w| w.borrow().contains(**key)))
            .map(|(key, badge)| (*key, *badge))
            .collect()
    });
    WATCHED.with(|watched| {
        let mut watched = watched.borrow_mut();
        for (key, _) in &candidates {
            watched.insert(*key);
        }
    });
    candidates
}

/// Whether `key` still has an effect that has not played.
fn is_watched(key: Key) -> bool {
    WATCHED.with(|watched| watched.borrow().contains(key))
}

fn picture(icon: &BadgeIcon) -> Option<AnyElement> {
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
            if image.intersect(&window.content_mask().bounds) != image {
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
            if unwatched && !is_watched(key) {
                window.request_animation_frame();
            }
        },
    )
    .absolute()
    .inset_0()
    .size_full();
    div()
        .id(("message-effect", key.1 as u64))
        .relative()
        .flex_none()
        .w(px(CELL))
        .h(px(CELL))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .role(Role::Button)
        .aria_label("Replay the message effect")
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
    use super::{
        BADGES, MAX_WATCHED, Seen, Watched, begin_pass, nearest, seen, take_unwatched, unwatched,
    };
    use gpui_kit::{Bounds, point, px, size};

    #[test]
    fn only_unread_incoming_and_just_sent_messages_have_unplayed_effects() {
        // Incoming: unread past the read marker.
        assert!(unwatched(false, false, 200, 100));
        assert!(!unwatched(false, false, 100, 100));
        assert!(!unwatched(false, false, 50, 100));
        // Outgoing: only while it is being sent from here.
        assert!(unwatched(true, true, -3, 100));
        assert!(!unwatched(true, false, 300, 100));
    }

    #[test]
    fn watched_keys_are_remembered_and_the_set_is_bounded() {
        let mut watched = Watched::default();
        assert!(!watched.contains((1, 2)));
        watched.insert((1, 2));
        assert!(watched.contains((1, 2)));
        for message in 0..MAX_WATCHED as i64 + 1 {
            watched.insert((9, message));
        }
        assert!(watched.keys.len() <= MAX_WATCHED);
    }

    #[test]
    fn nothing_starts_while_the_window_is_inactive() {
        BADGES.with(|badges| {
            badges.borrow_mut().insert(
                (5, 6),
                Seen {
                    bounds: Bounds::default(),
                    effect_id: 1,
                    outgoing: false,
                    unwatched: true,
                },
            );
        });
        assert!(take_unwatched(false).is_empty());
        let started = take_unwatched(true);
        assert_eq!(started.len(), 1);
        assert_eq!(started[0].0, (5, 6));
        // Once started it is watched: not again.
        assert!(take_unwatched(true).is_empty());
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
