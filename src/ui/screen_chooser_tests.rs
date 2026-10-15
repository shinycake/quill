//! The screen source chooser and privacy box over the demo session (no
//! live connection, a made-up backend; needs the `demo-capture` feature
//! for gpui-kit's test support).
use super::{initial_selection, shows_stop, split_bold};
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screen_chooser_demo::DemoScreens;
use crate::ui::screen_share_state::ScreenFor;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::calls::capture::screen::{ScreenAccess, ScreenSource, SourceId, SourceKind};
use quill::state::ActiveGroupCall;
use std::sync::Arc;

fn source(id: &str, kind: SourceKind) -> ScreenSource {
    ScreenSource {
        id: SourceId(id.into()),
        kind,
        title: String::new(),
        size: (100, 100),
    }
}

fn new_app(cx: &mut TestAppContext, access: ScreenAccess) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(
                window,
                cx,
                None,
                Some(ScreenshotDemo::named("ready-privacy")),
            )
        });
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let app = slot.borrow().clone().unwrap();
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    app.update_in(&mut vcx, |app, _, _| {
        app.screen_share.backend = Some(Arc::new(DemoScreens { access }));
        let session = app.demo_session.as_mut().unwrap();
        session.calls.active_group_call = Some(ActiveGroupCall {
            is_joined: true,
            ..ActiveGroupCall::fresh(7)
        });
    });
    (app, vcx)
}

fn group_sharing(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> bool {
    app.read_with(vcx, |app, _| {
        app.demo_session
            .as_ref()
            .unwrap()
            .calls
            .active_group_call
            .as_ref()
            .unwrap()
            .screen_sharing
    })
}

#[test]
fn the_first_screen_or_the_shared_source_starts_selected() {
    let sources = [
        source("window:1", SourceKind::Window),
        source("display:1", SourceKind::Screen),
        source("display:2", SourceKind::Screen),
    ];
    assert_eq!(
        initial_selection(&sources, None),
        Some(SourceId("display:1".into()))
    );
    let shared = SourceId("window:1".into());
    assert_eq!(initial_selection(&sources, Some(&shared)), Some(shared));
    // tdesktop selects nothing when the shared source is gone.
    assert_eq!(
        initial_selection(&sources, Some(&SourceId("window:9".into()))),
        None
    );
    assert_eq!(initial_selection(&sources[..1], None), None);
}

#[test]
fn stop_sharing_shows_for_the_shared_source_only() {
    let a = SourceId("a".into());
    let b = SourceId("b".into());
    assert!(!shows_stop(Some(&a), None, false, false));
    assert!(shows_stop(Some(&a), Some(&a), false, false));
    assert!(!shows_stop(Some(&b), Some(&a), false, false));
    assert!(shows_stop(None, Some(&a), false, false));
    // tdesktop: a different audio box is a new share, not a stop.
    assert!(shows_stop(Some(&a), Some(&a), true, true));
    assert!(!shows_stop(Some(&a), Some(&a), true, false));
    assert!(!shows_stop(Some(&a), Some(&a), false, true));
}

#[gpui_kit::test]
fn share_screen_opens_the_chooser_and_shares_the_pick(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, ScreenAccess::Granted);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, _, cx| app.toggle_group_call_screen_share(cx));
    vcx.run_until_parked();
    app.read_with(vcx, |app, _| {
        assert!(
            app.screen_share.window.is_some(),
            "the chooser window opened"
        );
        let labels: Vec<&str> = app
            .screen_share
            .sources
            .iter()
            .map(|entry| entry.label.as_str())
            .collect();
        assert_eq!(
            labels,
            ["Screen 1", "Screen 2", "Release notes", "Window 1"]
        );
        assert!(
            app.screen_share
                .sources
                .iter()
                .all(|e| e.thumbnail.is_some()),
            "every source got its one picture"
        );
        assert_eq!(
            app.screen_share.selected,
            Some(SourceId("display:1".into()))
        );
    });
    assert!(!group_sharing(&app, vcx), "opening alone shares nothing");
    app.update_in(vcx, |app, _, cx| {
        app.select_screen_source(SourceId("window:42".into()), cx);
        app.accept_screen_source(cx);
    });
    vcx.run_until_parked();
    assert!(group_sharing(&app, vcx));
    app.read_with(vcx, |app, _| {
        assert!(app.screen_share.window.is_none(), "Share Screen closes it");
        assert_eq!(
            app.screen_share.demo_active,
            Some(SourceId("window:42".into()))
        );
    });
    // The menu's Stop Sharing goes straight to the driver, without the
    // chooser.
    app.update_in(vcx, |app, _, cx| app.toggle_group_call_screen_share(cx));
    vcx.run_until_parked();
    app.read_with(vcx, |app, _| assert!(app.screen_share.window.is_none()));
}

