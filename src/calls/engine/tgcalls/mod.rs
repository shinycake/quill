//! `TgcallsEngine`: 1:1 and group calls on Telegram Desktop's own call
//! library, tgcalls, loaded at runtime as `libquill_calls`
//! (docs/decisions/tgcalls-engine.md), and live streams in broadcast
//! mode, whose time and part requests the driver answers.
//!
//! The library has no audio devices, as Telegram Desktop builds it: Quill's
//! microphone capture pushes 10 ms frames in (the external frame sender),
//! and the call's sound comes out to a `StreamPlayer`.
use super::*;
use crate::calls::stream_audio::StreamPlayer;
use arc_swap::ArcSwapOption;
use quill_calls_sys::{Loader, qc_p2p, qc_video_capture};
use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::ptr::NonNull;
use std::sync::{Arc, OnceLock};

mod call_engine;
mod callbacks;
mod config;
mod devices;
mod group;
mod group_callbacks;
mod group_conference;
mod group_config;
mod sender;

use callbacks::CallContext;

/// The library, loaded once and never unloaded (tgcalls keeps its threads).
static LIBRARY: OnceLock<Result<Loader, String>> = OnceLock::new();

fn library() -> Result<&'static Loader, EngineError> {
    LIBRARY
        .get_or_init(|| Loader::load_default().map_err(|err| err.to_string()))
        .as_ref()
        .map_err(|err| {
            crate::advanced::debug_mode::call_log(&format!("[call] tgcalls: {err}"));
            EngineError::Unavailable
        })
}

/// `value` as a C string; a NUL inside fails `op`.
fn text(op: &'static str, value: &str) -> Result<CString, EngineError> {
    CString::new(value).map_err(|_| EngineError::Engine { op, code: -1 })
}

/// Load the library off the UI thread, ahead of the first engine.
pub fn preload_tgcalls() {
    let _ = library();
}

/// One 1:1 call, by TDLib call id.
struct P2pCall {
    context: Arc<CallContext>,
    /// TDLib signaling that arrived before `connect`, in order.
    pending_signaling: Vec<Vec<u8>>,
    handle: Option<NonNull<qc_p2p>>,
    /// Feeder threads' way in; cleared before the handle goes away.
    gate: Arc<sender::Gate>,
    media: CallMediaConfig,
    muted: bool,
    camera: Option<NonNull<qc_video_capture>>,
    /// The external source a screen share pushes into, owned by Quill: the
    /// gate borrows it, so it's freed only after the gate lets go.
    screen: Option<NonNull<qc_video_capture>>,
    sound: Option<StreamPlayer>,
}

pub struct TgcallsEngine {
    api: &'static Loader,
    protocol: EngineProtocol,
    calls: HashMap<i32, P2pCall>,
    /// Group calls, by TDLib group call id.
    groups: HashMap<i32, group::GroupCall>,
    /// The media viewer's volume, for the streams Quill plays.
    stream_volume: f32,
    callback: Arc<CallbackShared>,
}

impl TgcallsEngine {
    pub fn load() -> Result<Self, EngineError> {
        let api = library()?;
        // SAFETY: the library returns a NUL-ended string it keeps for the
        // life of the process.
        let versions = unsafe { CStr::from_ptr((api.qc_library_versions)()) }
            .to_string_lossy()
            .lines()
            .map(str::to_owned)
            .collect();
        // SAFETY: no arguments; reads a constant.
        let max_layer = unsafe { (api.qc_max_layer)() };
        Ok(Self {
            api,
            protocol: EngineProtocol {
                udp_p2p: true,
                udp_reflector: true,
                // tdesktop's kMinLayer (calls_instance.cpp).
                min_layer: 65,
                max_layer,
                library_versions: versions,
            },
            calls: HashMap::new(),
            groups: HashMap::new(),
            stream_volume: 1.0,
            callback: Arc::new(CallbackShared::new()),
        })
    }

    fn call(&self, call_id: i32) -> Result<&P2pCall, EngineError> {
        self.calls
            .get(&call_id)
            .ok_or(EngineError::NoSuchCall(call_id))
    }

    fn call_mut(&mut self, call_id: i32) -> Result<&mut P2pCall, EngineError> {
        self.calls
            .get_mut(&call_id)
            .ok_or(EngineError::NoSuchCall(call_id))
    }

    /// Opens (or reopens, for another speaker) the output the call's sound
    /// plays on. A failed open leaves the call silent and logs why.
    fn open_sound(&mut self, call_id: i32) -> Result<(), EngineError> {
        let call = self.call_mut(call_id)?;
        let speaker = call.media.speaker.clone();
        let name = devices::speaker_name(speaker.as_deref());
        match StreamPlayer::open(name.as_deref(), 1.0, false) {
            Ok(player) => {
                call.context.sound.store(Some(player.audio().clone()));
                call.sound = Some(player);
            }
            Err(err) => {
                crate::advanced::debug_mode::call_log(&format!(
                    "[call] {call_id} sound output did not open ({err})"
                ));
                call.context.sound.store(None);
                call.sound = None;
            }
        }
        Ok(())
    }

