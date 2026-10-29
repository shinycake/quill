//! What tgcalls tells Quill about a group call or its presentation, from
//! its own threads: the `qc_group_callbacks` trampolines and the context
//! they read. `qc_group_destroy` returns only once no callback can run,
//! and the engine keeps the context alive until then (and past clearing
//! the camera preview).
use super::super::{CallbackShared, EngineEvent, GroupAudioLevel};
use super::callbacks::send_picture;
use crate::calls::live_video::LiveKey;
use crate::calls::stream_audio::StreamAudio;
use crate::locks::LockExt;
use arc_swap::ArcSwapOption;
use quill_calls_sys::{qc_group_callbacks, qc_group_level, qc_video_frame};
use std::collections::HashMap;
use std::ffi::{CStr, c_char, c_void};
use std::sync::{Arc, Mutex};

/// Who sends an incoming video endpoint: the participant, and whether it
/// is their screen rather than their camera.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct EndpointOwner {
    pub(super) user_id: i64,
    pub(super) screen: bool,
}

/// One group call's (or presentation's) view of the engine's hooks,
/// handed to tgcalls as `user`.
pub(super) struct GroupContext {
    pub(super) group_call_id: i32,
    pub(super) presentation: bool,
    pub(super) shared: Arc<CallbackShared>,
    /// Where the call's sound goes; `None` drops it (no output open).
    pub(super) sound: ArcSwapOption<StreamAudio>,
    /// The endpoints whose pictures Quill takes, by endpoint id.
    pub(super) endpoints: Mutex<HashMap<String, EndpointOwner>>,
}

impl GroupContext {
    pub(super) fn new(
        group_call_id: i32,
        presentation: bool,
        shared: &Arc<CallbackShared>,
    ) -> Self {
        Self {
            group_call_id,
            presentation,
            shared: Arc::clone(shared),
            sound: ArcSwapOption::empty(),
            endpoints: Mutex::new(HashMap::new()),
        }
    }

    /// The callbacks tdesktop wires for the instance: a presentation
    /// reports only its network state. Nobody answers media channel,
    /// time or stream part requests yet (live streams are the next
    /// step), so tgcalls accepts every ssrc as it comes and a broadcast
    /// waits for parts.
    pub(super) fn callbacks(self: &Arc<Self>) -> qc_group_callbacks {
        let call = !self.presentation;
        qc_group_callbacks {
            user: self.as_user(),
            network_state: Some(on_network_state),
            audio_levels: call.then_some(on_audio_levels as _),
            audio_out: call.then_some(on_audio as _),
            incoming_video: call.then_some(on_incoming_video as _),
            request_media_channels: None,
            request_current_time: None,
            request_broadcast_part: None,
            request_cancelled: None,
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
unsafe fn context<'a>(user: *mut c_void) -> Option<&'a GroupContext> {
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
            call.group_call_id,
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
        let hook = call.shared.engine_event_hook.lock_or_recover().clone();
        if let Some(hook) = hook {
            hook(EngineEvent::GroupAudioLevels {
                group_call_id: call.group_call_id,
                levels: levels.iter().map(level).collect(),
            });
        }
    });
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
        let Some(owner) = call.endpoints.lock_or_recover().get(endpoint).copied() else {
            return;
        };
        let key = LiveKey::Video {
            call_id: call.group_call_id,
            user_id: owner.user_id,
            screen: owner.screen,
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
            call_id: call.group_call_id,
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
        let context = Arc::new(GroupContext::new(7, false, &shared));
        context.endpoints.lock().unwrap().insert(
            "screen-42".to_owned(),
            EndpointOwner {
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
    fn levels_reach_the_engine_event_hook() {
        let shared = Arc::new(CallbackShared::new());
        let events = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&events);
        set_hook(
            &shared.engine_event_hook,
            Arc::new(move |event| sink.lock().unwrap().push(event)) as _,
        );
        let context = Arc::new(GroupContext::new(7, false, &shared));
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
        let screen = Arc::new(GroupContext::new(7, true, &shared)).callbacks();
        assert!(screen.network_state.is_some());
        assert!(screen.audio_levels.is_none());
        assert!(screen.audio_out.is_none());
        assert!(screen.incoming_video.is_none());
        let call = Arc::new(GroupContext::new(7, false, &shared)).callbacks();
        assert!(call.audio_levels.is_some() && call.audio_out.is_some());
        assert!(call.request_media_channels.is_none());
    }
}
