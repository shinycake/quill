#![cfg(test)]
#![cfg(feature = "demo-capture")]
//! Messages dragged out of a history onto the chat list: a press on a
//! selected message lifts the selection, the row under the pointer lights
//! up, the list keeps its order, the "Forward to..." banner shows over the
//! conversation, and a drop opens that chat with the messages in its
//! forward bar.

use crate::ui::HistoryRow;
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AppContext, Entity, Modifiers, MouseButton, Point, TestAppContext, VisualTestContext, point,
    px, size,
};
use quill::ids::{ChatId, MessageId};
use std::time::Duration;

fn frames(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    for _ in 0..3 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

fn open(cx: &mut TestAppContext, demo: &str, width: f32) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    crate::ui::interface_zoom::set_initial_zoom(1.);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named(demo);
    let handle = cx.open_window(size(px(width), px(800.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().expect("app");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    frames(&mut vcx);
    (app, vcx)
}

fn selected(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    open(cx, "ready-selection", 1100.)
}

fn centre_of(vcx: &mut VisualTestContext, id: (&'static str, u64)) -> Point<gpui_kit::Pixels> {
    vcx.update(|window, _| window.find(id).bounds().center())
}

fn row_centre(vcx: &mut VisualTestContext, chat: u64) -> Point<gpui_kit::Pixels> {
    centre_of(vcx, ("chat-row", chat))
}

fn lit(vcx: &mut VisualTestContext, chat: u64) -> bool {
    vcx.update(|window, _| window.try_find(("drop-row-hot", chat)).is_some())
}

fn banner(vcx: &mut VisualTestContext) -> bool {
    vcx.update(|window, _| window.try_find("forward-drag-banner").is_some())
}

fn open_chat(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Option<ChatId> {
    app.update(vcx, |app, _| app.session().and_then(|s| s.open_chat))
}

fn dragging(vcx: &mut VisualTestContext) -> bool {
    vcx.update(|_, cx| cx.has_active_drag())
}

/// A point on the row of `id` that is not on its bubble (the empty side near
/// the bottom), where the selection overlay takes the press.
fn message_centre(
    app: &Entity<QuillApp>,
    vcx: &mut VisualTestContext,
    id: MessageId,
) -> Point<gpui_kit::Pixels> {
    app.read_with(vcx, |app, _| {
        let hits = app.history.hit_rows.borrow();
        hits.iter()
            .find_map(|(ix, bounds)| match app.history.rows.get(*ix) {
                Some(HistoryRow::Single(inputs)) if inputs.message.id == id => {
                    let y = bounds.bottom() - px(10.);
                    Some(if inputs.message.is_outgoing {
                        point(bounds.left() + px(6.), y)
                    } else {
                        point(bounds.right() - px(6.), y)
                    })
                }
                _ => None,
            })
            .expect("the message row is painted")
    })
}

/// Presses on the selected message and moves to `to` in steps.
fn lift_and_move(
    app: &Entity<QuillApp>,
    vcx: &mut VisualTestContext,
    press: MessageId,
    to: Point<gpui_kit::Pixels>,
) {
    let from = message_centre(app, vcx, press);
    let none = Modifiers::default();
    vcx.simulate_mouse_move(from, None, none);
    vcx.simulate_mouse_down(from, MouseButton::Left, none);
    for step in 1..=4 {
        let t = step as f32 / 4.;
        let at = point(from.x + (to.x - from.x) * t, from.y + (to.y - from.y) * t);
        vcx.simulate_mouse_move(at, Some(MouseButton::Left), none);
        frames(vcx);
    }
}

#[gpui_kit::test]
fn a_selected_message_lifts_the_selection_and_lights_the_row_under_it(cx: &mut TestAppContext) {
    let (app, mut vcx) = selected(cx);
    let vcx = &mut vcx;
    let to = row_centre(vcx, 12);
    lift_and_move(&app, vcx, MessageId(101), to);
    assert!(dragging(vcx), "the press lifted the selection");
    assert!(lit(vcx, 12));
    assert!(!lit(vcx, 71));
    assert!(
        banner(vcx),
        "the forward banner shows over the conversation"
    );
    // The drag neither range-selected nor toggled anything.
    let ids = app.read_with(vcx, |app, _| {
        app.forward_draft()
            .map(|draft| draft.message_ids)
            .unwrap_or_default()
    });
    assert_eq!(ids, vec![MessageId(101), MessageId(102)]);
}

#[gpui_kit::test]
fn dropping_on_a_row_opens_the_chat_with_the_messages_in_its_forward_bar(cx: &mut TestAppContext) {
    let (app, mut vcx) = selected(cx);
    let vcx = &mut vcx;
    let to = row_centre(vcx, 12);
    lift_and_move(&app, vcx, MessageId(101), to);
    vcx.simulate_mouse_up(to, MouseButton::Left, Modifiers::default());
    frames(vcx);
    assert_eq!(open_chat(&app, vcx), Some(ChatId(12)));
    let (dest, from, ids) = app.read_with(vcx, |app, _| {
        let draft = app.share.pending_forward.as_ref();
        (
            app.share.forward_bar_dest,
            draft.map(|d| d.from_chat_id),
            draft.map(|d| d.message_ids.clone()).unwrap_or_default(),
        )
    });
    assert_eq!(dest, Some(ChatId(12)));
    assert_eq!(from, Some(ChatId(11)));
    assert_eq!(ids, vec![MessageId(101), MessageId(102)]);
    assert!(!lit(vcx, 12) && !banner(vcx));
}

#[gpui_kit::test]
fn a_forward_drag_does_not_open_the_row_it_rests_on(cx: &mut TestAppContext) {
    let (app, mut vcx) = selected(cx);
    let vcx = &mut vcx;
    let to = row_centre(vcx, 12);
    lift_and_move(&app, vcx, MessageId(101), to);
    vcx.executor().advance_clock(Duration::from_millis(1500));
    frames(vcx);
    assert_eq!(open_chat(&app, vcx), Some(ChatId(11)), "no wait timer");
    assert!(lit(vcx, 12));
}

#[gpui_kit::test]
fn the_list_keeps_its_order_while_a_forward_drag_hovers_it(cx: &mut TestAppContext) {
    let (app, mut vcx) = selected(cx);
    let vcx = &mut vcx;
    assert_eq!(app.read_with(vcx, |app, _| app.frozen_rows()), 0);
    let to = row_centre(vcx, 12);
    lift_and_move(&app, vcx, MessageId(101), to);
    assert!(app.read_with(vcx, |app, _| app.frozen_rows()) > 0);
}

#[gpui_kit::test]
fn a_drop_on_a_channel_the_user_cannot_post_in_changes_nothing(cx: &mut TestAppContext) {
    let (app, mut vcx) = selected(cx);
    let vcx = &mut vcx;
    app.update(vcx, |app, _| {
        if let Some(summary) = app.demo_session.as_mut().and_then(|s| s.chats.get_mut(&13)) {
            summary.kind = quill::telegram::envelope::ChatKind::Supergroup {
                supergroup_id: 4242,
                is_channel: true,
            };
            summary.my_member_status = Some(quill::telegram::envelope::ChannelMemberStatus::Member);
        }
    });
    frames(vcx);
    let to = row_centre(vcx, 13);
    lift_and_move(&app, vcx, MessageId(101), to);
    vcx.simulate_mouse_up(to, MouseButton::Left, Modifiers::default());
    frames(vcx);
    assert_eq!(open_chat(&app, vcx), Some(ChatId(11)));
    assert_eq!(
        app.read_with(vcx, |app, _| app.share.forward_bar_dest),
        None
    );
}

#[gpui_kit::test]
fn a_protected_chat_never_lifts_its_messages(cx: &mut TestAppContext) {
    let (app, mut vcx) = selected(cx);
    let vcx = &mut vcx;
    app.update(vcx, |app, _| {
        if let Some(session) = app.demo_session.as_mut() {
            session.chats_state.protected_chats.insert(11);
        }
    });
    frames(vcx);
    let to = row_centre(vcx, 12);
    lift_and_move(&app, vcx, MessageId(101), to);
    assert!(!dragging(vcx));
    assert!(!lit(vcx, 12) && !banner(vcx));
}

#[gpui_kit::test]
fn the_banner_shows_over_the_conversation_beside_the_list(cx: &mut TestAppContext) {
    let (_app, mut vcx) = open(cx, "ready-drop-forward", 1100.);
    assert!(banner(&mut vcx));
    assert!(lit(&mut vcx, 12));
}

#[gpui_kit::test]
fn the_banner_is_hidden_in_one_column(cx: &mut TestAppContext) {
    let (_app, mut vcx) = open(cx, "ready-drop-forward", 600.);
    assert!(!banner(&mut vcx));
}

#[gpui_kit::test]
fn a_wiggle_shorter_than_the_start_distance_is_still_a_click(cx: &mut TestAppContext) {
    let (app, mut vcx) = selected(cx);
    let vcx = &mut vcx;
    let from = message_centre(&app, vcx, MessageId(101));
    let near = point(from.x - px(4.), from.y);
    let none = Modifiers::default();
    vcx.simulate_mouse_move(from, None, none);
    vcx.simulate_mouse_down(from, MouseButton::Left, none);
    vcx.simulate_mouse_move(near, Some(MouseButton::Left), none);
    frames(vcx);
    vcx.simulate_mouse_up(near, MouseButton::Left, none);
    frames(vcx);
    assert!(!dragging(vcx), "the lifted picture went back");
    let ids = app.read_with(vcx, |app, _| {
        app.forward_draft()
            .map(|draft| draft.message_ids)
            .unwrap_or_default()
    });
    assert_eq!(ids, vec![MessageId(102)], "the click toggled the row off");
}
