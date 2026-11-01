//! The link box: presets pick the limits, approval hides the member
//! limit, Custom opens the date picker and the number box and returns
//! with their value in place among the presets.

use super::CustomBox;
use super::demo::demo_link;
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::scheduled::picker_value;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::{DialogKind, QuillShell};
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::invite_link_form::{DAY, Expire, InviteLinkForm, InviteLinkRequest, Usage};
use quill::local_time::now_unix;

fn frames(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    for _ in 0..3 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

fn open_demo(
    cx: &mut TestAppContext,
    name: &'static str,
) -> (Entity<QuillApp>, Entity<QuillShell>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named(name);
    let handle = cx.open_window(size(px(1200.), px(1000.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some((view, shell.clone()));
        Root::new(shell, window, cx)
    });
    let (app, shell) = slot.borrow().clone().expect("app");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    frames(&mut vcx);
    (app, shell, vcx)
}

fn form(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Option<InviteLinkForm> {
    app.read_with(vcx, |app, _| app.invite_link_box().map(|b| b.form.clone()))
}

fn shown(shell: &Entity<QuillShell>, vcx: &mut VisualTestContext) -> Option<DialogKind> {
    shell.read_with(vcx, |s, _| s.open_dialog)
}

fn click_in(vcx: &mut VisualTestContext, group: &'static str, index: usize) {
    vcx.update(|window, cx| window.within(group).click(index, cx));
    frames(vcx);
}

#[gpui_kit::test]
fn presets_and_a_custom_date_make_the_request(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = open_demo(cx, "ready-invite-link-new");
    let vcx = &mut vcx;
    assert_eq!(shown(&shell, vcx), Some(DialogKind::InviteLinkBox));
    click_in(vcx, "invite-link-expire", 2);
    assert_eq!(form(&app, vcx).map(|f| f.expire), Some(Expire::After(DAY)));
    click_in(vcx, "invite-link-usage", 2);
    assert_eq!(form(&app, vcx).map(|f| f.usage), Some(Usage::Limit(10)));

    vcx.update(|window, cx| window.click("invite-link-approval", cx));
    frames(vcx);
    let approving = form(&app, vcx).expect("box");
    assert!(approving.request_approval);
    assert!(
        vcx.update(|window, _| window.try_find("invite-link-usage"))
            .is_none()
    );
    vcx.update(|window, cx| window.click("invite-link-subscription", cx));
    frames(vcx);
    let paid = form(&app, vcx).expect("box");
    assert!(paid.subscription && !paid.request_approval);
    assert!(
        vcx.update(|window, _| window.try_find("invite-link-expire"))
            .is_none()
    );
    vcx.update(|window, cx| window.click("invite-link-subscription", cx));
    frames(vcx);

    click_in(vcx, "invite-link-expire", 4);
    assert_eq!(shown(&shell, vcx), Some(DialogKind::InviteLinkExpire));
    let date = now_unix() + 3 * 86_400;
    let date = date - date.rem_euclid(60);
    vcx.update(|window, cx| {
        app.update(cx, |app, cx| {
            if let Some(CustomBox::Date(picker)) =
                app.invite_link_box().and_then(|b| b.custom.as_ref())
                && let Some(value) = picker_value(date, cx)
            {
                picker.update(cx, |state, cx| state.set_date_time(value, window, cx));
            }
        });
    });
    frames(vcx);
    vcx.update(|window, cx| window.click("invite-link-custom-save", cx));
    frames(vcx);
    assert_eq!(shown(&shell, vcx), Some(DialogKind::InviteLinkBox));
    let picked = form(&app, vcx).expect("box");
    assert_eq!(picked.expire, Expire::At(date as i32));
    assert_eq!(
        picked.request("", 0, now_unix()),
        InviteLinkRequest::Create {
            name: String::new(),
            expiration_date: date as i32,
            member_limit: 10,
            creates_join_request: false,
        }
    );
}

#[gpui_kit::test]
fn a_custom_limit_returns_to_the_box(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = open_demo(cx, "ready-invite-link-new");
    let vcx = &mut vcx;
    click_in(vcx, "invite-link-usage", 4);
    assert_eq!(shown(&shell, vcx), Some(DialogKind::InviteLinkLimit));
    vcx.update(|window, cx| window.input("25", cx));
    frames(vcx);
    vcx.update(|window, cx| window.click("invite-link-custom-save", cx));
    frames(vcx);
    assert_eq!(shown(&shell, vcx), Some(DialogKind::InviteLinkBox));
    assert_eq!(form(&app, vcx).map(|f| f.usage), Some(Usage::Limit(25)));
}

#[gpui_kit::test]
fn editing_opens_on_the_link(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = open_demo(cx, "ready-invite-link-edit");
    let vcx = &mut vcx;
    assert_eq!(shown(&shell, vcx), Some(DialogKind::InviteLinkBox));
    let link = demo_link();
    let edit = form(&app, vcx).expect("box");
    assert_eq!(edit.link.as_deref(), Some(link.invite_link.as_str()));
    assert_eq!(edit.usage, Usage::Limit(25));
    let ahead = match edit.expire {
        Expire::At(date) => i64::from(date) - i64::from(link.expiration_date),
        other => panic!("unexpected {other:?}"),
    };
    assert!(ahead.abs() <= 5);
    assert!(!edit.offers_subscription());
    let name = app.read_with(vcx, |app, cx| {
        app.invite_link_box()
            .map(|b| b.name_input.read(cx).value().to_string())
    });
    assert_eq!(name.as_deref(), Some("Spring hikes"));
    vcx.update(|window, cx| window.click("invite-link-cancel", cx));
    frames(vcx);
    assert!(app.read_with(vcx, |app, _| app.invite_link_box().is_none()));
}
