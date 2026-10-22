//! Two ntgcalls engines in one process, calling each other over
//! localhost: the signaling bytes TDLib would relay are handed across
//! directly. Developer only (needs the library, opens UDP sockets on
//! loopback): `QUILL_VERIFY_NATIVE_CALLS=1 QUILL_NTGCALLS_LIB=<lib>
//! cargo test --no-default-features --lib loopback_p2p -- --nocapture`.
//!
//! Every source is external, so no camera, microphone or speaker opens.
use super::*;
use crate::calls::live_video::LiveKey;
use ntgcalls_sys::{NTG_STREAM_DEVICE_CAMERA, ntg_frame_data};
use std::sync::Mutex;
use std::sync::mpsc;
use std::time::{Duration, Instant};

const KEY: [u8; 256] = [7; 256];

struct Side {
    engine: NtgcallsEngine,
    call_id: i32,
    peer: i64,
    signaling: mpsc::Receiver<Vec<u8>>,
    frames: Arc<Mutex<Vec<LiveKey>>>,
    remote_video: Arc<Mutex<Vec<RemoteVideoState>>>,
    transport: Arc<Mutex<Vec<TransportState>>>,
}

fn side(call_id: i32, peer: i64, outgoing: bool) -> Side {
    let mut engine = NtgcallsEngine::load().expect("ntgcalls library");
    let (tx, signaling) = mpsc::channel();
    engine.set_signaling_emitted_callback(Arc::new(move |_, data| {
        let _ = tx.send(data);
    }));
    let frames = Arc::new(Mutex::new(Vec::new()));
    let sink = frames.clone();
    engine.set_live_frame_callback(Arc::new(move |key, _| sink.lock_or_recover().push(key)));
    let remote_video = Arc::new(Mutex::new(Vec::new()));
    let sink = remote_video.clone();
    engine.set_remote_video_state_callback(Arc::new(move |_, state| {
        sink.lock_or_recover().push(state)
    }));
    let transport = Arc::new(Mutex::new(Vec::new()));
    let sink = transport.clone();
    engine
        .set_transport_state_callback(Arc::new(move |_, state| sink.lock_or_recover().push(state)));
    engine
        .start_call(call_id, peer, outgoing)
        .expect("p2p call");
    Side {
        engine,
        call_id,
        peer,
        signaling,
        frames,
        remote_video,
        transport,
    }
}

fn external_video() -> ntg_video_description {
    ntg_video_description {
        media_source: NTG_MEDIA_SOURCE_EXTERNAL,
        width: 320,
        height: 240,
        fps: 30,
        input: null_mut(),
        keep_open: false,
    }
}

/// Capture with the external microphone and, when `camera`, an external
/// camera; playback with external camera and screen writers (as
/// `set_media_sources` issues them) and no sound device.
fn issue_sources(side: &mut Side, camera: bool) {
    let instance = side.engine.ensure_instance().expect("instance");
    let mut mic = external::capture_microphone_description(true, None);
    let mut camera_video = external_video();
    let capture = ntg_media_description {
        microphone: &mut mic,
        speaker: null_mut(),
        camera: if camera {
            &mut camera_video
        } else {
            null_mut()
        },
        screen: null_mut(),
    };
    // SAFETY: the instance is live; the descriptions outlive the call.
    let rc = unsafe {
        (side.engine.api.ntg_set_stream_sources)(
            instance.as_ptr(),
            side.peer,
            NTG_STREAM_MODE_CAPTURE,
            capture,
        )
    };
    assert_eq!(rc, NTG_OK, "capture sources");
    let mut peer_camera = external_video();
    let mut peer_screen = external_video();
    let playback = ntg_media_description {
        microphone: null_mut(),
        speaker: null_mut(),
        camera: &mut peer_camera,
        screen: &mut peer_screen,
    };
    // SAFETY: as above.
    let rc = unsafe {
        (side.engine.api.ntg_set_stream_sources)(
            instance.as_ptr(),
            side.peer,
            NTG_STREAM_MODE_PLAYBACK,
            playback,
        )
    };
    assert_eq!(rc, NTG_OK, "playback sources");
}

