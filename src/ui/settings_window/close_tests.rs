//! What Settings opens over itself belongs to it: closing Settings drops
//! Log out? and the privacy rule editor, a chat window's menu leaves the
//! main window's Settings open, Agent access from a chat window opens in
//! the main window, and the password check card draws its own field on
//! the Privacy page and in the 2FA box.

use crate::ui::app::QuillApp;
use crate::ui::frame_clock::frames_over;
use crate::ui::navigation::NavigationAction;
use crate::ui::privacy::PrivacyExceptionKind;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::ids::ChatId;
use quill::settings_catalog::Page;
use quill::telegram::envelope::PasswordState;

const OTHER: ChatId = ChatId(12);

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, AnyWindowHandle) {
    let root = quill::scratch::process_dir("quill-settings-close");
    let _ = std::fs::create_dir_all(&root);
    quill::settings::use_isolated_app_root(root);
    cx.update(gpui_kit::init);
    cx.update(crate::ui::bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(2400.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named("ready-chats")))
        });
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let app = slot.borrow().clone().expect("the app view");
    cx.run_until_parked();
    (app, handle.into())
}

fn frames(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    vcx.executor()
        .advance_clock(std::time::Duration::from_millis(300));
    for _ in 0..2 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
    vcx.run_until_parked();
}

