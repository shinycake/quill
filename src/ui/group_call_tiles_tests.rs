//! UI tests for the voice chat's video tiles over the screen-share demo
//! (your hidden window is paused, another camera sends): the paused tile
//! takes the live controller's paused overlay, and the full-screen stage
//! shows a volume slider for anyone but you. Nothing reaches Telegram.
use crate::ui::app::QuillApp;
use crate::ui::frame_clock::{frames_over, take_slice_renders};
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AppContext, Bounds, Entity, Modifiers, Pixels, ScrollDelta, ScrollEvent, TestAppContext,
    VisualTestContext, point, px, size,
};
use quill::calls::tile_pin::TileKey;
use quill::telegram::envelope::MessageSender;
use std::sync::Arc;

fn frames(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    for _ in 0..2 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

fn find(vcx: &mut VisualTestContext, selector: &'static str) -> Option<Bounds<Pixels>> {
    vcx.debug_bounds(selector)
}

/// The demo's call window; returns the app, the main window and the call
/// window.
fn open_share(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext, VisualTestContext) {
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
                Some(ScreenshotDemo::named("ready-call-screen-share")),
            )
        });
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().expect("the app");
    let mut main = VisualTestContext::from_window(handle.into(), cx);
    frames(&mut main);
    app.update(&mut main, |app, cx| {
        app.close_screen_chooser(cx);
        app.sync_group_call_window(cx);
    });
    frames(&mut main);
    let window = app
        .read_with(&main, |app, _| app.group_call.window)
        .expect("the call window opens");
    let mut call = VisualTestContext::from_window(window, cx);
    frames(&mut call);
    (app, main, call)
}

