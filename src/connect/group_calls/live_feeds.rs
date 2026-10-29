//! Connect driver: one live feed per group-call track, per stream and
//! per 1:1 call picture.
//!
//! The engine hook pushes decoded I420 planes into a [`LiveSource`] on an
//! engine thread; the UI holds the matching [`LiveSink`] in a live
//! player (`ui/video_player/backend_live.rs`). Nothing is converted here.
//!
//! A stream's feed exists only while its feeder runs, so a picture that
//! arrives between the drain and the native stop is dropped instead of
//! bringing the feed back. A participant's camera or screen, and your own
//! camera, get a feed when the UI asks for one while the tracked call
//! has that video on; the engine hook only fills feeds that exist. When
//! TDLib reports the video off (or the call left), the feed ends and goes
//! ([`ConnectDriver::sync_call_feeds`]), so turning it on again starts
//! from a new feed with no stale frame. A 1:1 call's pictures (the peer's
//! camera and screen share, your camera preview) follow the same rule
//! against the tracked 1:1 call ([`ConnectDriver::sync_one_to_one_feeds`]).
use super::*;
use crate::calls::engine::RemoteVideoState;
use crate::calls::engine::{LiveFrameCallback, StreamState};
use crate::calls::live_video::{LiveEnd, LiveFrame, LiveKey, LiveSink, LiveSource, LiveState};
use crate::state::{ActiveCall, ActiveGroupCall};
use crate::telegram::envelope::MessageSender;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// The driver's live feeds, shared with the engine hook.
pub(crate) type LiveFeeds = Arc<Mutex<HashMap<LiveKey, LiveSource>>>;

/// The engine hook: store the frame in its feed, if it has one, and wake
/// the player. A frame for a track nobody asked for (or one already
/// turned off) is dropped: feeds are made only for the tracked call.
pub(crate) fn live_frame_hook(feeds: LiveFeeds) -> LiveFrameCallback {
    Arc::new(move |key, frame| {
        // The source is cloned and the map unlocked before the push: the
        // push wakes the UI, and the waker must never run under this lock.
        let source = feeds.lock_or_recover().get(&key).cloned();
        if let Some(source) = source {
            source.push(frame);
        }
    })
}

/// Whether `call` has the video `key` names on: your camera, or a
/// participant's camera or screen (yours from when you start sharing,
/// for Quill's own preview). Streams have their own feeder.
pub(crate) fn call_has_video(call: &ActiveGroupCall, key: LiveKey) -> bool {
    match key {
        LiveKey::Stream { .. } | LiveKey::Call { .. } => false,
        LiveKey::Local { call_id } => call.id == call_id && call.is_my_video_enabled,
        LiveKey::Video {
            call_id,
            user_id,
            screen,
        } => {
            call.id == call_id
                && call.participants.iter().any(|participant| {
                    participant.participant_id == MessageSender::User { user_id }
                        && if screen {
                            participant.screen_sharing_enabled
                                || (participant.is_current_user
                                    && (call.screen_sharing || call.screen_share_pending))
                        } else {
                            participant.video_enabled
                        }
                })
        }
    }
}

/// Whether the 1:1 `call` has the picture `key` names on: your camera,
/// the peer's camera or the peer's screen share (not while TDLib or the
/// engine reports it inactive).
pub(crate) fn one_to_one_has_video(call: &ActiveCall, key: LiveKey) -> bool {
    let LiveKey::Call {
        call_id,
        local,
        screen,
    } = key
    else {
        return false;
    };
    call.id == call_id
        && match (local, screen) {
            (true, _) => call.camera_on,
            (false, false) => call.remote_video != RemoteVideoState::Inactive,
            (false, true) => call.remote_screen != RemoteVideoState::Inactive,
        }
}

/// What a stream's state means for its picture.
fn stream_feed_change(state: StreamState) -> Option<Result<LiveState, LiveEnd>> {
    match state {
        StreamState::Live => Some(Ok(LiveState::Active)),
        StreamState::NoStream => Some(Ok(LiveState::Inactive)),
        StreamState::Ended => Some(Err(LiveEnd::Over)),
        StreamState::Connecting => None,
    }
}

impl<S: JsonSender> ConnectDriver<S> {
    /// The UI side of `key`'s feed. A stream's feed exists while its
    /// feeder runs; a track's feed is made on demand while the tracked
    /// call has that video on (`None` for any other call or a video that
    /// is off, so a stale id leaves nothing behind).
    pub fn live_sink(&self, key: LiveKey) -> Option<LiveSink> {
        self.feed_source(key).map(|source| source.sink())
    }

    fn feed_source(&self, key: LiveKey) -> Option<LiveSource> {
        let calls = &self.session.calls;
        let tracked = key.call_id().is_some_and(|call_id| {
            calls.left_group_call != Some(call_id)
                && calls
                    .group_call(call_id)
                    .is_some_and(|call| call_has_video(call, key))
        }) || calls
            .active_call
            .as_ref()
            .is_some_and(|call| one_to_one_has_video(call, key));
        let mut feeds = self.live_feeds.lock_or_recover();
        if let Some(source) = feeds.get(&key) {
            return Some(source.clone());
        }
        if !tracked {
            return None;
        }
        Some(
            feeds
                .entry(key)
                .or_insert_with(|| LiveSink::new().1)
                .clone(),
        )
    }

