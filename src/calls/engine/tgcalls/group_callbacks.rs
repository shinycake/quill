//! What tgcalls tells Quill about a group call or its presentation, from
//! its own threads: the `qc_group_callbacks` trampolines and the context
//! they read. `qc_group_destroy` returns only once no callback can run,
//! and the engine keeps the context alive until then (and past clearing
//! the camera preview).
use super::super::{
    BroadcastChannel, BroadcastPartRequest, BroadcastRequestId, CallbackShared, EngineEvent,
    GroupAudioLevel, StreamQuality,
};
use super::callbacks::send_picture;
use crate::calls::live_video::LiveKey;
use crate::calls::stream_audio::StreamAudio;
use crate::locks::LockExt;
use arc_swap::ArcSwapOption;
use quill_calls_sys::{
    QC_GROUP_QUALITY_FULL, QC_GROUP_QUALITY_MEDIUM, qc_group_broadcast_part, qc_group_callbacks,
    qc_group_level, qc_video_frame,
};
use std::collections::HashMap;
use std::ffi::{CStr, c_char, c_void};
use std::sync::atomic::{AtomicI32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// Numbers every instance, so an answer meant for a replaced one (a
/// rejoin starts its request ids over) never reaches its successor.
static NEXT_INSTANCE: AtomicU64 = AtomicU64::new(1);

/// One group call's (or presentation's) view of the engine's hooks,
/// handed to tgcalls as `user`.
pub(super) struct GroupContext {
    /// TDLib's group call id; a conference call's changes once, when the
    /// join answer names the call created under a provisional key.
    group_call_id: AtomicI32,
    /// The chat the call is in: stream requests and pictures are keyed
    /// by it.
    pub(super) chat_id: i64,
    /// This instance's number in its broadcast request ids.
    pub(super) instance: u64,
    pub(super) presentation: bool,
    pub(super) shared: Arc<CallbackShared>,
    /// Where the call's sound goes; `None` drops it (no output open).
    pub(super) sound: ArcSwapOption<StreamAudio>,
    /// The endpoints whose pictures Quill takes and the feed each one
    /// goes to, by endpoint id.
    pub(super) endpoints: Mutex<HashMap<String, LiveKey>>,
    /// A conference call's encryption and media channels; `None` for a
    /// video chat or a stream.
    pub(super) conference: Option<super::group_conference::Conference>,
}

impl GroupContext {
    pub(super) fn new(
        group_call_id: i32,
        chat_id: i64,
        presentation: bool,
        shared: &Arc<CallbackShared>,
    ) -> Self {
        Self {
            group_call_id: AtomicI32::new(group_call_id),
            chat_id,
            instance: NEXT_INSTANCE.fetch_add(1, Ordering::Relaxed),
            presentation,
            shared: Arc::clone(shared),
            sound: ArcSwapOption::empty(),
            endpoints: Mutex::new(HashMap::new()),
            conference: None,
        }
    }

    /// The same context for a conference call: every frame goes through
    /// `crypto`.
    pub(super) fn conference(mut self, crypto: Arc<dyn crate::calls::engine::FrameCrypto>) -> Self {
        self.conference = Some(super::group_conference::Conference::new(crypto));
        self
    }

    pub(super) fn group_call_id(&self) -> i32 {
        self.group_call_id.load(Ordering::SeqCst)
    }

    pub(super) fn set_group_call_id(&self, group_call_id: i32) {
        self.group_call_id.store(group_call_id, Ordering::SeqCst);
    }

    /// The callbacks tdesktop wires for the instance: a presentation
    /// reports only its network state, the call also its levels, sound,
    /// videos and the stream's time and part requests. Media channel
    /// requests stay unwired outside a conference call, so tgcalls accepts
    /// every ssrc as it comes (docs/decisions/tgcalls-engine.md); a
    /// conference call needs each ssrc's sender to decrypt its frames
    /// (docs/decisions/tgcalls-conference-calls.md).
    pub(super) fn callbacks(self: &Arc<Self>) -> qc_group_callbacks {
        use super::group_conference::{on_e2e, on_media_channels};
        let call = !self.presentation;
        let conference = self.conference.is_some();
        qc_group_callbacks {
            user: self.as_user(),
            network_state: Some(on_network_state),
            audio_levels: call.then_some(on_audio_levels as _),
            audio_out: call.then_some(on_audio as _),
            incoming_video: call.then_some(on_incoming_video as _),
            request_media_channels: (call && conference).then_some(on_media_channels as _),
            request_current_time: call.then_some(on_current_time as _),
            request_broadcast_part: call.then_some(on_broadcast_part as _),
            request_cancelled: call.then_some(on_request_cancelled as _),
            e2e: conference.then_some(on_e2e as _),
        }
    }

    fn request(&self, id: u64) -> BroadcastRequestId {
        BroadcastRequestId {
            instance: self.instance,
            id,
        }
    }

    /// Hands an event to the connect side's hook, which only queues it.
    fn emit(&self, event: EngineEvent) {
        let hook = self.shared.engine_event_hook.lock_or_recover().clone();
        if let Some(hook) = hook {
            hook(event);
        }
    }

    /// The `user` for this context's callbacks and its camera preview.
    pub(super) fn as_user(self: &Arc<Self>) -> *mut c_void {
        Arc::as_ptr(self).cast_mut().cast::<c_void>()
    }
}

/// # Safety
/// `user` is the `GroupContext` the engine keeps alive past
/// `qc_group_destroy` (or past clearing the preview), or null.
pub(super) unsafe fn context<'a>(user: *mut c_void) -> Option<&'a GroupContext> {
    // SAFETY: as the function's contract says.
    unsafe { user.cast::<GroupContext>().cast_const().as_ref() }
}

unsafe extern "C" fn on_network_state(user: *mut c_void, connected: i32, to_rtc: i32) {
    crate::thread_guard::ffi_boundary((), || {
        // SAFETY: tgcalls passes back the `user` the engine registered.
        let Some(call) = (unsafe { context(user) }) else {
            return;
        };
        crate::advanced::debug_mode::call_log(&format!(
            "[call] group {}{} tgcalls network connected {} to rtc {}",
            call.group_call_id(),
            if call.presentation { " screen" } else { "" },
            connected != 0,
            to_rtc != 0
        ));
    });
}

unsafe extern "C" fn on_audio_levels(
    user: *mut c_void,
    levels: *const qc_group_level,
    count: usize,
) {
    crate::thread_guard::ffi_boundary((), || {
        // SAFETY: tgcalls passes back the `user` the engine registered.
        let Some(call) = (unsafe { context(user) }) else {
            return;
        };
        if levels.is_null() || count == 0 {
            return;
        }
        // SAFETY: the library hands `count` levels at `levels` for this call.
        let levels = unsafe { std::slice::from_raw_parts(levels, count) };
        call.emit(EngineEvent::GroupAudioLevels {
            group_call_id: call.group_call_id(),
            levels: levels.iter().map(level).collect(),
        });
    });
}

/// tgcalls' `requestCurrentTime`: answered with
/// `qc_group_answer_current_time` by the connect side.
unsafe extern "C" fn on_current_time(user: *mut c_void, request_id: u64) {
    crate::thread_guard::ffi_boundary((), || {
        // SAFETY: tgcalls passes back the `user` the engine registered.
        let Some(call) = (unsafe { context(user) }) else {
            return;
        };
        call.emit(EngineEvent::BroadcastTimeRequested {
            chat_id: call.chat_id,
            request: call.request(request_id),
        });
    });
}

/// tgcalls' `requestAudioBroadcastPart` and `requestVideoBroadcastPart`:
/// answered with `qc_group_answer_broadcast_part`.
unsafe extern "C" fn on_broadcast_part(
    user: *mut c_void,
    request_id: u64,
    part: *const qc_group_broadcast_part,
) {
    crate::thread_guard::ffi_boundary((), || {
        // SAFETY: tgcalls passes back the `user` the engine registered.
        let Some(call) = (unsafe { context(user) }) else {
            return;
        };
        // SAFETY: the library hands a part valid for this call, or null.
        let Some(part) = (unsafe { part.as_ref() }) else {
            return;
        };
        call.emit(EngineEvent::BroadcastPartRequested {
            chat_id: call.chat_id,
            request: call.request(request_id),
            part: part_request(part),
        });
    });
}

/// tgcalls' `cancel()` of an open request.
unsafe extern "C" fn on_request_cancelled(user: *mut c_void, request_id: u64) {
    crate::thread_guard::ffi_boundary((), || {
        // SAFETY: tgcalls passes back the `user` the engine registered.
        let Some(call) = (unsafe { context(user) }) else {
            return;
        };
        if call
            .conference
            .as_ref()
            .is_some_and(|conference| conference.cancel(request_id))
        {
            return;
        }
        call.emit(EngineEvent::BroadcastRequestCancelled {
            chat_id: call.chat_id,
            request: call.request(request_id),
        });
    });
}

/// The part tgcalls asks for, with its `QC_GROUP_QUALITY_*` read as
/// tdesktop reads tgcalls' quality (anything else is Thumbnail).
pub(super) fn part_request(part: &qc_group_broadcast_part) -> BroadcastPartRequest {
    let channel = if part.is_video == 0 {
        BroadcastChannel::Audio
    } else {
        BroadcastChannel::Video {
            channel_id: part.video_channel,
            quality: match part.video_quality {
                QC_GROUP_QUALITY_FULL => StreamQuality::Full,
                QC_GROUP_QUALITY_MEDIUM => StreamQuality::Medium,
                _ => StreamQuality::Thumbnail,
            },
        }
    };
    BroadcastPartRequest {
        timestamp_ms: part.timestamp_ms,
        duration_ms: part.duration_ms,
        channel,
    }
}

fn level(native: &qc_group_level) -> GroupAudioLevel {
    GroupAudioLevel {
        ssrc: native.ssrc,
        level: native.level,
        voice: native.voice != 0,
    }
}

unsafe extern "C" fn on_audio(user: *mut c_void, samples: *const i16, count: usize) {
    crate::thread_guard::ffi_boundary((), || {
        // SAFETY: tgcalls passes back the `user` the engine registered.
        let Some(call) = (unsafe { context(user) }) else {
            return;
        };
        if samples.is_null() || count == 0 {
            return;
        }
        let Some(sound) = call.sound.load_full() else {
            return;
        };
        // SAFETY: the library hands `count` samples at `samples` for this
        // call; i16 is plain data, so its bytes can be read as u8.
        let bytes = unsafe { std::slice::from_raw_parts(samples.cast::<u8>(), count * 2) };
        sound.push_frames([bytes]);
    });
}

unsafe extern "C" fn on_incoming_video(
    user: *mut c_void,
    endpoint: *const c_char,
    frame: *const qc_video_frame,
) {
    crate::thread_guard::ffi_boundary((), || {
        // SAFETY: tgcalls passes back the `user` the engine registered.
        let Some(call) = (unsafe { context(user) }) else {
            return;
        };
        if endpoint.is_null() {
            return;
        }
        // SAFETY: a NUL-ended endpoint id the library keeps for this call.
        let endpoint = unsafe { CStr::from_ptr(endpoint) };
        let Ok(endpoint) = endpoint.to_str() else {
            return;
        };
        // An endpoint dropped since the picture left tgcalls has no tile.
        let Some(key) = call.endpoints.lock_or_recover().get(endpoint).copied() else {
            return;
        };
        // SAFETY: the library keeps the picture valid for this call.
        unsafe { send_picture(&call.shared, key, frame) };
    });
}

/// Your own camera preview in the group call (the self tile),
/// registered with `qc_video_capture_set_preview`.
pub(super) unsafe extern "C" fn on_local_video(user: *mut c_void, frame: *const qc_video_frame) {
    crate::thread_guard::ffi_boundary((), || {
        // SAFETY: the engine registers a live `GroupContext` and clears
        // the preview before the context goes.
        let Some(call) = (unsafe { context(user) }) else {
            return;
        };
        let key = LiveKey::Local {
            call_id: call.group_call_id(),
        };
        // SAFETY: the library keeps the picture valid for this call.
        unsafe { send_picture(&call.shared, key, frame) };
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calls::engine::set_hook;
    use crate::calls::live_video::LiveFrame;

    fn picture(width: i32, height: i32, planes: &[u8]) -> qc_video_frame {
        let luma = (width * height) as usize;
        qc_video_frame {
            width,
            height,
            rotation: 0,
            timestamp_us: 0,
            y: planes.as_ptr(),
            u: planes[luma..].as_ptr(),
            v: planes[luma + luma / 4..].as_ptr(),
            stride_y: width,
            stride_u: width / 2,
            stride_v: width / 2,
        }
    }

    fn recorder(shared: &CallbackShared) -> Arc<Mutex<Vec<LiveKey>>> {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&seen);
        set_hook(
            &shared.live_frame_hook,
            Arc::new(move |key: LiveKey, _frame: LiveFrame| sink.lock().unwrap().push(key)) as _,
        );
        seen
    }

    #[test]
    fn pictures_go_to_the_endpoints_owner_and_the_self_tile() {
        let shared = Arc::new(CallbackShared::new());
        let seen = recorder(&shared);
        let context = Arc::new(GroupContext::new(7, -70, false, &shared));
        context.endpoints.lock().unwrap().insert(
            "screen-42".to_owned(),
            LiveKey::Video {
                call_id: 7,
                user_id: 42,
                screen: true,
            },
        );
        let planes = vec![128u8; 4 * 2 * 3 / 2];
        let frame = picture(4, 2, &planes);
        let known = c"screen-42";
        let unknown = c"camera-43";
        // SAFETY: the context outlives these calls; the picture's planes
        // hold `stride * rows` bytes each.
        unsafe {
            on_incoming_video(context.as_user(), known.as_ptr(), &frame);
            on_incoming_video(context.as_user(), unknown.as_ptr(), &frame);
            on_local_video(context.as_user(), &frame);
        }
        assert_eq!(
            *seen.lock().unwrap(),
            vec![
                LiveKey::Video {
                    call_id: 7,
                    user_id: 42,
                    screen: true
                },
                LiveKey::Local { call_id: 7 },
            ]
        );
    }

    #[test]
    fn the_unified_stream_goes_to_the_chats_stream_feed() {
        let shared = Arc::new(CallbackShared::new());
        let seen = recorder(&shared);
        let context = Arc::new(GroupContext::new(7, -70, false, &shared));
        context
            .endpoints
            .lock()
            .unwrap()
            .insert("unified".to_owned(), LiveKey::Stream { chat_id: -70 });
        let planes = vec![128u8; 4 * 2 * 3 / 2];
        let frame = picture(4, 2, &planes);
        // SAFETY: as above.
        unsafe { on_incoming_video(context.as_user(), c"unified".as_ptr(), &frame) };
        assert_eq!(
            *seen.lock().unwrap(),
            vec![LiveKey::Stream { chat_id: -70 }]
        );
    }

    fn events(shared: &CallbackShared) -> Arc<Mutex<Vec<EngineEvent>>> {
        let events = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&events);
        set_hook(
            &shared.engine_event_hook,
            Arc::new(move |event| sink.lock().unwrap().push(event)) as _,
        );
        events
    }

    #[test]
    fn stream_requests_reach_the_hook_with_the_instances_id() {
        let shared = Arc::new(CallbackShared::new());
        let events = events(&shared);
        let context = Arc::new(GroupContext::new(7, -70, false, &shared));
        let other = Arc::new(GroupContext::new(7, -70, false, &shared));
        assert_ne!(
            context.instance, other.instance,
            "each instance has its own"
        );
        let audio = qc_group_broadcast_part {
            timestamp_ms: 1_700_000_000_000,
            duration_ms: 1000,
            is_video: 0,
            video_channel: 0,
            video_quality: 0,
        };
        let unified = qc_group_broadcast_part {
            timestamp_ms: 0,
            duration_ms: 500,
            is_video: 1,
            video_channel: 1,
            video_quality: QC_GROUP_QUALITY_FULL,
        };
        // SAFETY: the context and the parts outlive the calls.
        unsafe {
            on_current_time(context.as_user(), 1);
            on_broadcast_part(context.as_user(), 2, &audio);
            on_broadcast_part(context.as_user(), 3, &unified);
            on_broadcast_part(context.as_user(), 4, std::ptr::null());
            on_request_cancelled(context.as_user(), 2);
        }
        let id = |id| BroadcastRequestId {
            instance: context.instance,
            id,
        };
        assert_eq!(
            *events.lock().unwrap(),
            vec![
                EngineEvent::BroadcastTimeRequested {
                    chat_id: -70,
                    request: id(1),
                },
                EngineEvent::BroadcastPartRequested {
                    chat_id: -70,
                    request: id(2),
                    part: BroadcastPartRequest {
                        timestamp_ms: 1_700_000_000_000,
                        duration_ms: 1000,
                        channel: BroadcastChannel::Audio,
                    },
                },
                EngineEvent::BroadcastPartRequested {
                    chat_id: -70,
                    request: id(3),
                    part: BroadcastPartRequest {
                        timestamp_ms: 0,
                        duration_ms: 500,
                        channel: BroadcastChannel::Video {
                            channel_id: 1,
                            quality: StreamQuality::Full,
                        },
                    },
                },
                EngineEvent::BroadcastRequestCancelled {
                    chat_id: -70,
                    request: id(2),
                },
            ]
        );
    }

    #[test]
    fn video_qualities_map_like_tgcalls() {
        let part = |quality| qc_group_broadcast_part {
            timestamp_ms: 5,
            duration_ms: 1000,
            is_video: 1,
            video_channel: 2,
            video_quality: quality,
        };
        let qualities: Vec<_> = [0, QC_GROUP_QUALITY_MEDIUM, QC_GROUP_QUALITY_FULL, 9]
            .into_iter()
            .map(|quality| part_request(&part(quality)).channel)
            .collect();
        let video = |quality| BroadcastChannel::Video {
            channel_id: 2,
            quality,
        };
        assert_eq!(
            qualities,
            vec![
                video(StreamQuality::Thumbnail),
                video(StreamQuality::Medium),
                video(StreamQuality::Full),
                video(StreamQuality::Thumbnail),
            ]
        );
    }

    #[test]
    fn levels_reach_the_engine_event_hook() {
        let shared = Arc::new(CallbackShared::new());
        let events = events(&shared);
        let context = Arc::new(GroupContext::new(7, -70, false, &shared));
        let levels = [
            qc_group_level {
                ssrc: 0,
                level: 0.5,
                voice: 1,
            },
            qc_group_level {
                ssrc: 99,
                level: 0.25,
                voice: 0,
            },
        ];
        // SAFETY: the context and the levels outlive the call.
        unsafe { on_audio_levels(context.as_user(), levels.as_ptr(), levels.len()) };
        assert_eq!(
            *events.lock().unwrap(),
            vec![EngineEvent::GroupAudioLevels {
                group_call_id: 7,
                levels: vec![
                    GroupAudioLevel {
                        ssrc: 0,
                        level: 0.5,
                        voice: true
                    },
                    GroupAudioLevel {
                        ssrc: 99,
                        level: 0.25,
                        voice: false
                    },
                ],
            }]
        );
    }

    #[test]
    fn a_presentation_reports_only_its_network() {
        let shared = Arc::new(CallbackShared::new());
        let screen = Arc::new(GroupContext::new(7, -70, true, &shared)).callbacks();
        assert!(screen.network_state.is_some());
        assert!(screen.audio_levels.is_none());
        assert!(screen.audio_out.is_none());
        assert!(screen.incoming_video.is_none());
        assert!(screen.request_current_time.is_none());
        assert!(screen.request_broadcast_part.is_none());
        let call = Arc::new(GroupContext::new(7, -70, false, &shared)).callbacks();
        assert!(call.audio_levels.is_some() && call.audio_out.is_some());
        assert!(call.request_current_time.is_some());
        assert!(call.request_broadcast_part.is_some());
        assert!(call.request_cancelled.is_some());
        assert!(call.request_media_channels.is_none());
    }
}
