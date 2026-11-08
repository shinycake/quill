//! A failed members page is asked for again at the next scroll; an empty
//! one is not (round 12, area B, finding 6).

use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::ScreenshotDemo;
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, px, size};
use quill::state::{InfoPanelTarget, MemberListFilter};
use quill::telegram::envelope::ChatKind;

fn demo(cx: &mut TestAppContext) -> Entity<QuillApp> {
    cx.update(gpui_kit::init);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    cx.open_window(size(px(1100.), px(700.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named("ready-chats")))
        });
        *slot_in.borrow_mut() = Some(view.clone());
        Root::new(view, window, cx)
    });
    cx.run_until_parked();
    slot.take().expect("the window builder fills the slot")
}

/// Opens the info panel of the demo's first supergroup or channel;
/// returns its chat id.
fn open_group_panel(app: &Entity<QuillApp>, cx: &mut TestAppContext) -> i64 {
    app.update(cx, |app, _| {
        let session = app.session_mut().expect("the demo session");
        let (chat, supergroup) = session
            .chats
            .values()
            .find_map(|chat| match chat.kind {
                ChatKind::Supergroup { supergroup_id, .. } => Some((chat.id.0, supergroup_id)),
                _ => None,
            })
            .expect("a supergroup in the demo");
        session.users_state.open_info_panel = Some(InfoPanelTarget::Supergroup(supergroup));
        chat
    })
}

#[gpui_kit::test]
fn a_failed_page_is_asked_for_again_at_the_next_scroll(cx: &mut TestAppContext) {
    let app = demo(cx);
    let chat = open_group_panel(&app, cx);
    let failed_offset = |app: &Entity<QuillApp>, cx: &mut TestAppContext| {
        app.read_with(cx, |app, _| app.inline_members.failed_offset)
    };
    app.update(cx, |app, cx| {
        app.inline_members.failed_offset = Some(20);
        if let Some(session) = app.session_mut() {
            session
                .groups
                .member_more_failed
                .insert((chat, MemberListFilter::Recent));
        }
        // A frame looks again: it does not ask a failing server in a loop.
        app.maybe_load_more_inline(false, cx);
    });
    assert_eq!(failed_offset(&app, cx), Some(20));
    app.update(cx, |app, cx| app.maybe_load_more_inline(true, cx));
    assert_eq!(failed_offset(&app, cx), None, "the scroll may ask again");

    // A page that came back empty stays where it stopped.
    app.update(cx, |app, cx| {
        app.inline_members.failed_offset = Some(20);
        app.maybe_load_more_inline(true, cx);
    });
    assert_eq!(failed_offset(&app, cx), Some(20));
}
