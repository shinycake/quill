//! UI tests for starting a video chat or live stream: the call button's
//! visibility, the create confirmation with its "schedule" link, the
//! "join as" box, the schedule box (default time, live countdown, range
//! check), the leave-current confirmation (also for a join still in
//! flight, and never for a live story playing in its own slot), joining the created call, and a refused create. Nothing
//! reaches Telegram: the app has no connection, so a confirmed create only
//! records the join, and the tests play the server's part.
use super::{CreateStep, GroupCallCreate};
use crate::ui::app::QuillApp;
use crate::ui::group_call_stream_layout::StreamJoinIntent;
use crate::ui::keybindings::bind_keys;
use crate::ui::scheduled::picked_unix;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::{Root, WindowExt};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, Modifiers, TestAppContext, VisualTestContext, px, size};
use quill::calls::group_call_create::{CreateKind, default_schedule_at};
use quill::ids::{ChatId, RequestId};
use quill::local_time::{now_unix, utc_offset_at};
use quill::state::{ActiveGroupCall, VideoChatInfo};
use quill::telegram::envelope::{ChannelMemberStatus, ChatKind, MessageSender};

/// A channel of the "ready-channels" demo (the create demos use it too).
const CHANNEL: i64 = 13;
/// The id the tests give the created call.
const NEW_CALL: i32 = 5;
/// Another chat's call you are in.
const OTHER_CALL: i32 = 77;
/// A live story's call.
const STORY_CALL: i32 = 4242;
const ME: MessageSender = MessageSender::User { user_id: 1 };

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

fn open(cx: &mut TestAppContext, kind: &'static str) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx
            .new(|cx| QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named(kind))));
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().unwrap();
    let mut main = VisualTestContext::from_window(handle.into(), cx);
    frames(&mut main);
    (app, main)
}

fn open_main(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    open(cx, "ready-channels")
}

/// The viewer's own status in the channel, as `updateSupergroup` gives it.
fn set_status(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, status: ChannelMemberStatus) {
    app.update(vcx, |app, cx| {
        let Some(session) = app.demo_session.as_mut() else {
            return;
        };
        session.my_user_id = Some(1);
        let Some(chat) = session.chats.get_mut(&CHANNEL) else {
            return;
        };
        chat.video_chat = None;
        if let ChatKind::Supergroup { supergroup_id, .. } = chat.kind {
            session
                .groups
                .supergroup_member_status
                .insert(supergroup_id, status);
            session
                .groups
                .supergroup_manage_video_chats_right
                .insert(supergroup_id, status == ChannelMemberStatus::Administrator);
        }
        cx.notify();
    });
}

fn manager(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    set_status(app, vcx, ChannelMemberStatus::Administrator);
}

