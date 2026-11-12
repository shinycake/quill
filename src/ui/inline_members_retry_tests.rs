//! A failed members page is asked for again at the next scroll; an empty
//! one is not (round 12, area B, finding 6).

use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::ScreenshotDemo;
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, px, size};
use quill::state::{InfoPanelTarget, MemberList, MemberListFilter, MoreMembers};
use quill::telegram::envelope::ChatKind;

fn demo(cx: &mut TestAppContext) -> Entity<QuillApp> {
    demo_window(cx).0
}

fn demo_window(cx: &mut TestAppContext) -> (Entity<QuillApp>, gpui_kit::WindowHandle<Root>) {
    cx.update(gpui_kit::init);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(700.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named("ready-chats")))
        });
        *slot_in.borrow_mut() = Some(view.clone());
        Root::new(view, window, cx)
    });
    cx.run_until_parked();
    let app = slot.take().expect("the window builder fills the slot");
    (app, handle)
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
    let key = (chat, MemberListFilter::Recent);
    let set = |app: &Entity<QuillApp>, cx: &mut TestAppContext, more: MoreMembers| {
        app.update(cx, |app, _| {
            if let Some(session) = app.session_mut() {
                session
                    .groups
                    .supergroup_members
                    .entry(key)
                    .or_insert_with(|| {
                        MemberList::of(
                            "",
                            quill::state::SupergroupMembersFetch::Loaded {
                                members: Vec::new(),
                                total_count: 0,
                            },
                        )
                    })
                    .more = more;
            }
        })
    };
    let blocked = |app: &Entity<QuillApp>, cx: &mut TestAppContext| {
        app.read_with(cx, |app, _| {
            app.session()
                .and_then(|s| s.groups.member_more_blocked(key))
        })
    };
    set(&app, cx, MoreMembers::Failed { offset: 20 });
    // A frame looks again: it does not ask a failing server in a loop.
    app.update(cx, |app, cx| app.maybe_load_more_inline(false, cx));
    assert_eq!(blocked(&app, cx), Some(20));
    app.update(cx, |app, cx| app.maybe_load_more_inline(true, cx));
    assert_eq!(blocked(&app, cx), None, "the scroll may ask again");

    // A page that came back empty stays where it stopped.
    set(&app, cx, MoreMembers::Settled { offset: 20 });
    app.update(cx, |app, cx| app.maybe_load_more_inline(true, cx));
    assert_eq!(blocked(&app, cx), Some(20));
}

/// Recent members failed, so the block falls back to the administrators.
/// That fallback is asked while the cache has no entry for it, so a list
/// that fails too does not loop; Retry drops the
/// failed entry and asks again.
#[gpui_kit::test]
fn the_administrators_fallback_is_asked_once_until_retry(cx: &mut TestAppContext) {
    let app = demo(cx);
    let chat = open_group_panel(&app, cx);
    let admins = (chat, MemberListFilter::Administrators);
    let held = |app: &Entity<QuillApp>, cx: &mut TestAppContext| {
        app.read_with(cx, |app, _| {
            app.session()
                .is_some_and(|s| s.groups.supergroup_members.contains_key(&admins))
        })
    };
    app.update(cx, |app, cx| {
        if let Some(session) = app.session_mut() {
            session.groups.set_member_fetch(
                (chat, MemberListFilter::Recent),
                quill::state::SupergroupMembersFetch::Failed("CHANNEL_PRIVATE".into()),
            );
        }
        app.maybe_load_more_inline(false, cx);
    });
    // A demo has no connection to ask with: nothing is held yet.
    assert!(!held(&app, cx), "nothing was sent, so nothing is held");
    app.update(cx, |app, cx| {
        if let Some(session) = app.session_mut() {
            session.groups.set_member_fetch(
                admins,
                quill::state::SupergroupMembersFetch::Failed("CHANNEL_PRIVATE".into()),
            );
        }
        app.maybe_load_more_inline(false, cx);
    });
    assert!(
        held(&app, cx),
        "a failed list is not asked again by a frame"
    );
    // A membership change leaves a failed list as it is (it draws Retry).
    app.update(cx, |app, cx| {
        if let Some(session) = app.session_mut() {
            session.groups.members_changed(chat);
        }
        app.inline_members_refresh_stale(cx);
    });
    assert!(held(&app, cx), "a failed list keeps its Retry");
    app.update(cx, |app, _| {
        if let Some(session) = app.session_mut() {
            session.groups.forget_failed_member_lists(chat);
        }
    });
    assert!(!held(&app, cx), "Retry drops the failed list to ask again");
}

