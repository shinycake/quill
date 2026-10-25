//! A send the agent typed into the message box and the user confirmed: the
//! user's own press on Confirm answers the agent's prompt and never pauses
//! it, and once the send went out the box clears, even when the user
//! pressed Stop while the confirm was open.
use super::tests::{click, drive, grant, message_box, new_app, read_tree, settle, turn_on};
use crate::ui::agent_state::TEST_LOCK;
use crate::ui::app::QuillApp;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{Entity, Modifiers, Pixels, Point, TestAppContext, VisualTestContext, point, px};
use quill::agent::activity;
use quill::agent::grants::Scope;
use quill::agent::prompts::{self, Ask, Decision};
use quill::agent::ui_driver::bridge::{Target, UiCommand, UiReply};
use quill::agent::ui_driver::keys::parse_sequence;
use quill::agent::ui_driver::takeover::{self, Phase};
use quill::ids::AccountKey;
use std::sync::mpsc::Receiver;
use std::time::Duration;

const TEXT: &str = "hi there";

/// The agent clicks the box of chat 11, types and presses Enter: the
/// window hands the send over and keeps the draft.
fn type_and_enter(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    let tree = read_tree(app, vcx);
    let reply = drive(app, vcx, click(Target::Node(message_box(&tree))));
    assert!(matches!(reply, Ok(UiReply::Done { .. })), "{reply:?}");
    let reply = drive(app, vcx, UiCommand::TypeText { text: TEXT.into() });
    assert!(matches!(reply, Ok(UiReply::Done { .. })), "{reply:?}");
    let chords = parse_sequence("enter").unwrap_or_default();
    let reply = drive(app, vcx, UiCommand::PressKeys { chords });
    assert!(
        matches!(
            &reply,
            Ok(UiReply::Done {
                ui_send: Some(_),
                ..
            })
        ),
        "{reply:?}"
    );
    assert_eq!(draft(app, vcx), TEXT);
}

/// What `finish_ui_send` does next on the tool thread: "Confirm each send"
/// asks the user (`common::confirm`).
fn ask_confirm(vcx: &mut VisualTestContext) -> (u64, Receiver<Decision>) {
    let filed = prompts::file(
        AccountKey::primary(),
        "Test agent".into(),
        Ask::Write {
            chat_id: 11,
            title: "chat 11".into(),
            kind: "ui_send".into(),
            preview: TEXT.into(),
        },
        String::new(),
    );
    let filed = filed.unwrap_or_else(|| panic!("filed"));
    settle(vcx);
    filed
}

fn center(vcx: &mut VisualTestContext, id: &'static str) -> Point<Pixels> {
    let bounds = vcx.update(|window, _| window.find(id).bounds());
    point(
        bounds.origin.x + bounds.size.width / 2.,
        bounds.origin.y + bounds.size.height / 2.,
    )
}

/// The user moves the mouse to Confirm and presses it, as real input
/// through the window's input probe.
fn press_confirm(vcx: &mut VisualTestContext) {
    let at = center(vcx, "agent-request-allow");
    vcx.simulate_mouse_move(point(px(20.), px(20.)), None, Modifiers::none());
    vcx.simulate_mouse_move(at, None, Modifiers::none());
    vcx.simulate_click(at, Modifiers::none());
    settle(vcx);
}

