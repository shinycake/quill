//! Stream-mode group calls (channel live streams over RTMP and large
//! broadcast calls over MTProto): the types the engine and the connect
//! side share. ntgcalls switches a group call into stream mode on its own
//! when the `joinVideoChat` answer says so; the engine reports the mode and
//! forwards ntgcalls' segment requests, the connect side answers them with
//! TDLib (`StreamFeeder`).
use super::*;
use ntgcalls_sys::{
    NTG_CONNECTION_MODE_RTC, NTG_CONNECTION_MODE_RTMP, NTG_CONNECTION_MODE_STREAM,
    NTG_MEDIA_SEGMENT_PART_STATUS_NOT_READY, NTG_MEDIA_SEGMENT_PART_STATUS_RESYNC_NEEDED,
    NTG_MEDIA_SEGMENT_PART_STATUS_SUCCESS, NTG_MEDIA_SEGMENT_QUALITY_FULL,
    NTG_MEDIA_SEGMENT_QUALITY_MEDIUM, NTG_MEDIA_SEGMENT_QUALITY_NONE,
    NTG_MEDIA_SEGMENT_QUALITY_THUMBNAIL, NTG_STREAM_DEVICE_CAMERA, NTG_STREAM_DEVICE_SCREEN,
    NTG_STREAM_MODE_CAPTURE, NTG_STREAM_MODE_PLAYBACK, ntg_connection_mode,
    ntg_media_segment_part_status, ntg_media_segment_quality, ntg_segment_part_request,
    ntg_stream_device, ntg_stream_mode,
};
use std::sync::Arc;

/// How a joined group call carries its media, from `ntg_get_connection_mode`
/// after `ntg_connect`.
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

    /// `ntg_connection_mode` to a mode; `NONE` (not connected) and unknown
    /// values have no mode.
    pub(crate) fn from_native(mode: ntg_connection_mode) -> Option<Self> {
        match mode {
            NTG_CONNECTION_MODE_RTC => Some(GroupCallMode::Rtc),
            NTG_CONNECTION_MODE_STREAM => Some(GroupCallMode::Stream),
            NTG_CONNECTION_MODE_RTMP => Some(GroupCallMode::Rtmp),
            _ => None,
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

    /// ntgcalls' quality for a part: `NONE` is an audio part (no video
    /// quality); an unknown value is treated as Thumbnail.
    pub(crate) fn from_native(quality: ntg_media_segment_quality) -> Option<Self> {
        match quality {
            NTG_MEDIA_SEGMENT_QUALITY_NONE => None,
            NTG_MEDIA_SEGMENT_QUALITY_MEDIUM => Some(StreamQuality::Medium),
            NTG_MEDIA_SEGMENT_QUALITY_FULL => Some(StreamQuality::Full),
            NTG_MEDIA_SEGMENT_QUALITY_THUMBNAIL => Some(StreamQuality::Thumbnail),
            _ => Some(StreamQuality::Thumbnail),
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

/// One segment part ntgcalls asks for, copied out of the native callback
/// (`ntg_segment_part_request`). `timestamp` is the segment's start in
/// Unix milliseconds and `segment_id` the index ntgcalls derived from it;
/// Quill never derives either.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BroadcastPartRequest {
    pub segment_id: i64,
    pub part_id: i32,
    pub limit: i32,
    pub timestamp: i64,
    pub quality_update: bool,
    pub channel_id: i32,
    /// `None` for an audio part.
    pub quality: Option<StreamQuality>,
}

impl BroadcastPartRequest {
    pub(crate) fn from_native(request: &ntg_segment_part_request) -> Self {
        Self {
            segment_id: request.segment_id,
            part_id: request.part_id,
            limit: request.limit,
            timestamp: request.timestamp,
            quality_update: request.quality_update,
            channel_id: request.channel_id,
            quality: StreamQuality::from_native(request.quality),
        }
    }
}

/// The answer status for a part. ntgcalls also defines `DOWNLOADING`, but
/// `send_broadcast_part` rejects it ("Invalid part status"), so Quill never
/// sends it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BroadcastPartStatus {
    NotReady,
    ResyncNeeded,
    Success,
}

impl BroadcastPartStatus {
    pub(crate) fn to_native(self) -> ntg_media_segment_part_status {
        match self {
            BroadcastPartStatus::NotReady => NTG_MEDIA_SEGMENT_PART_STATUS_NOT_READY,
            BroadcastPartStatus::ResyncNeeded => NTG_MEDIA_SEGMENT_PART_STATUS_RESYNC_NEEDED,
            BroadcastPartStatus::Success => NTG_MEDIA_SEGMENT_PART_STATUS_SUCCESS,
        }
    }
}

/// The answer to one part request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BroadcastPartAnswer {
    pub segment_id: i64,
    pub part_id: i32,
    pub status: BroadcastPartStatus,
    pub quality_update: bool,
    /// The segment bytes for `Success`; empty otherwise.
    pub data: Vec<u8>,
}

/// Engine -> connect side: what ntgcalls asked for on one of its own
/// threads. The native trampolines copy the request and hand it over
/// without blocking.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineEvent {
    /// RTMP only: ntgcalls needs the stream's current time
    /// (`getGroupCallStreams`). MTProto streams use the server time
    /// ntgcalls already has and never ask.
    BroadcastTimestampRequested { chat_id: i64 },
    /// ntgcalls needs a segment part (`getGroupCallStreamSegment`).
    BroadcastPartRequested {
        chat_id: i64,
        request: BroadcastPartRequest,
    },
}

