//! Pushed video frames: streams, live stories, video chat tiles and 1:1
//! calls.
//!
//! tgcalls hands decoded I420 planes to the engine thread. A [`LiveSource`]
//! (the engine side, `Send`) keeps only the newest one in a single slot
//! under a mutex; a later frame replaces one the UI has not shown yet, and
//! the replaced count is kept for the trace. The UI side, a [`LiveSink`],
//! takes the frame when it draws and converts it once into the platform's
//! picture (`ui/video_player/backend_live.rs`). Nothing is converted on the
//! engine thread.
//!
//! The UI is told about a new frame through a one-shot waker it arms after
//! each wake ([`LiveSink::arm`]); a push, state change or end with no
//! waker armed is remembered and fires the next one at once, so no frame
//! is missed between two arms.
//!
//! Design: `quill-tools/designs/video-player.md`, "Live video (pushed
//! frames)"; decision doc `docs/decisions/video-player-live.md`.
#![deny(clippy::indexing_slicing)]

pub mod convert;

use crate::locks::LockExt;
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// How the sender turned the picture (WebRTC's `VideoRotation`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LiveRotation {
    #[default]
    None,
    R90,
    R180,
    R270,
}

impl LiveRotation {
    /// A quarter turn: the shown picture's sides are swapped.
    pub fn swaps_sides(self) -> bool {
        matches!(self, Self::R90 | Self::R270)
    }
}

/// One decoded frame as the engine delivered it: I420 planes (Y `w × h`,
/// then U and V `⌈w/2⌉ × ⌈h/2⌉`), not yet turned.
#[derive(Debug, PartialEq, Eq)]
pub struct LiveFrame {
    pub seq: u64,
    pub width: u16,
    pub height: u16,
    pub rotation: LiveRotation,
    pub planes: Vec<u8>,
    pub arrived_at: Instant,
}

impl LiveFrame {
    /// A frame whose planes hold exactly `width × height` I420 pixels;
    /// `None` for an empty or short buffer (never shown as garbage).
    pub fn new(
        seq: u64,
        width: u16,
        height: u16,
        rotation: LiveRotation,
        planes: Vec<u8>,
        arrived_at: Instant,
    ) -> Option<Self> {
        if width == 0 || height == 0 || planes.len() != convert::i420_size(width, height) {
            return None;
        }
        Some(Self {
            seq,
            width,
            height,
            rotation,
            planes,
            arrived_at,
        })
    }

    /// The planes as slices.
    pub fn i420(&self) -> Option<convert::I420<'_>> {
        convert::I420::new(&self.planes, self.width, self.height)
    }

    /// The picture's size once turned.
    pub fn shown_size(&self) -> (u32, u32) {
        let (w, h) = (u32::from(self.width), u32::from(self.height));
        if self.rotation.swaps_sides() {
            (h, w)
        } else {
            (w, h)
        }
    }
}

/// What the sender's video is doing (`RemoteVideoState`, or a
/// participant's `is_paused`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LiveState {
    Active,
    /// The sender paused: the last picture stays, blurred.
    Paused,
    /// No video from the sender (camera off, nobody broadcasting).
    Inactive,
}

/// Why a live feed ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LiveEnd {
    /// The stream or the track is over.
    Over,
    /// You left the call.
    Left,
    /// The engine failed.
    Failed(String),
}

/// Called once, from any thread, when there is something new to draw.
pub type LiveWaker = Box<dyn FnOnce() + Send>;

#[derive(Default)]
struct Latest {
    frame: Option<Arc<LiveFrame>>,
    /// `frame` was not taken by the UI yet.
    unshown: bool,
    pushed: u64,
    /// Frames replaced before the UI took them.
    dropped: u64,
    state: Option<LiveState>,
    end: Option<LiveEnd>,
    waker: Option<LiveWaker>,
    /// Something changed while no waker was armed.
    wake_pending: bool,
}

impl Latest {
    /// The waker to call once the lock is released, or a note for the
    /// next arm.
    fn wake(&mut self) -> Option<LiveWaker> {
        let waker = self.waker.take();
        if waker.is_none() {
            self.wake_pending = true;
        }
        waker
    }
}

type Shared = Arc<Mutex<Latest>>;

/// The engine side of a live feed: `Send` and cheap to clone.
#[derive(Clone)]
pub struct LiveSource(Shared);

/// The UI side of a live feed.
#[derive(Clone)]
pub struct LiveSink(Shared);

/// The feed's state as the UI reads it on each pull.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LiveSnapshot {
    /// `None` until the sender's state was set.
    pub state: Option<LiveState>,
    pub end: Option<LiveEnd>,
    pub pushed: u64,
    pub dropped: u64,
    /// A frame is waiting to be taken.
    pub unshown: bool,
    /// The newest frame's sequence number and arrival, taken or not.
    pub latest: Option<(u64, Instant)>,
}

impl LiveSink {
    /// A new feed: the sink for the UI, the source for the engine.
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> (LiveSink, LiveSource) {
        let shared: Shared = Arc::default();
        (LiveSink(shared.clone()), LiveSource(shared))
    }

    /// The newest frame the UI has not taken yet.
    pub fn take(&self) -> Option<Arc<LiveFrame>> {
        self.take_if(|_| true)
    }

