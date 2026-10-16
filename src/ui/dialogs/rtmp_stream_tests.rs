//! UI tests for "Stream with other apps": the call button's menu facts for
//! a manager, the box opening only once the server answered, the Stream Key
//! hidden until its warning is confirmed, Revoke behind its confirmation,
//! and "Start Streaming" ending the process. Nothing reaches Telegram: the
//! app has no connection.
use super::demo::{CHANNEL, REQUEST, seed_manager, seed_rtmp_start};
use crate::ui::app::QuillApp;
use crate::ui::group_call_stream_layout::StreamJoinIntent;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::{Root, WindowExt};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, Modifiers, TestAppContext, VisualTestContext, px, size};
use quill::calls::rtmp_start::{StartEntry, start_entries};
use quill::ids::ChatId;
use quill::telegram::envelope::{ChannelMemberStatus, ChatKind, MessageSender};

fn frames(vcx: &mut VisualTestContext) {
    // QuillApp resets reduce-motion when it is built; dialog entrances run
    // on the wall clock, so a click aimed at a mid-slide button misses
    // under load. At rest the layout is the same on every run.
    vcx.update(|_, cx| cx.set_reduce_motion(true));
    vcx.run_until_parked();
    for _ in 0..3 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

fn open_main(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
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
                Some(ScreenshotDemo::named("ready-channels")),
            )
        });
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().unwrap();
    let mut main = VisualTestContext::from_window(handle.into(), cx);
    frames(&mut main);
    (app, main)
}

fn answer(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    app.update(vcx, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            seed_rtmp_start(session);
        }
        cx.notify();
    });
    frames(vcx);
}

fn click(vcx: &mut VisualTestContext, selector: &'static str) {
    let bounds = vcx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} is on screen"));
    vcx.simulate_click(bounds.center(), Modifiers::default());
    frames(vcx);
}

fn toasts(vcx: &mut VisualTestContext) -> usize {
    vcx.update(|window, cx| window.notifications(cx).len())
}

fn start_open(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> bool {
    app.read_with(vcx, |app, _| {
        app.session().is_some_and(|s| s.calls.rtmp_start.is_some())
    })
}

#[gpui_kit::test]
fn a_manager_without_a_call_gets_the_start_menu(cx: &mut TestAppContext) {
    let (app, mut main) = open_main(cx);
    app.update(&mut main, |app, _| {
        if let Some(session) = app.demo_session.as_mut() {
            seed_rtmp_start(session);
            session.calls.rtmp_start = None;
        }
        let facts = app.rtmp_start_facts(ChatId(CHANNEL));
        assert!(facts.manager, "the seeded viewer administers the channel");
        assert_eq!(
            start_entries(facts),
            vec![
                StartEntry::Start,
                StartEntry::Schedule,
                StartEntry::StreamWith
            ]
        );
        // A chat that already has a call joins it instead.
        if let Some(chat) = app
            .demo_session
            .as_mut()
            .and_then(|s| s.chats.get_mut(&CHANNEL))
        {
            chat.video_chat = Some(quill::state::VideoChatInfo {
                group_call_id: 5,
                has_participants: true,
                default_participant_id: None,
            });
        }
        assert!(start_entries(app.rtmp_start_facts(ChatId(CHANNEL))).is_empty());
    });
}

#[gpui_kit::test]
fn the_box_shows_once_the_server_answered(cx: &mut TestAppContext) {
    let (app, mut main) = open_main(cx);
    app.update_in(&mut main, |app, window, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            seed_manager(session);
        }
        app.open_rtmp_start(ChatId(CHANNEL), window, cx);
    });
    frames(&mut main);
    assert!(start_open(&app, &mut main), "the process is running");
    assert!(
        main.debug_bounds("rtmp-start-url").is_none(),
        "no box before the answer"
    );
    answer(&app, &mut main);
    assert!(main.debug_bounds("rtmp-start-url").is_some(), "Server URL");
    assert!(main.debug_bounds("rtmp-start-key").is_some(), "Stream Key");
    assert!(
        main.debug_bounds("rtmp-start-go").is_some(),
        "Start Streaming"
    );
}

#[gpui_kit::test]
fn the_key_stays_hidden_until_the_warning_is_confirmed(cx: &mut TestAppContext) {
    let (app, mut main) = open_main(cx);
    answer(&app, &mut main);
    let shown = |app: &Entity<QuillApp>, vcx: &mut VisualTestContext| {
        app.read_with(vcx, |app, _| app.group_call.rtmp_start.key_shown)
    };
    assert!(!shown(&app, &mut main));
    click(&mut main, "rtmp-start-key");
    assert!(
        main.debug_bounds("rtmp-start-confirm-text").is_some(),
        "the warning asks first"
    );
    assert!(!shown(&app, &mut main), "still hidden behind the warning");
    click(&mut main, "rtmp-start-confirm");
    assert!(shown(&app, &mut main), "revealed after \"I understand\"");
    assert!(main.debug_bounds("rtmp-start-confirm-text").is_none());
}

/// The created stream is tracked (unjoined, as `getGroupCall` leaves it)
/// and the window runs the join you asked for.
fn track_new_stream(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    app.update(vcx, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            if let Some(chat) = session.chats.get_mut(&CHANNEL) {
                chat.video_chat = Some(quill::state::VideoChatInfo {
                    group_call_id: 5,
                    has_participants: false,
                    default_participant_id: None,
                });
            }
            let mut call = quill::state::ActiveGroupCall::fresh(5);
            call.is_video_chat = true;
            call.is_rtmp_stream = true;
            session.calls.active_group_call = Some(call);
        }
        app.sync_group_call_window(cx);
    });
    frames(vcx);
}