#[gpui_kit::test]
fn the_chooser_stops_the_shared_source_and_cancel_keeps_it(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, ScreenAccess::Granted);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, _, cx| {
        app.screen_share.demo_active = Some(SourceId("window:42".into()));
        app.demo_session
            .as_mut()
            .unwrap()
            .calls
            .active_group_call
            .as_mut()
            .unwrap()
            .screen_sharing = true;
        assert!(app.open_screen_chooser(ScreenFor::Group, cx));
    });
    vcx.run_until_parked();
    app.update_in(vcx, |app, _, cx| {
        assert_eq!(
            app.screen_share.selected,
            Some(SourceId("window:42".into()))
        );
        app.close_screen_chooser(cx);
    });
    vcx.run_until_parked();
    assert!(group_sharing(&app, vcx), "Cancel changes nothing");
    app.update_in(vcx, |app, _, cx| {
        assert!(app.open_screen_chooser(ScreenFor::Group, cx));
    });
    vcx.run_until_parked();
    app.update_in(vcx, |app, _, cx| app.stop_screen_source(cx));
    vcx.run_until_parked();
    assert!(!group_sharing(&app, vcx));
    app.read_with(vcx, |app, _| {
        assert!(app.screen_share.window.is_none());
        assert!(app.screen_share.demo_active.is_none());
    });
}

#[gpui_kit::test]
fn without_access_the_privacy_box_shows_and_escape_closes_it(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, ScreenAccess::Denied);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, _, cx| {
        app.toggle_group_call_screen_share(cx);
        assert_eq!(app.screen_share.privacy_for, Some(ScreenFor::Group));
        assert!(app.screen_privacy_overlay(true, cx).is_some());
        assert!(
            app.screen_privacy_overlay(false, cx).is_none(),
            "only over the voice chat window"
        );
    });
    vcx.run_until_parked();
    app.update_in(vcx, |app, window, cx| {
        assert!(
            app.screen_share.window.is_none(),
            "no chooser without access"
        );
        assert!(
            app.dismiss_topmost_layer(window, cx),
            "Escape closes the box"
        );
        assert!(app.screen_share.privacy_for.is_none());
    });
    assert!(!group_sharing(&app, vcx));
}

#[gpui_kit::test]
fn without_a_backend_sharing_stays_direct(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, ScreenAccess::Granted);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, _, cx| {
        app.screen_share.backend = None;
        assert!(!app.uses_screen_chooser(), "the demo has no live driver");
        assert!(!app.open_screen_chooser(ScreenFor::Group, cx));
        assert!(app.screen_share.privacy_for.is_none());
    });
}

#[gpui_kit::test]
fn the_system_audio_box_starts_as_the_share_and_changes_it(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, ScreenAccess::Granted);
    let vcx = &mut vcx;
    // Without loopback (macOS, Linux) there is no box, and Share Screen
    // never asks for audio.
    app.update_in(vcx, |app, _, cx| {
        assert!(!app.offers_system_audio());
        app.screen_share.with_audio = true;
        assert!(app.open_screen_chooser(ScreenFor::Group, cx));
    });
    vcx.run_until_parked();
    app.update_in(vcx, |app, _, cx| {
        assert!(!app.screen_share.with_audio, "reset on open");
        app.screen_share.with_audio = true;
        app.accept_screen_source(cx);
        assert!(!app.screen_share.demo_active_audio);
        app.stop_screen_source(cx);
    });
    vcx.run_until_parked();

    // As on Windows: the box starts as the running share has it.
    app.update_in(vcx, |app, _, cx| {
        app.screen_share.demo_system_audio = true;
        assert!(app.open_screen_chooser(ScreenFor::Group, cx));
    });
    vcx.run_until_parked();
    app.update_in(vcx, |app, _, cx| {
        assert!(!app.screen_share.with_audio, "not sharing: off");
        app.set_screen_audio(true, cx);
        app.accept_screen_source(cx);
        assert_eq!(
            app.screen_share.demo_active,
            Some(SourceId("display:1".into()))
        );
        assert!(app.screen_share.demo_active_audio);
    });
    vcx.run_until_parked();
    assert!(group_sharing(&app, vcx));
    app.update_in(vcx, |app, _, cx| {
        assert!(app.open_screen_chooser(ScreenFor::Group, cx));
    });
    vcx.run_until_parked();
    app.update_in(vcx, |app, _, cx| {
        assert!(app.screen_share.with_audio, "the share has audio");
        // Clearing it on the shared screen shares again without audio
        // instead of stopping.
        app.set_screen_audio(false, cx);
        app.accept_screen_source(cx);
        assert!(!app.screen_share.demo_active_audio);
    });
    vcx.run_until_parked();
    assert!(group_sharing(&app, vcx));
}

