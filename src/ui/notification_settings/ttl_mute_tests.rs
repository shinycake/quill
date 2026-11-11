//! The auto-delete picker and the custom mute duration against a demo
//! window (needs the `demo-capture` feature for gpui-kit's test support).
use crate::ui::app::QuillApp;
use crate::ui::index_sliders::StepSlider;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::component::slider::{SliderEvent, SliderValue};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::ids::ChatId;

const DAY: i32 = 86_400;

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    let root = quill::scratch::process_dir("quill-ttl-mute");
    let _ = std::fs::create_dir_all(&root);
    quill::settings::use_isolated_app_root(root);
    cx.update(gpui_kit::init);
    cx.update(crate::ui::bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(900.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(
                window,
                cx,
                None,
                Some(ScreenshotDemo::named("ready-mute-archive")),
            )
        });
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let Some(app) = slot.borrow().clone() else {
        panic!("demo window did not build");
    };
    (app, VisualTestContext::from_window(handle.into(), cx))
}

fn frames(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    for _ in 0..2 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

fn shown(vcx: &mut VisualTestContext, selector: &'static str) -> bool {
    vcx.update(|window, _| window.try_find(selector).is_some())
}

fn open_chat(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> ChatId {
    app.read_with(vcx, |app, _| {
        let Some(chat_id) = app.session().and_then(|s| s.open_chat) else {
            panic!("demo has no open chat");
        };
        chat_id
    })
}

fn ttl_of(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, chat_id: ChatId) -> i32 {
    app.read_with(vcx, |app, _| {
        let Some(chat) = app.session().and_then(|s| s.chats.get(&chat_id.0)) else {
            panic!("chat missing");
        };
        chat.message_auto_delete_time
    })
}

fn move_ttl_slider(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, index: usize) {
    app.update_in(vcx, |app, window, cx| {
        let Some(slider) = app
            .preferences
            .index_sliders
            .get(StepSlider::AutoDelete)
            .cloned()
        else {
            panic!("auto-delete slider missing");
        };
        slider.update(cx, |state, cx| {
            let value = SliderValue::Single(index as f32);
            state.set_value(value, window, cx);
            cx.emit(SliderEvent::Change(value));
        });
    });
    frames(vcx);
}

#[gpui_kit::test]
fn auto_delete_slider_saves_the_period_it_is_on(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    frames(vcx);
    let chat_id = open_chat(&app, vcx);
    app.update(vcx, |app, cx| app.open_ttl_picker(chat_id, cx));
    frames(vcx);
    assert!(shown(vcx, "ttl-slider"));
    assert!(shown(vcx, "ttl-save"));
    assert!(!shown(vcx, "ttl-disable"), "Off is a slider step");
    // No timer yet: the slider starts on Off.
    let start = app.read_with(vcx, |app, cx| {
        app.preferences
            .index_sliders
            .index(StepSlider::AutoDelete, cx)
    });
    assert_eq!(start, Some(0));

    // Index 3 is 3 days (0 is Off); nothing is sent until Save.
    move_ttl_slider(&app, vcx, 3);
    assert_eq!(ttl_of(&app, vcx, chat_id), 0);
    vcx.update(|window, cx| window.click("ttl-save", cx));
    frames(vcx);
    assert_eq!(ttl_of(&app, vcx, chat_id), 3 * DAY);
    assert!(!shown(vcx, "ttl-slider"), "saving closes the picker");

    // Reopened on the saved period; Off and Save disables it.
    app.update(vcx, |app, cx| app.open_ttl_picker(chat_id, cx));
    frames(vcx);
    let start = app.read_with(vcx, |app, cx| {
        app.preferences
            .index_sliders
            .index(StepSlider::AutoDelete, cx)
    });
    assert_eq!(start, Some(3));
    move_ttl_slider(&app, vcx, 0);
    vcx.update(|window, cx| window.click("ttl-save", cx));
    frames(vcx);
    assert_eq!(ttl_of(&app, vcx, chat_id), 0);
    assert!(!shown(vcx, "ttl-slider"));
}

#[gpui_kit::test]
fn cancel_closes_the_auto_delete_picker_without_a_change(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    frames(vcx);
    let chat_id = open_chat(&app, vcx);
    app.update(vcx, |app, cx| app.open_ttl_picker(chat_id, cx));
    frames(vcx);
    move_ttl_slider(&app, vcx, 5);
    vcx.update(|window, cx| window.click("ttl-cancel", cx));
    frames(vcx);
    assert!(!shown(vcx, "ttl-slider"));
    assert_eq!(ttl_of(&app, vcx, chat_id), 0);
}

#[gpui_kit::test]
fn custom_mute_is_three_fields_that_start_at_eight_hours(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    frames(vcx);
    let chat_id = open_chat(&app, vcx);
    app.update(vcx, |app, cx| app.open_mute_menu_for(chat_id, true, cx));
    frames(vcx);
    for id in [
        "mute-days",
        "mute-hours",
        "mute-minutes",
        "mute-custom-apply",
    ] {
        assert!(shown(vcx, id), "{id}");
    }
    let value = app.read_with(vcx, |app, cx| app.mute_time_value(cx));
    assert_eq!(value.map(|v| v.mute_for()), Some(8 * 3600));

    // Typing into the fields changes the total.
    app.update_in(vcx, |app, window, cx| {
        let Some(inputs) = app.notify.mute_time.as_ref() else {
            panic!("fields missing");
        };
        inputs.set_for_test("1", "2", "30", window, cx);
    });
    frames(vcx);
    let value = app.read_with(vcx, |app, cx| app.mute_time_value(cx));
    assert_eq!(value.map(|v| v.mute_for()), Some(DAY + 2 * 3600 + 30 * 60));

    // All zero reads "Unmute", and applying it lifts the mute.
    app.update_in(vcx, |app, window, cx| {
        let Some(inputs) = app.notify.mute_time.as_ref() else {
            panic!("fields missing");
        };
        inputs.set_for_test("0", "0", "0", window, cx);
    });
    frames(vcx);
    let value = app.read_with(vcx, |app, cx| app.mute_time_value(cx));
    assert_eq!(value.map(|v| v.mute_for()), Some(0));
}

#[gpui_kit::test]
fn reopening_custom_mute_starts_at_eight_hours_again(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    frames(vcx);
    let chat_id = open_chat(&app, vcx);
    app.update(vcx, |app, cx| app.open_mute_menu_for(chat_id, true, cx));
    frames(vcx);
    app.update_in(vcx, |app, window, cx| {
        let Some(inputs) = app.notify.mute_time.as_ref() else {
            panic!("fields missing");
        };
        inputs.set_for_test("3", "0", "0", window, cx);
    });
    frames(vcx);
    app.update(vcx, |app, cx| app.close_mute_menu(cx));
    app.update(vcx, |app, cx| app.open_mute_menu_for(chat_id, true, cx));
    frames(vcx);
    let value = app.read_with(vcx, |app, cx| app.mute_time_value(cx));
    assert_eq!(value.map(|v| v.mute_for()), Some(8 * 3600));
}

#[gpui_kit::test]
fn the_mute_panel_reopens_collapsed_after_a_story_sound_picker(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    frames(vcx);
    app.update(vcx, |app, cx| {
        app.open_mute_menu(cx);
        app.notify
            .set_mute_section(crate::ui::notify_state::MuteSection::StorySoundPicker);
        app.close_mute_menu(cx);
        app.open_mute_menu(cx);
        assert_eq!(
            app.notify.mute_section(),
            crate::ui::notify_state::MuteSection::Collapsed
        );
    });
}

#[gpui_kit::test]
fn custom_mute_applies_the_total_and_closes_the_panel(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    frames(vcx);
    let chat_id = open_chat(&app, vcx);
    app.update(vcx, |app, cx| app.open_mute_menu_for(chat_id, true, cx));
    frames(vcx);
    vcx.update(|window, cx| window.click("mute-custom-apply", cx));
    frames(vcx);
    app.read_with(vcx, |app, _| {
        assert!(app.notify.mute_box.is_none());
        let Some(chat) = app.session().and_then(|s| s.chats.get(&chat_id.0)) else {
            panic!("chat missing");
        };
        assert!(chat.is_muted(), "muted for the default 8 hours");
    });
}

/// The header's Mute box is for the chat it was opened on: a chat opened
/// under it (a notification click, another window) does not retarget it.
#[gpui_kit::test]
fn the_mute_box_keeps_the_chat_it_was_opened_for(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    frames(&mut vcx);
    let first = open_chat(&app, &mut vcx);
    app.update(&mut vcx, |app, cx| app.open_mute_menu(cx));
    let other = app.read_with(&vcx, |app, _| {
        app.session()
            .and_then(|s| s.chats.keys().copied().find(|id| *id != first.0))
            .map(ChatId)
    });
    let Some(other) = other else {
        panic!("demo has a second chat");
    };
    app.update_in(&mut vcx, |app, window, cx| {
        app.select_listed_chat(other, window, cx)
    });
    frames(&mut vcx);
    app.read_with(&vcx, |app, _| {
        assert_eq!(app.session().and_then(|s| s.open_chat), Some(other));
        assert_eq!(app.notify.mute_chat(), Some(first), "the box's own chat");
    });
}

/// The auto-delete picker sets the timer of the chat it was opened for
/// (a profile's menu opens it for a chat that is not the open one).
#[gpui_kit::test]
fn the_auto_delete_picker_saves_for_the_chat_it_was_opened_for(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    frames(vcx);
    let open = open_chat(&app, vcx);
    let other = app.read_with(vcx, |app, _| {
        app.session()
            .and_then(|s| s.chats.keys().copied().find(|id| *id != open.0))
            .map(ChatId)
    });
    let Some(other) = other else {
        panic!("demo has a second chat");
    };
    app.update(vcx, |app, cx| app.open_ttl_picker(other, cx));
    frames(vcx);
    move_ttl_slider(&app, vcx, 3);
    vcx.update(|window, cx| window.click("ttl-save", cx));
    frames(vcx);
    assert_eq!(ttl_of(&app, vcx, other), 3 * DAY, "the picker's chat");
    assert_eq!(ttl_of(&app, vcx, open), 0, "not the open chat");
}
