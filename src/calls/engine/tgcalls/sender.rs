//! Feeder threads' way into a call (`calls::capture`): microphone frames to
//! `qc_p2p_push_audio` (a 1:1 call) or `qc_group_push_audio` (a group
//! call), screen pictures to the call's external video source (a group
//! call's is its presentation's). The gate is closed before the call or
//! source goes away, and a push holds the gate, so none reaches a freed
//! handle.
use super::super::{
    EXTERNAL_AUDIO_FRAME_SAMPLES, EngineError, ExternalDevice, ExternalFrameMeta,
    ExternalFrameSender, external_frame_valid,
};
use crate::locks::LockExt;
use quill_calls_sys::{Loader, qc_group, qc_p2p, qc_video_capture, qc_video_frame};
use std::sync::Mutex;

#[derive(Default)]
pub(super) struct Gate(Mutex<Open>);

#[derive(Default)]
struct Open {
    api: Option<&'static Loader>,
    call: Option<CallPtr>,
    screen: Option<CapturePtr>,
}

/// The call microphone frames go to.
enum CallPtr {
    P2p(*mut qc_p2p),
    Group(*mut qc_group),
}
struct CapturePtr(*mut qc_video_capture);

// SAFETY: the library's push functions may be called from any thread; the
// pointers are only used under the gate's lock while the engine keeps them
// alive.
unsafe impl Send for CallPtr {}
// SAFETY: as above.
unsafe impl Send for CapturePtr {}

impl Gate {
    pub(super) fn open(&self, api: &'static Loader, call: *mut qc_p2p) {
        let mut open = self.0.lock_or_recover();
        open.api = Some(api);
        open.call = Some(CallPtr::P2p(call));
    }

    pub(super) fn open_group(&self, api: &'static Loader, group: *mut qc_group) {
        let mut open = self.0.lock_or_recover();
        open.api = Some(api);
        open.call = Some(CallPtr::Group(group));
    }

    /// The external source screen pictures go to; `None` stops them.
    pub(super) fn set_screen(&self, screen: Option<*mut qc_video_capture>) {
        self.0.lock_or_recover().screen = screen.map(CapturePtr);
    }

    pub(super) fn close(&self) {
        *self.0.lock_or_recover() = Open::default();
    }
}

impl ExternalFrameSender for Gate {
    fn send(
        &self,
        device: ExternalDevice,
        data: &[u8],
        meta: ExternalFrameMeta,
    ) -> Result<(), EngineError> {
        if !external_frame_valid(device, data.len(), meta) {
            return Err(EngineError::Engine {
                op: "qc_external_frame",
                code: -1,
            });
        }
        let open = self.0.lock_or_recover();
        let (Some(api), Some(call)) = (open.api, open.call.as_ref()) else {
            return Err(EngineError::Unavailable);
        };
        match device {
            ExternalDevice::Microphone => {
                let samples = microphone_samples(data);
                match call {
                    // SAFETY: the call is live while the gate is open and
                    // held; `samples` holds one 10 ms frame.
                    CallPtr::P2p(call) => unsafe {
                        (api.qc_p2p_push_audio)(*call, samples.as_ptr(), samples.len());
                    },
                    // SAFETY: as above.
                    CallPtr::Group(group) => unsafe {
                        (api.qc_group_push_audio)(*group, samples.as_ptr(), samples.len());
                    },
                }
            }
            ExternalDevice::Screen => {
                let screen = open.screen.as_ref().ok_or(EngineError::Unavailable)?;
                let frame = i420_frame(data, meta);
                // SAFETY: the source is live while the gate is open and
                // held; `frame` points into `data` for this call only.
                unsafe { (api.qc_video_capture_push_i420)(screen.0, &frame) };
            }
        }
        Ok(())
    }
}

/// A 10 ms frame of native-endian 16-bit samples, as the feeder sends it.
fn microphone_samples(data: &[u8]) -> [i16; EXTERNAL_AUDIO_FRAME_SAMPLES] {
    let mut samples = [0i16; EXTERNAL_AUDIO_FRAME_SAMPLES];
    let (pairs, _) = data.as_chunks::<2>();
    for (sample, pair) in samples.iter_mut().zip(pairs) {
        *sample = i16::from_ne_bytes(*pair);
    }
    samples
}

/// One tightly packed I420 picture (checked by `external_frame_valid`).
fn i420_frame(data: &[u8], meta: ExternalFrameMeta) -> qc_video_frame {
    let (width, height) = (usize::from(meta.width), usize::from(meta.height));
    let luma = width * height;
    let chroma = luma / 4;
    let base = data.as_ptr();
    qc_video_frame {
        width: i32::from(meta.width),
        height: i32::from(meta.height),
        rotation: 0,
        timestamp_us: meta.timestamp_ms.saturating_mul(1000),
        y: base,
        // SAFETY: `data` holds luma + 2 * chroma bytes (validated).
        u: unsafe { base.add(luma) },
        // SAFETY: as above.
        v: unsafe { base.add(luma + chroma) },
        stride_y: i32::from(meta.width),
        stride_u: i32::from(meta.width / 2),
        stride_v: i32::from(meta.width / 2),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn microphone_bytes_become_samples() {
        let mut data = vec![0u8; EXTERNAL_AUDIO_FRAME_SAMPLES * 2];
        data[..2].copy_from_slice(&1234i16.to_ne_bytes());
        data[2..4].copy_from_slice(&(-5i16).to_ne_bytes());
        let samples = microphone_samples(&data);
        assert_eq!(&samples[..3], &[1234, -5, 0]);
    }

    #[test]
    fn i420_planes_follow_each_other() {
        let data = vec![0u8; 4 * 2 * 3 / 2];
        let meta = ExternalFrameMeta {
            timestamp_ms: 7,
            width: 4,
            height: 2,
        };
        let frame = i420_frame(&data, meta);
        assert_eq!(frame.u as usize - frame.y as usize, 8);
        assert_eq!(frame.v as usize - frame.u as usize, 2);
        assert_eq!((frame.stride_y, frame.stride_u), (4, 2));
        assert_eq!(frame.timestamp_us, 7000);
    }

    #[test]
    fn a_closed_gate_refuses_frames() {
        let gate = Gate::default();
        let frame = vec![0u8; EXTERNAL_AUDIO_FRAME_SAMPLES * 2];
        assert_eq!(
            gate.send(
                ExternalDevice::Microphone,
                &frame,
                ExternalFrameMeta::default()
            ),
            Err(EngineError::Unavailable)
        );
    }
}