fn connect(side: &mut Side, outgoing: bool, camera: bool) {
    let instance = side.engine.ensure_instance().expect("instance");
    // SAFETY: the instance is live and the key outlives the call.
    let rc = unsafe {
        (side.engine.api.ntg_skip_exchange)(
            instance.as_ptr(),
            side.peer,
            KEY.as_ptr(),
            KEY.len(),
            outgoing,
        )
    };
    assert_eq!(rc, NTG_OK, "skip exchange");
    issue_sources(side, camera);
    let version = CString::new("13.0.0").unwrap();
    let versions = [version.as_ptr()];
    // SAFETY: as above; no servers, so only host candidates are used.
    let rc = unsafe {
        (side.engine.api.ntg_connect_p2p)(
            instance.as_ptr(),
            side.peer,
            std::ptr::null(),
            0,
            versions.as_ptr(),
            versions.len(),
            true,
            std::ptr::null(),
        )
    };
    assert_eq!(rc, NTG_OK, "connect p2p");
}

fn pump(a: &mut Side, b: &mut Side, for_how_long: Duration, mut tick: impl FnMut(&mut Side)) {
    let until = Instant::now() + for_how_long;
    while Instant::now() < until {
        while let Ok(data) = a.signaling.try_recv() {
            let _ = b.engine.send_signaling_data(b.call_id, &data);
        }
        while let Ok(data) = b.signaling.try_recv() {
            let _ = a.engine.send_signaling_data(a.call_id, &data);
        }
        tick(a);
        std::thread::sleep(Duration::from_millis(33));
    }
}

fn send_camera_frame(side: &Side, n: u64) -> i32 {
    let instance = side.engine.instance.expect("instance");
    let mut i420 = vec![0_u8; 320 * 240 * 3 / 2];
    for (i, px) in i420.iter_mut().enumerate().take(320 * 240) {
        *px = ((i as u64 + n * 4) % 256) as u8;
    }
    let meta = ntg_frame_data {
        absolute_capture_timestamp_ms: 0,
        rotation: ntgcalls_sys::NTG_VIDEO_ROTATION_VIDEO_ROTATION_0,
        width: 320,
        height: 240,
    };
    // SAFETY: the instance is live; ntgcalls copies the bytes.
    unsafe {
        (side.engine.api.ntg_send_external_frame)(
            instance.as_ptr(),
            side.peer,
            NTG_STREAM_DEVICE_CAMERA,
            i420.as_ptr(),
            i420.len(),
            meta,
        )
    }
}

fn report(label: &str, side: &Side) -> (usize, Vec<RemoteVideoState>) {
    let remote = side
        .frames
        .lock_or_recover()
        .iter()
        .filter(|key| {
            matches!(
                key,
                LiveKey::Call {
                    local: false,
                    screen: false,
                    ..
                }
            )
        })
        .count();
    let states = side.remote_video.lock_or_recover().clone();
    eprintln!(
        "{label}: transport {:?}, remote camera states {states:?}, peer frames {remote}",
        side.transport.lock_or_recover()
    );
    (remote, states)
}

fn run(camera_from_start: bool) {
    if std::env::var_os("QUILL_VERIFY_NATIVE_CALLS").is_none() {
        return;
    }
    let mut a = side(1, 200, true);
    let mut b = side(2, 100, false);
    connect(&mut a, true, camera_from_start);
    connect(&mut b, false, false);
    pump(&mut a, &mut b, Duration::from_secs(4), |_| {});
    if !camera_from_start {
        issue_sources(&mut a, true);
    }
    let mut n = 0;
    let mut send_errors = 0;
    pump(&mut a, &mut b, Duration::from_secs(5), |a| {
        n += 1;
        if send_camera_frame(a, n) != NTG_OK {
            send_errors += 1;
        }
    });
    eprintln!("sent {n} camera frames, {send_errors} refused");
    report("caller", &a);
    let (frames, states) = report("callee", &b);
    let _ = a.engine.hangup(a.call_id);
    let _ = b.engine.hangup(b.call_id);
    assert!(
        states.last() == Some(&RemoteVideoState::Active),
        "the callee hears the camera is on"
    );
    assert!(frames > 0, "the callee gets the caller's camera");
}

#[test]
fn loopback_p2p_camera_from_the_start() {
    run(true);
}

#[test]
fn loopback_p2p_camera_turned_on_mid_call() {
    run(false);
}