fn other_chat(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> ChatId {
    app.read_with(vcx, |app, _| {
        let open = app.session().and_then(|s| s.open_chat);
        app.session()
            .and_then(|s| {
                s.chats
                    .keys()
                    .map(|id| ChatId(*id))
                    .find(|id| Some(*id) != open)
            })
            .expect("a demo chat")
    })
}

fn open_chat_window(
    app: &Entity<QuillApp>,
    main: AnyWindowHandle,
    cx: &mut TestAppContext,
) -> AnyWindowHandle {
    cx.update_window(main, |_, window, cx| {
        app.update(cx, |app, cx| app.open_chat_in_new_window(OTHER, window, cx));
    })
    .expect("the main window");
    cx.run_until_parked();
    cx.windows()
        .into_iter()
        .find(|handle| handle.window_id() != main.window_id())
        .expect("the chat window opened")
}

/// Log out? goes with Settings: opening a chat closes both, and the next
/// Settings open shows no box.
#[gpui_kit::test]
fn closing_settings_drops_log_out(cx: &mut TestAppContext) {
    let (app, main) = new_app(cx);
    let mut vcx = VisualTestContext::from_window(main, cx);
    let vcx = &mut vcx;
    app.update(vcx, |app, cx| {
        app.demo_settings(Page::Account);
        cx.notify();
    });
    frames(vcx);
    vcx.update(|window, cx| window.click("settings-row-log-out", cx));
    frames(vcx);
    assert!(app.read_with(vcx, |app, _| app.settings.log_out_confirm));
    let chat = other_chat(&app, vcx);
    app.update_in(vcx, |app, window, cx| {
        app.select_listed_chat(chat, window, cx)
    });
    frames(vcx);
    app.update(vcx, |app, cx| {
        app.open_settings(None);
        cx.notify();
    });
    frames(vcx);
    assert!(
        vcx.update(|window, _| window.try_find("log-out-confirm").is_none()),
        "no Log out?"
    );
    assert!(!app.read_with(vcx, |app, _| app.settings.log_out_confirm));
}

/// The privacy rule editor and its exceptions list close with Settings,
/// so nothing of them floats over the chat.
#[gpui_kit::test]
fn closing_settings_closes_the_privacy_editor(cx: &mut TestAppContext) {
    let (app, main) = new_app(cx);
    let mut vcx = VisualTestContext::from_window(main, cx);
    let vcx = &mut vcx;
    app.update(vcx, |app, cx| {
        app.demo_settings(Page::Account);
        app.show_settings_page(Page::Privacy, cx);
    });
    frames(vcx);
    vcx.update(|window, cx| window.click("privacy-rule-userPrivacySettingShowStatus", cx));
    frames(vcx);
    assert!(vcx.update(|window, _| !window.find_all("privacy-close-editor").is_empty()));
    app.update(vcx, |app, cx| {
        let rule = app.privacy.editor.expect("the rule editor is open");
        app.privacy.exceptions = Some((rule, PrivacyExceptionKind::Always));
        cx.notify();
    });
    frames(vcx);
    let chat = other_chat(&app, vcx);
    app.update_in(vcx, |app, window, cx| {
        app.select_listed_chat(chat, window, cx)
    });
    frames(vcx);
    app.read_with(vcx, |app, _| {
        assert!(!app.settings.open);
        assert!(app.privacy.editor.is_none(), "the editor closed");
        assert!(app.privacy.exceptions.is_none(), "the exceptions closed");
    });
    assert!(vcx.update(|window, _| window.find_all("privacy-close-editor").is_empty()));
    assert!(vcx.update(|window, _| window.find_all("privacy-close-exceptions").is_empty()));
}

/// A chat window's ⋮ › Mute acts in that window: the main window keeps
/// its Settings page.
#[gpui_kit::test]
fn a_chat_window_menu_action_leaves_the_main_windows_settings_open(cx: &mut TestAppContext) {
    let (app, main) = new_app(cx);
    let chat_window = open_chat_window(&app, main, cx);
    app.update(cx, |app, cx| {
        app.open_settings(Some(Page::Language));
        cx.notify();
    });
    cx.run_until_parked();
    cx.update_window(chat_window, |_, window, cx| {
        app.update(cx, |app, cx| {
            app.in_window_scope(Some(OTHER), cx, |app, cx| {
                app.navigate(NavigationAction::ChatMute, window, cx);
            });
        });
    })
    .expect("the chat window");
    cx.run_until_parked();
    app.read_with(cx, |app, _| {
        assert!(app.settings.open, "Settings stays open");
        assert_eq!(app.settings.page, Some(Page::Language));
    });
}

/// Agent access from a chat window's menu opens in the main window, which
/// draws Settings, and brings it forward.
#[gpui_kit::test]
fn agent_access_from_a_chat_window_opens_in_the_main_window(cx: &mut TestAppContext) {
    let (app, main) = new_app(cx);
    let chat_window = open_chat_window(&app, main, cx);
    cx.update_window(chat_window, |_, window, _| window.activate_window())
        .expect("the chat window");
    cx.run_until_parked();
    let ran_in = app.update(cx, |app, cx| {
        app.in_window_scope(Some(OTHER), cx, |app, cx| {
            app.open_agent_access_for_chat(OTHER.0, cx);
            app.settings.open
        })
    });
    assert_eq!(ran_in, Some(false), "nothing opens in the chat window");
    cx.run_until_parked();
    app.read_with(cx, |app, _| {
        assert!(app.settings.open);
        assert_eq!(app.settings.page, Some(Page::AgentAccess));
        assert_eq!(app.agent_ui.page.focus_chat, Some(OTHER.0));
    });
    let main_active = cx
        .update_window(main, |_, window, _| window.is_window_active())
        .expect("the main window");
    assert!(main_active, "the main window came forward");
}

/// The Privacy page and the 2FA box over it each draw the password check
/// card with a field of their own: what is typed in the box's card does
/// not show on the page, and the window then goes idle. One field painted
/// in both places showed the text twice and took its clicks and caret from
/// whichever card painted last (`lock-field-per-window.md`).
#[gpui_kit::test]
fn the_password_check_on_the_page_and_in_the_2fa_box_has_a_field_each(cx: &mut TestAppContext) {
    let (app, main) = new_app(cx);
    let mut vcx = VisualTestContext::from_window(main, cx);
    let vcx = &mut vcx;
    app.update(vcx, |app, cx| {
        app.demo_settings(Page::Account);
        app.show_settings_page(Page::Privacy, cx);
        app.open_twofa(cx);
        if let Some(session) = app.demo_session.as_mut() {
            session.settings.privacy_data.check_password_suggested = true;
            session.auth_state.password_state = Some(PasswordState {
                has_password: true,
                password_hint: String::new(),
                has_recovery_email_address: false,
                has_passport_data: false,
                pending_email_pattern: None,
                pending_email_code_length: 0,
                login_email_address_pattern: String::new(),
                pending_reset_date: 0,
            });
        }
        cx.notify();
    });
    frames(vcx);
    let cards = vcx.update(|window, _| window.find_all("password-check-submit").len());
    assert_eq!(cards, 2, "the page and the box each draw the card");
    app.update_in(vcx, |app, window, cx| {
        let field = app.twofa.status_check_password.clone();
        field.update(cx, |input, cx| input.focus(window, cx));
    });
    frames(vcx);
    vcx.simulate_input("secret");
    frames(vcx);
    let (page, the_box) = app.read_with(vcx, |app, cx| {
        (
            app.twofa.check_password.read(cx).value().to_string(),
            app.twofa.status_check_password.read(cx).value().to_string(),
        )
    });
    assert_eq!(the_box, "secret", "typed into the box's card");
    assert_eq!(page, "", "the page's card has its own field");
    let _ = frames_over(vcx, 5);
    let drawn = frames_over(vcx, 60);
    assert!(drawn.is_empty(), "drawn while nothing changes: {drawn:?}");
}

/// "Abort recovery email setup" asks inside the 2FA box: a separate
/// confirm box opened under it and showed only once the 2FA box closed.
#[gpui_kit::test]
fn aborting_the_recovery_email_asks_inside_the_2fa_box(cx: &mut TestAppContext) {
    let (app, main) = new_app(cx);
    let mut vcx = VisualTestContext::from_window(main, cx);
    let vcx = &mut vcx;
    app.update(vcx, |app, cx| {
        app.demo_settings(Page::Account);
        app.show_settings_page(Page::Privacy, cx);
        app.open_twofa(cx);
        if let Some(session) = app.demo_session.as_mut() {
            session.auth_state.password_state = Some(PasswordState {
                has_password: true,
                password_hint: String::new(),
                has_recovery_email_address: false,
                has_passport_data: false,
                pending_email_pattern: Some("a***@example.com".into()),
                pending_email_code_length: 6,
                login_email_address_pattern: String::new(),
                pending_reset_date: 0,
            });
        }
        cx.notify();
    });
    frames(vcx);
    let trigger = format!(
        "twofa-confirm-open-{}",
        quill::lang::tr(quill::lang::keys::Q_PREFS_ABORT_RECOVERY_EMAIL_SETUP)
    );
    vcx.update(|window, cx| window.click(gpui_kit::SharedString::from(trigger), cx));
    frames(vcx);
    app.read_with(vcx, |app, _| {
        assert_eq!(
            app.twofa.confirm,
            Some(crate::ui::dialogs::TwofaConfirm::AbortRecoveryEmail)
        );
        assert!(app.admin.group_confirm_dialog.is_none(), "no box under it");
    });
    assert!(
        vcx.update(|window, _| window.find_all("twofa-confirm-yes").len()) == 1,
        "the question shows in the 2FA box"
    );
}
