//! Advanced settings in the UI: the secret codes typed on the Settings list,
//! the Experimental settings page and the update channel switch (needs the
//! `demo-capture` feature for gpui-kit's test support). Nothing leaves the
//! app: no connection, and the app data root is a temp directory.
use crate::ui::advanced_ui::{ADVANCED_PAGE, EXPERIMENTAL_PAGE, parent_page};
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::{DialogKind, QuillShell};
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AppContext, Entity, Keystroke, Modifiers, TestAppContext, VisualTestContext, px, size,
};
use quill::advanced::experimental::{self, Imported, Values};

/// The experimental options are process-wide; the tests that set them take
/// this lock so parallel tests don't flip them under each other.
static EXPERIMENTAL_OPTIONS: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, Entity<QuillShell>, VisualTestContext) {
    new_app_on(cx, "ready-privacy")
}

fn new_app_on(
    cx: &mut TestAppContext,
    demo: &'static str,
) -> (Entity<QuillApp>, Entity<QuillShell>, VisualTestContext) {
    let root = std::env::temp_dir().join(format!("quill-advanced-ui-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&root);
    quill::settings::use_isolated_app_root(root);
    quill::fast_buttons::set_persistence(false);
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx
            .new(|cx| QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named(demo))));
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some((view, shell.clone()));
        Root::new(shell, window, cx)
    });
    let (app, shell) = slot.borrow().clone().unwrap();
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    (app, shell, vcx)
}

fn typed(text: &str) -> Vec<Keystroke> {
    text.chars()
        .map(|c| Keystroke {
            modifiers: Modifiers::default(),
            key: c.to_string(),
            key_char: Some(c.to_string()),
        })
        .collect()
}

fn type_into(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, text: &str) {
    for keystroke in typed(text) {
        app.update_in(vcx, |app, window, cx| {
            app.feed_secret_code(&keystroke, window, cx)
        });
    }
}

#[gpui_kit::test]
fn debugmode_typed_in_settings_asks_before_turning_the_logs_on(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, _, cx| {
        app.settings.open = true;
        cx.notify();
    });
    type_into(&app, vcx, "debugmode");
    vcx.run_until_parked();
    app.read_with(vcx, |app, _| {
        assert_eq!(
            app.settings.advanced.debug_confirm,
            Some(quill::advanced::debug_mode::enabled())
        );
    });
    shell.read_with(vcx, |shell, _| {
        assert_eq!(shell.open_dialog, Some(DialogKind::DebugModeConfirm));
    });
    // Cancel keeps the mode as it was and clears the question.
    let was = quill::advanced::debug_mode::enabled();
    app.update_in(vcx, |app, _, cx| {
        app.settings.advanced.debug_confirm = None;
        cx.notify();
    });
    vcx.run_until_parked();
    assert_eq!(quill::advanced::debug_mode::enabled(), was);
    shell.read_with(vcx, |shell, _| {
        assert_ne!(shell.open_dialog, Some(DialogKind::DebugModeConfirm));
    });
}

#[gpui_kit::test]
fn codes_do_nothing_where_no_screen_takes_them(cx: &mut TestAppContext) {
    let (app, _shell, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, _, _| {
        app.settings.open = false;
    });
    type_into(&app, vcx, "debugmode");
    app.read_with(vcx, |app, _| {
        assert_eq!(app.settings.advanced.debug_confirm, None);
    });
    // A Settings subpage does not read them either (tdesktop reads the list).
    app.update_in(vcx, |app, _, _| {
        app.settings.open = true;
        app.settings.page = Some(ADVANCED_PAGE);
    });
    type_into(&app, vcx, "debugmode");
    app.read_with(vcx, |app, _| {
        assert_eq!(app.settings.advanced.debug_confirm, None);
    });
    // Ctrl-chords are not typed text.
    app.update_in(vcx, |app, _, _| app.settings.page = None);
    let chord = Keystroke {
        modifiers: Modifiers {
            control: true,
            ..Modifiers::default()
        },
        key: "d".into(),
        key_char: Some("d".into()),
    };
    for _ in 0..1 {
        app.update_in(vcx, |app, window, cx| {
            app.feed_secret_code(&chord, window, cx)
        });
    }
    type_into(&app, vcx, "ebugmod");
    app.read_with(vcx, |app, _| {
        assert_eq!(app.settings.advanced.debug_confirm, None);
    });
}

