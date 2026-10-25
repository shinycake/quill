//! "Hide new chats?" over the demo account, and the archive-and-mute
//! switch in Privacy and Security (needs the `demo-capture` feature for
//! gpui-kit's test support). Nothing reaches Telegram.
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::{DialogKind, QuillShell};
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::archive_mute::ACTION;
use quill::settings_catalog::Page;
use quill::telegram::envelope::OptionValue;

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, Entity<QuillShell>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(900.)), move |window, cx| {
        let demo = ScreenshotDemo::named("ready-archive-mute-suggestion");
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some((view, shell.clone()));
        Root::new(shell, window, cx)
    });
    let (app, shell) = slot.borrow().clone().expect("the app view");
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    (app, shell, vcx)
}

/// The kit dialog that shows, once its open animation has finished so
/// clicks land where the controls are drawn.
fn shown_dialog(shell: &Entity<QuillShell>, vcx: &mut VisualTestContext) -> Option<DialogKind> {
    vcx.run_until_parked();
    vcx.executor()
        .advance_clock(std::time::Duration::from_millis(400));
    vcx.update(|window, cx| window.render_frame(cx));
    shell.read_with(vcx, |shell, _| shell.open_dialog)
}

fn suggested(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> bool {
    app.read_with(vcx, |app, _| {
        app.demo_session
            .as_ref()
            .is_some_and(|s| s.chat_list.suggestions.actions.contains(ACTION))
    })
}

fn on_privacy_page(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> bool {
    app.read_with(vcx, |app, _| {
        app.settings.open && app.settings.page == Some(Page::Privacy)
    })
}

#[gpui_kit::test]
fn go_to_settings_opens_privacy_and_dismisses(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    assert_eq!(
        shown_dialog(&shell, vcx),
        Some(DialogKind::ArchiveMuteSuggestion)
    );
    vcx.update(|window, cx| window.click("archive-mute-suggestion-settings", cx));
    assert_eq!(shown_dialog(&shell, vcx), None);
    assert!(!suggested(&app, vcx), "dismissed");
    assert!(on_privacy_page(&app, vcx));
}

#[gpui_kit::test]
fn cancel_dismisses_and_stays_put(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    assert_eq!(
        shown_dialog(&shell, vcx),
        Some(DialogKind::ArchiveMuteSuggestion)
    );
    vcx.update(|window, cx| window.click("archive-mute-suggestion-cancel", cx));
    assert_eq!(shown_dialog(&shell, vcx), None);
    assert!(!suggested(&app, vcx), "dismissed");
    assert!(!on_privacy_page(&app, vcx));
}

#[gpui_kit::test]
fn escape_dismisses_too(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    assert_eq!(
        shown_dialog(&shell, vcx),
        Some(DialogKind::ArchiveMuteSuggestion)
    );
    vcx.simulate_keystrokes("escape");
    assert_eq!(shown_dialog(&shell, vcx), None);
    assert!(!suggested(&app, vcx), "dismissed");
}

#[gpui_kit::test]
fn no_box_while_the_setting_is_on(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update(vcx, |app, cx| {
        if let Some(session) = app.demo_session.as_mut()
            && let Some(settings) = session.chat_list.archive_chat_list_settings.as_mut()
        {
            settings.archive_and_mute_new_chats_from_unknown_users = true;
        }
        cx.notify();
    });
    assert_eq!(shown_dialog(&shell, vcx), None);
    assert!(suggested(&app, vcx), "kept for later");
}

#[gpui_kit::test]
fn the_privacy_switch_shows_when_allowed_or_premium(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let mute_switch = |vcx: &mut VisualTestContext| {
        vcx.update(|window, _| {
            assert!(
                window.try_find("archive-privacy-1").is_some(),
                "other rows stay"
            );
            window.try_find("archive-privacy-0").is_some()
        })
    };
    let seed = |vcx: &mut VisualTestContext, premium: bool, allowed: bool| {
        app.update(vcx, |app, cx| {
            if let Some(session) = app.demo_session.as_mut() {
                session.chat_list.suggestions.actions.remove(ACTION);
                session.payments.premium_option = Some(premium);
                session.chat_list.archive_mute.apply_option(
                    "can_archive_and_mute_new_chats_from_unknown_users",
                    &OptionValue::Boolean(allowed),
                );
            }
            app.open_settings(Some(Page::Privacy));
            cx.notify();
        });
    };
    seed(vcx, false, false);
    assert_eq!(shown_dialog(&shell, vcx), None);
    assert!(!mute_switch(vcx), "hidden without Premium or the server");
    seed(vcx, true, false);
    shown_dialog(&shell, vcx);
    assert!(mute_switch(vcx), "Premium");
    seed(vcx, false, true);
    shown_dialog(&shell, vcx);
    assert!(mute_switch(vcx), "server allows it");
}