#[test]
fn the_privacy_text_bolds_its_marked_words() {
    let (plain, bold) = split_bold("Please allow **Screen Recording** for Quill.");
    assert_eq!(plain, "Please allow Screen Recording for Quill.");
    assert_eq!(bold, vec![std::ops::Range { start: 13, end: 29 }]);
    assert_eq!(split_bold("plain"), ("plain".to_string(), Vec::new()));
}

/// The Wayland backend's shape: the portal picks, so one unique source.
struct PortalScreens;

impl quill::calls::capture::screen::ScreenBackend for PortalScreens {
    fn access(&self) -> ScreenAccess {
        ScreenAccess::Granted
    }

    fn open_privacy_settings(&self) {}

    fn unique_source(&self) -> Option<SourceId> {
        Some(SourceId("portal".into()))
    }

    fn sources(&self) -> Result<Vec<ScreenSource>, quill::calls::capture::CaptureError> {
        panic!("the chooser never lists with a unique source");
    }

    fn thumbnail(
        &self,
        _source: &ScreenSource,
        _max: (u32, u32),
    ) -> Option<quill::calls::capture::screen::Thumbnail> {
        None
    }

    fn start(
        &self,
        _source: &SourceId,
        _options: quill::calls::capture::screen::ScreenOptions,
        _sink: quill::calls::capture::FrameSink<quill::calls::capture::VideoFrame>,
        _preview: Arc<quill::calls::capture::screen::PreviewSlot>,
    ) -> Result<
        Box<dyn quill::calls::capture::screen::ScreenCapture>,
        quill::calls::capture::CaptureError,
    > {
        Err(quill::calls::capture::CaptureError::Unsupported(
            "demo".into(),
        ))
    }
}

#[gpui_kit::test]
fn with_a_unique_source_the_button_shares_and_stops_without_the_chooser(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, ScreenAccess::Granted);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, _, cx| {
        app.screen_share.backend = Some(Arc::new(PortalScreens));
        app.toggle_group_call_screen_share(cx);
    });
    vcx.run_until_parked();
    assert!(group_sharing(&app, vcx), "shared at once");
    app.read_with(vcx, |app, _| {
        assert!(app.screen_share.window.is_none(), "no chooser window");
        assert!(app.screen_share.sources.is_empty());
        assert_eq!(
            app.screen_share.demo_active,
            Some(SourceId("portal".into()))
        );
        assert!(!app.screen_share.demo_active_audio);
    });
    // Pressed again while sharing (the 1:1 button always comes here):
    // tdesktop stops the share.
    app.update_in(vcx, |app, _, cx| {
        assert!(app.open_screen_chooser(ScreenFor::Group, cx));
    });
    vcx.run_until_parked();
    assert!(!group_sharing(&app, vcx));
    app.read_with(vcx, |app, _| {
        assert!(app.screen_share.window.is_none());
        assert!(app.screen_share.demo_active.is_none());
    });
}

#[gpui_kit::test]
fn a_one_to_one_call_toggles_the_unique_source_too(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, ScreenAccess::Granted);
    let vcx = &mut vcx;
    let sharing = |app: &Entity<QuillApp>, vcx: &mut VisualTestContext| {
        app.read_with(vcx, |app, _| {
            app.demo_session
                .as_ref()
                .unwrap()
                .calls
                .active_call
                .as_ref()
                .unwrap()
                .screen_sharing
        })
    };
    app.update_in(vcx, |app, _, cx| {
        app.screen_share.backend = Some(Arc::new(PortalScreens));
        let session = app.demo_session.as_mut().unwrap();
        session.calls.active_group_call = None;
        session.calls.active_call = Some(quill::state::ActiveCall {
            id: 5,
            user_id: 1,
            is_outgoing: true,
            is_video: false,
            state: quill::telegram::envelope::CallState::Pending {
                is_created: true,
                is_received: true,
            },
            started_at: std::time::Instant::now(),
            ready_at: None,
            ready: None,
            transport: None,
            transport_error: None,
            signaling_queue: Vec::new(),
            signaling_dropped: 0,
            muted: false,
            camera_on: false,
            screen_sharing: false,
            remote_video: quill::calls::engine::RemoteVideoState::Inactive,
            remote_screen: quill::calls::engine::RemoteVideoState::Inactive,
            remote_audio_muted: false,
        });
        assert!(app.open_screen_chooser(ScreenFor::Private(5), cx));
    });
    vcx.run_until_parked();
    assert!(sharing(&app, vcx));
    app.update_in(vcx, |app, _, cx| {
        assert!(app.open_screen_chooser(ScreenFor::Private(5), cx));
    });
    vcx.run_until_parked();
    assert!(!sharing(&app, vcx));
}