#[gpui_kit::test]
fn numberbuttons_switches_fast_buttons_mode_and_lists_it_on_the_page(cx: &mut TestAppContext) {
    let (app, _shell, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let _options = EXPERIMENTAL_OPTIONS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    experimental::set_fast_buttons_mode(false);
    app.update_in(vcx, |app, _, _| app.settings.open = true);
    type_into(&app, vcx, "numberbuttons");
    app.read_with(vcx, |app, _| {
        assert!(experimental::fast_buttons_mode());
        assert_eq!(
            app.connection.status_note(),
            experimental::FAST_BUTTONS_ENABLED_TOAST
        );
        assert!(app.experimental_values().fast_buttons_mode);
    });
    // The per-bot switch only works while the option is on.
    quill::fast_buttons::set_enabled(4242, true);
    assert!(quill::fast_buttons::is_enabled(4242));
    type_into(&app, vcx, "numberbuttons");
    app.read_with(vcx, |app, _| {
        assert!(!experimental::fast_buttons_mode());
        assert_eq!(
            app.connection.status_note(),
            experimental::FAST_BUTTONS_DISABLED_TOAST
        );
    });
    assert!(!quill::fast_buttons::is_enabled(4242));
    quill::fast_buttons::set_enabled(4242, false);
}

#[gpui_kit::test]
fn the_experimental_row_on_the_advanced_page_opens_the_experimental_page(cx: &mut TestAppContext) {
    let (app, _shell, mut vcx) = new_app_on(cx, "ready-settings-advanced");
    let vcx = &mut vcx;
    app.update(vcx, |_, cx| cx.notify());
    for _ in 0..3 {
        vcx.run_until_parked();
        std::thread::sleep(std::time::Duration::from_millis(300));
        vcx.update(|window, cx| window.render_frame(cx));
    }
    let open = _shell.read_with(vcx, |shell, _| shell.open_dialog);
    assert_eq!(open, Some(DialogKind::Settings));
    vcx.update(|window, cx| window.click("settings-experimental", cx));
    vcx.run_until_parked();
    app.update(vcx, |app, _| {
        assert_eq!(app.settings.page, Some(EXPERIMENTAL_PAGE));
    });
}

#[gpui_kit::test]
fn the_experimental_page_changes_restores_exports_and_imports(cx: &mut TestAppContext) {
    let (app, _shell, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let _options = EXPERIMENTAL_OPTIONS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    experimental::set_fast_buttons_mode(false);
    app.update_in(vcx, |app, _, cx| {
        app.settings.open = true;
        app.settings.page = Some(EXPERIMENTAL_PAGE);
        assert_eq!(parent_page(EXPERIMENTAL_PAGE), Some(ADVANCED_PAGE));
        assert_eq!(parent_page(ADVANCED_PAGE), None);
        assert!(!app.experimental_values().changed());
        app.set_experimental_option(experimental::SPLIT_MEDIA_ID, true, cx);
        assert!(app.chat_prefs.profile_media_tabs_expanded);
        assert!(app.experimental_values().changed());
    });
    vcx.run_until_parked();
    let code = app.update_in(vcx, |app, _, _| {
        experimental::encode_export(app.experimental_values())
    });
    // Restore default values puts everything back.
    app.update_in(vcx, |app, _, cx| {
        app.set_experimental_option(experimental::SPLIT_MEDIA_ID, false, cx);
        assert!(!app.chat_prefs.profile_media_tabs_expanded);
    });
    let Imported::Values(values) = experimental::decode_import(&code) else {
        panic!("the exported code must import");
    };
    assert_eq!(
        values,
        Values {
            split_profile_media: true,
            ..Values::default()
        }
    );
    app.update_in(vcx, |app, _, cx| {
        app.set_experimental_option(experimental::SPLIT_MEDIA_ID, false, cx);
    });
}

#[gpui_kit::test]
fn the_hardware_video_switch_on_the_advanced_page_flips_the_setting(cx: &mut TestAppContext) {
    use quill::advanced::hardware_video;
    let (app, _shell, mut vcx) = new_app_on(cx, "ready-settings-advanced");
    let vcx = &mut vcx;
    hardware_video::set_enabled(true);
    app.update(vcx, |_, cx| cx.notify());
    for _ in 0..3 {
        vcx.run_until_parked();
        std::thread::sleep(std::time::Duration::from_millis(300));
        vcx.update(|window, cx| window.render_frame(cx));
    }
    vcx.update(|window, cx| window.click("advanced-hardware-video", cx));
    vcx.run_until_parked();
    assert!(!hardware_video::enabled(), "off after one click");
    vcx.update(|window, cx| window.render_frame(cx));
    vcx.update(|window, cx| window.click("advanced-hardware-video", cx));
    vcx.run_until_parked();
    assert!(hardware_video::enabled(), "on again");
}

#[gpui_kit::test]
fn the_install_beta_switch_follows_the_update_switch(cx: &mut TestAppContext) {
    let (app, _shell, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, window, cx| {
        let _ = window;
        app.appearance.check_updates_on_launch = true;
        assert!(app.install_beta_row(cx).is_some());
        app.appearance.check_updates_on_launch = false;
        assert!(app.install_beta_row(cx).is_none());
        app.appearance.check_updates_on_launch = true;
        let release = quill::updater::ReleaseInfo {
            version: "1.0.0".into(),
            notes: String::new(),
            url: String::new(),
            asset: None,
        };
        app.settings.update_state = quill::updater::UpdateState::Downloading(release);
        assert!(
            app.install_beta_row(cx).is_none(),
            "hidden while an update downloads"
        );
    });
}

/// A signed-out screen with no connection: the screenshot demo of the
/// phone step.
fn sign_in_screen(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named("wait-phone")))
        });
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let Some(app) = slot.borrow().clone() else {
        panic!("the window opened");
    };
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    vcx.run_until_parked();
    vcx.update(|window, cx| window.render_frame(cx));
    (app, vcx)
}

