//! Two ntgcalls engines in one process, calling each other over
//! localhost: the signaling bytes TDLib would relay are handed across
//! directly. Developer only (needs the library, opens UDP sockets on
//! loopback): `QUILL_VERIFY_NATIVE_CALLS=1 QUILL_NTGCALLS_LIB=<lib>
//! cargo test --no-default-features --lib loopback_p2p -- --nocapture`.
//!
//! Every source is external, so no camera or microphone opens; only
//! the sound test's callee plays on an output (you hear a tone).
use super::*;
use crate::calls::live_video::LiveKey;
use ntgcalls_sys::{NTG_STREAM_DEVICE_CAMERA, NTG_STREAM_DEVICE_SCREEN, ntg_frame_data};
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
    remote_screen: Arc<Mutex<Vec<RemoteVideoState>>>,
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
    let remote_screen = Arc::new(Mutex::new(Vec::new()));
    let sink = remote_screen.clone();
    engine.set_remote_screen_state_callback(Arc::new(move |_, state| {
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
        remote_screen,
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

/// What the caller sends.
#[derive(Clone, Copy, PartialEq)]
enum Send {
    Nothing,
    Camera,
    Screen,
}

/// Capture with the external microphone and an external camera or
/// screen; playback with external camera and screen writers (as
/// `set_media_sources` issues them) and no sound device.
fn issue_sources(side: &mut Side, send: Send) {
    let instance = side.engine.ensure_instance().expect("instance");
    let mut mic = external::capture_microphone_description(true, None);
    let mut camera_video = external_video();
    let mut screen_video = external_video();
    let capture = ntg_media_description {
        microphone: &mut mic,
        speaker: null_mut(),
        camera: if send == Send::Camera {
            &mut camera_video
        } else {
            null_mut()
        },
        screen: if send == Send::Screen {
            &mut screen_video
        } else {
            null_mut()
        },
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

fn connect(side: &mut Side, outgoing: bool, send: Send) {
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
    issue_sources(side, send);
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

fn send_frame(side: &Side, send: Send, n: u64) -> i32 {
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
            if send == Send::Screen {
                NTG_STREAM_DEVICE_SCREEN
            } else {
                NTG_STREAM_DEVICE_CAMERA
            },
            i420.as_ptr(),
            i420.len(),
            meta,
        )
    }
}

fn report(label: &str, side: &Side, screen: bool) -> (usize, Vec<RemoteVideoState>) {
    let remote = side
        .frames
        .lock_or_recover()
        .iter()
        .filter(|key| matches!(key, LiveKey::Call { local: false, screen: s, .. } if *s == screen))
        .count();
    let states = if screen {
        side.remote_screen.lock_or_recover().clone()
    } else {
        side.remote_video.lock_or_recover().clone()
    };
    eprintln!(
        "{label}: transport {:?}, remote states {states:?}, peer frames {remote}",
        side.transport.lock_or_recover()
    );
    (remote, states)
}

/// The caller sends `send`, from the start or turned on after the call
/// connected; the callee must see it on and get its frames.
fn run(send: Send, from_start: bool) {
    if std::env::var_os("QUILL_VERIFY_NATIVE_CALLS").is_none() {
        return;
    }
    let mut a = side(1, 200, true);
    let mut b = side(2, 100, false);
    connect(&mut a, true, if from_start { send } else { Send::Nothing });
    connect(&mut b, false, Send::Nothing);
    pump(&mut a, &mut b, Duration::from_secs(4), |_| {});
    if !from_start {
        issue_sources(&mut a, send);
    }
    let mut n = 0;
    let mut send_errors = 0;
    pump(&mut a, &mut b, Duration::from_secs(5), |a| {
        n += 1;
        if send_frame(a, send, n) != NTG_OK {
            send_errors += 1;
        }
    });
    eprintln!("sent {n} frames, {send_errors} refused");
    let screen = send == Send::Screen;
    report("caller", &a, screen);
    let (frames, states) = report("callee", &b, screen);
    let _ = a.engine.hangup(a.call_id);
    let _ = b.engine.hangup(b.call_id);
    assert!(
        states.last() == Some(&RemoteVideoState::Active),
        "the callee hears the video is on"
    );
    assert!(frames > 0, "the callee gets the caller's frames");
}

#[test]
fn loopback_p2p_camera_from_the_start() {
    run(Send::Camera, true);
}

#[test]
fn loopback_p2p_camera_turned_on_mid_call() {
    run(Send::Camera, false);
}

#[test]
fn loopback_p2p_screen_share_mid_call() {
    run(Send::Screen, false);
}

#[test]
fn loopback_p2p_screen_share_from_the_start() {
    run(Send::Screen, true);
}

/// One 10 ms frame of a 440 Hz tone, from sample `start`.
fn tone(start: usize) -> Vec<u8> {
    (start..start + crate::calls::engine::EXTERNAL_AUDIO_FRAME_SAMPLES)
        .map(|n| {
            let t = n as f32 / 48_000.0;
            ((t * 440.0 * std::f32::consts::TAU).sin() * 12_000.0) as i16
        })
        .flat_map(i16::to_le_bytes)
        .collect()
}

/// The caller sends a tone from its external microphone; the callee
/// connects through [`CallEngine::connect`] as a real call does, so its
/// sound plays on Quill's output (UI builds) or the device (core builds,
/// listen for it).
#[test]
fn loopback_p2p_sound_reaches_the_callee() {
    if std::env::var_os("QUILL_VERIFY_NATIVE_CALLS").is_none() {
        return;
    }
    let mut a = side(1, 200, true);
    let mut b = side(2, 100, false);
    connect(&mut a, true, Send::Nothing);
    let params = ConnectParams {
        encryption_key: KEY.to_vec(),
        custom_parameters: String::new(),
        is_outgoing: false,
        servers: Vec::new(),
        library_versions: vec!["13.0.0".into()],
        p2p_allowed: true,
        mic_input: None,
        speaker_input: None,
        camera_input: None,
    };
    b.engine
        .connect(b.call_id, &params)
        .expect("callee connect");
    pump(&mut a, &mut b, Duration::from_secs(3), |_| {});
    let instance = a.engine.instance.expect("instance");
    let mut clock = crate::calls::capture::CaptureClock::new();
    let mut sample = 0;
    pump(&mut a, &mut b, Duration::from_secs(5), |a| {
        // Three 10 ms frames per 33 ms pump.
        for _ in 0..3 {
            let pcm = tone(sample);
            sample += crate::calls::engine::EXTERNAL_AUDIO_FRAME_SAMPLES;
            let meta = ntg_frame_data {
                absolute_capture_timestamp_ms: clock.map(Instant::now()),
                rotation: ntgcalls_sys::NTG_VIDEO_ROTATION_VIDEO_ROTATION_0,
                width: 0,
                height: 0,
            };
            // SAFETY: the instance is live; ntgcalls copies the bytes.
            unsafe {
                (a.engine.api.ntg_send_external_frame)(
                    instance.as_ptr(),
                    a.peer,
                    ntgcalls_sys::NTG_STREAM_DEVICE_MICROPHONE,
                    pcm.as_ptr(),
                    pcm.len(),
                    meta,
                )
            };
        }
    });
    let health = b.engine.call_sound_health(b.call_id);
    eprintln!(
        "callee transport {:?}, sound {health:?}",
        b.transport.lock_or_recover()
    );
    let _ = a.engine.hangup(a.call_id);
    let _ = b.engine.hangup(b.call_id);
    if cfg!(feature = "ui") {
        let health = health.expect("Quill plays the call");
        assert!(health.frames > 100, "the caller's sound arrives");
        assert!(health.peak > 6_000, "and is the tone, not silence");
    }
}
