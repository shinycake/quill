//! A bot keyboard's location request: the box opens, looks the location up
//! and offers Share only once it is known (needs the `demo-capture`
//! feature for gpui-kit's test support).
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::request_share::{LocateStage, RequestShareKind};
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
            RequestShareKind::Location {
                one_time: true,
                stage: LocateStage::Located(GeoOutcome::Found(_)),
            }
        ));
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

#[gpui_kit::test]
fn closing_the_box_drops_its_lookup(cx: &mut TestAppContext) {
    let (app, _shell, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, _, cx| {
        app.press_request_location(ChatId(21), MessageId(307), true, cx);
    });
    // The lookup has not answered yet: the box is in its searching stage.
    let lookup = app.read_with(vcx, |app, _| {
        match app
            .message_ui
            .request_share
            .as_ref()
            .map(|share| &share.kind)
        {
            Some(RequestShareKind::Location {
                stage: LocateStage::Locating(task),
                ..
            }) => std::rc::Rc::downgrade(task),
            _ => panic!("the box is searching"),
        }
    });
    assert!(lookup.upgrade().is_some());
    app.update(vcx, |app, cx| {
        app.message_ui.request_share = None;
        cx.notify();
    });
    assert!(
        lookup.upgrade().is_none(),
        "the lookup the user cancelled is dropped with the box"
    );
}

/// Sweep 12-C015: a users or chat request is picked and confirmed in its own
/// stage; a phone request has none to leave.
#[gpui_kit::test]
fn a_chat_request_confirms_the_picked_chat_and_a_phone_request_ignores_picks(
    cx: &mut TestAppContext,
) {
    use quill::telegram::envelope::{RequestChatSpec, RequestUsersSpec};
    let (app, _shell, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let chat = RequestShareKind::chat(RequestChatSpec {
        id: 1,
        chat_is_channel: false,
        chat_is_forum: None,
        chat_has_username: None,
        chat_is_created: false,
        bot_is_member: false,
    });
    app.update_in(vcx, |app, _, cx| {
        app.open_request_share(ChatId(21), MessageId(1), chat, cx);
        app.request_toggle(ChatId(11), cx);
    });
    app.read_with(vcx, |app, _| {
        let share = app.message_ui.request_share.as_ref().expect("box open");
        assert!(share.kind.confirming());
        assert_eq!(share.kind.selected(), [ChatId(11)]);
    });
    let users = RequestShareKind::users(RequestUsersSpec {
        id: 2,
        user_is_bot: None,
        user_is_premium: None,
        max_quantity: 2,
    });
    app.update_in(vcx, |app, _, cx| {
        app.open_request_share(ChatId(21), MessageId(2), users, cx);
        app.request_toggle(ChatId(11), cx);
        app.request_toggle(ChatId(12), cx);
    });
    app.read_with(vcx, |app, _| {
        let share = app.message_ui.request_share.as_ref().expect("box open");
        assert!(!share.kind.confirming(), "two may be picked: Next confirms");
        assert_eq!(share.kind.selected(), [ChatId(11), ChatId(12)]);
    });
    app.update_in(vcx, |app, _, cx| {
        let phone = RequestShareKind::Phone { bot_user_id: 5 };
        app.open_request_share(ChatId(21), MessageId(3), phone, cx);
        app.request_toggle(ChatId(11), cx);
    });
    app.read_with(vcx, |app, _| {
        let share = app.message_ui.request_share.as_ref().expect("box open");
        assert!(share.kind.selected().is_empty() && !share.kind.confirming());
    });
}
