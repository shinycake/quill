//! The profile modal belongs to the chat it opened over and to the info
//! column it found: a chat change drops it without opening the column,
//! and its close gives the column back (needs the `demo-capture` feature
//! for gpui-kit's test support).
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::ids::ChatId;
use quill::state::InfoPanelTarget;
use quill::telegram::envelope::MessageSender;

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named("ready-chats")))
        });
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().expect("the app");
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    vcx.run_until_parked();
    (app, vcx)
}

fn info_panel(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Option<InfoPanelTarget> {
    app.read_with(vcx, |app, _| {
        app.session()
            .and_then(|session| session.users_state.open_info_panel)
    })
}

fn open_modal(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    app.update_in(vcx, |app, window, cx| {
        app.open_avatar_profile(MessageSender::User { user_id: 11 }, window, cx);
        assert!(app.profile_modal_active(), "the modal shows");
    });
}

#[gpui_kit::test]
fn a_chat_change_drops_the_modal_and_opens_no_column(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    assert_eq!(info_panel(&app, vcx), None);
    open_modal(&app, vcx);
    app.update_in(vcx, |app, window, cx| {
        app.select_listed_chat(ChatId(12), window, cx);
    });
    vcx.run_until_parked();
    app.read_with(vcx, |app, _| {
        assert!(app.dialogs.profile_modal.is_none(), "the modal went");
    });
    assert_eq!(
        info_panel(&app, vcx),
        None,
        "no column the user never opened"
    );
}

#[gpui_kit::test]
fn closing_the_modal_gives_the_column_back(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let column = app.update_in(vcx, |app, window, cx| {
        app.select_listed_chat(ChatId(12), window, cx);
        let target = app
            .session()
            .and_then(|session| session.chat_info_target(ChatId(12)))
            .expect("chat 12 has info");
        app.open_info_panel_target(target, window, cx);
        target
    });
    open_modal(&app, vcx);
    app.update(vcx, |app, cx| {
        // In the background the fade snaps: no timer to wait on.
        app.frame.window_active.set(false);
        app.close_profile_modal(cx);
    });
    vcx.run_until_parked();
    app.read_with(vcx, |app, _| {
        assert!(app.dialogs.profile_modal.is_none());
    });
    assert_eq!(
        info_panel(&app, vcx),
        Some(column),
        "the chat's column stays"
    );
}
