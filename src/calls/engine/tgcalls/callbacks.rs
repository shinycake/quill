//! What tgcalls tells Quill about a 1:1 call, from its own threads: the
//! `qc_p2p_callbacks` trampolines and the per-call context they read.
//! `qc_p2p_destroy` returns only once no callback can run, and the engine
//! keeps the context alive until then.
use super::super::{CallbackShared, EngineEvent, RemoteVideoState, TransportState};
use crate::calls::live_video::{LiveFrame, LiveKey, LiveRotation};
use crate::calls::stream_audio::StreamAudio;
use crate::locks::LockExt;
use arc_swap::ArcSwapOption;
use quill_calls_sys::{
    QC_P2P_ESTABLISHED, QC_P2P_FAILED, QC_P2P_RECONNECTING, QC_P2P_WAIT_INIT, QC_P2P_WAIT_INIT_ACK,
    QC_VIDEO_ACTIVE, QC_VIDEO_PAUSED, qc_p2p_callbacks, qc_video_frame,
};
use std::ffi::c_void;
use std::sync::Arc;
use std::sync::atomic::Ordering;

/// One call's view of the engine's hooks, handed to tgcalls as `user`.
pub(super) struct CallContext {
    pub(super) call_id: i32,
    pub(super) shared: Arc<CallbackShared>,
    /// Where the call's sound goes; `None` drops it (no output open).
    pub(super) sound: ArcSwapOption<StreamAudio>,
}

impl CallContext {
    pub(super) fn callbacks(self: &Arc<Self>) -> qc_p2p_callbacks {
        qc_p2p_callbacks {
            user: Arc::as_ptr(self).cast_mut().cast::<c_void>(),
            state: Some(on_state),
            signal_bars: Some(on_signal_bars),
            remote_media_state: Some(on_remote_media),
            remote_battery_low: Some(on_remote_battery_low),
            signaling: Some(on_signaling),
            audio_out: Some(on_audio),
            remote_video: Some(on_remote_video),
        }
    }

    /// Hands an event to the connect side's hook, which only queues it.
    fn emit(&self, event: EngineEvent) {
        let hook = self.shared.engine_event_hook.lock_or_recover().clone();
        if let Some(hook) = hook {
            hook(event);
        }
    }

    /// The preview callback's `user` for this call's own camera.
    pub(super) fn as_user(self: &Arc<Self>) -> *mut c_void {
        Arc::as_ptr(self).cast_mut().cast::<c_void>()
    }
}

/// tgcalls' `State` as Quill's transport state.
pub(super) fn transport_state(state: i32) -> Option<TransportState> {
    match state {
        QC_P2P_WAIT_INIT | QC_P2P_WAIT_INIT_ACK => Some(TransportState::Connecting),
        QC_P2P_ESTABLISHED => Some(TransportState::Connected),
        QC_P2P_RECONNECTING => Some(TransportState::Reconnecting),
        QC_P2P_FAILED => Some(TransportState::Failed),
        _ => None,
    }
}

pub(super) fn remote_video_state(state: i32) -> RemoteVideoState {
    match state {
        QC_VIDEO_ACTIVE => RemoteVideoState::Active,
        QC_VIDEO_PAUSED => RemoteVideoState::Paused,
        _ => RemoteVideoState::Inactive,
    }
}

pub(super) fn live_rotation(degrees: i32) -> LiveRotation {
    match degrees {
        90 => LiveRotation::R90,
        180 => LiveRotation::R180,
        270 => LiveRotation::R270,
        _ => LiveRotation::None,
    }
}

/// # Safety
/// `user` is the `CallContext` the engine keeps alive past
/// `qc_p2p_destroy` (or past clearing the preview), or null.
unsafe fn context<'a>(user: *mut c_void) -> Option<&'a CallContext> {
    // SAFETY: as the function's contract says.
    unsafe { user.cast::<CallContext>().cast_const().as_ref() }
}

unsafe extern "C" fn on_state(user: *mut c_void, state: i32) {
    crate::thread_guard::ffi_boundary((), || {
        // SAFETY: tgcalls passes back the `user` the engine registered.
        let Some(call) = (unsafe { context(user) }) else {
            return;
        };
        let Some(state) = transport_state(state) else {
            return;
        };
        crate::advanced::debug_mode::call_log(&format!(
            "[call] {} tgcalls state {state:?}",
            call.call_id
        ));
        let hook = call.shared.transport_hook.lock_or_recover().clone();
        if let Some(hook) = hook {
            hook(call.call_id, state);
        }
    });
}