/// A failed page of an earlier visit does not stay latched when the panel
/// of the same group opens again, and closing the panel drops the block.
#[gpui_kit::test]
fn the_block_starts_over_each_time_the_panel_opens(cx: &mut TestAppContext) {
    let (app, window) = demo_window(cx);
    open_group_panel(&app, cx);
    let target = app.read_with(cx, |app, _| {
        app.session().and_then(|s| s.users_state.open_info_panel)
    });
    let target = target.expect("the open group panel");
    let chat = app.read_with(cx, |app, _| {
        app.session()
            .and_then(|s| match s.users_state.open_info_panel {
                Some(InfoPanelTarget::Supergroup(_)) => {
                    s.chats.values().find_map(|c| match c.kind {
                        ChatKind::Supergroup { .. } => Some(c.id.0),
                        _ => None,
                    })
                }
                _ => None,
            })
    });
    let chat = chat.expect("the open group");
    let key = (chat, MemberListFilter::Recent);
    let stale = |app: &Entity<QuillApp>, cx: &mut TestAppContext| {
        app.update(cx, |app, _| {
            if let Some(session) = app.session_mut() {
                session
                    .groups
                    .supergroup_members
                    .entry(key)
                    .or_insert_with(|| {
                        MemberList::of(
                            "",
                            quill::state::SupergroupMembersFetch::Loaded {
                                members: Vec::new(),
                                total_count: 0,
                            },
                        )
                    })
                    .more = MoreMembers::Failed { offset: 40 };
                session.groups.set_member_fetch(
                    (chat, MemberListFilter::Administrators),
                    quill::state::SupergroupMembersFetch::Failed("gone".into()),
                );
            }
            app.inline_members.shown = 120;
        });
    };
    let fresh = |app: &Entity<QuillApp>, cx: &mut TestAppContext| {
        app.read_with(cx, |app, _| {
            let session = app.session().expect("the demo session");
            assert_eq!(session.groups.member_more_blocked(key), None);
            assert!(
                !session
                    .groups
                    .supergroup_members
                    .contains_key(&(chat, MemberListFilter::Administrators))
            );
            assert_eq!(app.inline_members.shown, quill::inline_members::ROWS_STEP);
        });
    };
    stale(&app, cx);
    app.update(cx, |app, cx| {
        app.close_info_panel(cx);
    });
    app.read_with(cx, |app, _| {
        assert_eq!(app.inline_members.shown, quill::inline_members::ROWS_STEP)
    });

    // Opening the same panel again starts over too.
    stale(&app, cx);
    window
        .update(cx, |_, window, cx| {
            app.update(cx, |app, cx| app.inline_members_opened(target, window, cx))
        })
        .expect("the window is open");
    fresh(&app, cx);
}

/// A send the app refused locally says nothing about whether the viewer may
/// see the members: the list stays on Recent (with its Retry) and does not
/// switch to the administrators. Only TDLib's refusal does (sweep r2-01).
#[gpui_kit::test]
fn a_locally_refused_send_keeps_the_recent_list(cx: &mut TestAppContext) {
    let app = demo(cx);
    let chat = open_group_panel(&app, cx);
    let filter_after = |app: &Entity<QuillApp>,
                        cx: &mut TestAppContext,
                        fetch: quill::state::SupergroupMembersFetch| {
        app.update(cx, |app, _| {
            let shape = app.inline_members_current().expect("the open group");
            if let Some(session) = app.session_mut() {
                session
                    .groups
                    .set_member_fetch((chat, MemberListFilter::Recent), fetch);
            }
            app.inline_members_filter(shape)
        })
    };
    assert_eq!(
        filter_after(
            &app,
            cx,
            quill::state::SupergroupMembersFetch::NotSent("offline".into())
        ),
        MemberListFilter::Recent
    );
    assert_eq!(
        filter_after(
            &app,
            cx,
            quill::state::SupergroupMembersFetch::Failed("CHANNEL_PRIVATE".into())
        ),
        MemberListFilter::Administrators
    );
}