fn joined_as(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Option<Option<MessageSender>> {
    app.read_with(vcx, |app, _| {
        app.session()
            .and_then(|s| s.calls.active_group_call.as_ref())
            .filter(|call| call.is_joined)
            .map(|call| call.join_as)
    })
}

#[gpui_kit::test]
fn start_streaming_ends_the_process_and_joins_the_new_call(cx: &mut TestAppContext) {
    let (app, mut main) = open_main(cx);
    answer(&app, &mut main);
    click(&mut main, "rtmp-start-go");
    assert!(!start_open(&app, &mut main), "the process ended");
    assert!(main.debug_bounds("rtmp-start-go").is_none(), "box closed");
    assert!(
        app.read_with(&main, |app, _| matches!(
            app.group_call.stream.pending_join,
            Some((
                StreamJoinIntent::Created {
                    chat: CHANNEL,
                    join_as: None
                },
                _
            ))
        )),
        "an administrator joins as themselves"
    );
    track_new_stream(&app, &mut main);
    assert_eq!(joined_as(&app, &mut main), Some(None), "joined as yourself");
}

#[gpui_kit::test]
fn the_creator_streams_as_the_channel(cx: &mut TestAppContext) {
    let (app, mut main) = open_main(cx);
    answer(&app, &mut main);
    // You created the channel (`StartRtmpProcess::createBox`:
    // `amCreator() ? peer : self`).
    app.update(&mut main, |app, _| {
        if let Some(session) = app.demo_session.as_mut()
            && let Some(ChatKind::Supergroup { supergroup_id, .. }) =
                session.chats.get(&CHANNEL).map(|chat| chat.kind.clone())
        {
            session
                .groups
                .supergroup_member_status
                .insert(supergroup_id, ChannelMemberStatus::Creator);
        }
    });
    click(&mut main, "rtmp-start-go");
    track_new_stream(&app, &mut main);
    assert_eq!(
        joined_as(&app, &mut main),
        Some(Some(MessageSender::Chat { chat_id: CHANNEL })),
        "joined as the channel"
    );
}

#[gpui_kit::test]
fn a_failed_request_is_a_toast_and_opens_no_box(cx: &mut TestAppContext) {
    let (app, mut main) = open_main(cx);
    app.update_in(&mut main, |app, window, cx| {
        app.open_rtmp_start(ChatId(CHANNEL), window, cx)
    });
    let before = toasts(&mut main);
    app.update(&mut main, |app, cx| {
        if let Some(start) = app
            .demo_session
            .as_mut()
            .and_then(|s| s.calls.rtmp_start.as_mut())
        {
            start.sent(REQUEST);
            start.fail(Some(REQUEST));
        }
        cx.notify();
    });
    frames(&mut main);
    assert!(
        !start_open(&app, &mut main),
        "a failed first request drops the process"
    );
    assert_eq!(toasts(&mut main), before + 1, "Internal server error.");
    // A failure after the box showed keeps it.
    answer(&app, &mut main);
    app.update(&mut main, |app, _| {
        if let Some(start) = app
            .demo_session
            .as_mut()
            .and_then(|s| s.calls.rtmp_start.as_mut())
        {
            start.sent(REQUEST);
            start.fail(Some(REQUEST));
        }
    });
    frames(&mut main);
    assert!(start_open(&app, &mut main));
}

#[gpui_kit::test]
fn revoke_asks_first_then_closes_its_confirmation(cx: &mut TestAppContext) {
    let (app, mut main) = open_main(cx);
    answer(&app, &mut main);
    // The "..." menu's "Revoke" item opens this confirmation.
    app.update(&mut main, |app, cx| {
        app.group_call.rtmp_start.confirm = Some(super::RtmpConfirm::Revoke);
        cx.notify();
    });
    frames(&mut main);
    assert!(main.debug_bounds("rtmp-start-confirm-text").is_some());
    click(&mut main, "rtmp-start-confirm");
    assert!(
        app.read_with(&main, |app, _| app.group_call.rtmp_start.confirm.is_none()),
        "Revoke closes the confirmation and asks for a new key"
    );
}

#[gpui_kit::test]
fn start_streaming_during_a_call_asks_to_leave_it_first(cx: &mut TestAppContext) {
    let (app, mut main) = open_main(cx);
    answer(&app, &mut main);
    // You are in another chat's video chat.
    app.update(&mut main, |app, _| {
        if let Some(session) = app.demo_session.as_mut() {
            let mut call = quill::state::ActiveGroupCall::fresh(77);
            call.is_joined = true;
            session.calls.active_group_call = Some(call);
        }
        // Its own window stays closed: the test is about this one.
        app.group_call.window_closed_by_user = Some(77);
    });
    click(&mut main, "rtmp-start-go");
    assert!(
        main.debug_bounds("group-call-create-leave").is_some(),
        "leave the current call?"
    );
    assert!(start_open(&app, &mut main), "nothing created yet");
    click(&mut main, "group-call-create-leave-confirm");
    assert!(!start_open(&app, &mut main), "the stream was created");
    assert!(
        app.read_with(&main, |app, _| app
            .session()
            .is_some_and(|s| s.calls.active_group_call.is_none())),
        "the other call was left"
    );
}
