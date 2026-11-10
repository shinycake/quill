#![cfg(test)]
#![cfg(feature = "demo-capture")]
//! Confirm boxes act on what the user confirmed, not on what is live when
//! they press OK (sweep 09-050, 10-087, 10-068, 12-049).

use crate::ui::app::QuillApp;
use crate::ui::dialogs::{ForumTopicAction, GroupConfirmAction};
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::ids::ChatId;

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
    vcx.run_until_parked();
    vcx.update(|window, cx| window.render_frame(cx));
    let chats = app.read_with(&vcx, |app, _| {
        let session = app.demo_session.as_ref().expect("a demo session");
        let mut ids: Vec<i64> = session
            .chats
            .values()
            .filter(|chat| chat.in_main_list)
            .map(|chat| chat.id.0)
            .collect();
        ids.sort_unstable();
        ids
    });
    assert!(chats.len() >= 2, "two chats to select");
    (app, vcx, chats)
}

#[gpui_kit::test]
fn bulk_delete_acts_on_the_chats_that_were_selected_when_it_opened(cx: &mut TestAppContext) {
    let (app, mut vcx, chats) = open(cx);
    let (first, second) = (chats[0], chats[1]);
    app.update(&mut vcx, |app, cx| {
        app.chat_list.selected = [first, second].into_iter().collect();
        app.open_group_confirm(ChatId(first), GroupConfirmAction::RemoveSelectedChats, cx);
        // The selection changes while the box is up.
        app.chat_list.selected.clear();
    });
    let targets = app.read_with(&vcx, |app, _| {
        app.admin
            .group_confirm_dialog
            .as_ref()
            .map(|dialog| dialog.targets.clone())
            .unwrap_or_default()
    });
    let mut ids: Vec<i64> = targets.iter().map(|chat| chat.0).collect();
    ids.sort_unstable();
    assert_eq!(
        ids,
        vec![first, second],
        "the confirmed chats, not the live selection"
    );
}

#[gpui_kit::test]
fn deleting_a_forum_topic_asks_first(cx: &mut TestAppContext) {
    let (app, mut vcx, chats) = open(cx);
    let chat = ChatId(chats[0]);
    app.update(&mut vcx, |app, cx| {
        app.forum_topic_action(chat, 42, ForumTopicAction::Delete, cx);
    });
    let asked = app.read_with(&vcx, |app, _| {
        app.admin.group_confirm_dialog.as_ref().map(|dialog| {
            (
                dialog.chat_id,
                matches!(
                    dialog.action,
                    GroupConfirmAction::DeleteForumTopic { forum_topic_id: 42 }
                ),
            )
        })
    });
    assert_eq!(
        asked,
        Some((chat, true)),
        "a confirm box, nothing deleted yet"
    );
}

#[gpui_kit::test]
fn a_chat_windows_menus_use_its_own_chat(cx: &mut TestAppContext) {
    let (app, mut vcx, chats) = open(cx);
    let (main, window_chat) = (ChatId(chats[0]), ChatId(chats[1]));
    app.update(&mut vcx, |app, _| {
        if let Some(session) = app.demo_session.as_mut() {
            session.open_chat = Some(main);
        }
        assert_eq!(app.viewed_chat(), Some(main));
        app.windows.scope = Some(window_chat);
        assert_eq!(
            app.viewed_chat(),
            Some(window_chat),
            "the window in scope, not the main window's chat"
        );
        app.windows.scope = None;
    });
}