fn draft(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> String {
    vcx.update(|_, cx| app.read(cx).composer_markup(cx))
}

fn composer_sent(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, text: &str) {
    let reply = drive(
        app,
        vcx,
        UiCommand::ComposerSent {
            chat_id: 11,
            text: text.into(),
        },
    );
    assert!(matches!(reply, Ok(UiReply::Ok)), "{reply:?}");
}

fn setup(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext, activity::Presence) {
    let (app, vcx) = new_app(cx);
    turn_on();
    takeover::reset_for_test();
    grant(11, Scope::ReadWrite);
    let presence = activity::connect("Test agent");
    (app, vcx, presence)
}

#[gpui_kit::test]
fn a_real_press_on_confirm_clears_the_box_and_never_pauses(cx: &mut TestAppContext) {
    let _guard = quill::locks::lock_or_recover(&TEST_LOCK);
    let (app, mut vcx, _presence) = setup(cx);
    let vcx = &mut vcx;
    settle(vcx);
    type_and_enter(&app, vcx);
    assert!(matches!(takeover::current(), Phase::Driving { .. }));

    let (id, rx) = ask_confirm(vcx);
    press_confirm(vcx);
    assert_eq!(
        prompts::wait(id, &rx, Duration::from_millis(10)),
        Some(Decision::Allowed)
    );
    assert!(!takeover::paused(), "Confirm answered the agent's prompt");
    // The hand leaves the button once the dialog closed: still the answer.
    vcx.simulate_mouse_move(point(px(900.), px(600.)), None, Modifiers::none());
    settle(vcx);
    assert!(!takeover::paused(), "the pointer leaving Confirm");
    assert_eq!(takeover::refusal(), None);

    // The send went out: the box clears (only for the same chat and text).
    composer_sent(&app, vcx, "something else");
    assert_eq!(draft(&app, vcx), TEXT, "another text leaves the box alone");
    composer_sent(&app, vcx, TEXT);
    assert_eq!(draft(&app, vcx), "", "no duplicate on the next Enter");
    assert!(!takeover::paused());
    takeover::reset_for_test();
}

#[gpui_kit::test]
fn stop_while_the_confirm_is_open_still_clears_a_confirmed_send(cx: &mut TestAppContext) {
    let _guard = quill::locks::lock_or_recover(&TEST_LOCK);
    let (app, mut vcx, _presence) = setup(cx);
    let vcx = &mut vcx;
    settle(vcx);
    type_and_enter(&app, vcx);
    let (id, rx) = ask_confirm(vcx);

    vcx.update(|window, cx| window.click("agent-takeover-stop", cx));
    settle(vcx);
    assert!(takeover::stopped());
    assert!(
        prompts::current().is_some_and(|prompt| prompt.id == id),
        "Stop leaves the user's own confirm open"
    );
    press_confirm(vcx);
    assert_eq!(
        prompts::wait(id, &rx, Duration::from_millis(10)),
        Some(Decision::Allowed)
    );
    composer_sent(&app, vcx, TEXT);
    assert_eq!(draft(&app, vcx), "", "the confirmed send clears the box");
    assert!(takeover::stopped(), "and Stop holds");
    let reply = drive(&app, vcx, UiCommand::TypeText { text: "x".into() });
    assert!(
        matches!(&reply, Ok(UiReply::Refused(text)) if text.contains("Stop")),
        "{reply:?}"
    );
    assert_eq!(draft(&app, vcx), "");
    takeover::reset_for_test();
}

#[gpui_kit::test]
fn a_read_answered_after_a_pause_is_refused(cx: &mut TestAppContext) {
    let _guard = quill::locks::lock_or_recover(&TEST_LOCK);
    let (app, mut vcx, _presence) = setup(cx);
    let vcx = &mut vcx;
    settle(vcx);
    let tree = read_tree(&app, vcx);
    assert!(
        takeover::current() == Phase::Idle,
        "a read alone does not start driving"
    );
    let reply = drive(&app, vcx, click(Target::Node(message_box(&tree))));
    assert!(matches!(reply, Ok(UiReply::Done { .. })), "{reply:?}");
    // A read is filed, and the user pauses before its frame comes.
    let ticket = quill::agent::ui_driver::bridge::post("Test agent", UiCommand::ReadTree)
        .unwrap_or_else(|_| panic!("filed"));
    vcx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.agent_driver_tick(window, cx);
        })
    });
    takeover::pause(takeover::PauseReason::Escape);
    settle(vcx);
    match ticket.poll() {
        Some(UiReply::Refused(text)) => assert!(text.contains("took control"), "{text}"),
        other => panic!("the read is refused, got {other:?}"),
    }
    takeover::reset_for_test();
}