fn test_mode_label_shown(vcx: &mut VisualTestContext) -> bool {
    vcx.run_until_parked();
    vcx.update(|window, cx| window.render_frame(cx));
    vcx.debug_bounds("auth-test-mode-label").is_some()
}

#[gpui_kit::test]
fn testmode_typed_on_the_sign_in_screen_shows_the_test_mode_label(cx: &mut TestAppContext) {
    use quill::advanced::test_environment::{SWITCHED_TO_PRODUCTION, SWITCHED_TO_TEST};
    let (app, mut vcx) = sign_in_screen(cx);
    let vcx = &mut vcx;
    assert!(!test_mode_label_shown(vcx), "production at first");
    // Typed keys, as the window gets them.
    vcx.simulate_keystrokes("t e s t m o d e");
    assert!(
        test_mode_label_shown(vcx),
        "the sign-in screen says Test Mode"
    );
    app.read_with(vcx, |app, _| {
        assert!(app.settings.advanced.test_environment());
        assert_eq!(app.connection.status_note(), SWITCHED_TO_TEST);
        assert!(app.live.is_none(), "nothing connected");
    });
    // Typed again, it switches back to production.
    vcx.simulate_keystrokes("t e s t m o d e");
    assert!(!test_mode_label_shown(vcx));
    app.read_with(vcx, |app, _| {
        assert_eq!(app.connection.status_note(), SWITCHED_TO_PRODUCTION);
    });
}
