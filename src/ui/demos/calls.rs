//! Screenshot demos: calls.

use crate::ui::app::QuillApp;
use crate::ui::calls::apply_ready_call;
use crate::ui::calls::{
    apply_ready_call_swap, apply_ready_call_video, apply_ready_calls_settings,
    apply_ready_group_call, apply_ready_group_call_invitation, apply_ready_group_call_invite,
    apply_ready_group_call_join_as, apply_ready_group_call_manage, apply_ready_group_call_polish,
    apply_ready_group_call_scheduled,
};
use crate::ui::chat_list::apply_ready_mute_archive;
use crate::ui::demo::demo_video_frame;
use crate::ui::group_calls::demo_group_video_frames;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use crate::ui::secret_chats::apply_ready_chat_ttl;
use gpui_kit::*;
use std::sync::atomic::Ordering;

register_demos![
    // Auto-delete in a regular chat: the header menu's picker with the
    // Custom stepper expanded (injected, no live Telegram).
    DemoSpec::chats(
        "ready-auto-delete",
        "screenshot demo — auto-delete timer in a regular chat (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_ready_auto_delete),
    // Phase C1: call signaling UI (injected, no live Telegram) — an
    // incoming `callStatePending` voice call from Zed, so the overlay
    // renders the ringing incoming-call card (Accept / Decline, clock
    // ticking). Signaling only: the card carries the honest
    // no-audio-transport note.
    DemoSpec::chats(
        "ready-call",
        "screenshot demo — incoming call (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_ready_call),
    // Phase C2c: connected voice call with microphone/speaker choices.
    DemoSpec::chats(
        "ready-call-devices",
        "screenshot demo — connected call audio devices (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_ready_call_devices)
    .window_title("Quill — Call audio devices"),
    // Phase C2d: Ready voice call while the audio driver reconnects.
    DemoSpec::chats(
        "ready-call-reconnecting",
        "screenshot demo — reconnecting call audio (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_ready_call_reconnecting),
    // Phase C2i: connected video call with the 1:1 screen-share send
    // toggle engaged — the overlay shows "Sharing your screen", the
    // "Stop sharing" button, and the local tile's screen-share
    // status. Injected demo state, no live Telegram, no real
    // capture.
    DemoSpec::chats(
        "ready-call-screenshare",
        "screenshot demo — connected video call, screen-share (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_ready_call_screen_share),
    // Phase C2l: connected video call while the *peer* shares their
    // screen — the overlay renders the screen-share tile as the main
    // video-stage tile with the badge, the local camera as the PiP.
    // Injected demo state, no live Telegram, no real frames.
    DemoSpec::chats(
        "ready-call-screenshare-receive",
        "screenshot demo — connected video call, screen-share (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_ready_call_screen_share_receive),
    // Swap prompt: an active outgoing voice call with Zed plus an
    // incoming pending video call from Ada, so the state machine
    // raises the swap prompt and the kit dialog renders ("End &
    // answer" / "Decline"). Injected, no live Telegram.
    DemoSpec::chats(
        "ready-call-swap",
        "screenshot demo — swap prompt (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_ready_call_swap),
    // Phase C1b: a *connected* (`callStateReady`) incoming video call
    // from Zed, so the overlay renders the video-stage placeholder
    // grid (remote + local tiles), the 📹 "Video call" kind line, the
    // duration clock, Mute / Hang up, and the honest no-video-transport
    // note. Signaling only — no live Telegram, no media.
    DemoSpec::chats(
        "ready-call-video",
        "screenshot demo — connected video call (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_ready_call_video),
    // Phase C2i: Recent-calls tab — server-side `searchCallMessages`
    // history (missed / declined / answered) + call settings
    // (injected, no live Telegram).
    DemoSpec::chats(
        "ready-calls-settings",
        "screenshot demo — recent calls + call settings (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_ready_calls_settings),
    // Phase B4: chat-level auto-delete / self-destruct timer (injected,
    // no live Telegram) — the Ready secret chat with Zed (id 41) with
    // `message_auto_delete_time` 3600, one message carrying a live
    // `auto_delete_in` countdown, a
    // `messageChatSetMessageAutoDeleteTime` service row, and the timer
    // picker expanded under the header.
    DemoSpec::chats(
        "ready-chat-ttl",
        "screenshot demo — chat self-destruct timer (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_ready_chat_ttl),
    // Phase C3a: a joined group voice chat (injected, no live
    // Telegram) — the overlay renders the title, participant grid
    // (speaking / muted / hand-raised badges), E2E verification
    // emojis, self controls, admin controls, and the always-visible
    // honest audio-state note (Phase C2g: transport-aware).
    DemoSpec::chats(
        "ready-group-call",
        "screenshot demo — group voice chat (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_ready_group_call),
    // Phase C2f: incoming `messageGroupCall` invitation in a Ready
    // group chat's history, with Accept / Decline (injected, no live
    // Telegram).
    DemoSpec::chats(
        "ready-group-call-invitation",
        "screenshot demo — incoming voice-chat invitation (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_ready_group_call_invitation),
    // Phase C2f: group-call invite picker — the Ready voice chat with
    // the invite panel open (two contacts seeded), per-participant
    // volume steppers and owner-gated Ban buttons (injected, no live
    // Telegram).
    DemoSpec::chats(
        "ready-group-call-invite",
        "screenshot demo — group voice chat invite picker (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_ready_group_call_invite),
    // Calls polish: the "Join as" picker on an unjoined voice chat
    // (injected, no live Telegram).
    DemoSpec::chats(
        "ready-group-call-join-as",
        "screenshot demo — join a voice chat as a channel (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_ready_group_call_join_as),
    // Phase C2h: group-call management surface (injected, no live
    // Telegram) — the Ready voice chat with the rename dialog's
    // sibling state: invite link fetched (Copy/Revoke), recording
    // indicator live, RTMP URL + key fetched, and two in-call chat
    // messages with the composer.
    DemoSpec::chats(
        "ready-group-call-manage",
        "screenshot demo — voice chat management (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_ready_group_call_manage),
    // Calls polish: a pinned video tile plus paused camera and screen
    // streams in a joined voice chat (injected, no live Telegram).
    DemoSpec::chats(
        "ready-group-call-polish",
        "screenshot demo — pinned tile and paused streams (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_ready_group_call_polish),
    // "Notify me when a scheduled video chat starts" (injected, no
    // live Telegram) — a scheduled (not yet started) video chat, so
    // the overlay renders the "Scheduled voice chat" card with the
    // "Starts in …" line, the admin "Start now" button, and the
    // "Notify me when it starts" toggle
    // (`toggleVideoChatEnabledStartNotification`, :14282).
    DemoSpec::chats(
        "ready-group-call-scheduled",
        "screenshot demo — scheduled voice chat (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_ready_group_call_scheduled),
    // Calls live: the pinned stream across the whole window, as the
    // voice chat window shows it in full screen (injected, no live
    // Telegram).
    DemoSpec::chats(
        "ready-group-call-stage",
        "screenshot demo — full-screen pinned stream (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_ready_group_call_stage),
    // Notifications and mute: the open chat's Mute submenu with the
    // Custom duration row expanded, Unmute and Disable sound (injected,
    // no live Telegram).
    DemoSpec::chats(
        "ready-mute-custom",
        "screenshot demo — mute menu with custom duration (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_ready_mute_custom),
];

impl QuillApp {
    fn demo_ready_auto_delete(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        // Auto-delete in a regular chat: 1 week timer, picker with the
        // Custom stepper on 2 weeks.
        if let Some(session) = self.demo_session.as_mut()
            && let Some(chat) = session
                .open_chat
                .and_then(|id| session.chats.get_mut(&id.0))
        {
            chat.message_auto_delete_time = 604_800;
        }
        self.ttl_picker_open = true;
        self.ttl_custom_open = true;
        self.ttl_custom_secs = 1_209_600;
        self.status_note = "screenshot demo — auto-delete timer · custom".into();
    }

    fn demo_ready_call(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        // Phase C1: incoming-call fixture — Zed rings with a pending
        // voice call, so the overlay renders the incoming-call card
        // (Accept / Decline + ticking clock).
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_call(session, &self.demo_sink, &self.demo_seq);
        }
        self.status_note =
            "screenshot demo — incoming call from Zed (injected, no live Telegram)".into();
    }

    fn demo_ready_call_devices(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_call_video(session, &self.demo_sink, &self.demo_seq);
            if let Some(call) = session.active_call.as_mut() {
                call.is_video = false;
                call.transport = Some(quill::calls::engine::TransportState::Connected);
            }
        }
        // Injected demo devices only (no live Telegram, no real
        // hardware); pre-selects the USB headset pair.
        self.demo_call_devices = Some(vec![
            quill::calls::engine::MediaDevice {
                id: "default-mic".into(),
                name: "Default - Built-in Microphone".into(),
                kind: quill::calls::engine::MediaDeviceKind::Microphone,
            },
            quill::calls::engine::MediaDevice {
                id: "usb-headset-mic".into(),
                name: "USB Headset Microphone".into(),
                kind: quill::calls::engine::MediaDeviceKind::Microphone,
            },
            quill::calls::engine::MediaDevice {
                id: "default-sp".into(),
                name: "Default - Built-in Output".into(),
                kind: quill::calls::engine::MediaDeviceKind::Speaker,
            },
            quill::calls::engine::MediaDevice {
                id: "usb-headset-sp".into(),
                name: "USB Headset".into(),
                kind: quill::calls::engine::MediaDeviceKind::Speaker,
            },
        ]);
        self.demo_selected_devices = (
            Some("usb-headset-mic".into()),
            Some("usb-headset-sp".into()),
        );
        self.status_note =
            "screenshot demo — real-audio device selection (injected, no live Telegram)".into();
    }

    fn demo_ready_call_reconnecting(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        // Phase C2d: reconnecting-audio fixture — a ready voice call
        // whose driver is retrying the retained connect parameters.
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_call_video(session, &self.demo_sink, &self.demo_seq);
            if let Some(call) = session.active_call.as_mut() {
                call.is_video = false;
                call.transport = Some(quill::calls::engine::TransportState::Reconnecting);
            }
        }
        self.status_note =
            "screenshot demo — reconnecting call audio (injected, no live Telegram)".into();
    }

    fn demo_ready_call_screen_share(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        // Phase C2i: 1:1 screen-share send fixture — the connected
        // video call with the toggle engaged (injected state, no live
        // Telegram, no real capture). The demo devices carry a screen
        // source so the "Stop sharing" button renders.
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_call_video(session, &self.demo_sink, &self.demo_seq);
        }
        self.demo_call_devices = Some(vec![
            quill::calls::engine::MediaDevice {
                id: "demo-cam".into(),
                name: "Demo Camera (synthetic)".into(),
                kind: quill::calls::engine::MediaDeviceKind::Camera,
            },
            quill::calls::engine::MediaDevice {
                id: "demo-screen".into(),
                name: "Demo Screen (synthetic)".into(),
                kind: quill::calls::engine::MediaDeviceKind::Screen,
            },
        ]);
        self.demo_remote_frame = Some(demo_video_frame(false));
        self.demo_local_frame = Some(demo_video_frame(true));
        if let Some(call) = self
            .demo_session
            .as_mut()
            .and_then(|session| session.active_call.as_mut())
        {
            call.remote_video = quill::calls::engine::RemoteVideoState::Active;
            call.transport = Some(quill::calls::engine::TransportState::Connected);
            call.screen_sharing = true;
            call.camera_on = false;
        }
        self.status_note =
            "screenshot demo — 1:1 call screen-share send (injected, no live Telegram)".into();
    }

    fn demo_ready_call_screen_share_receive(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        // Phase C2l: 1:1 screen-share receive fixture — the peer shares
        // their screen, so the main video-stage tile renders the
        // screen frame with the badge; the local camera stays the PiP.
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_call_video(session, &self.demo_sink, &self.demo_seq);
        }
        self.demo_call_devices = Some(vec![
            quill::calls::engine::MediaDevice {
                id: "demo-cam".into(),
                name: "Demo Camera (synthetic)".into(),
                kind: quill::calls::engine::MediaDeviceKind::Camera,
            },
            quill::calls::engine::MediaDevice {
                id: "demo-screen".into(),
                name: "Demo Screen (synthetic)".into(),
                kind: quill::calls::engine::MediaDeviceKind::Screen,
            },
        ]);
        self.demo_local_frame = Some(demo_video_frame(true));
        self.demo_screen_frame = Some(QuillApp::demo_screen_frame());
        if let Some(call) = self
            .demo_session
            .as_mut()
            .and_then(|session| session.active_call.as_mut())
        {
            call.remote_video = quill::calls::engine::RemoteVideoState::Active;
            call.remote_screen = quill::calls::engine::RemoteVideoState::Active;
            call.transport = Some(quill::calls::engine::TransportState::Connected);
        }
        self.status_note =
            "screenshot demo — 1:1 peer screen-share receive (injected, no live Telegram)".into();
    }

    fn demo_ready_call_swap(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        // Swap prompt: active outgoing call with Zed + incoming pending
        // video call from Ada, so the state machine raises the swap
        // prompt and the kit dialog renders (End & answer / Decline).
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_call_swap(session, &self.demo_sink, &self.demo_seq);
        }
        self.status_note =
            "screenshot demo — swap prompt: Ada calling while in a call with Zed (injected, no live Telegram)".into();
    }

    fn demo_ready_call_video(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        // Phase C1b: connected-video-call fixture — Zed's incoming
        // video call goes pending → exchanging keys → ready, so the
        // overlay renders the video-stage placeholder grid (remote +
        // local tiles), the 📹 kind line, duration clock, Mute / Hang
        // up, and the honest no-video-transport note.
        // Phase C2e: now with synthetic video — an injected camera
        // device, the peer streaming (Active), and two distinct
        // generated test patterns (remote: teal + circle; local: warm +
        // crosshair). NOT a real camera: generated in code, demo only.
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_call_video(session, &self.demo_sink, &self.demo_seq);
        }
        self.demo_call_devices = Some(vec![quill::calls::engine::MediaDevice {
            id: "demo-cam".into(),
            name: "Demo Camera (synthetic)".into(),
            kind: quill::calls::engine::MediaDeviceKind::Camera,
        }]);
        self.demo_remote_frame = Some(demo_video_frame(false));
        self.demo_local_frame = Some(demo_video_frame(true));
        if let Some(call) = self
            .demo_session
            .as_mut()
            .and_then(|session| session.active_call.as_mut())
        {
            call.remote_video = quill::calls::engine::RemoteVideoState::Active;
            call.transport = Some(quill::calls::engine::TransportState::Connected);
            // `QUILL_DEMO_CALL_REMOTE_MUTED=1`: the peer's microphone is off.
            call.remote_audio_muted = std::env::var_os("QUILL_DEMO_CALL_REMOTE_MUTED").is_some();
        }
        self.status_note =
            "screenshot demo — connected video call with Zed (injected, no live Telegram)".into();
    }

    fn demo_ready_calls_settings(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        // Phase C2i: Recent-calls tab with injected `foundMessages`
        // (through the real reducer), privacy values, and the
        // confirm-before-calling pref on.
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_calls_settings(session, &self.demo_sink, &self.demo_seq);
            session.call_prefs.push_to_talk.enabled = true;
            session.call_prefs.push_to_talk.key = "f13".into();
        }
        // `QUILL_DEMO_MIC_TEST=<peak 0..1>`: the microphone test running
        // with a fixed level (no device is opened).
        if let Some(level) = std::env::var("QUILL_DEMO_MIC_TEST")
            .ok()
            .and_then(|v| v.parse::<f32>().ok())
        {
            self.start_mic_test_demo(level);
            self.settings_open = true;
            self.settings_page = Some("Calls");
        }
        self.calls_tab_open = true;
        self.status_note =
            "screenshot demo — recent calls + call settings (injected, no live Telegram)".into();
    }

    fn demo_ready_chat_ttl(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        // Phase B4: chat TTL fixture — the Ready secret chat with a 1h
        // self-destruct timer, a live `auto_delete_in` countdown on one
        // message, the timer-change service row, and the picker expanded.
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_chat_ttl(session, &self.demo_sink, &self.demo_seq);
        }
        self.ttl_picker_open = true;
        self.status_note =
                "screenshot demo — chat self-destruct timer 1h · picker open (injected, no live Telegram)"
                    .into();
    }

    fn demo_ready_group_call(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_group_call(session, &self.demo_sink, &self.demo_seq);
            // Phase C2g: synthetic per-participant video frames for
            // the demo tiles (camera for Zed, screen share for Mia).
            // Injected demo data, not real media.
            self.demo_group_frames = demo_group_video_frames();
            // Slice calls-group-self-tile: the self tile renders the
            // local camera preview (fixture camera is on).
            self.demo_local_frame = Some(demo_video_frame(true));
            // `QUILL_DEMO_SELF_LEVEL=<level>`: your own microphone level on
            // the mute button and your row (tgcalls scale, voice from 1.0).
            if let Some(level) = std::env::var("QUILL_DEMO_SELF_LEVEL")
                .ok()
                .and_then(|v| v.parse::<f32>().ok())
            {
                self.group_call.demo_level = Some(level);
                session.set_group_call_self_muted(false);
            }
        }
        self.status_note = "screenshot demo — group voice chat (injected, no live Telegram)".into();
    }

    fn demo_ready_group_call_invitation(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_group_call_invitation(session, &self.demo_sink, &self.demo_seq);
        }
        self.status_note =
            "screenshot demo — incoming voice-chat invitation (injected, no live Telegram)".into();
    }

    fn demo_ready_group_call_invite(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_group_call_invite(session, &self.demo_sink, &self.demo_seq);
            // Slice calls-group-self-tile: fixture camera is on —
            // the self tile behind the panel renders the preview.
            self.demo_local_frame = Some(demo_video_frame(true));
        }
        self.group_call.invite_open = true;
        self.status_note =
            "screenshot demo — group voice chat invite picker (injected, no live Telegram)".into();
    }

    fn demo_ready_group_call_join_as(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_group_call_join_as(session, &self.demo_sink, &self.demo_seq);
        }
        self.status_note =
            "screenshot demo — join a voice chat as a channel (injected, no live Telegram)".into();
    }

    fn demo_ready_group_call_manage(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_group_call_manage(session, &self.demo_sink, &self.demo_seq);
        }
        self.status_note =
                "screenshot demo — voice chat management: title, invite link, recording, RTMP, chat (injected, no live Telegram)"
                    .into();
    }

    fn demo_ready_group_call_polish(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_group_call_polish(session, &self.demo_sink, &self.demo_seq);
            self.demo_group_frames = demo_group_video_frames();
            self.demo_local_frame = Some(demo_video_frame(true));
        }
        // Pin Zed's camera: the large tile, the rest in the strip.
        self.group_call.pin.toggle(quill::calls::tile_pin::TileKey {
            participant: quill::telegram::envelope::MessageSender::User { user_id: 41 },
            screen: false,
        });
        self.status_note =
            "screenshot demo — pinned tile and paused streams (injected, no live Telegram)".into();
    }

    fn demo_ready_group_call_scheduled(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_group_call_scheduled(session, &self.demo_sink, &self.demo_seq);
        }
        self.status_note =
                "screenshot demo — scheduled voice chat: start time, Start now, notify-me toggle (injected, no live Telegram)"
                    .into();
    }

    fn demo_ready_group_call_stage(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_group_call_polish(session, &self.demo_sink, &self.demo_seq);
            self.demo_group_frames = demo_group_video_frames();
            self.demo_local_frame = Some(demo_video_frame(true));
        }
        // Zed's camera, pinned and shown across the window.
        self.group_call.pin.toggle(quill::calls::tile_pin::TileKey {
            participant: quill::telegram::envelope::MessageSender::User { user_id: 41 },
            screen: false,
        });
        self.demo_group_stage = true;
        self.status_note =
            "screenshot demo — full-screen pinned stream (injected, no live Telegram)".into();
    }

    fn demo_ready_mute_custom(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        // Notifications and mute: chat 11 muted, the Mute submenu open on
        // the Custom duration row (2 days 3 hours).
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_mute_archive(session, &self.demo_sink, &self.demo_seq);
        }
        self.mute_menu_open = true;
        self.mute_custom_open = true;
        self.mute_custom = quill::mute_menu::CustomMute { days: 2, hours: 3 };
        self.status_note = "screenshot demo — mute menu · custom duration".into();
    }
}