unsafe extern "C" fn on_remote_media(user: *mut c_void, audio_active: i32, video_state: i32) {
    crate::thread_guard::ffi_boundary((), || {
        // SAFETY: tgcalls passes back the `user` the engine registered.
        let Some(call) = (unsafe { context(user) }) else {
            return;
        };
        let audio = call.shared.remote_audio_hook.lock_or_recover().clone();
        if let Some(hook) = audio {
            hook(call.call_id, audio_active == 0);
        }
        let video = call.shared.remote_video_hook.lock_or_recover().clone();
        if let Some(hook) = video {
            hook(call.call_id, remote_video_state(video_state));
        }
    });
}

/// tgcalls' `signalBarsUpdated`: 0 to 4 bars, for the call window's
/// signal bars (tdesktop `Calls::SignalBars`).
unsafe extern "C" fn on_signal_bars(user: *mut c_void, bars: i32) {
    crate::thread_guard::ffi_boundary((), || {
        // SAFETY: tgcalls passes back the `user` the engine registered.
        let Some(call) = (unsafe { context(user) }) else {
            return;
        };
        call.emit(EngineEvent::SignalBars {
            call_id: call.call_id,
            bars: signal_bar_count(bars),
        });
    });
}

/// tgcalls' bar count on tdesktop's 0 to 4 scale.
pub(super) fn signal_bar_count(bars: i32) -> u8 {
    u8::try_from(bars.clamp(0, 4)).unwrap_or(0)
}

/// tdesktop's `remoteBatteryLevelIsLowUpdated`: the call panel shows
/// "<name>'s battery level is low" while it is.
unsafe extern "C" fn on_remote_battery_low(user: *mut c_void, is_low: i32) {
    crate::thread_guard::ffi_boundary((), || {
        // SAFETY: tgcalls passes back the `user` the engine registered.
        let Some(call) = (unsafe { context(user) }) else {
            return;
        };
        call.emit(EngineEvent::PeerBatteryLow {
            call_id: call.call_id,
            is_low: is_low != 0,
        });
    });
}

