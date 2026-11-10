//! The Add to folder box keeps the chat it was opened for when another
//! chat opens under it.

use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::ids::ChatId;
use quill::telegram::envelope::ChatList;

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named("ready-chats")))
        });
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let app = slot.borrow().clone().expect("the app view");
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    (app, vcx)
}

fn frames(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    vcx.executor()
        .advance_clock(std::time::Duration::from_millis(300));
    for _ in 0..2 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
    vcx.run_until_parked();
}

/// ⋮ › Add to folder on one chat, then a notification opens another: the
/// box still lists the first chat's destinations, not "Loading folder
/// options…" for the new one.
#[gpui_kit::test]
fn the_folder_box_keeps_its_chat_when_another_opens(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    frames(vcx);
    let (first, second) = app.read_with(vcx, |app, _| {
        let mut ids: Vec<i64> = app
            .session()
            .map(|s| s.chats.keys().copied().collect())
            .unwrap_or_default();
        ids.sort_unstable();
        let mut ids = ids.into_iter().map(ChatId);
        (
            ids.next().expect("a chat"),
            ids.next().expect("another chat"),
        )
    });
    app.update_in(vcx, |app, window, cx| {
        app.select_listed_chat(first, window, cx);
        if let Some(session) = app.demo_session.as_mut() {
            session.chat_list.chat_lists_for_add.insert(
                first.0,
                quill::state::fetch::Fetch::Ready(vec![ChatList::Main]),
            );
            session.chat_list.chat_lists_for_add.remove(&second.0);
        }
        app.open_folder_menu(first, cx);
    });
    frames(vcx);
    assert!(vcx.update(|window, _| window.try_find("folder-menu-add-main").is_some()));
    app.update_in(vcx, |app, window, cx| {
        app.select_listed_chat(second, window, cx)
    });
    frames(vcx);
    assert_eq!(
        app.read_with(vcx, |app, _| app.session().and_then(|s| s.open_chat)),
        Some(second)
    );
    assert!(
        vcx.update(|window, _| window.try_find("folder-menu-add-main").is_some()),
        "the box lists the chat it was opened for"
    );
}
