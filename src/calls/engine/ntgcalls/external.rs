//! External sources on the ntgcalls engine: the microphone description
//! for Quill's own capture, and the `Send` frame sender a feeder thread
//! drives (`ntg_send_external_frame`).
//!
//! ntgcalls registers an external reader per device when the stream
//! description's source is `NTG_MEDIA_SOURCE_EXTERNAL`
//! (`stream_manager.cpp`) and rejects frames for a device that was not set
//! up that way, so the description is issued with the call's sources and
//! the sender is handed out only after that.
use super::*;
use crate::locks::RwLockExt;
use ntgcalls_sys::{
    NTG_STREAM_DEVICE_MICROPHONE, NTG_STREAM_DEVICE_SCREEN, NTG_VIDEO_ROTATION_VIDEO_ROTATION_0,
    ntg_audio_description, ntg_frame_data, ntg_result, ntg_stream_device,
};
use std::sync::RwLock;

type SendExternalFrame = unsafe extern "C" fn(
    *mut ntg_instance,
    i64,
    ntg_stream_device,
    *const u8,
    usize,
    ntg_frame_data,
) -> ntg_result;

/// The live native instance, as feeder threads may use it.
#[derive(Clone, Copy)]
struct LiveInstance {
    instance: NonNull<ntg_instance>,
    send: SendExternalFrame,
}

// SAFETY: the pointer is only dereferenced by ntgcalls inside
// `ntg_send_external_frame`, whose C API is safe to call from any thread
// (it posts to ntgcalls' own worker). `ExternalGate` guarantees the
// instance is alive for the whole call: `detach` takes the write lock,
// which waits for every in-flight send, before the instance is destroyed.
unsafe impl Send for LiveInstance {}
// SAFETY: see `Send`; the value is never mutated through a shared reference.
unsafe impl Sync for LiveInstance {}

/// Lets feeder threads reach the native instance while it lives, and
/// shuts them out before it is destroyed.
#[derive(Default)]
pub(crate) struct ExternalGate {
    live: RwLock<Option<LiveInstance>>,
}

impl ExternalGate {
    /// The engine created `instance`; senders may use it from now on.
    pub(crate) fn attach(&self, instance: NonNull<ntg_instance>, send: SendExternalFrame) {
        *self.live.write_or_recover() = Some(LiveInstance { instance, send });
    }

    /// The engine is about to destroy its instance. Returns once no send
    /// is in flight; later sends fail with `Unavailable`.
    pub(crate) fn detach(&self) {
        *self.live.write_or_recover() = None;
    }
}

/// One call's external sources (native chat id = the peer's user id for
/// a 1:1 call, the group's chat id for a group call).
struct NtgcallsFrameSender {
    gate: Arc<ExternalGate>,
    chat_id: i64,
}

impl ExternalFrameSender for NtgcallsFrameSender {
    fn send(
        &self,
        device: ExternalDevice,
        data: &[u8],
        meta: ExternalFrameMeta,
    ) -> Result<(), EngineError> {
        if !external_frame_valid(device, data.len(), meta) {
            return Err(EngineError::Engine {
                op: "ntg_send_external_frame",
                code: NTG_ERR_INVALID_PARAMS,
            });
        }
        let native_device = match device {
            ExternalDevice::Microphone => NTG_STREAM_DEVICE_MICROPHONE,
            ExternalDevice::Screen => NTG_STREAM_DEVICE_SCREEN,
        };
        let frame = ntg_frame_data {
            absolute_capture_timestamp_ms: meta.timestamp_ms,
            rotation: NTG_VIDEO_ROTATION_VIDEO_ROTATION_0,
            width: meta.width,
            height: meta.height,
        };
        let live = self.gate.live.read_or_recover();
        let Some(live) = live.as_ref() else {
            return Err(EngineError::Unavailable);
        };
        // SAFETY: the read guard keeps the instance alive (see
        // `LiveInstance`); `data` is a live slice of `data.len()` bytes that
        // ntgcalls copies before returning.
        let rc = unsafe {
            (live.send)(
                live.instance.as_ptr(),
                self.chat_id,
                native_device,
                data.as_ptr(),
                data.len(),
                frame,
            )
        };
        if rc == NTG_OK {
            Ok(())
        } else {
            Err(EngineError::Engine {
                op: "ntg_send_external_frame",
                code: rc,
            })
        }
    }
}

impl NtgcallsEngine {
    /// The microphone metadata for a call's capture sources: none when
    /// Quill captures the microphone itself, otherwise the device JSON
    /// ntgcalls opens (`QUILL_CALL_CAPTURE=native`).
    pub(super) fn capture_microphone_input(
        &self,
        selected: Option<&str>,
    ) -> Result<Option<CString>, EngineError> {
        if external_capture_enabled() {
            return Ok(None);
        }
        self.device_input(selected, MediaDeviceKind::Microphone)
    }

