//! The profile action row and More menu through the `ready-profile-*`
//! demos: Mute toggles on click and offers durations on right click, More
//! lists the contact entries and opens the Disable Sharing box, and a
//! group shows its link, location and members online.

use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::{TestQueryExt, TestWindowExt};
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::ids::ChatId;

/// The contact and group of the `ready-profile-*` demos.
const PROFILE_USER: i64 = 81;
const PROFILE_GROUP: i64 = 82;

fn frames(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    for _ in 0..3 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

fn open_demo(cx: &mut TestAppContext, kind: &'static str) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named(kind);
    let handle = cx.open_window(size(px(1200.), px(1400.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let app = slot.borrow().clone().expect("app");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    frames(&mut vcx);
    (app, vcx)
}

fn muted(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, chat: i64) -> bool {
    app.read_with(vcx, |app, _| {
        app.session()
            .and_then(|s| s.chats.get(&chat).map(|c| s.effective_muted(c)))
            .unwrap_or(false)
    })
}

fn shown(vcx: &mut VisualTestContext, id: &'static str) -> bool {
    vcx.update(|window, _| window.try_find(id).is_some())
}

fn labelled(vcx: &mut VisualTestContext, label: &str) -> bool {
    vcx.update(|window, _| !window.find_all_by_label(label).is_empty())
}

#[gpui_kit::test]
fn a_contact_gets_message_mute_call_and_more(cx: &mut TestAppContext) {
    let (_app, mut vcx) = open_demo(cx, "ready-profile-user");
    let vcx = &mut vcx;
    for id in [
        "info-panel-message",
        "info-panel-mute",
        "info-panel-call",
        "info-panel-more",
    ] {
        assert!(shown(vcx, id), "{id} missing");
    }
    assert!(
        !shown(vcx, "info-panel-gift"),
        "the third place went to Call"
    );
}

#[gpui_kit::test]
fn mute_toggles_on_click_and_offers_durations_on_right_click(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_demo(cx, "ready-profile-user");
    let vcx = &mut vcx;
    assert!(!muted(&app, vcx, PROFILE_USER));
    vcx.update(|window, cx| window.click("info-panel-mute", cx));
    frames(vcx);
    assert!(muted(&app, vcx, PROFILE_USER), "a click mutes forever");
    vcx.update(|window, cx| window.click("info-panel-mute", cx));
    frames(vcx);
    assert!(!muted(&app, vcx, PROFILE_USER), "a second click unmutes");
    vcx.update(|window, cx| window.right_click("info-panel-mute", cx));
    frames(vcx);
    assert!(labelled(vcx, "Mute for 8 hours"));
    assert!(labelled(vcx, "Mute forever"));
    let item = vcx.update(|window, _| window.find_by_label("Mute for 1 hour").bounds());
    vcx.simulate_click(item.center(), gpui_kit::Modifiers::default());
    frames(vcx);
    let mute_for = app.read_with(vcx, |app, _| {
        app.session()
            .and_then(|s| s.chats.get(&PROFILE_USER))
            .map(|c| c.notification_settings.mute_for)
    });
    assert!(
        mute_for.is_some_and(|secs| secs > 0 && secs <= 3600),
        "muted for an hour: {mute_for:?}"
    );
}

#[gpui_kit::test]
fn more_lists_the_contact_entries_and_opens_the_disable_sharing_box(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_demo(cx, "ready-profile-user");
    let vcx = &mut vcx;
    vcx.update(|window, cx| window.click("info-panel-more", cx));
    frames(vcx);
    for label in [
        "Edit contact",
        "Disable Sharing",
        "Block user",
        "Delete contact",
    ] {
        assert!(labelled(vcx, label), "{label} missing from More");
    }
    let item = vcx.update(|window, _| window.find_by_label("Disable Sharing").bounds());
    vcx.simulate_click(item.center(), gpui_kit::Modifiers::default());
    frames(vcx);
    let (open, shows) = app.read_with(vcx, |app, _| {
        (
            app.dialogs.disable_sharing,
            app.chat_prefs.disable_sharing_box_shown,
        )
    });
    assert_eq!(open, Some(ChatId(PROFILE_USER)));
    assert!(shows >= 1);
    assert!(shown(vcx, "disable-sharing-confirm"));
    assert!(labelled(vcx, "Unlock with Telegram Premium"));
}

#[gpui_kit::test]
fn a_group_member_gets_mute_leave_and_the_link_rows(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_demo(cx, "ready-profile-group");
    let vcx = &mut vcx;
    for id in [
        "info-panel-mute",
        "info-panel-leave",
        "info-panel-more",
        "group-link",
        "group-link-qr",
        "info-business-location",
    ] {
        assert!(shown(vcx, id), "{id} missing");
    }
    assert!(!shown(vcx, "info-panel-join"));
    assert!(labelled(
        vcx,
        "also @quill_de, @berlinquill: t.me/quillberlin"
    ));
    let online = app.read_with(vcx, |app, _| {
        app.session().and_then(|s| {
            s.chats
                .get(&PROFILE_GROUP)
                .and_then(|c| s.group_member_counts(c))
        })
    });
    assert_eq!(online, Some((1284, 37)));
}

#[gpui_kit::test]
fn a_channel_subscriber_gets_discuss_and_gift(cx: &mut TestAppContext) {
    let (_app, mut vcx) = open_demo(cx, "ready-profile-channel");
    let vcx = &mut vcx;
    for id in [
        "info-panel-mute",
        "info-panel-discuss",
        "info-panel-gift",
        "info-panel-more",
    ] {
        assert!(shown(vcx, id), "{id} missing");
    }
    assert!(
        !shown(vcx, "info-panel-leave"),
        "the cap keeps Leave for More"
    );
}

#[gpui_kit::test]
fn the_contact_photo_rows_live_in_the_edit_contact_box(cx: &mut TestAppContext) {
    let (_app, mut vcx) = open_demo(cx, "ready-profile-user");
    let vcx = &mut vcx;
    assert!(!shown(vcx, "contact-photo-set"), "not in the panel");
    assert!(!shown(vcx, "contact-photo-suggest"), "not in the panel");
    vcx.update(|window, cx| window.click("info-panel-edit-contact", cx));
    frames(vcx);
    assert!(shown(vcx, "contact-photo-set"));
    assert!(shown(vcx, "contact-photo-suggest"));
}

#[gpui_kit::test]
fn only_a_channel_keeps_a_leave_row(cx: &mut TestAppContext) {
    let (_app, mut vcx) = open_demo(cx, "ready-profile-group");
    assert!(shown(&mut vcx, "info-panel-leave"));
    assert!(
        !shown(&mut vcx, "g1-leave-chat"),
        "the group leaves from the buttons"
    );
    let (_app, mut vcx) = open_demo(cx, "ready-profile-channel");
    assert!(shown(&mut vcx, "g1-leave-chat"));
}