    /// A stream feeder started: open its picture's feed.
    pub(super) fn open_stream_feed(&self, chat_id: i64) {
        let key = LiveKey::Stream { chat_id };
        let old = self
            .live_feeds
            .lock_or_recover()
            .insert(key, LiveSink::new().1);
        if let Some(old) = old {
            old.end(LiveEnd::Left);
        }
    }

    /// A stream feeder stopped: end and drop its picture's feed.
    pub(super) fn close_stream_feed(&self, chat_id: i64) {
        let old = self
            .live_feeds
            .lock_or_recover()
            .remove(&LiveKey::Stream { chat_id });
        if let Some(old) = old {
            old.end(LiveEnd::Left);
        }
    }

    /// The call ended, was left or replaced: end and drop its tracks'
    /// feeds, so no stale picture can render.
    pub(super) fn close_call_feeds(&self, group_call_id: i32) {
        let mut ended = Vec::new();
        self.live_feeds.lock_or_recover().retain(|key, source| {
            let keep = key.call_id() != Some(group_call_id);
            if !keep {
                ended.push(source.clone());
            }
            keep
        });
        for source in ended {
            source.end(LiveEnd::Left);
        }
    }

    /// After every reducer update: end and drop the tracked call's track
    /// feeds whose video TDLib reports off (all of them once the call is
    /// left). The player sees the end; a feed asked for again later is a
    /// new one, so no frame from before shows when the video comes back.
    pub(super) fn sync_call_feeds(&self, group_call_id: i32) {
        let calls = &self.session.calls;
        let call = calls
            .group_call(group_call_id)
            .filter(|call| calls.left_group_call != Some(call.id));
        let mut ended = Vec::new();
        self.live_feeds.lock_or_recover().retain(|key, source| {
            let ours =
                key.call_id() == Some(group_call_id) && !matches!(key, LiveKey::Stream { .. });
            let keep = !ours || call.is_some_and(|call| call_has_video(call, *key));
            if !keep {
                ended.push(source.clone());
            }
            keep
        });
        for source in ended {
            source.end(LiveEnd::Left);
        }
        // Open the feeds of others' video now, not when a tile first
        // asks: a static screen share may send its only frame before the
        // tile draws, and the hook drops frames that have no feed.
        let Some(call) = call else {
            return;
        };
        for participant in &call.participants {
            let MessageSender::User { user_id } = participant.participant_id else {
                continue;
            };
            if participant.is_current_user {
                continue;
            }
            for screen in [false, true] {
                let key = LiveKey::Video {
                    call_id: group_call_id,
                    user_id,
                    screen,
                };
                if call_has_video(call, key) {
                    let _ = self.feed_source(key);
                }
            }
        }
    }

    /// After every engine pump and camera toggle: end and drop the 1:1
    /// call pictures' feeds whose video is off or whose call is no longer
    /// the tracked one (all of them once the call ends), so a picture
    /// turned on again starts from a new feed.
    pub(crate) fn sync_one_to_one_feeds(&self) {
        let call = self.session.calls.active_call.as_ref();
        let mut ended = Vec::new();
        self.live_feeds.lock_or_recover().retain(|key, source| {
            let keep = key.one_to_one_call().is_none()
                || call.is_some_and(|call| one_to_one_has_video(call, *key));
            if !keep {
                ended.push(source.clone());
            }
            keep
        });
        for source in ended {
            source.end(LiveEnd::Left);
        }
        // The peer's pictures open as soon as they are on, for the same
        // reason as a group call's (`sync_call_feeds`).
        if let Some(call) = call {
            for screen in [false, true] {
                let key = LiveKey::Call {
                    call_id: call.id,
                    local: false,
                    screen,
                };
                if one_to_one_has_video(call, key) {
                    let _ = self.feed_source(key);
                }
            }
        }
    }

    /// Mirror a stream's state onto its picture's feed.
    pub(super) fn sync_stream_feed(&self, chat_id: i64, state: StreamState) {
        let Some(change) = stream_feed_change(state) else {
            return;
        };
        let source = self
            .live_feeds
            .lock_or_recover()
            .get(&LiveKey::Stream { chat_id })
            .cloned();
        let Some(source) = source else {
            return;
        };
        match change {
            Ok(state) => source.set_state(state),
            Err(reason) => source.end(reason),
        }
    }

    /// Push a frame into `key`'s feed, making it while the tracked call
    /// has that video on (your own screen preview, which Quill captures
    /// itself).
    pub(crate) fn push_live_frame(&self, key: LiveKey, frame: LiveFrame) {
        if let Some(source) = self.feed_source(key) {
            source.push(frame);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_states_map_to_feed_changes() {
        assert_eq!(
            stream_feed_change(StreamState::Live),
            Some(Ok(LiveState::Active))
        );
        assert_eq!(
            stream_feed_change(StreamState::NoStream),
            Some(Ok(LiveState::Inactive))
        );
        assert_eq!(
            stream_feed_change(StreamState::Ended),
            Some(Err(LiveEnd::Over))
        );
        assert_eq!(stream_feed_change(StreamState::Connecting), None);
    }
}
