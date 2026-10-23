//! A bot keyboard's location request: the box opens, looks the location up
//! and offers Share only once it is known (needs the `demo-capture`
//! feature for gpui-kit's test support).
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::request_share::RequestShareKind;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::{DialogKind, QuillShell};
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::ids::{ChatId, MessageId};
use quill::location_picker::GeoOutcome;

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, Entity<QuillShell>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(
                window,
                cx,
                None,
                Some(ScreenshotDemo::named("ready-bot-location-request")),
            )
        });
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some((view, shell.clone()));
        Root::new(shell, window, cx)
    });
    let (app, shell) = slot.borrow().clone().expect("window built");
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    (app, shell, vcx)
}

#[gpui_kit::test]
fn pressing_the_button_opens_the_box_and_the_location_arrives(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, _, cx| {
        app.press_request_location(ChatId(21), MessageId(307), true, cx);
    });
    vcx.run_until_parked();
    shell.read_with(vcx, |shell, _| {
        assert_eq!(shell.open_dialog, Some(DialogKind::RequestShare));
    });
    app.read_with(vcx, |app, _| {
        let share = app.message_ui.request_share.as_ref().expect("box open");
        assert!(matches!(
            share.kind,
            RequestShareKind::Location { one_time: true }
        ));
        assert!(matches!(share.locate, Some(GeoOutcome::Found(_))));
    });
}

#[gpui_kit::test]
fn a_chat_that_is_not_a_bot_opens_no_box(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, _, cx| {
        app.press_request_location(ChatId(11), MessageId(1), false, cx);
    });
    vcx.run_until_parked();
    shell.read_with(vcx, |shell, _| assert_eq!(shell.open_dialog, None));
    app.read_with(vcx, |app, _| {
        assert!(app.message_ui.request_share.is_none())
    });
}
