//! A live picture follows its feed. A stream's feed is keyed by chat
//! (`LiveKey::Stream`), so a rejoin or a restart keeps the key and gets a
//! new [`LiveSink`]; a live story's presenter switch gives a new one too.
//! A player shows one feed for life, so the host builds a new player for
//! the new feed and lets go of the old one (tdesktop re-chooses the large
//! tile's endpoint when its track ends, `GroupCall::setVideoEndpointLarge`).
//!
//! The stream window, the live story viewer and the live PiP all use this
//! one rule; see `docs/decisions/live-pip-feed-follow.md`.
#![deny(clippy::indexing_slicing)]

use super::{Purpose, VideoPlayer};
use gpui_kit::App;
use quill::calls::live_video::LiveSink;

/// Whether `player` shows `sink`.
pub(crate) fn shows_feed(player: &VideoPlayer, sink: &LiveSink) -> bool {
    player.live_sink().is_some_and(|own| own.same_as(sink))
}

/// A new player for `sink` when `current` does not show it already;
/// `None` when it does.
pub(crate) fn follow_live_feed(
    current: Option<&VideoPlayer>,
    sink: &LiveSink,
    cx: &mut App,
) -> Option<VideoPlayer> {
    if current.is_some_and(|player| shows_feed(player, sink)) {
        return None;
    }
    Some(VideoPlayer::live(
        sink.clone(),
        Purpose::Live { tile: false },
        cx,
    ))
}

/// A host's player slot follows `sink`: a new feed gets a new player (the
/// old one is released, its images leave every atlas); no feed, no
/// player. Returns whether the slot changed.
pub(crate) fn follow_live_slot(
    slot: &mut Option<VideoPlayer>,
    sink: Option<&LiveSink>,
    cx: &mut App,
) -> bool {
    let next = match sink {
        Some(sink) => match follow_live_feed(slot.as_ref(), sink, cx) {
            Some(player) => Some(player),
            None => return false,
        },
        None if slot.is_none() => return false,
        None => None,
    };
    if let Some(old) = std::mem::replace(slot, next) {
        old.release_live(cx);
    }
    true
}