unsafe extern "C" fn on_signaling(user: *mut c_void, data: *const u8, length: usize) {
    crate::thread_guard::ffi_boundary((), || {
        // SAFETY: tgcalls passes back the `user` the engine registered.
        let Some(call) = (unsafe { context(user) }) else {
            return;
        };
        if data.is_null() || length == 0 {
            return;
        }
        // SAFETY: the library hands `length` bytes at `data` for this call.
        let bytes = unsafe { std::slice::from_raw_parts(data, length) }.to_vec();
        let hook = call.shared.hook.lock_or_recover().clone();
        if let Some(hook) = hook {
            hook(call.call_id, bytes);
        }
    });
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

unsafe extern "C" fn on_remote_video(user: *mut c_void, frame: *const qc_video_frame) {
    // SAFETY: as `deliver_frame` requires.
    unsafe { deliver_frame(user, frame, false) };
}

/// The local camera preview, registered with `qc_video_capture_set_preview`.
pub(super) unsafe extern "C" fn on_local_video(user: *mut c_void, frame: *const qc_video_frame) {
    // SAFETY: as `deliver_frame` requires.
    unsafe { deliver_frame(user, frame, true) };
}

/// # Safety
/// `user` is a live `CallContext` or null; `frame` is null or a picture
/// the library keeps valid for this call.
unsafe fn deliver_frame(user: *mut c_void, frame: *const qc_video_frame, local: bool) {
    crate::thread_guard::ffi_boundary((), || {
        // SAFETY: as the function's contract says.
        let Some(call) = (unsafe { context(user) }) else {
            return;
        };
        let key = LiveKey::Call {
            call_id: call.call_id,
            local,
            screen: false,
        };
        // SAFETY: as the function's contract says.
        unsafe { send_picture(&call.shared, key, frame) };
    });
}

/// Hands one library picture to the live frame hook under `key`.
///
/// # Safety
/// `frame` is null or a picture the library keeps valid for this call.
pub(super) unsafe fn send_picture(
    shared: &CallbackShared,
    key: LiveKey,
    frame: *const qc_video_frame,
) {
    // SAFETY: as the function's contract says.
    let Some(frame) = (unsafe { frame.as_ref() }) else {
        return;
    };
    // SAFETY: the library keeps the planes valid for this call.
    let Some(planes) = (unsafe { i420_planes(frame) }) else {
        return;
    };
    let (Ok(width), Ok(height)) = (u16::try_from(frame.width), u16::try_from(frame.height)) else {
        return;
    };
    let seq = shared.frame_seq.fetch_add(1, Ordering::Relaxed);
    let Some(picture) = LiveFrame::new(
        seq,
        width,
        height,
        live_rotation(frame.rotation),
        planes,
        std::time::Instant::now(),
    ) else {
        return;
    };
    let hook = shared.live_frame_hook.lock_or_recover().clone();
    if let Some(hook) = hook {
        hook(key, picture);
    }
}

/// The picture's planes packed tightly (Y, then U, then V), as live feeds
/// carry I420.
///
/// # Safety
/// Each plane pointer is null or valid for its stride times its rows.
unsafe fn i420_planes(frame: &qc_video_frame) -> Option<Vec<u8>> {
    let width = usize::try_from(frame.width).ok()?;
    let height = usize::try_from(frame.height).ok()?;
    if width == 0 || height == 0 {
        return None;
    }
    let (chroma_width, chroma_height) = (width.div_ceil(2), height.div_ceil(2));
    let mut out = Vec::with_capacity(width * height + 2 * chroma_width * chroma_height);
    for (plane, stride, row, rows) in [
        (frame.y, frame.stride_y, width, height),
        (frame.u, frame.stride_u, chroma_width, chroma_height),
        (frame.v, frame.stride_v, chroma_width, chroma_height),
    ] {
        let stride = usize::try_from(stride).ok()?;
        if plane.is_null() || stride < row {
            return None;
        }
        for line in 0..rows {
            // SAFETY: the caller guarantees `rows` lines of `stride` bytes.
            let bytes = unsafe { std::slice::from_raw_parts(plane.add(line * stride), row) };
            out.extend_from_slice(bytes);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tgcalls_states_map_to_transport_states() {
        assert_eq!(
            transport_state(QC_P2P_WAIT_INIT),
            Some(TransportState::Connecting)
        );
        assert_eq!(
            transport_state(QC_P2P_WAIT_INIT_ACK),
            Some(TransportState::Connecting)
        );
        assert_eq!(
            transport_state(QC_P2P_ESTABLISHED),
            Some(TransportState::Connected)
        );
        assert_eq!(
            transport_state(QC_P2P_RECONNECTING),
            Some(TransportState::Reconnecting)
        );
        assert_eq!(transport_state(QC_P2P_FAILED), Some(TransportState::Failed));
        assert_eq!(transport_state(42), None);
    }

    #[test]
    fn signal_bars_stay_on_tdesktops_scale() {
        let counts: Vec<u8> = [-1, 0, 2, 4, 9].into_iter().map(signal_bar_count).collect();
        assert_eq!(counts, vec![0, 0, 2, 4, 4]);
    }

    #[test]
    fn video_states_and_rotations_map() {
        assert_eq!(
            remote_video_state(QC_VIDEO_ACTIVE),
            RemoteVideoState::Active
        );
        assert_eq!(
            remote_video_state(QC_VIDEO_PAUSED),
            RemoteVideoState::Paused
        );
        assert_eq!(remote_video_state(0), RemoteVideoState::Inactive);
        assert_eq!(live_rotation(90), LiveRotation::R90);
        assert_eq!(live_rotation(270), LiveRotation::R270);
        assert_eq!(live_rotation(0), LiveRotation::None);
    }

    #[test]
    fn strided_planes_are_packed() {
        // 4x2 picture, luma stride 6, chroma 2x1 with stride 3.
        let y: Vec<u8> = (0..12).collect();
        let u = [100, 101, 0];
        let v = [200, 201, 0];
        let frame = qc_video_frame {
            width: 4,
            height: 2,
            rotation: 0,
            timestamp_us: 0,
            y: y.as_ptr(),
            u: u.as_ptr(),
            v: v.as_ptr(),
            stride_y: 6,
            stride_u: 3,
            stride_v: 3,
        };
        // SAFETY: the planes above hold `stride * rows` bytes each.
        let packed = unsafe { i420_planes(&frame) }.unwrap();
        assert_eq!(packed, vec![0, 1, 2, 3, 6, 7, 8, 9, 100, 101, 200, 201]);
    }

    #[test]
    fn a_stride_shorter_than_the_row_is_refused() {
        let y = [0u8; 8];
        let frame = qc_video_frame {
            width: 4,
            height: 2,
            rotation: 0,
            timestamp_us: 0,
            y: y.as_ptr(),
            u: y.as_ptr(),
            v: y.as_ptr(),
            stride_y: 3,
            stride_u: 2,
            stride_v: 2,
        };
        // SAFETY: refused before any plane is read past its end.
        assert!(unsafe { i420_planes(&frame) }.is_none());
    }
}