pub type EngineEventCallback = Arc<dyn Fn(EngineEvent) + Send + Sync + 'static>;

/// Stream-mode pictures: `(chat id, frame)`. A stream has no participant
/// ssrc map, so its frames go to one slot per chat.
pub type StreamFrameCallback = Arc<dyn Fn(i64, VideoFrame) + Send + Sync + 'static>;

/// Where a group-call frame goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupVideoTarget {
    /// Your own camera preview (the self tile).
    Local,
    /// A participant's camera or screen share, resolved by ssrc.
    Participant { user_id: i64, screen: bool },
    /// The picture of a stream-mode call.
    Stream { chat_id: i64 },
}

/// Routes one group-call frame batch. Stream-mode chats take every
/// playback picture into their stream slot without an ssrc lookup (an
/// RTMP stream reports ssrc 1 on its single "unified" endpoint); other
/// calls resolve the participant through `participant_for_ssrc`. Audio and
/// the screen-share capture preview have no target.
pub(crate) fn group_video_target(
    chat_id: i64,
    is_stream_chat: bool,
    mode: ntg_stream_mode,
    device: ntg_stream_device,
    participant_for_ssrc: impl FnOnce() -> Option<i64>,
) -> Option<GroupVideoTarget> {
    let is_screen = device == NTG_STREAM_DEVICE_SCREEN;
    let is_video = is_screen || device == NTG_STREAM_DEVICE_CAMERA;
    if mode == NTG_STREAM_MODE_CAPTURE && device == NTG_STREAM_DEVICE_CAMERA {
        return Some(GroupVideoTarget::Local);
    }
    if mode != NTG_STREAM_MODE_PLAYBACK || !is_video {
        return None;
    }
    if is_stream_chat {
        return Some(GroupVideoTarget::Stream { chat_id });
    }
    participant_for_ssrc().map(|user_id| GroupVideoTarget::Participant {
        user_id,
        screen: is_screen,
    })
}

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
    /// This build's ntgcalls cannot play the stream
    /// (`NTG_ERR_RTMP_STREAMING_UNSUPPORTED`).
    Unsupported,
}

#[cfg(test)]
mod tests {
    use super::*;
    use ntgcalls_sys::{NTG_STREAM_DEVICE_MICROPHONE, NTG_STREAM_DEVICE_SPEAKER};

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
    fn native_quality_and_mode_conversions() {
        assert_eq!(
            StreamQuality::from_native(NTG_MEDIA_SEGMENT_QUALITY_NONE),
            None
        );
        assert_eq!(
            StreamQuality::from_native(NTG_MEDIA_SEGMENT_QUALITY_FULL),
            Some(StreamQuality::Full)
        );
        assert_eq!(
            StreamQuality::from_native(99),
            Some(StreamQuality::Thumbnail)
        );
        assert_eq!(
            GroupCallMode::from_native(NTG_CONNECTION_MODE_RTMP),
            Some(GroupCallMode::Rtmp)
        );
        assert_eq!(GroupCallMode::from_native(0), None);
        assert!(GroupCallMode::Stream.is_stream());
        assert!(!GroupCallMode::Rtc.is_stream());
    }

    #[test]
    fn stream_chats_route_playback_video_to_the_stream_slot() {
        let target = group_video_target(
            9,
            true,
            NTG_STREAM_MODE_PLAYBACK,
            NTG_STREAM_DEVICE_CAMERA,
            || panic!("stream frames never look up an ssrc"),
        );
        assert_eq!(target, Some(GroupVideoTarget::Stream { chat_id: 9 }));
        let screen = group_video_target(
            9,
            true,
            NTG_STREAM_MODE_PLAYBACK,
            NTG_STREAM_DEVICE_SCREEN,
            || None,
        );
        assert_eq!(screen, Some(GroupVideoTarget::Stream { chat_id: 9 }));
        // Audio never routes.
        for device in [NTG_STREAM_DEVICE_MICROPHONE, NTG_STREAM_DEVICE_SPEAKER] {
            assert_eq!(
                group_video_target(9, true, NTG_STREAM_MODE_PLAYBACK, device, || None),
                None
            );
        }
    }

    #[test]
    fn rtc_chats_keep_the_ssrc_lookup() {
        let known = group_video_target(
            9,
            false,
            NTG_STREAM_MODE_PLAYBACK,
            NTG_STREAM_DEVICE_SCREEN,
            || Some(42),
        );
        assert_eq!(
            known,
            Some(GroupVideoTarget::Participant {
                user_id: 42,
                screen: true
            })
        );
        let unknown = group_video_target(
            9,
            false,
            NTG_STREAM_MODE_PLAYBACK,
            NTG_STREAM_DEVICE_CAMERA,
            || None,
        );
        assert_eq!(unknown, None);
        let local = group_video_target(
            9,
            false,
            NTG_STREAM_MODE_CAPTURE,
            NTG_STREAM_DEVICE_CAMERA,
            || None,
        );
        assert_eq!(local, Some(GroupVideoTarget::Local));
        let screen_preview = group_video_target(
            9,
            false,
            NTG_STREAM_MODE_CAPTURE,
            NTG_STREAM_DEVICE_SCREEN,
            || Some(1),
        );
        assert_eq!(screen_preview, None);
    }
}
