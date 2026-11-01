//! The composer's Formatting > Date box and "Insert Unicode control
//! character" on a demo `QuillApp` ("hello from composer" in the field).

use crate::ui::app::QuillApp;
use crate::ui::composer_unicode::CONTROL_CHARACTERS;
use crate::ui::keybindings::bind_keys;
use crate::ui::scheduled::picker_value;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};

const TIME: i64 = 1_790_632_800;

fn frames(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    for _ in 0..2 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

fn open_demo(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named("ready-chats-composer");
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().expect("app");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    frames(&mut vcx);
    (app, vcx)
}

/// Select `part` of the draft and focus the field.
fn select(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, part: &str) {
    app.update_in(vcx, |app, window, cx| {
        let text = app.composer.read(cx).value().to_string();
        let start = text.find(part).expect("part");
        app.composer.update(cx, |input, cx| {
            input.focus(window, cx);
            input.set_selected_range(start..start + part.len(), cx);
        });
    });
}

#[gpui_kit::test]
fn the_date_box_turns_the_selection_into_a_date(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_demo(cx);
    let vcx = &mut vcx;
    // Nothing selected and no date under the caret: Date is off.
    app.update(vcx, |app, cx| assert!(!app.composer_date_enabled(cx)));

    select(&app, vcx, "composer");
    #[cfg(target_os = "macos")]
    vcx.simulate_keystrokes("cmd-shift-d");
    #[cfg(not(target_os = "macos"))]
    vcx.simulate_keystrokes("ctrl-shift-d");
    frames(vcx);
    app.update(vcx, |app, _| assert!(app.composer_ui.date_dialog.is_some()));
    assert!(vcx.update(|window, _| window.try_find("composer-date-dialog").is_some()));

    app.update_in(vcx, |app, window, cx| {
        let dialog = app.composer_ui.date_dialog.as_ref().expect("box");
        let value = picker_value(TIME, cx).expect("value");
        dialog
            .picker
            .state
            .update(cx, |state, cx| state.set_date_time(value, window, cx));
    });
    vcx.update(|window, cx| window.click("composer-date-save", cx));
    frames(vcx);
    app.update(vcx, |app, cx| {
        assert!(app.composer_ui.date_dialog.is_none());
        assert_eq!(
            app.composer_markup(cx),
            format!("hello from [composer](tg://time?unix={TIME})")
        );
    });

    // The caret in the date: Date is on and reopens at that time.
    app.update_in(vcx, |app, window, cx| {
        let end = app.composer.read(cx).value().len();
        app.composer.update(cx, |input, cx| {
            input.set_selected_range(end - 2..end - 2, cx);
        });
        assert!(app.composer_date_enabled(cx));
        assert_eq!(
            app.composer_date_at_caret(cx)
                .map(|(_, time)| i64::from(time)),
            Some(TIME)
        );
        app.open_composer_date_dialog(window, cx);
        let dialog = app.composer_ui.date_dialog.as_ref().expect("box");
        assert_eq!(
            crate::ui::scheduled::picked_unix(&dialog.picker, cx),
            Some(TIME)
        );
        app.close_composer_date_dialog(window, cx);
        assert!(app.composer_ui.date_dialog.is_none());
    });
}

#[gpui_kit::test]
fn a_control_character_is_typed_over_the_selection(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_demo(cx);
    let vcx = &mut vcx;
    assert_eq!(CONTROL_CHARACTERS.len(), 14);
    select(&app, vcx, "from");
    app.update_in(vcx, |app, window, cx| {
        app.insert_composer_control_char('\u{200f}', window, cx);
        assert_eq!(app.composer.read(cx).value(), "hello \u{200f} composer");
    });
}
