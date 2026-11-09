#![cfg(test)]
#![cfg(feature = "demo-capture")]
//! A pinned-chat drag ends with the rows that drive it (round 18 B #2, #4):
//! Contacts, Calls or search taking the rows' place, or another folder or
//! the archive, end it unsaved, so its release never sends an order for a
//! list the user cannot see.

use crate::ui::app::{ChatListFilter, QuillApp};
use crate::ui::chat_row::PinnedChatDrag;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::ids::ChatId;

fn frames(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    for _ in 0..2 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

/// The ready chat list with its first two main-list chats pinned; their ids.
fn open(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext, Vec<i64>) {
    cx.update(gpui_kit::init);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named("ready-chats")))
        });
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().expect("app");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    frames(&mut vcx);
    let pinned = app.update(&mut vcx, |app, _| {
        let session = app.demo_session.as_mut().expect("a demo session");
        let mut ids: Vec<i64> = session
            .chats
            .values()
            .filter(|chat| chat.in_main_list)
            .map(|chat| chat.id.0)
            .collect();
        ids.sort_unstable();
        ids.truncate(2);
        for (order, id) in ids.iter().enumerate() {
            if let Some(chat) = session.chats.get_mut(id) {
                chat.is_pinned = true;
                chat.order = i64::MAX - order as i64;
            }
        }
        session.pinned_chat_ids(false)
    });
    assert_eq!(pinned.len(), 2, "two pinned chats to drag");
    (app, vcx, pinned)
}

fn drag(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, chat: i64, y: f32) {
    app.update(vcx, |app, cx| {
        let drag = PinnedChatDrag {
            chat_id: ChatId(chat),
            archived: false,
        };
        app.pin_drag_move(&drag, y, cx);
    });
}

fn pinned_order(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Vec<i64> {
    app.read_with(vcx, |app, _| {
        app.demo_session
            .as_ref()
            .map(|session| session.pinned_chat_ids(false))
            .unwrap_or_default()
    })
}

#[gpui_kit::test]
fn contacts_over_the_rows_end_a_pinned_drag_unsaved(cx: &mut TestAppContext) {
    let (app, mut vcx, pinned) = open(cx);
    let first = *pinned.first().expect("a pinned chat");
    drag(&app, &mut vcx, first, 10.0);
    drag(&app, &mut vcx, first, 200.0);
    assert!(app.read_with(&vcx, |app, _| app.chat_list.pin_reorder.is_some()));
    // Cmd+J while the button is held: the rows give way to Contacts.
    app.update(&mut vcx, |app, cx| {
        app.chat_list.contacts_tab_open = true;
        cx.notify();
    });
    frames(&mut vcx);
    assert!(
        app.read_with(&vcx, |app, _| app.chat_list.pin_reorder.is_none()),
        "the drag ends with its rows"
    );
    // Back to the rows: the next release on them sends nothing.
    app.update(&mut vcx, |app, cx| {
        app.chat_list.contacts_tab_open = false;
        app.finish_pin_drag(cx);
    });
    frames(&mut vcx);
    assert_eq!(
        pinned_order(&app, &mut vcx),
        pinned,
        "the order is unchanged"
    );
}

#[gpui_kit::test]
fn another_folder_ends_a_pinned_drag_unsaved(cx: &mut TestAppContext) {
    let (app, mut vcx, pinned) = open(cx);
    let first = *pinned.first().expect("a pinned chat");
    drag(&app, &mut vcx, first, 10.0);
    drag(&app, &mut vcx, first, 200.0);
    app.update(&mut vcx, |app, _| {
        app.set_chat_list_view(ChatListFilter::Archived, None);
    });
    assert!(app.read_with(&vcx, |app, _| app.chat_list.pin_reorder.is_none()));
    // Moving on in the archive does not start the main list's drag again.
    drag(&app, &mut vcx, first, 240.0);
    assert!(
        app.read_with(&vcx, |app, _| app.chat_list.pin_reorder.is_none()),
        "the main list's drag does not run over the archive"
    );
    app.update(&mut vcx, |app, cx| app.finish_pin_drag(cx));
    assert_eq!(
        pinned_order(&app, &mut vcx),
        pinned,
        "the order is unchanged"
    );
}
