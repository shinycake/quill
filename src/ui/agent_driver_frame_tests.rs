//! The tree an agent reads follows the chat its own frame drew, and a send
//! the window handed over belongs to the command that was running.
use super::tests::{click, drive, grant, message_box, new_app, read_tree, settle, turn_on};
use crate::ui::agent_state::TEST_LOCK;
use gpui_kit::TestAppContext;
use quill::agent::activity;
use quill::agent::grants::Scope;
use quill::agent::ui_driver::bridge::{Target, UiReply, UiSend};
use quill::agent::ui_driver::redact::{self, chat_tag};
use quill::agent::ui_driver::surface::DriverScope;
use serde_json::Value;

#[gpui_kit::test]
fn a_frame_of_a_chat_is_cut_by_that_chats_grant(cx: &mut TestAppContext) {
    let _guard = quill::locks::lock_or_recover(&TEST_LOCK);
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    turn_on();
    grant(11, Scope::ReadWrite);
    let _presence = activity::connect("Test agent");
    settle(vcx);
    let open = vcx.update(|_, cx| app.read(cx).open_chat_id().map(|chat| chat.0));
    assert_eq!(open, Some(11));
    // An agent reading makes the window build a tree every frame.
    read_tree(&app, vcx);
    settle(vcx);
    // The frame the window drew for chat 11.
    let frame = vcx
        .update(|window, _| crate::ui::agent_driver_tree::raw_tree(window))
        .expect("a frame with a tree");
    let named = |tree: &quill::agent::ui_driver::tree::UiTree| {
        tree.nodes.iter().filter(|node| node.name.is_some()).count()
    };
    // The user opened another chat since: the grants now are chat 12's.
    // The frame still shows chat 11, whatever the surface of the moment
    // says, so its conversation is cut.
    let granted = redact::redact(frame.clone(), DriverScope::Granted, &|chat| chat == 11);
    let cut = redact::redact(frame, DriverScope::Granted, &|chat| chat == 12);
    assert!(
        named(&cut) < named(&granted),
        "chat 11's content is gone when only chat 12 is granted"
    );
}

#[gpui_kit::test]
fn a_send_left_by_an_earlier_command_is_not_the_next_ones(cx: &mut TestAppContext) {
    let _guard = quill::locks::lock_or_recover(&TEST_LOCK);
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    turn_on();
    grant(11, Scope::ReadWrite);
    let _presence = activity::connect("Test agent");
    settle(vcx);
    let tree = read_tree(&app, vcx);
    let composer = message_box(&tree);
    // A deferred half of an earlier command left its send in the slot.
    vcx.update(|_, cx| {
        app.update(cx, |app, _| {
            let epoch = app.screen_epoch();
            app.agent_bridge.ui_send = Some((
                epoch,
                UiSend {
                    chat_id: 11,
                    chat_title: "chat 11".into(),
                    text: "left behind".into(),
                    request: Value::Null,
                },
            ));
        });
    });
    let reply = drive(&app, vcx, click(Target::Node(composer)));
    let Ok(UiReply::Done { ui_send, .. }) = reply else {
        panic!("expected Done, got {reply:?}");
    };
    assert!(ui_send.is_none(), "the next command got a send of its own");
    assert!(vcx.update(|_, cx| app.read(cx).agent_bridge.ui_send.is_none()));
}
