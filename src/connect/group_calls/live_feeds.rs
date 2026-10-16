//! Connect driver: one live feed per group-call track and per stream.
//!
//! The engine hook pushes decoded I420 planes into a [`LiveSource`] on an
//! ntgcalls thread; the UI holds the matching [`LiveSink`] in a live
//! player (`ui/video_player/backend_live.rs`). Nothing is converted here.
//!
//! A stream's feed exists only while its feeder runs, so a picture that
//! arrives between the drain and the native stop is dropped instead of
//! bringing the feed back. A participant's camera or screen, and your own
//! camera, get a feed on the first frame or when the UI asks for one.
use super::*;
use crate::calls::engine::{LiveFrameCallback, StreamState};
use crate::calls::live_video::{LiveEnd, LiveFrame, LiveKey, LiveSink, LiveSource, LiveState};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// The driver's live feeds, shared with the engine hook.
pub(crate) type LiveFeeds = Arc<Mutex<HashMap<LiveKey, LiveSource>>>;

/// The engine hook: store the frame in its feed and wake the player.
pub(crate) fn live_frame_hook(feeds: LiveFeeds) -> LiveFrameCallback {
    Arc::new(move |key, frame| {
        // The source is cloned and the map unlocked before the push: the
        // push wakes the UI, and the waker must never run under this lock.
        let source = {
            let mut feeds = feeds.lock_or_recover();
            match key {
                LiveKey::Stream { .. } => feeds.get(&key).cloned(),
                LiveKey::Video { .. } | LiveKey::Local { .. } => Some(
                    feeds
                        .entry(key)
                        .or_insert_with(|| LiveSink::new().1)
                        .clone(),
                ),
            }
        };
        if let Some(source) = source {
            source.push(frame);
        }
    })
}

/// What a stream's state means for its picture.
fn stream_feed_change(state: StreamState) -> Option<Result<LiveState, LiveEnd>> {
    match state {
        StreamState::Live => Some(Ok(LiveState::Active)),
        StreamState::NoStream => Some(Ok(LiveState::Inactive)),
        StreamState::Ended => Some(Err(LiveEnd::Over)),
        StreamState::Connecting | StreamState::Unsupported => None,
    }
}

impl<S: JsonSender> ConnectDriver<S> {
    /// The UI side of `key`'s feed. A stream's feed exists while its
    /// feeder runs; a track's feed is made on demand for the tracked call
    /// until you leave it (`None` for any other call, so a stale id
    /// leaves nothing behind).
    pub fn live_sink(&self, key: LiveKey) -> Option<LiveSink> {
        self.feed_source(key).map(|source| source.sink())
    }

    fn feed_source(&self, key: LiveKey) -> Option<LiveSource> {
        let calls = &self.session.calls;
        let tracked = key.call_id().is_some_and(|call_id| {
            calls.left_group_call != Some(call_id)
                && calls
                    .active_group_call
                    .as_ref()
                    .is_some_and(|call| call.id == call_id)
        });
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

    /// Push a frame into `key`'s feed, making it for the tracked call
    /// (your own screen preview, which Quill captures itself).
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
        assert_eq!(stream_feed_change(StreamState::Unsupported), None);
    }
}
