//! Stream-mode group calls (channel live streams over RTMP, live stories
//! and large broadcast calls over MTProto): the types the engine and the
//! connect side share. The engine switches a group call into stream mode
//! when the `joinVideoChat` answer says so, reports the mode and forwards
//! the engine's time and part requests, each with a request id, as
//! tgcalls asks them (`requestCurrentTime`, `requestAudioBroadcastPart`,
//! `requestVideoBroadcastPart`); the connect side answers them with TDLib
//! (`StreamFeeder`).
use super::*;
use std::sync::Arc;

/// How a joined group call carries its media, from the `joinVideoChat`
/// answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GroupCallMode {
    /// A normal voice or video chat over WebRTC.
    #[default]
    Rtc,
    /// A large call heard as an MTProto broadcast (`{"stream": true}` in
    /// the join answer); segments come from `getGroupCallStreamSegment`.
    Stream,
    /// A channel live stream fed by RTMP (`{"rtmp": true}`); one unified
    /// audio and video channel.
    Rtmp,
}

impl GroupCallMode {
    /// Whether segments come from TDLib instead of WebRTC.
    pub fn is_stream(self) -> bool {
        !matches!(self, GroupCallMode::Rtc)
    }

    /// The mode a `joinVideoChat` answer asks for: an `rtmp` key is RTMP,
    /// a `stream` key is an MTProto stream, anything else is RTC.
    pub fn from_join_answer(answer: &str) -> Self {
        let Ok(serde_json::Value::Object(fields)) = serde_json::from_str(answer) else {
            return GroupCallMode::Rtc;
        };
        if fields.contains_key("rtmp") {
            GroupCallMode::Rtmp
        } else if fields.contains_key("stream") {
            GroupCallMode::Stream
        } else {
            GroupCallMode::Rtc
        }
    }
}

/// The video quality asked of `getGroupCallStreamSegment`; maps one to one
/// to TDLib's `GroupCallVideoQuality`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamQuality {
    Thumbnail,
    Medium,
    Full,
}

impl StreamQuality {
    /// The quality a stream tile of this width (logical pixels) needs, as
    /// tdesktop picks it: 640 and wider is Full, 320 and wider Medium,
    /// anything smaller (or not a number) Thumbnail.
    pub fn for_tile_width(width: f32) -> Self {
        if width >= 640.0 {
            StreamQuality::Full
        } else if width >= 320.0 {
            StreamQuality::Medium
        } else {
            StreamQuality::Thumbnail
        }
    }

    /// The TDLib `GroupCallVideoQuality` constructor.
    pub fn td_type(self) -> &'static str {
        match self {
            StreamQuality::Thumbnail => "groupCallVideoQualityThumbnail",
            StreamQuality::Medium => "groupCallVideoQualityMedium",
            StreamQuality::Full => "groupCallVideoQualityFull",
        }
    }
}

/// Names one broadcast request until it is answered or cancelled: the
/// native instance that asked (a rejoin makes a new one, whose ids start
/// over) and the id that instance gave it. An answer for an instance that
/// is gone is dropped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BroadcastRequestId {
    pub instance: u64,
    pub id: u64,
}

/// Which stream channel a part is from (tgcalls asks
/// `requestAudioBroadcastPart` or `requestVideoBroadcastPart`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BroadcastChannel {
    /// The mixed sound of an MTProto stream.
    Audio,
    /// A video channel at a quality; an RTMP stream's unified part is
    /// channel 1 at Full.
    Video {
        channel_id: i32,
        quality: StreamQuality,
    },
}

impl BroadcastChannel {
    /// `getGroupCallStreamSegment`'s `channel_id` and `video_quality`, as
    /// tdesktop's `broadcastPartStart` fills `inputGroupCallStream`: the
    /// video fields only for a video channel other than 0.
    pub fn segment_channel(self) -> (i32, Option<StreamQuality>) {
        match self {
            BroadcastChannel::Audio => (0, None),
            BroadcastChannel::Video { channel_id: 0, .. } => (0, None),
            BroadcastChannel::Video {
                channel_id,
                quality,
            } => (channel_id, Some(quality)),
        }
    }
}

/// One stream part the engine asks for (tdesktop's `LoadPartTask`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BroadcastPartRequest {
    /// The part's start in server Unix milliseconds; 0 means now.
    pub timestamp_ms: i64,
    /// 1000, 500, 250 or 125.
    pub duration_ms: i64,
    pub channel: BroadcastChannel,
}