    /// Sends `camera` (a device id) or stops sending video for `None`.
    fn apply_camera(&mut self, call_id: i32) -> Result<(), EngineError> {
        let api = self.api;
        let call = self.call_mut(call_id)?;
        let wanted = call.media.camera.clone();
        match (wanted, call.camera) {
            (Some(device), Some(capture)) => {
                let device = text("qc_video_capture_switch_camera", &device)?;
                // SAFETY: `capture` is Quill's live reference.
                unsafe { (api.qc_video_capture_switch_camera)(capture.as_ptr(), device.as_ptr()) };
            }
            (Some(device), None) => {
                let device = text("qc_video_capture_camera", &device)?;
                // SAFETY: a NUL-ended id; the library returns a new source.
                let capture = unsafe { (api.qc_video_capture_camera)(device.as_ptr()) };
                let capture = NonNull::new(capture).ok_or(EngineError::NoCamera)?;
                // SAFETY: the context outlives the preview: it is cleared
                // before the capture is freed (`release_camera`).
                unsafe {
                    (api.qc_video_capture_set_preview)(
                        capture.as_ptr(),
                        Some(callbacks::on_local_video),
                        call.context.as_user(),
                    );
                }
                call.camera = Some(capture);
            }
            (None, Some(_)) => {
                if let Some(handle) = call.handle {
                    // SAFETY: a live call; NULL stops sending video.
                    unsafe {
                        (api.qc_p2p_set_video_capture)(handle.as_ptr(), std::ptr::null_mut())
                    };
                }
                release_camera(api, call);
                return Ok(());
            }
            (None, None) => return Ok(()),
        }
        if let (Some(handle), Some(capture)) = (call.handle, call.camera) {
            // SAFETY: both are live; the call takes its own reference.
            unsafe { (api.qc_p2p_set_video_capture)(handle.as_ptr(), capture.as_ptr()) };
        }
        Ok(())
    }
}

/// Clears the preview and drops Quill's reference to the call's camera.
fn release_camera(api: &Loader, call: &mut P2pCall) {
    if let Some(capture) = call.camera.take() {
        // SAFETY: Quill's live reference; once the preview is cleared no
        // picture reaches the context, then the reference goes.
        unsafe {
            (api.qc_video_capture_set_preview)(capture.as_ptr(), None, std::ptr::null_mut());
            (api.qc_video_capture_free)(capture.as_ptr());
        }
    }
}

/// Stops screen pictures and drops Quill's reference to the share's source.
/// The call must no longer send it (another source, NULL, or destroyed).
fn release_screen(api: &Loader, call: &mut P2pCall) {
    call.gate.set_screen(None);
    if let Some(screen) = call.screen.take() {
        // SAFETY: Quill's live reference; the gate no longer borrows it.
        unsafe { (api.qc_video_capture_free)(screen.as_ptr()) };
    }
}

/// Ends the call in the library: no callback or feeder reaches it after.
fn destroy(api: &Loader, call: &mut P2pCall) {
    call.gate.close();
    if let Some(handle) = call.handle.take() {
        // SAFETY: a live call destroyed once, never from its own callback
        // (the engine runs on the connect driver).
        unsafe { (api.qc_p2p_destroy)(handle.as_ptr()) };
    }
    release_camera(api, call);
    release_screen(api, call);
    call.context.sound.store(None);
    call.sound = None;
}

impl Drop for TgcallsEngine {
    fn drop(&mut self) {
        for call in self.calls.values_mut() {
            destroy(self.api, call);
        }
        for (_, call) in self.groups.drain() {
            group::destroy_group(self.api, call);
        }
    }
}

/// The library's debug line for a call, for the call log.
fn debug_info(api: &Loader, handle: NonNull<qc_p2p>) -> String {
    // SAFETY: a live call; the string is the caller's to free.
    let raw = unsafe { (api.qc_p2p_debug_info)(handle.as_ptr()) };
    take_string(api, raw).unwrap_or_default()
}

/// Copies and frees a string the library allocated for the caller;
/// `None` for NULL.
fn take_string(api: &Loader, raw: *mut std::ffi::c_char) -> Option<String> {
    if raw.is_null() {
        return None;
    }
    // SAFETY: non-null and NUL-ended, freed once below.
    let text = unsafe { CStr::from_ptr(raw) }
        .to_string_lossy()
        .into_owned();
    // SAFETY: allocated by the library for the caller.
    unsafe { (api.qc_string_free)(raw) };
    Some(text)
}

fn new_call(call_id: i32, shared: &Arc<CallbackShared>) -> P2pCall {
    P2pCall {
        context: Arc::new(CallContext {
            call_id,
            shared: Arc::clone(shared),
            sound: ArcSwapOption::empty(),
        }),
        pending_signaling: Vec::new(),
        handle: None,
        gate: Arc::new(sender::Gate::default()),
        media: CallMediaConfig {
            mic: None,
            speaker: None,
            camera: None,
            screen_share_on: false,
        },
        muted: false,
        camera: None,
        screen: None,
        sound: None,
    }
}

