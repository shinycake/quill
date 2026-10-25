//! Playing message effects (tdesktop `EmojiInteractions::playEffect` and
//! `playEffectOnRead`).
//!
//! A message sent with an effect plays it over the chat when its badge first
//! shows while the effect has not played (`quill::message_effect_watch`:
//! an incoming message that was unread when it arrived, or one sent from
//! here), and again whenever the badge is clicked. Each window plays the
//! effects of the chat it shows, over its own history. The first kind is
//! skipped while effects are off in power saving; the message is then
//! marked as played all the same. A click always plays.
//!
//! The effect's description comes from `getMessageEffect` and its animation
//! from the sticker files it names. An effect that cannot play yet waits for
//! them. The animation is drawn by the shared overlay (`emoji_effect`),
//! centred on the badge and mirrored for messages on the left like the other
//! effects there.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::message_effect_badge::{self as badge, Key};
use gpui_kit::*;
use quill::emoji_interaction::EmojiEffect;
use quill::ids::{ChatId, MessageId};
use quill::message_effects::{EffectAnimation, MessageEffect};
use quill::power_saving::{self, Flag};
use quill::state::Session;
use quill::telegram::envelope::{StickerFormat, StickerItem};
use std::cell::RefCell;
use std::time::{Duration, Instant};

/// How long an effect waits for its description and animation. tdesktop
/// waits as long as the message is on screen; this only keeps a lost
/// answer from waiting for ever.
const WAIT: Duration = Duration::from_secs(30);

/// An effect that was asked for before it could play.
struct Waiting {
    key: Key,
    effect_id: i64,
    outgoing: bool,
    since: Instant,
}

thread_local! {
    static WAITING: RefCell<Vec<Waiting>> = const { RefCell::new(Vec::new()) };
}

/// The sticker the overlay plays for `effect`.
fn effect_sticker(animation: EffectAnimation) -> StickerItem {
    StickerItem {
        custom_emoji_id: None,
        id: 0,
        set_id: 0,
        emoji: String::new(),
        width: 0,
        height: 0,
        format: animation.format,
        file_id: animation.file,
        thumb_file_id: None,
        thumb_width: 0,
        thumb_height: 0,
        requires_premium: false,
        premium_animation: None,
    }
}

/// The animation of `effect`, if it names one that can be decoded.
fn playable(effect: &MessageEffect) -> Option<EffectAnimation> {
    effect
        .animation()
        .filter(|animation| matches!(animation.format, StickerFormat::Tgs | StickerFormat::Webm))
}

impl QuillApp {
    /// The chats some window shows now (main window and chat windows). The
    /// effect bookkeeping of any other chat goes.
    pub(super) fn chats_on_screen(&self) -> Vec<i64> {
        self.session()
            .map(|session| {
                session
                    .chats_shown()
                    .into_iter()
                    .map(|chat| chat.0)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Whether the window being built counts as active for its effects
    /// (tdesktop `markingContentsRead`): as its history last reported.
    pub(super) fn effects_window_active(&self) -> bool {
        self.history.window_active || super::frame_clock::assume_active()
    }

    fn effects_session_mut(&mut self) -> Option<&mut Session> {
        match self.live.as_mut() {
            Some(live) => Some(&mut live.driver.session),
            None => self.demo_session.as_deref_mut(),
        }
    }

    /// Start the message effects of the chat this window shows (its
    /// `viewed_chat`) asked for since the last frame, and those that were
    /// waiting for their description. Every window runs it for its own
    /// chat (tdesktop: one `EmojiInteractions` per history widget);
    /// `window_active` is this window's (`HistoryUi::window_active`).
    pub(super) fn message_effects_pass(&mut self, window_active: bool, cx: &mut Context<Self>) {
        badge::set_app(cx.entity().downgrade());
        let Some(open) = self.viewed_chat() else {
            return;
        };
        let shown = self.chats_on_screen();
        // First show of an unplayed effect (tdesktop `playEffectOnRead`):
        // marked as played whether or not power saving lets it play.
        let shown_now = badge::shown_unwatched(open.0, window_active);
        let marked: Vec<(Key, badge::Seen)> = match self.effects_session_mut() {
            Some(session) => shown_now
                .into_iter()
                .filter(|(key, _)| session.stickers.effects.watch.mark_watched(*key))
                .collect(),
            None => Vec::new(),
        };
        if !marked.is_empty() {
            // The badges drawn as unplayed stop asking for frames.
            self.notify_conversation(cx);
        }
        let automatic = marked
            .into_iter()
            .filter(|_| !power_saving::on(Flag::ChatEffects));
        let replays = badge::take_replays(open.0, &|chat| shown.contains(&chat))
            .into_iter()
            .filter_map(|key| badge::seen(key).map(|seen| (key, seen)));
        let requests: Vec<(Key, i64, bool)> = automatic
            .chain(replays)
            .map(|(key, seen)| (key, seen.effect_id, seen.outgoing))
            .collect();
        WAITING.with(|waiting| {
            let mut waiting = waiting.borrow_mut();
            for (key, effect_id, outgoing) in requests {
                waiting.push(Waiting {
                    key,
                    effect_id,
                    outgoing,
                    since: Instant::now(),
                });
            }
            waiting.retain(|w| shown.contains(&w.key.0) && w.since.elapsed() < WAIT);
        });
        let ids: Vec<i64> = WAITING.with(|waiting| {
            waiting
                .borrow()
                .iter()
                .filter(|w| w.key.0 == open.0)
                .map(|w| w.effect_id)
                .collect()
        });
        if ids.is_empty() {
            return;
        }
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.load_message_effects(ids);
        }
        let Some(session) = self.session() else {
            return;
        };
        // Only a badge on screen plays (tdesktop returns for a view that
        // is not visible).
        let ready: Vec<(Key, i64, bool, EffectAnimation)> = WAITING.with(|waiting| {
            let mut waiting = waiting.borrow_mut();
            let mut ready = Vec::new();
            waiting.retain(|w| {
                if w.key.0 != open.0 || badge::seen(w.key).is_none() {
                    return true;
                }
                match session.stickers.effects.get(w.effect_id) {
                    Some(effect) => {
                        if let Some(animation) = playable(effect) {
                            ready.push((w.key, w.effect_id, w.outgoing, animation));
                        }
                        false
                    }
                    // Still being looked up, unless it is gone for good.
                    None => !session.stickers.effects.is_gone(w.effect_id),
                }
            });
            ready
        });
        for (key, effect_id, outgoing, animation) in ready {
            super::emoji_effect::push_message(
                EmojiEffect {
                    chat_id: ChatId(key.0),
                    message_id: MessageId(key.1),
                    sticker: effect_sticker(animation),
                    incoming: false,
                },
                outgoing,
                effect_id,
                WAIT,
                cx.background_executor().now(),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{effect_sticker, playable};
    use quill::ids::FileId;
    use quill::message_effects::{EffectAnimation, EffectKind, MessageEffect};
    use quill::telegram::envelope::StickerFormat;

    #[test]
    fn the_overlay_sticker_carries_the_animation_file() {
        let sticker = effect_sticker(EffectAnimation {
            file: FileId(33),
            format: StickerFormat::Tgs,
        });
        assert_eq!(sticker.file_id, FileId(33));
        assert_eq!(sticker.format, StickerFormat::Tgs);
    }

    #[test]
    fn an_effect_without_an_animation_cannot_play() {
        let effect = MessageEffect {
            id: 1,
            static_icon: None,
            emoji: String::new(),
            is_premium: false,
            kind: EffectKind::Sticker { sticker: None },
        };
        assert!(playable(&effect).is_none());
    }
}
