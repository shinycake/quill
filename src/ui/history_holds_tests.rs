//! What the UI holds from closed-chat pruning: the chats whose side tables
//! an open surface reads stay claimed while it is open and are let go when
//! it closes.

use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::ids::ChatId;
use quill::state::InfoPanelTarget;

fn frames(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    for _ in 0..3 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

fn open_demo(cx: &mut TestAppContext, name: &'static str) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named(name);
    let handle = cx.open_window(size(px(1200.), px(1000.)), move |window, cx| {
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

fn held(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Vec<ChatId> {
    app.read_with(vcx, |app, _| {
        app.session()
            .map(|session| session.history_pruning.held_chats().collect())
            .unwrap_or_default()
    })
}

/// The demo opens the Edit box on its supergroup.
fn edit_box_chat(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> ChatId {
    app.read_with(vcx, |app, _| app.group_edit_box().map(|edit| edit.chat_id))
        .expect("the box is open")
}

/// The Edit box's Manage rows count the chat's admins.
#[gpui_kit::test]
fn an_open_edit_box_holds_its_chat(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_demo(cx, "ready-group-edit-box");
    let vcx = &mut vcx;
    let chat = edit_box_chat(&app, vcx);
    assert_eq!(held(&app, vcx), vec![chat]);
    vcx.update(|window, cx| window.press("escape", cx));
    frames(vcx);
    assert!(held(&app, vcx).is_empty(), "closing the box lets it go");
}

/// The member dialog lists the chat's members, admins and exceptions.
#[gpui_kit::test]
fn a_member_dialog_holds_its_chat(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_demo(cx, "ready-group-edit-box");
    let vcx = &mut vcx;
    let chat = edit_box_chat(&app, vcx);
    vcx.update(|window, cx| window.press("escape", cx));
    frames(vcx);
    assert!(held(&app, vcx).is_empty());
    vcx.update(|window, cx| app.update(cx, |app, cx| app.open_member_dialog(chat, window, cx)));
    frames(vcx);
    assert_eq!(held(&app, vcx), vec![chat]);
    app.update(vcx, |app, cx| app.close_member_dialog(cx));
    frames(vcx);
    assert!(held(&app, vcx).is_empty(), "closing the dialog lets it go");
}

/// The panel's member list and media rows.
#[gpui_kit::test]
fn the_info_panel_holds_the_group_it_shows(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_demo(cx, "ready-group-edit-box");
    let vcx = &mut vcx;
    let chat = edit_box_chat(&app, vcx);
    vcx.update(|window, cx| window.press("escape", cx));
    frames(vcx);
    let supergroup = app.read_with(vcx, |app, _| {
        app.session()
            .and_then(|session| session.chat_supergroup(chat))
    });
    let target = InfoPanelTarget::Supergroup(supergroup.expect("a supergroup"));
    vcx.update(|window, cx| {
        app.update(cx, |app, cx| app.open_info_panel_target(target, window, cx))
    });
    frames(vcx);
    assert_eq!(held(&app, vcx), vec![chat]);
    app.update(vcx, |app, cx| app.close_info_panel(cx));
    frames(vcx);
    assert!(held(&app, vcx).is_empty(), "closing the panel lets it go");
}

/// The report's option picker stays open across chat switches; the option
/// picked must still find the ad.
#[gpui_kit::test]
fn an_ad_being_reported_holds_its_chat(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_demo(cx, "ready-sponsored");
    let vcx = &mut vcx;
    let ad = app.read_with(vcx, |app, _| {
        let session = app.session()?;
        let ad = session.open_sponsored_rows().first()?.message_id;
        Some((session.open_chat?, ad))
    });
    let (chat, ad) = ad.expect("the demo shows an ad");
    assert!(held(&app, vcx).is_empty());
    app.update(vcx, |app, cx| app.report_sponsored_message_ui(chat, ad, cx));
    frames(vcx);
    assert_eq!(held(&app, vcx), vec![chat], "the option picker is open");
    app.update(vcx, |app, cx| app.dismiss_sponsored_report_ui(cx));
    frames(vcx);
    assert!(held(&app, vcx).is_empty(), "cancelling lets it go");
}