impl BroadcastPartRequest {
    /// TDLib's segment scale for the part's duration, as tdesktop maps
    /// it (1000 ms is scale 0, each halving one more); `None` for a
    /// duration tdesktop does not know.
    pub fn scale(&self) -> Option<i32> {
        match self.duration_ms {
            1000 => Some(0),
            500 => Some(1),
            250 => Some(2),
            125 => Some(3),
            _ => None,
        }
    }
}

/// The answer status for a part (tgcalls' `BroadcastPart::Status`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BroadcastPartStatus {
    NotReady,
    ResyncNeeded,
    Success,
}

/// The answer to one part request (tgcalls' `BroadcastPart`).
#[derive(Debug, Clone, PartialEq)]
pub struct BroadcastPartAnswer {
    pub request: BroadcastRequestId,
    pub status: BroadcastPartStatus,
    /// The part that was loaded: the request's start, or the time Quill
    /// chose for a request that said "now".
    pub timestamp_ms: i64,
    /// The server time the answer came at, in seconds; tgcalls resyncs
    /// to it after NotReady and ResyncNeeded.
    pub response_time: f64,
    /// The segment bytes for `Success`; empty otherwise.
    pub data: Vec<u8>,
}

impl BroadcastPartAnswer {
    /// The data pointer for `qc_group_answer_broadcast_part`: null with
    /// length 0 for NOT_READY and RESYNC_NEEDED, and for SUCCESS always a
    /// non-null pointer, even for 0 bytes (an empty `Vec`'s pointer is
    /// dangling but non-null, and is read for 0 bytes).
    pub(crate) fn native_data_ptr(&self) -> *const u8 {
        match self.status {
            BroadcastPartStatus::Success => self.data.as_ptr(),
            BroadcastPartStatus::NotReady | BroadcastPartStatus::ResyncNeeded => std::ptr::null(),
        }
    }

    /// The length to pass with [`Self::native_data_ptr`].
    pub(crate) fn native_data_len(&self) -> usize {
        match self.status {
            BroadcastPartStatus::Success => self.data.len(),
            BroadcastPartStatus::NotReady | BroadcastPartStatus::ResyncNeeded => 0,
        }
    }
}

/// One speaker's level in a group call (tgcalls' `GroupLevelUpdate`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GroupAudioLevel {
    /// The participant's audio ssrc; 0 is your own microphone.
    pub ssrc: u32,
    /// 0 to 1.
    pub level: f32,
    /// The engine hears speech rather than noise.
    pub voice: bool,
}

/// Engine -> connect side: what the engine asked for or reported on one
/// of its own threads. The native trampolines copy the data and hand it
/// over without blocking.
#[derive(Debug, Clone, PartialEq)]
pub enum EngineEvent {
    /// The stream's current server time in ms, answered with
    /// `answer_broadcast_time` (tgcalls' `requestCurrentTime`).
    BroadcastTimeRequested {
        chat_id: i64,
        request: BroadcastRequestId,
    },
    /// A stream part, answered with `answer_broadcast_part`.
    BroadcastPartRequested {
        chat_id: i64,
        request: BroadcastRequestId,
        part: BroadcastPartRequest,
    },
    /// The engine no longer wants `request` (tgcalls' `cancel()`); an
    /// answer after this is ignored.
    BroadcastRequestCancelled {
        chat_id: i64,
        request: BroadcastRequestId,
    },
    /// A batch of speaker levels in a group call (tgcalls only; a batch
    /// of only your own level comes only when it changed, as in
    /// tdesktop).
    GroupAudioLevels {
        group_call_id: i32,
        levels: Vec<GroupAudioLevel>,
    },
    /// The peer of a 1:1 call says its battery is low, or no longer is
    /// (tgcalls' `remoteBatteryLevelIsLowUpdated`).
    PeerBatteryLow { call_id: i32, is_low: bool },
}

pub type EngineEventCallback = Arc<dyn Fn(EngineEvent) + Send + Sync + 'static>;

/// Every picture the engine decoded (group-call tracks, streams, 1:1
/// calls): I420 planes and the rotation flag, keyed by the feed they
/// belong to. The hook runs on an engine thread and only stores the
/// frame.
pub type LiveFrameCallback = Arc<dyn Fn(LiveKey, LiveFrame) + Send + Sync + 'static>;

