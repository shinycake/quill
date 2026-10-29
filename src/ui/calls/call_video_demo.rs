//! Screenshot demo of a 1:1 video call (injected, no live Telegram, no
//! call, no camera): Maya's camera fills the call window and your camera
//! sits mirrored in the corner, both pictures sent at 30 fps through live
//! feeds. The call window opens beside the main window; capture it with
//! `QUILL_DEMO_CAPTURE_WINDOWS=1`. `QUILL_DEMO_SCREEN=1`: Maya shares her
//! screen instead. `QUILL_DEMO_DEVICES=1`: the device menu is open.
//! `QUILL_DEMO_VOICE=1`: the call started as a voice call and both
//! cameras were turned on mid-call. `QUILL_DEMO_BATTERY=1`: Maya's
//! battery is low.
#![deny(clippy::indexing_slicing)]

use super::demo_feed::{DemoFeed, demo_feed};
use crate::ui::app::QuillApp;
use crate::ui::demo::demo_media_allowlist;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use gpui_kit::*;
use quill::calls::engine::{MediaDevice, MediaDeviceKind, RemoteVideoState, TransportState};
use quill::calls::live_video::LiveKey;
use quill::diagnostics::DiagnosticSink;
use quill::state::{ActiveCall, Session};
use quill::telegram::client::copy_and_parse;
use quill::telegram::envelope::{CallState, ReadyParams};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// The demo call and its peer.
pub(super) const CALL: i32 = 4501;
pub(super) const PEER: i64 = 4502;

register_demos![
    DemoSpec::chats(
        "ready-video-call",
        "screenshot demo — 1:1 video call window with your mirrored camera"
    )
    .setup(QuillApp::demo_video_call),
];

impl QuillApp {
    fn demo_video_call(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let screen = std::env::var_os("QUILL_DEMO_SCREEN").is_some();
        self.demo_high_contrast(cx);
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            let sink: Arc<dyn DiagnosticSink> = self.demo_ui.sink.clone();
            seed_video_call(session, screen, &sink, &self.demo_ui.seq);
        }
        let key = |local, screen| LiveKey::Call {
            call_id: CALL,
            local,
            screen,
        };
        let feeds = [
            (key(false, screen), "showcase/p2.jpg"),
            (key(true, false), "showcase/p5.jpg"),
        ];
        for (key, picture) in feeds {
            if let Some(feed) = picture_feed(picture, cx) {
                self.demo_ui.group_feeds.insert(key, feed);
            }
        }
        if std::env::var_os("QUILL_DEMO_DEVICES").is_some() {
            self.demo_ui.call_devices = Some(demo_devices());
            self.demo_ui.selected_devices = (Some("mic-1".into()), Some("spk-1".into()));
        }
        cx.notify();
    }
}

/// The devices the menu lists under `QUILL_DEMO_DEVICES=1`.
fn demo_devices() -> Vec<MediaDevice> {
    let device = |id: &str, name: &str, kind| MediaDevice {
        id: id.into(),
        name: name.into(),
        kind,
    };
    vec![
        device(
            "mic-1",
            "MacBook Pro Microphone",
            MediaDeviceKind::Microphone,
        ),
        device("mic-2", "AirPods Pro", MediaDeviceKind::Microphone),
        device("spk-1", "MacBook Pro Speakers", MediaDeviceKind::Speaker),
        device("spk-2", "AirPods Pro", MediaDeviceKind::Speaker),
    ]
}

/// Maya, and a connected video call with her camera (or screen) on and
/// yours too.
pub(in crate::ui) fn seed_video_call(
    session: &mut Session,
    screen: bool,
    sink: &Arc<dyn DiagnosticSink>,
    seq: &AtomicU64,
) {
    let user = format!(
        r#"{{"@type":"updateUser","user":{{"@type":"user","id":{PEER},"first_name":"Maya","last_name":"Cohen","usernames":null,"phone_number":"","status":null,"profile_photo":null,"is_contact":true,"is_mutual_contact":true,"is_close_friend":false,"is_verified":false,"is_premium":false,"is_scam":false,"is_fake":false,"is_bot":false,"type":{{"@type":"userTypeRegular"}}}}}}"#
    );
    if let Some(owned) = copy_and_parse(&user, seq, sink) {
        session.apply(owned);
    }
    let now = Instant::now();
    let ready_at = now.checked_sub(Duration::from_secs(83)).unwrap_or(now);
    let (remote_video, remote_screen) = if screen {
        (RemoteVideoState::Inactive, RemoteVideoState::Active)
    } else {
        (RemoteVideoState::Active, RemoteVideoState::Inactive)
    };
    session.calls.active_call = Some(ActiveCall {
        id: CALL,
        user_id: PEER,
        is_outgoing: true,
        is_video: std::env::var_os("QUILL_DEMO_VOICE").is_none(),
        state: CallState::Ready,
        started_at: ready_at,
        ready_at: Some(ready_at),
        ready: Some(ReadyParams {
            encryption_key: Vec::new(),
            library_versions: Vec::new(),
            custom_parameters: String::new(),
            servers: Vec::new(),
            allow_p2p: true,
            emojis: ["🐙", "🌵", "🎈", "🍋"].map(String::from).to_vec(),
        }),
        transport: Some(TransportState::Connected),
        transport_error: None,
        signaling_queue: Vec::new(),
        signaling_dropped: 0,
        muted: false,
        camera_on: true,
        screen_sharing: false,
        remote_video,
        remote_screen,
        remote_audio_muted: false,
        remote_battery_low: std::env::var_os("QUILL_DEMO_BATTERY").is_some(),
    });
}

/// A showcase picture, sent at 30 fps.
fn picture_feed(name: &str, cx: &mut App) -> Option<DemoFeed> {
    let rgba = image::open(demo_media_allowlist().join(name))
        .ok()?
        .to_rgba8();
    let width = u16::try_from(rgba.width()).ok()?;
    let height = u16::try_from(rgba.height()).ok()?;
    demo_feed(rgba.as_raw(), width, height, true, cx)
}