/// The engine against the real library, when it's built: a call is made,
/// fed and ended without waiting on the network or a clock.
#[cfg(test)]
mod tests {
    use super::*;

    fn engine() -> Option<TgcallsEngine> {
        match TgcallsEngine::load() {
            Ok(engine) => Some(engine),
            Err(_) => {
                eprintln!("SKIP: libquill_calls not built; run scripts/build-tgcalls.sh");
                None
            }
        }
    }

    fn params(engine: &TgcallsEngine) -> ConnectParams {
        ConnectParams {
            encryption_key: vec![42; 256],
            custom_parameters: String::new(),
            is_outgoing: true,
            servers: Vec::new(),
            library_versions: vec![engine.protocol.library_versions[0].clone()],
            p2p_allowed: true,
            mic_input: None,
            speaker_input: None,
            camera_input: None,
        }
    }

    #[test]
    fn the_protocol_offers_tdesktops_versions() {
        let Some(engine) = engine() else { return };
        let protocol = engine.protocol();
        assert!(protocol.udp_p2p && protocol.udp_reflector);
        assert!(protocol.max_layer >= 92);
        for version in ["2.7.7", "7.0.0", "11.0.0"] {
            assert!(protocol.library_versions.iter().any(|v| v == version));
        }
    }

    #[test]
    fn a_call_is_made_fed_and_ended() {
        let Some(mut engine) = engine() else { return };
        let params = params(&engine);
        engine.start_call(1, 77, true).unwrap();
        // Signaling before connect waits for the call.
        engine.send_signaling_data(1, &[1, 2, 3]).unwrap();
        assert!(
            engine
                .external_frame_sender(CaptureTarget::Private(1))
                .is_none()
        );

        engine.connect(1, &params).unwrap();
        assert!(engine.calls[&1].pending_signaling.is_empty());
        engine.set_muted(1, true).unwrap();
        let sender = engine
            .external_frame_sender(CaptureTarget::Private(1))
            .unwrap();
        let frame = vec![0u8; EXTERNAL_AUDIO_FRAME_BYTES];
        sender
            .send(
                ExternalDevice::Microphone,
                &frame,
                ExternalFrameMeta::default(),
            )
            .unwrap();

        // A retry replaces the instance.
        engine.connect(1, &params).unwrap();

        engine.hangup(1).unwrap();
        assert_eq!(
            sender.send(
                ExternalDevice::Microphone,
                &frame,
                ExternalFrameMeta::default()
            ),
            Err(EngineError::Unavailable)
        );
        assert!(engine.hangup(1).is_ok());
        assert_eq!(engine.set_muted(1, false), Err(EngineError::NoSuchCall(1)));
    }

    #[test]
    fn a_call_without_an_agreed_version_is_refused() {
        let Some(mut engine) = engine() else { return };
        let mut params = params(&engine);
        params.library_versions.clear();
        engine.start_call(2, 77, false).unwrap();
        assert!(engine.connect(2, &params).is_err());
        assert!(
            engine
                .external_frame_sender(CaptureTarget::Private(2))
                .is_none()
        );
    }

    #[test]
    fn screen_share_swaps_the_video_source() {
        let Some(mut engine) = engine() else { return };
        let params = params(&engine);
        engine.start_call(3, 77, true).unwrap();
        engine.connect(3, &params).unwrap();
        engine.set_screen_share_enabled(3, true).unwrap();
        let sender = engine
            .external_frame_sender(CaptureTarget::Private(3))
            .unwrap();
        let meta = ExternalFrameMeta {
            timestamp_ms: 0,
            width: 64,
            height: 48,
        };
        let picture = vec![128u8; 64 * 48 * 3 / 2];
        sender.send(ExternalDevice::Screen, &picture, meta).unwrap();
        // Enabling again keeps the same source going.
        engine.set_screen_share_enabled(3, true).unwrap();
        sender.send(ExternalDevice::Screen, &picture, meta).unwrap();
        engine.set_screen_share_enabled(3, false).unwrap();
        assert_eq!(
            sender.send(ExternalDevice::Screen, &picture, meta),
            Err(EngineError::Unavailable)
        );
        engine.hangup(3).unwrap();
    }

    #[test]
    fn a_share_started_before_connect_survives_a_retry() {
        let Some(mut engine) = engine() else { return };
        let params = params(&engine);
        engine.start_call(4, 77, true).unwrap();
        engine.set_screen_share_enabled(4, true).unwrap();
        let meta = ExternalFrameMeta {
            timestamp_ms: 0,
            width: 64,
            height: 48,
        };
        let picture = vec![128u8; 64 * 48 * 3 / 2];
        for _ in 0..2 {
            engine.connect(4, &params).unwrap();
            assert!(engine.calls[&4].media.screen_share_on);
            let sender = engine
                .external_frame_sender(CaptureTarget::Private(4))
                .unwrap();
            sender.send(ExternalDevice::Screen, &picture, meta).unwrap();
        }
        engine.hangup(4).unwrap();
    }
}