    /// The newest frame not taken yet, when `wanted` says so; else it
    /// stays for a later take.
    pub fn take_if(&self, wanted: impl FnOnce(&LiveFrame) -> bool) -> Option<Arc<LiveFrame>> {
        let mut latest = self.0.lock_or_recover();
        if !latest.unshown {
            return None;
        }
        let frame = latest.frame.clone().filter(|frame| wanted(frame))?;
        latest.unshown = false;
        Some(frame)
    }

    /// The newest frame, taken or not.
    pub fn latest(&self) -> Option<Arc<LiveFrame>> {
        self.0.lock_or_recover().frame.clone()
    }

    pub fn snapshot(&self) -> LiveSnapshot {
        let latest = self.0.lock_or_recover();
        LiveSnapshot {
            state: latest.state,
            end: latest.end.clone(),
            pushed: latest.pushed,
            dropped: latest.dropped,
            unshown: latest.unshown,
            latest: latest
                .frame
                .as_ref()
                .map(|frame| (frame.seq, frame.arrived_at)),
        }
    }

    /// Wake the armed waker (or the next arm) though nothing changed:
    /// the player came back into sight and takes the waiting frame.
    pub fn poke(&self) {
        let waker = self.0.lock_or_recover().wake();
        if let Some(waker) = waker {
            waker();
        }
    }

    /// Call `waker` once there is something new: at once when something
    /// changed since the last wake, else on the next push, state change
    /// or end. Replaces a waker armed before.
    pub fn arm(&self, waker: LiveWaker) {
        let now = {
            let mut latest = self.0.lock_or_recover();
            if std::mem::take(&mut latest.wake_pending) {
                Some(waker)
            } else {
                latest.waker = Some(waker);
                None
            }
        };
        if let Some(waker) = now {
            waker();
        }
    }

    /// Fire the armed waker (a picture converted off the UI thread is
    /// ready), or remember it for the next arm.
    pub fn wake(&self) {
        let waker = self.0.lock_or_recover().wake();
        if let Some(waker) = waker {
            waker();
        }
    }

    /// The sender's state as the UI knows it (a participant's
    /// `is_paused` comes with TDLib's participant updates, not from the
    /// engine).
    pub fn set_state(&self, state: LiveState) {
        set_state(&self.0, state);
    }

    /// Whether `other` is the same feed.
    pub fn same_as(&self, other: &LiveSink) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl LiveSource {
    /// Keep `frame` as the newest; one the UI has not taken is replaced.
    /// The new frame is allocated and the old one freed (a few MB for a
    /// 1080p frame) outside the lock the UI thread takes.
    pub fn push(&self, frame: LiveFrame) {
        let frame = Some(Arc::new(frame));
        let (waker, _replaced) = {
            let mut latest = self.0.lock_or_recover();
            if latest.end.is_some() {
                return;
            }
            if latest.unshown {
                latest.dropped = latest.dropped.saturating_add(1);
            }
            let replaced = std::mem::replace(&mut latest.frame, frame);
            latest.unshown = true;
            latest.pushed = latest.pushed.saturating_add(1);
            (latest.wake(), replaced)
        };
        if let Some(waker) = waker {
            waker();
        }
    }

    /// The sender's video state (`RemoteVideoState`).
    pub fn set_state(&self, state: LiveState) {
        set_state(&self.0, state);
    }

    /// The feed is over; later frames are ignored.
    pub fn end(&self, reason: LiveEnd) {
        let waker = {
            let mut latest = self.0.lock_or_recover();
            if latest.end.is_some() {
                return;
            }
            latest.end = Some(reason);
            latest.wake()
        };
        if let Some(waker) = waker {
            waker();
        }
    }

    /// The UI side of this feed.
    pub fn sink(&self) -> LiveSink {
        LiveSink(self.0.clone())
    }

    pub fn is_ended(&self) -> bool {
        self.0.lock_or_recover().end.is_some()
    }
}

fn set_state(shared: &Shared, state: LiveState) {
    let waker = {
        let mut latest = shared.lock_or_recover();
        if latest.state == Some(state) {
            return;
        }
        latest.state = Some(state);
        latest.wake()
    };
    if let Some(waker) = waker {
        waker();
    }
}

/// Which feed a group call's frame belongs to (the driver keeps one
/// [`LiveSource`] per key).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LiveKey {
    /// A stream-mode call's picture (RTMP live stream, live story stream),
    /// by chat.
    Stream { chat_id: i64 },
    /// A participant's camera or screen in a video chat.
    Video {
        call_id: i32,
        user_id: i64,
        screen: bool,
    },
    /// Your own camera preview in a video chat.
    Local { call_id: i32 },
    /// A 1:1 call's picture, by its call id: the peer's camera or screen
    /// share, or your camera preview (`local`).
    Call {
        call_id: i32,
        local: bool,
        screen: bool,
    },
}

impl LiveKey {
    /// The group call this feed belongs to (`None` for a stream, keyed by
    /// chat).
    pub fn call_id(self) -> Option<i32> {
        match self {
            Self::Stream { .. } | Self::Call { .. } => None,
            Self::Video { call_id, .. } | Self::Local { call_id } => Some(call_id),
        }
    }

    /// The 1:1 call this feed belongs to (`None` for a group call's or a
    /// stream's; 1:1 and group call ids are separate number spaces).
    pub fn one_to_one_call(self) -> Option<i32> {
        match self {
            Self::Call { call_id, .. } => Some(call_id),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests;