/// You and the other participant.
fn people(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> (MessageSender, MessageSender) {
    app.read_with(vcx, |app, _| {
        let call = app
            .session()
            .and_then(|s| s.calls.active_group_call.as_ref())
            .expect("the call");
        let me = call
            .participants
            .iter()
            .find(|p| p.is_current_user)
            .map(|p| p.participant_id)
            .expect("you");
        let other = call
            .participants
            .iter()
            .find(|p| !p.is_current_user)
            .map(|p| p.participant_id)
            .expect("the other participant");
        (me, other)
    })
}

#[gpui_kit::test]
fn a_paused_tile_takes_the_shared_paused_overlay(cx: &mut TestAppContext) {
    let (_app, _main, mut call) = open_share(cx);
    let call = &mut call;
    assert!(
        find(call, "live-paused").is_some(),
        "your hidden window's tile reads paused"
    );
}

#[gpui_kit::test]
fn the_stage_shows_the_volume_of_anyone_but_you(cx: &mut TestAppContext) {
    let (app, _main, mut call) = open_share(cx);
    let call = &mut call;
    let (me, other) = people(&app, call);
    app.update_in(call, |app, window, cx| {
        app.toggle_group_call_tile_pin(
            TileKey {
                participant: other,
                screen: false,
            },
            cx,
        );
        app.enter_group_call_stage(window, cx);
    });
    frames(call);
    assert!(find(call, "group-call-stage").is_some(), "the stage");
    assert!(find(call, "group-stage-volume").is_some(), "their volume");
    app.update(call, |app, cx| {
        app.toggle_group_call_tile_pin(
            TileKey {
                participant: me,
                screen: true,
            },
            cx,
        );
    });
    frames(call);
    assert!(
        find(call, "group-call-stage").is_some(),
        "your share on the stage"
    );
    assert!(find(call, "group-stage-volume").is_none(), "none for you");
}

/// The microphone's halo plays by ticks for the call window alone: while it
/// moves, the main window draws nothing for it.
#[gpui_kit::test]
fn the_halo_redraws_the_call_window_alone(cx: &mut TestAppContext) {
    let (app, mut main, mut call) = open_share(cx);
    main.update(|window, _| window.activate_window());
    let _ = frames_over(&mut main, 3);
    app.update(&mut main, |app, cx| {
        let live = app.demo_session.as_deref_mut();
        if let Some(call) = live.and_then(|s| s.calls.active_group_call.as_mut()) {
            call.is_muted_self = false;
        }
        let now = app.group_call_now_ms();
        app.group_call.level_anim.retarget(1.0, now);
        cx.notify();
    });
    let _ = frames_over(&mut call, 1);
    let drawn = frames_over(&mut call, 2);
    let panel = drawn.get("group-call-panel").copied().unwrap_or(0);
    assert!(panel > 5, "the halo moves: {drawn:?}");
    for slice in ["app", "sidebar", "conversation", "composer"] {
        assert_eq!(drawn.get(slice), None, "{drawn:?}");
    }
}

fn crowd(app: &Entity<QuillApp>, count: i64, vcx: &mut VisualTestContext) {
    app.update(vcx, |app, cx| {
        let session = app.demo_session.as_mut().expect("the demo session");
        let call = session.calls.active_group_call.as_mut().expect("the call");
        let participants = Arc::make_mut(&mut call.participants);
        let listener = participants
            .iter()
            .find(|p| !p.is_current_user)
            .cloned()
            .expect("the other participant");
        for person in 0..count {
            participants.push(quill::telegram::envelope::ParsedGroupCallParticipant {
                participant_id: MessageSender::User {
                    user_id: 5_000 + person,
                },
                audio_source_id: 5_000 + person as i32,
                video_enabled: false,
                video_info: None,
                screen_sharing_enabled: false,
                screen_sharing_video_info: None,
                ..listener.clone()
            });
        }
        cx.notify();
    });
    frames(vcx);
}

#[gpui_kit::test]
fn video_frames_redraw_the_panel_and_build_no_member_row(cx: &mut TestAppContext) {
    let (app, _main, mut call) = open_share(cx);
    crowd(&app, 100, &mut call);
    let drawn = frames_over(&mut call, 1);
    let panel = drawn.get("group-call-panel").copied().unwrap_or(0);
    assert!(panel > 5, "the camera keeps sending: {drawn:?}");
    assert_eq!(drawn.get("group-member-row"), None, "{drawn:?}");
}

#[gpui_kit::test]
fn a_change_to_the_call_builds_only_the_rows_on_screen(cx: &mut TestAppContext) {
    let (app, _main, mut call) = open_share(cx);
    crowd(&app, 100, &mut call);
    let _ = take_slice_renders();
    app.update(&mut call, |_, cx| cx.notify());
    frames(&mut call);
    let rows = take_slice_renders()
        .get("group-member-row")
        .copied()
        .unwrap_or(0);
    assert!(rows > 0, "the rows follow the call");
    assert!(rows < 30, "{rows} rows built");
}

#[gpui_kit::test]
fn the_wheel_builds_the_rows_it_brings_in(cx: &mut TestAppContext) {
    let (app, _main, mut call) = open_share(cx);
    crowd(&app, 100, &mut call);
    assert!(find(&mut call, "group-member-row-5000").is_some());
    assert!(find(&mut call, "group-member-row-5099").is_none());

    let first_row = find(&mut call, "group-member-row-5000").unwrap_or_default();
    let over_the_list = first_row.center();
    call.simulate_mouse_move(over_the_list, None, Modifiers::none());
    call.simulate_event(ScrollEvent {
        position: over_the_list,
        delta: ScrollDelta::Pixels(point(px(0.), px(-100_000.))),
        ..Default::default()
    });
    frames(&mut call);
    assert!(find(&mut call, "group-member-row-5099").is_some());
    assert!(find(&mut call, "group-member-row-5000").is_none());
}

#[gpui_kit::test]
fn opening_the_chat_scrolls_past_a_long_list_to_it(cx: &mut TestAppContext) {
    let (app, _main, mut call) = open_share(cx);
    crowd(&app, 100, &mut call);
    let offset = |app: &QuillApp| app.group_call.panel_scroll.offset().y;
    assert_eq!(app.read_with(&call, |app, _| offset(app)), px(0.));

    app.update(&mut call, |app, cx| app.toggle_group_call_messages(cx));
    frames(&mut call);

    let (at, end) = app.read_with(&call, |app, _| {
        (offset(app), app.group_call.panel_scroll.max_offset().y)
    });
    assert!(end > px(0.));
    assert_eq!(at, -end);
}
