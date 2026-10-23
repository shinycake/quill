//! The Type box: it opens over the Edit box on the chat's links, a typed
//! taken link shows tdesktop's error and blocks Save, and Save hands the
//! choice back to the Edit box.

use super::demo::{PUBLIC_GROUP, TAKEN};
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::{DialogKind, QuillShell};
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::group_type::{LinkProblem, LinkStatus};
use quill::ids::ChatId;
use quill::telegram::envelope::UsernameCheckResult;

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

fn status(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Option<LinkStatus> {
    app.read_with(vcx, |app, cx| app.group_type_link_status(cx))
}

#[gpui_kit::test]
fn a_taken_link_shows_the_error_and_blocks_save(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = open_demo(cx, "ready-group-type-box");
    let vcx = &mut vcx;
    assert!(shell.read_with(vcx, |s, _| s.open_dialog == Some(DialogKind::GroupTypeBox)));
    assert_eq!(status(&app, vcx), Some(LinkStatus::Current));
    vcx.update(|window, cx| window.click("group-type-private", cx));
    frames(vcx);
    assert_eq!(
        app.read_with(vcx, |app, _| app.group_type_box().map(|b| b.draft.public)),
        Some(false)
    );
    vcx.update(|window, cx| window.click("group-type-public", cx));
    frames(vcx);
    vcx.update(|window, cx| {
        app.update(cx, |app, cx| {
            if let Some(type_box) = app.group_type_box() {
                type_box
                    .link_input
                    .update(cx, |input, cx| input.set_value(TAKEN, window, cx));
            }
        });
    });
    frames(vcx);
    assert_eq!(status(&app, vcx), Some(LinkStatus::Pending));
    app.update(vcx, |app, cx| {
        app.demo_answer_username_check(ChatId(PUBLIC_GROUP), TAKEN, UsernameCheckResult::Occupied);
        cx.notify();
    });
    frames(vcx);
    assert_eq!(
        status(&app, vcx),
        Some(LinkStatus::Problem(LinkProblem::Occupied))
    );
    vcx.update(|window, cx| window.click("group-type-save", cx));
    frames(vcx);
    assert!(app.read_with(vcx, |app, _| app.group_type_box().is_some()));
    app.update(vcx, |app, cx| {
        app.demo_answer_username_check(ChatId(PUBLIC_GROUP), TAKEN, UsernameCheckResult::Available);
        cx.notify();
    });
    frames(vcx);
    vcx.update(|window, cx| window.click("group-type-save", cx));
    frames(vcx);
    let choice = app.read_with(vcx, |app, _| {
        app.group_edit_box()
            .and_then(|edit| edit.type_choice.clone())
            .map(|choice| choice.username)
    });
    assert_eq!(choice.as_deref(), Some(TAKEN));
    assert!(app.read_with(vcx, |app, _| app.group_type_box().is_none()));
    assert!(shell.read_with(vcx, |s, _| s.open_dialog == Some(DialogKind::GroupEditBox)));
}
