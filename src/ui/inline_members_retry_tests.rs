//! A failed members page is asked for again at the next scroll; an empty
//! one is not (round 12, area B, finding 6).

use crate::ui::app::QuillApp;
use crate::ui::inline_members_state::Paging;
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
        app.read_with(cx, |app, _| app.inline_members.paging.blocked())
    };
    app.update(cx, |app, cx| {
        app.inline_members.paging = Paging::Asked { offset: 20 };
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
        app.inline_members.paging = Paging::Asked { offset: 20 };
        app.maybe_load_more_inline(true, cx);
    });
    assert_eq!(failed_offset(&app, cx), Some(20));
}

/// Recent members failed, so the block falls back to the administrators.
/// That fallback is asked once per open, not on every frame, so a list that
/// fails too does not loop; Retry asks again (round 12, area B2, finding 1).
#[gpui_kit::test]
fn the_administrators_fallback_is_asked_once_until_retry(cx: &mut TestAppContext) {
    let app = demo(cx);
    let chat = open_group_panel(&app, cx);
    let asked = |app: &Entity<QuillApp>, cx: &mut TestAppContext| {
        app.read_with(cx, |app, _| app.inline_members.admins_asked)
    };
    app.update(cx, |app, cx| {
        if let Some(session) = app.session_mut() {
            for filter in [MemberListFilter::Recent, MemberListFilter::Administrators] {
                session.groups.supergroup_members.insert(
                    (chat, filter),
                    quill::state::SupergroupMembersFetch::Failed("CHANNEL_PRIVATE".into()),
                );
            }
        }
        app.maybe_load_more_inline(false, cx);
    });
    assert!(asked(&app, cx), "the fallback was asked");
    app.update(cx, |app, cx| {
        app.inline_members.admins_asked = true;
        app.maybe_load_more_inline(false, cx);
        app.inline_members_stale(&[chat], cx);
    });
    assert!(!asked(&app, cx), "Retry may ask again");
}

/// A failed page of an earlier visit does not stay latched when the panel
/// of the same group opens again, and closing the panel drops the block.
#[gpui_kit::test]
fn the_block_starts_over_each_time_the_panel_opens(cx: &mut TestAppContext) {
    let app = demo(cx);
    open_group_panel(&app, cx);
    let target = app.read_with(cx, |app, _| {
        app.session().and_then(|s| s.users_state.open_info_panel)
    });
    let target = target.expect("the open group panel");
    app.update(cx, |app, _| {
        app.inline_members.paging = Paging::Failed { offset: 40 };
        app.inline_members.admins_asked = true;
        app.inline_members.shown = 120;
    });
    app.update(cx, |app, cx| {
        app.close_info_panel(cx);
    });
    app.read_with(cx, |app, _| {
        assert_eq!(app.inline_members.paging, Paging::Idle);
        assert!(!app.inline_members.admins_asked);
        assert_eq!(app.inline_members.chat, None);
    });
    let _ = target;
}
