//! UI tests for starting a video chat or live stream: the call button's
//! visibility, the create confirmation with its "schedule" link, the
//! schedule box (default time, live countdown, range check) and the
//! leave-current confirmation. Nothing reaches Telegram: the app has no
//! connection, so a confirmed create only records the join.
use super::{CreateStep, GroupCallCreate};
use crate::ui::app::QuillApp;
use crate::ui::group_call_stream_layout::StreamJoinIntent;
use crate::ui::keybindings::bind_keys;
use crate::ui::scheduled::picked_unix;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, Modifiers, TestAppContext, VisualTestContext, px, size};
use quill::calls::group_call_create::{CreateKind, default_schedule_at};
use quill::ids::ChatId;
use quill::local_time::{now_unix, utc_offset_at};
use quill::telegram::envelope::{ChannelMemberStatus, ChatKind};

/// A channel of the "ready-channels" demo.
const CHANNEL: i64 = 13;

fn frames(vcx: &mut VisualTestContext) {
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

/// The viewer's own status in the channel, as `updateSupergroup` gives it.
fn set_status(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, manager: bool) {
    app.update(vcx, |app, cx| {
        let Some(session) = app.demo_session.as_mut() else {
            return;
        };
        let Some(chat) = session.chats.get_mut(&CHANNEL) else {
            return;
        };
        chat.video_chat = None;
        if let ChatKind::Supergroup { supergroup_id, .. } = chat.kind {
            session.groups.supergroup_member_status.insert(
                supergroup_id,
                if manager {
                    ChannelMemberStatus::Administrator
                } else {
                    ChannelMemberStatus::Member
                },
            );
            session
                .groups
                .supergroup_manage_video_chats_right
                .insert(supergroup_id, manager);
        }
        cx.notify();
    });
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

fn join_noted(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> bool {
    app.read_with(vcx, |app, _| {
        matches!(
            app.group_call.stream.pending_join,
            Some((StreamJoinIntent::Chat(CHANNEL), _))
        )
    })
}

#[gpui_kit::test]
fn the_call_button_shows_for_managers_and_calls_nobody_is_in(cx: &mut TestAppContext) {
    let (app, mut main) = open_main(cx);
    set_status(&app, &mut main, false);
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
                chat.video_chat = Some(quill::state::VideoChatInfo {
                    group_call_id: 5,
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
    set_status(&app, &mut main, true);
    assert!(visible(&app, &mut main), "a manager");
}

#[gpui_kit::test]
fn start_asks_first_and_create_starts_it(cx: &mut TestAppContext) {
    let (app, mut main) = open_main(cx);
    set_status(&app, &mut main, true);
    begin(&app, &mut main, CreateKind::Now);
    assert!(
        main.debug_bounds("group-call-create-sure").is_some(),
        "the create question"
    );
    assert!(main.debug_bounds("group-call-create-schedule").is_some());
    assert!(!join_noted(&app, &mut main), "nothing created yet");
    click(&mut main, "group-call-create-confirm");
    assert!(!flow_open(&app, &mut main), "the box closed");
    assert!(join_noted(&app, &mut main), "the new call is joined");
}

#[gpui_kit::test]
fn cancel_creates_nothing(cx: &mut TestAppContext) {
    let (app, mut main) = open_main(cx);
    set_status(&app, &mut main, true);
    begin(&app, &mut main, CreateKind::Now);
    let bounds = main
        .debug_bounds("group-call-create-sure")
        .expect("the question");
    assert!(bounds.size.width > px(0.));
    app.update(&mut main, |app, cx| {
        app.close_group_call_create(cx);
    });
    frames(&mut main);
    assert!(main.debug_bounds("group-call-create-sure").is_none());
    assert!(!join_noted(&app, &mut main));
}

#[gpui_kit::test]
fn the_schedule_link_opens_the_schedule_box_on_the_next_hour(cx: &mut TestAppContext) {
    let (app, mut main) = open_main(cx);
    set_status(&app, &mut main, true);
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
    assert!(
        !join_noted(&app, &mut main),
        "a scheduled chat is not joined"
    );
}

#[gpui_kit::test]
fn a_time_in_the_past_is_refused(cx: &mut TestAppContext) {
    let (app, mut main) = open_main(cx);
    set_status(&app, &mut main, true);
    begin(&app, &mut main, CreateKind::Schedule);
    app.update_in(&mut main, |app, window, cx| {
        if let Some(GroupCallCreate {
            step: CreateStep::Schedule(step),
            ..
        }) = app.group_call.start_dialog.as_ref()
            && let Some(value) = crate::ui::scheduled::picker_value(now_unix() - 3600)
        {
            step.picker
                .update(cx, |state, cx| state.set_date_time(value, window, cx));
        }
    });
    frames(&mut main);
    click(&mut main, "group-call-schedule-confirm");
    assert!(flow_open(&app, &mut main), "still open");
    assert!(main.debug_bounds("group-call-schedule-error").is_some());
    assert!(main.debug_bounds("group-call-schedule-picker").is_some());
}

#[gpui_kit::test]
fn starting_while_in_another_call_asks_to_leave_it(cx: &mut TestAppContext) {
    let (app, mut main) = open_main(cx);
    set_status(&app, &mut main, true);
    app.update(&mut main, |app, _| {
        if let Some(session) = app.demo_session.as_mut() {
            let mut call = quill::state::ActiveGroupCall::fresh(77);
            call.is_joined = true;
            session.calls.active_group_call = Some(call);
        }
        // Its own window stays closed: the test is about this one.
        app.group_call.window_closed_by_user = Some(77);
    });
    begin(&app, &mut main, CreateKind::Now);
    assert!(main.debug_bounds("group-call-create-leave").is_some());
    click(&mut main, "group-call-create-leave-confirm");
    // Then the usual question; the other call ends only on Create.
    assert!(main.debug_bounds("group-call-create-sure").is_some());
    let in_call = |app: &Entity<QuillApp>, vcx: &mut VisualTestContext| {
        app.read_with(vcx, |app, _| {
            app.session()
                .is_some_and(|s| s.calls.active_group_call.is_some())
        })
    };
    assert!(in_call(&app, &mut main), "still in the other call");
    click(&mut main, "group-call-create-confirm");
    assert!(!in_call(&app, &mut main), "left for the new one");
    assert!(join_noted(&app, &mut main));
}