fn begin(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, kind: CreateKind) {
    app.update_in(vcx, |app, window, cx| {
        app.begin_group_call_create(ChatId(CHANNEL), kind, window, cx)
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

fn flow_open(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> bool {
    app.read_with(vcx, |app, _| app.group_call.start_dialog.is_some())
}

/// The join recorded for the channel's new call, and who it joins as.
fn join_noted(
    app: &Entity<QuillApp>,
    vcx: &mut VisualTestContext,
) -> Option<Option<MessageSender>> {
    app.read_with(vcx, |app, _| match app.group_call.stream.pending_join {
        Some((StreamJoinIntent::Created { chat, join_as }, _)) if chat == CHANNEL => Some(join_as),
        _ => None,
    })
}

fn toasts(vcx: &mut VisualTestContext) -> usize {
    vcx.update(|window, cx| window.notifications(cx).len())
}

/// The server answered the create: the channel's new call is tracked,
/// unjoined, as `getGroupCall` leaves it.
fn track_new_call(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    app.update(vcx, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            if let Some(chat) = session.chats.get_mut(&CHANNEL) {
                chat.video_chat = Some(VideoChatInfo {
                    group_call_id: NEW_CALL,
                    has_participants: false,
                    default_participant_id: None,
                });
            }
            let mut call = ActiveGroupCall::fresh(NEW_CALL);
            call.is_video_chat = true;
            session.calls.active_group_call = Some(call);
        }
        app.sync_group_call_window(cx);
    });
    frames(vcx);
}

fn new_call_joined_as(
    app: &Entity<QuillApp>,
    vcx: &mut VisualTestContext,
) -> Option<Option<MessageSender>> {
    app.read_with(vcx, |app, _| {
        app.session()
            .and_then(|s| s.calls.active_group_call.as_ref())
            .filter(|call| call.id == NEW_CALL && call.is_joined)
            .map(|call| call.join_as)
    })
}

/// You are in (or joining) another chat's video chat; its window stays
/// closed, the test is about this one.
fn in_other_call(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, joined: bool) {
    app.update(vcx, |app, _| {
        if let Some(session) = app.demo_session.as_mut() {
            let mut call = ActiveGroupCall::fresh(OTHER_CALL);
            call.is_joined = joined;
            session.calls.active_group_call = Some(call);
        }
        if !joined {
            // The join went out and waits for its answer.
            app.group_call.stream.joining = Some(OTHER_CALL);
        }
        app.group_call.window_closed_by_user = Some(OTHER_CALL);
    });
}

fn other_call_tracked(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> bool {
    app.read_with(vcx, |app, _| {
        app.session()
            .and_then(|s| s.calls.active_group_call.as_ref())
            .is_some_and(|call| call.id == OTHER_CALL)
    })
}

#[gpui_kit::test]
fn the_call_button_shows_for_managers_and_calls_nobody_is_in(cx: &mut TestAppContext) {
    let (app, mut main) = open_main(cx);
    set_status(&app, &mut main, ChannelMemberStatus::Member);
    let visible = |app: &Entity<QuillApp>, vcx: &mut VisualTestContext| {
        app.read_with(vcx, |app, _| app.chat_call_button_visible(ChatId(CHANNEL)))
    };
    assert!(!visible(&app, &mut main), "a subscriber, no call");
    let set_call = |app: &Entity<QuillApp>, vcx: &mut VisualTestContext, people: bool| {
        app.update(vcx, |app, _| {
            if let Some(chat) = app
                .demo_session
                .as_mut()
                .and_then(|s| s.chats.get_mut(&CHANNEL))
            {
                chat.video_chat = Some(VideoChatInfo {
                    group_call_id: NEW_CALL,
                    has_participants: people,
                    default_participant_id: None,
                });
            }
        })
    };
    set_call(&app, &mut main, false);
    assert!(visible(&app, &mut main), "a call nobody is in yet");
    set_call(&app, &mut main, true);
    assert!(!visible(&app, &mut main), "people are in it: the Join bar");
    manager(&app, &mut main);
    assert!(visible(&app, &mut main), "a manager");
}

#[gpui_kit::test]
fn the_create_demo_shows_the_question_and_create_joins_the_new_call(cx: &mut TestAppContext) {
    // `ready-group-call-create`: a manager pressed Start; with no TDLib
    // the only identity is yourself, so the question shows.
    let (app, mut main) = open(cx, "ready-group-call-create");
    assert!(
        main.debug_bounds("group-call-create-sure").is_some(),
        "the create question"
    );
    assert!(main.debug_bounds("group-call-create-schedule").is_some());
    assert_eq!(join_noted(&app, &mut main), None, "nothing created yet");
    click(&mut main, "group-call-create-confirm");
    assert!(!flow_open(&app, &mut main), "the box closed");
    assert!(
        join_noted(&app, &mut main).is_some(),
        "the join is recorded"
    );
    // The created call is tracked: it is joined right away, as tdesktop's
    // `createGroupCall` does, though it is not a stream.
    track_new_call(&app, &mut main);
    assert!(
        new_call_joined_as(&app, &mut main).is_some(),
        "the new video chat is joined"
    );
    assert_eq!(join_noted(&app, &mut main), None, "the wish is used once");
}

#[gpui_kit::test]
fn cancel_creates_nothing(cx: &mut TestAppContext) {
    let (app, mut main) = open_main(cx);
    manager(&app, &mut main);
    begin(&app, &mut main, CreateKind::Now);
    assert!(main.debug_bounds("group-call-create-sure").is_some());
    click(&mut main, "group-call-create-cancel");
    assert!(main.debug_bounds("group-call-create-sure").is_none());
    assert!(!flow_open(&app, &mut main));
    assert_eq!(join_noted(&app, &mut main), None);
}

#[gpui_kit::test]
fn several_identities_show_the_join_as_box(cx: &mut TestAppContext) {
    let (app, mut main) = open_main(cx);
    manager(&app, &mut main);
    begin(&app, &mut main, CreateKind::Now);
    let channel = MessageSender::Chat { chat_id: CHANNEL };
    // The "join as" list offers you and the channel (`processList`).
    app.update_in(&mut main, |app, window, cx| {
        app.accept_create_join_as(Ok(vec![ME, channel]), window, cx)
    });
    frames(&mut main);
    assert!(main.debug_bounds("group-call-join-as-about").is_some());
    assert!(
        main.debug_bounds("group-call-create-sure").is_none(),
        "no \"personal account\" question"
    );
    assert!(main.debug_bounds("group-call-join-as-u1").is_some());
    assert!(
        main.debug_bounds("group-call-create-schedule").is_some(),
        "an administrator can schedule instead"
    );
    click(&mut main, "group-call-join-as-c13");
    click(&mut main, "group-call-join-as-continue");
    assert!(!flow_open(&app, &mut main));
    assert_eq!(join_noted(&app, &mut main), Some(Some(channel)));
    track_new_call(&app, &mut main);
    assert_eq!(
        new_call_joined_as(&app, &mut main),
        Some(Some(channel)),
        "joined as the channel"
    );
}

#[gpui_kit::test]
fn only_yourself_is_the_question_and_none_is_the_server_error(cx: &mut TestAppContext) {
    let (app, mut main) = open_main(cx);
    manager(&app, &mut main);
    begin(&app, &mut main, CreateKind::Now);
    app.update_in(&mut main, |app, window, cx| {
        app.accept_create_join_as(Ok(vec![ME]), window, cx)
    });
    frames(&mut main);
    assert!(main.debug_bounds("group-call-create-sure").is_some());
    let before = toasts(&mut main);
    app.update_in(&mut main, |app, window, cx| {
        app.accept_create_join_as(Ok(Vec::new()), window, cx)
    });
    frames(&mut main);
    assert!(!flow_open(&app, &mut main));
    assert_eq!(toasts(&mut main), before + 1, "Internal server error.");
}

#[gpui_kit::test]
fn the_schedule_link_opens_the_schedule_box_on_the_next_hour(cx: &mut TestAppContext) {
    let (app, mut main) = open_main(cx);
    manager(&app, &mut main);
    begin(&app, &mut main, CreateKind::Now);
    click(&mut main, "group-call-create-schedule");
    assert!(main.debug_bounds("group-call-schedule-picker").is_some());
    let picked = app.read_with(&main, |app, cx| {
        match app.group_call.start_dialog.as_ref() {
            Some(GroupCallCreate {
                step: CreateStep::Schedule(step),
                ..
            }) => picked_unix(&step.picker, cx),
            _ => None,
        }
    });
    let now = now_unix();
    let expected = default_schedule_at(now, &utc_offset_at);
    // The hour may roll over between the two reads.
    let picked = picked.expect("a default time");
    assert!(
        picked == expected || picked == default_schedule_at(now - 5, &utc_offset_at),
        "{picked} vs {expected}"
    );
    assert!(picked - now >= 30 * 60 - 5, "at least half an hour away");
    assert!(
        main.debug_bounds("group-call-schedule-notified").is_some(),
        "members are told when it starts"
    );
    click(&mut main, "group-call-schedule-confirm");
    assert!(!flow_open(&app, &mut main), "scheduled");
    assert_eq!(
        join_noted(&app, &mut main),
        None,
        "a scheduled chat is not joined"
    );
}

#[gpui_kit::test]
fn the_schedule_demo_refuses_a_time_in_the_past(cx: &mut TestAppContext) {
    let (app, mut main) = open(cx, "ready-group-call-schedule");
    assert!(main.debug_bounds("group-call-schedule-picker").is_some());
    app.update_in(&mut main, |app, window, cx| {
        if let Some(GroupCallCreate {
            step: CreateStep::Schedule(step),
            ..
        }) = app.group_call.start_dialog.as_ref()
            && let Some(value) = crate::ui::scheduled::picker_value(now_unix() - 3600, cx)
        {
            step.picker
                .update(cx, |state, cx| state.set_date_time(value, window, cx));
        }
    });
    frames(&mut main);
    click(&mut main, "group-call-schedule-confirm");
    assert!(flow_open(&app, &mut main), "still open");
    assert!(main.debug_bounds("group-call-schedule-error").is_some());
}

#[gpui_kit::test]
fn starting_while_in_another_call_asks_to_leave_it(cx: &mut TestAppContext) {
    let (app, mut main) = open_main(cx);
    manager(&app, &mut main);
    in_other_call(&app, &mut main, true);
    begin(&app, &mut main, CreateKind::Now);
    assert!(main.debug_bounds("group-call-create-leave").is_some());
    click(&mut main, "group-call-create-leave-confirm");
    // Then the usual question; the other call ends only on Create.
    assert!(main.debug_bounds("group-call-create-sure").is_some());
    assert!(
        other_call_tracked(&app, &mut main),
        "still in the other call"
    );
    click(&mut main, "group-call-create-confirm");
    assert!(!other_call_tracked(&app, &mut main), "left for the new one");
    assert!(join_noted(&app, &mut main).is_some());
}

/// A live story plays in its own slot (tdesktop `registerVideoStream`):
/// it is not a call to leave.
fn story_playing(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    app.update(vcx, |app, _| {
        if let Some(session) = app.demo_session.as_mut() {
            session.calls.story_stream = Some(ActiveGroupCall {
                is_live_story: true,
                is_joined: true,
                ..ActiveGroupCall::fresh(STORY_CALL)
            });
        }
    });
}

fn story_tracked(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> bool {
    app.read_with(vcx, |app, _| {
        app.session()
            .and_then(|s| s.calls.story_stream.as_ref())
            .is_some_and(|call| call.id == STORY_CALL)
    })
}

#[gpui_kit::test]
fn a_playing_live_story_is_not_asked_about(cx: &mut TestAppContext) {
    let (app, mut main) = open_main(cx);
    manager(&app, &mut main);
    story_playing(&app, &mut main);
    begin(&app, &mut main, CreateKind::Now);
    assert!(main.debug_bounds("group-call-create-leave").is_none());
    assert!(main.debug_bounds("group-call-create-sure").is_some());
    click(&mut main, "group-call-create-confirm");
    assert!(join_noted(&app, &mut main).is_some());
    assert!(story_tracked(&app, &mut main), "the story plays on");
}

#[gpui_kit::test]
fn a_real_call_still_asks_with_a_live_story_playing(cx: &mut TestAppContext) {
    let (app, mut main) = open_main(cx);
    manager(&app, &mut main);
    story_playing(&app, &mut main);
    in_other_call(&app, &mut main, true);
    begin(&app, &mut main, CreateKind::Now);
    assert!(main.debug_bounds("group-call-create-leave").is_some());
    click(&mut main, "group-call-create-leave-confirm");
    click(&mut main, "group-call-create-confirm");
    assert!(!other_call_tracked(&app, &mut main), "left for the new one");
    assert!(story_tracked(&app, &mut main), "the story plays on");
}

#[gpui_kit::test]
fn a_join_still_in_flight_counts_as_being_in_that_call(cx: &mut TestAppContext) {
    let (app, mut main) = open_main(cx);
    manager(&app, &mut main);
    in_other_call(&app, &mut main, false);
    begin(&app, &mut main, CreateKind::Now);
    assert!(
        main.debug_bounds("group-call-create-leave").is_some(),
        "the leave question, though the join has not landed"
    );
    click(&mut main, "group-call-create-leave-confirm");
    click(&mut main, "group-call-create-confirm");
    // Left through `leave_active_group_call` (in a live session that is
    // `leaveGroupCall` and the transport teardown), not dropped quietly.
    assert!(!other_call_tracked(&app, &mut main));
    assert!(
        app.read_with(&main, |app, _| app.group_call.stream.joining.is_none()),
        "no join of the old call is awaited any more"
    );
}

#[gpui_kit::test]
fn a_call_only_looked_at_is_dropped_without_leaving_it(cx: &mut TestAppContext) {
    let (app, mut main) = open_main(cx);
    manager(&app, &mut main);
    // Another chat's call window is open, but nobody joined it.
    app.update(&mut main, |app, _| {
        if let Some(session) = app.demo_session.as_mut() {
            session.calls.active_group_call = Some(ActiveGroupCall::fresh(OTHER_CALL));
        }
        // Its own window is not part of this test.
        app.group_call.window_closed_by_user = Some(OTHER_CALL);
    });
    begin(&app, &mut main, CreateKind::Now);
    assert!(
        main.debug_bounds("group-call-create-leave").is_none(),
        "not in that call: no leave question"
    );
    click(&mut main, "group-call-create-confirm");
    assert!(!other_call_tracked(&app, &mut main));
    // Dropped locally: no `leaveGroupCall` for a call that was never
    // joined (TDLib would refuse it and its error would land on the new
    // call).
    let note = app.read_with(&main, |app, _| app.connection.status_note().to_string());
    assert!(!note.contains("left the voice chat"), "{note}");
}

#[gpui_kit::test]
fn a_refused_create_toasts_and_drops_the_join(cx: &mut TestAppContext) {
    let (app, mut main) = open_main(cx);
    manager(&app, &mut main);
    begin(&app, &mut main, CreateKind::Now);
    click(&mut main, "group-call-create-confirm");
    assert!(join_noted(&app, &mut main).is_some());
    let before = toasts(&mut main);
    // The server refuses that `createVideoChat`.
    app.update(&mut main, |app, cx| {
        let request = app.group_call.create_request.expect("the create went out");
        if let Some(session) = app.demo_session.as_mut() {
            // Another request's failure is not this one.
            session.calls.failed_creates.push(RequestId(request.0 + 1));
        }
        cx.notify();
    });
    frames(&mut main);
    assert!(join_noted(&app, &mut main).is_some(), "not ours");
    assert_eq!(toasts(&mut main), before);
    app.update(&mut main, |app, cx| {
        let request = app.group_call.create_request.expect("still waiting");
        if let Some(session) = app.demo_session.as_mut() {
            session.calls.failed_creates.push(request);
        }
        cx.notify();
    });
    frames(&mut main);
    assert_eq!(toasts(&mut main), before + 1, "Internal server error.");
    assert_eq!(join_noted(&app, &mut main), None, "nothing to join");
    assert!(
        app.read_with(&main, |app, _| app
            .session()
            .is_some_and(|s| s.calls.group_call_error.is_none())),
        "no error left for the next call"
    );
}