    /// A sender for `target`, once its sources are issued.
    pub(super) fn external_sender_for(
        &self,
        target: CaptureTarget,
    ) -> Option<Arc<dyn ExternalFrameSender>> {
        let chat_id = match target {
            CaptureTarget::Private(call_id) => {
                // `call_media` is set when `connect` issues the sources.
                self.call_media.get(&call_id)?;
                self.call_to_user.get(&call_id).copied()?
            }
            CaptureTarget::Group(group_call_id) => {
                self.group_calls
                    .get(&group_call_id)
                    .filter(|media| media.connected)?
                    .chat_id
            }
        };
        Some(Arc::new(NtgcallsFrameSender {
            gate: self.external_gate.clone(),
            chat_id,
        }))
    }
}

/// The capture-side microphone description: Quill's 48 kHz mono frames
/// when it captures the microphone (`external`), the ntgcalls device
/// otherwise.
pub(super) fn capture_microphone_description(
    external: bool,
    input: Option<&CString>,
) -> ntg_audio_description {
    if external {
        ntg_audio_description {
            media_source: NTG_MEDIA_SOURCE_EXTERNAL,
            sample_rate: EXTERNAL_AUDIO_RATE,
            channel_count: 1,
            input: null_mut(),
            keep_open: false,
        }
    } else {
        audio_description(input)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn microphone_description_is_external_unless_native() {
        let external = capture_microphone_description(true, None);
        assert_eq!(external.media_source, NTG_MEDIA_SOURCE_EXTERNAL);
        assert_eq!(external.sample_rate, 48_000);
        assert_eq!(external.channel_count, 1);
        assert!(external.input.is_null());
        // QUILL_CALL_CAPTURE=native: ntgcalls opens the device itself.
        let metadata = CString::new("{\"uid\":\"\"}").unwrap();
        let native = capture_microphone_description(false, Some(&metadata));
        assert_eq!(native.media_source, NTG_MEDIA_SOURCE_DEVICE);
        assert_eq!(native.input.cast_const(), metadata.as_ptr());
    }

    /// Loopback harness (developer only, needs the ntgcalls library):
    /// `QUILL_NTGCALLS_LIB=<libntgcalls> cargo test --lib loopback_external
    /// -- --ignored`. A local
    /// instance with a 1:1 call that never connects: external
    /// microphone and screen sources only, so no device opens and no
    /// packet leaves the process. 100 frames each must be accepted.
    #[test]
    #[ignore = "needs the ntgcalls library (vendor/ntgcalls)"]
    fn loopback_external_frames_are_accepted() {
        let mut engine = NtgcallsEngine::load().expect("ntgcalls library");
        engine.start_call(1, 42, true).expect("local call");
        engine.call_media.insert(
            1,
            CallMediaConfig {
                mic: None,
                speaker: None,
                camera: None,
                camera_enabled: false,
                screen_share_on: false,
            },
        );
        let instance = engine.instance.expect("live instance");
        let mut mic = capture_microphone_description(true, None);
        let mut screen = ntg_video_description {
            media_source: NTG_MEDIA_SOURCE_EXTERNAL,
            width: 320,
            height: 240,
            fps: 15,
            input: null_mut(),
            keep_open: false,
        };
        let capture = ntg_media_description {
            microphone: &mut mic,
            speaker: null_mut(),
            camera: null_mut(),
            screen: &mut screen,
        };
        // SAFETY: `instance` is live; the descriptions outlive this call.
        let rc = unsafe {
            (engine.api.ntg_set_stream_sources)(
                instance.as_ptr(),
                42,
                NTG_STREAM_MODE_CAPTURE,
                capture,
            )
        };
        assert_eq!(rc, NTG_OK, "external sources issued");
        let sender = engine
            .external_frame_sender(CaptureTarget::Private(1))
            .expect("sender once sources exist");
        let mut clock = crate::calls::capture::CaptureClock::new();
        let pcm: Vec<u8> = (0..EXTERNAL_AUDIO_FRAME_SAMPLES)
            .flat_map(|i| (((i % 96) as i16 - 48) * 200).to_le_bytes())
            .collect();
        for i in 0..100 {
            let meta = ExternalFrameMeta {
                timestamp_ms: clock.map(std::time::Instant::now()),
                width: 0,
                height: 0,
            };
            sender
                .send(ExternalDevice::Microphone, &pcm, meta)
                .unwrap_or_else(|err| panic!("audio frame {i}: {err}"));
        }
        let i420 = vec![128_u8; 320 * 240 * 3 / 2];
        for i in 0..100 {
            let meta = ExternalFrameMeta {
                timestamp_ms: clock.map(std::time::Instant::now()),
                width: 320,
                height: 240,
            };
            sender
                .send(ExternalDevice::Screen, &i420, meta)
                .unwrap_or_else(|err| panic!("screen frame {i}: {err}"));
        }
        // A wrong size is refused before it reaches ntgcalls.
        assert!(
            sender
                .send(
                    ExternalDevice::Microphone,
                    &pcm[2..],
                    ExternalFrameMeta::default()
                )
                .is_err()
        );
        engine.hangup(1).expect("teardown");
        drop(engine);
        // The instance is gone: the sender fails cleanly, never crashes.
        assert_eq!(
            sender.send(
                ExternalDevice::Microphone,
                &pcm,
                ExternalFrameMeta::default()
            ),
            Err(EngineError::Unavailable)
        );
    }
}