/// The state of a stream-mode call as the call window shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StreamState {
    /// Joined, waiting for the stream's current time and first segments.
    #[default]
    Connecting,
    /// `getGroupCallStreams` returned no channels: nobody is broadcasting.
    NoStream,
    /// Segments are flowing.
    Live,
    /// The stream finished while it was being watched.
    Ended,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn answer(status: BroadcastPartStatus, data: Vec<u8>) -> BroadcastPartAnswer {
        BroadcastPartAnswer {
            request: BroadcastRequestId { instance: 1, id: 1 },
            status,
            timestamp_ms: 0,
            response_time: 0.0,
            data,
        }
    }

    #[test]
    fn join_answer_mode_follows_the_answer_payload() {
        assert_eq!(
            GroupCallMode::from_join_answer(r#"{"rtmp":true}"#),
            GroupCallMode::Rtmp
        );
        assert_eq!(
            GroupCallMode::from_join_answer(r#"{"stream":true}"#),
            GroupCallMode::Stream
        );
        assert_eq!(
            GroupCallMode::from_join_answer(r#"{"transport":{}}"#),
            GroupCallMode::Rtc
        );
        assert_eq!(
            GroupCallMode::from_join_answer("not json"),
            GroupCallMode::Rtc
        );
    }

    #[test]
    fn success_always_passes_a_non_null_data_pointer() {
        let empty = answer(BroadcastPartStatus::Success, Vec::new());
        assert!(!empty.native_data_ptr().is_null());
        assert_eq!(empty.native_data_len(), 0);
        let full = answer(BroadcastPartStatus::Success, vec![1, 2, 3]);
        assert_eq!(full.native_data_ptr(), full.data.as_ptr());
        assert_eq!(full.native_data_len(), 3);
    }

    #[test]
    fn not_ready_and_resync_pass_no_data() {
        for status in [
            BroadcastPartStatus::NotReady,
            BroadcastPartStatus::ResyncNeeded,
        ] {
            let answer = answer(status, vec![9]);
            assert!(answer.native_data_ptr().is_null());
            assert_eq!(answer.native_data_len(), 0);
        }
    }

    #[test]
    fn durations_map_to_tdesktops_scales() {
        let part = |duration_ms| BroadcastPartRequest {
            timestamp_ms: 0,
            duration_ms,
            channel: BroadcastChannel::Audio,
        };
        let scales: Vec<_> = [1000, 500, 250, 125, 2000, 0]
            .into_iter()
            .map(|duration| part(duration).scale())
            .collect();
        assert_eq!(scales, vec![Some(0), Some(1), Some(2), Some(3), None, None]);
    }

    #[test]
    fn only_a_video_channel_carries_a_quality() {
        assert_eq!(BroadcastChannel::Audio.segment_channel(), (0, None));
        let video = |channel_id| BroadcastChannel::Video {
            channel_id,
            quality: StreamQuality::Medium,
        };
        assert_eq!(video(2).segment_channel(), (2, Some(StreamQuality::Medium)));
        // tdesktop sets the video flags only for a channel other than 0.
        assert_eq!(video(0).segment_channel(), (0, None));
    }

    #[test]
    fn quality_buckets_follow_tile_width() {
        assert_eq!(StreamQuality::for_tile_width(1280.0), StreamQuality::Full);
        assert_eq!(StreamQuality::for_tile_width(640.0), StreamQuality::Full);
        assert_eq!(StreamQuality::for_tile_width(639.9), StreamQuality::Medium);
        assert_eq!(StreamQuality::for_tile_width(320.0), StreamQuality::Medium);
        assert_eq!(
            StreamQuality::for_tile_width(319.0),
            StreamQuality::Thumbnail
        );
        assert_eq!(StreamQuality::for_tile_width(0.0), StreamQuality::Thumbnail);
        assert_eq!(
            StreamQuality::for_tile_width(f32::NAN),
            StreamQuality::Thumbnail
        );
    }

    #[test]
    fn only_rtc_is_not_a_stream() {
        assert!(GroupCallMode::Stream.is_stream());
        assert!(GroupCallMode::Rtmp.is_stream());
        assert!(!GroupCallMode::Rtc.is_stream());
    }
}
