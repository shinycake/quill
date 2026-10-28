//! Advanced settings in the UI: the secret codes typed on the Settings list,
//! the Experimental settings page and the update channel switch (needs the
//! `demo-capture` feature for gpui-kit's test support). Nothing leaves the
//! app: no connection, and the app data root is a temp directory.
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::settings_window::SubPage;
use crate::ui::shell::{DialogKind, QuillShell};
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AppContext, Entity, Keystroke, Modifiers, TestAppContext, VisualTestContext, px, size,
};
use quill::advanced::experimental::{self, Imported, Values};
use quill::settings_catalog::Page;

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
    let root = quill::scratch::process_dir("quill-advanced-ui");
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
    // A Settings subpage does not read them either (tdesktop reads them on
    // the settings pages, not on boxes and subpages opened from them).
    app.update_in(vcx, |app, _, _| {
        app.settings.open = true;
        app.settings.page = Some(Page::Advanced);
        app.settings.sub = Some(SubPage::Experimental);
    });
    type_into(&app, vcx, "debugmode");
    app.read_with(vcx, |app, _| {
        assert_eq!(app.settings.advanced.debug_confirm, None);
    });
    // Ctrl-chords are not typed text.
    app.update_in(vcx, |app, _, _| app.settings.sub = None);
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
        vcx.executor()
            .advance_clock(std::time::Duration::from_millis(300));
        vcx.update(|window, cx| window.render_frame(cx));
    }
    vcx.update(|window, cx| window.click("settings-experimental", cx));
    vcx.run_until_parked();
    app.update(vcx, |app, _| {
        assert_eq!(app.settings.page, Some(Page::Advanced));
        assert_eq!(app.settings.sub, Some(SubPage::Experimental));
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
        app.settings.page = Some(Page::Advanced);
        app.settings.sub = Some(SubPage::Experimental);
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
        vcx.executor()
            .advance_clock(std::time::Duration::from_millis(300));
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

/// The self-update answer is process-wide; the tests that force it take
/// this lock.
static AUTO_UPDATE_STATE: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// A release the check finds downloads at once on a copy that can replace
/// itself; a copy a package manager owns only shows it.
#[gpui_kit::test]
fn a_release_the_check_finds_downloads_on_its_own(cx: &mut TestAppContext) {
    use quill::updater::{RELEASES_URL, ReleaseAsset, ReleaseInfo, UpdateState};
    let _guard = AUTO_UPDATE_STATE.lock().unwrap_or_else(|e| e.into_inner());
    let (app, _shell, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let release = ReleaseInfo {
        version: "999.0.0".into(),
        notes: String::new(),
        url: format!("{RELEASES_URL}/tag/v999.0.0"),
        asset: Some(ReleaseAsset {
            url: format!(
                "{RELEASES_URL}/download/v999.0.0/{}",
                quill::updater::update_asset_name()
            ),
            size: 100,
            sha256: "a".repeat(64),
        }),
    };
    quill::update_install::force_self_update_available(Some(Err(
        quill::update_install::Unavailable::PackageManager,
    )));
    app.update_in(vcx, |app, _, cx| {
        app.apply_check_result(UpdateState::Available(release.clone()), cx);
        assert!(
            matches!(app.settings.update_state, UpdateState::Available(_)),
            "a package manager installs it: only shown"
        );
    });
    // Forced: the download in the background refuses at once, offline.
    quill::update_install::force_self_update_available(Some(Ok(())));
    app.update_in(vcx, |app, _, cx| {
        app.settings.update_state = UpdateState::Idle;
        app.apply_check_result(UpdateState::Available(release.clone()), cx);
        assert!(
            matches!(&app.settings.update_state, UpdateState::Downloading(r, _) if r.version == "999.0.0"),
            "the release downloads without a click"
        );
    });
    vcx.run_until_parked();
    app.update_in(vcx, |app, _, _| {
        assert!(
            matches!(app.settings.update_state, UpdateState::DownloadFailed(..)),
            "the download finished (offline here) and its result is shown"
        );
    });
    quill::update_install::force_self_update_available(None);
}

fn shown(vcx: &mut VisualTestContext, id: &'static str) -> bool {
    vcx.update(|window, cx| window.render_frame(cx));
    vcx.update(|window, _| window.try_find(id).is_some())
}

fn click_id(vcx: &mut VisualTestContext, id: &'static str) {
    vcx.update(|window, cx| {
        window.render_frame(cx);
        window.click(id, cx);
    });
    vcx.run_until_parked();
}

/// A staged update puts "Restart to update" in the title bar, and no
/// banner asks the same.
#[gpui_kit::test]
fn a_staged_update_asks_for_a_restart_from_the_title_bar(cx: &mut TestAppContext) {
    let _guard = AUTO_UPDATE_STATE.lock().unwrap_or_else(|e| e.into_inner());
    let (app, _shell, mut vcx) = new_app_on(cx, "ready-update-staged");
    let vcx = &mut vcx;
    // The demo opens Settings over the chat list.
    app.update(vcx, |app, _| app.close_settings());
    vcx.run_until_parked();
    assert!(shown(vcx, "title-bar-update"));
    assert!(!shown(vcx, "dismiss-update-banner"));
    app.update_in(vcx, |app, _, _| {
        assert_eq!(app.update_control(), quill::updater::UpdateControl::Restart);
        app.settings.update_state = quill::updater::UpdateState::UpToDate;
    });
    assert!(
        !shown(vcx, "title-bar-update"),
        "nothing to act on: the title bar has no update button"
    );
    quill::update_install::force_self_update_available(None);
}

/// "Update Quill" in the title bar starts the download; the button follows
/// it (offline here, so the download fails and offers the update again).
#[gpui_kit::test]
fn update_quill_in_the_title_bar_downloads_the_release(cx: &mut TestAppContext) {
    use quill::updater::{UpdateControl, UpdateState};
    let _guard = AUTO_UPDATE_STATE.lock().unwrap_or_else(|e| e.into_inner());
    let (app, _shell, mut vcx) = new_app_on(cx, "ready-update-install");
    let vcx = &mut vcx;
    app.update(vcx, |app, _| app.close_settings());
    vcx.run_until_parked();
    app.update_in(vcx, |app, _, _| {
        assert!(matches!(app.update_control(), UpdateControl::Install(_)));
    });
    click_id(vcx, "title-bar-update");
    app.update_in(vcx, |app, _, _| {
        assert!(
            matches!(app.settings.update_state, UpdateState::DownloadFailed(..)),
            "the click downloaded (and, offline, failed)"
        );
        assert!(matches!(app.update_control(), UpdateControl::Install(_)));
    });
    assert!(shown(vcx, "title-bar-update"));
    quill::update_install::force_self_update_available(None);
}
