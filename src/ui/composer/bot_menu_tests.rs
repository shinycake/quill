//! UI tests for the bot Menu button's command list, over the
//! `ready-bot-chat` demo (a bot with /start, /help and /ping).
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AppContext, Entity, EntityInputHandler, Focusable, TestAppContext, VisualTestContext, px, size,
};

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(700.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(
                window,
                cx,
                None,
                Some(ScreenshotDemo::named("ready-bot-chat")),
            )
        });
        *slot_in.borrow_mut() = Some(view.clone());
        let focus = view.focus_handle(cx);
        window.focus(&focus, cx);
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().unwrap();
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    for _ in 0..2 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
    vcx.run_until_parked();
    (app, vcx)
}

fn set_text(app: &Entity<QuillApp>, cx: &mut VisualTestContext, text: &str) {
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.composer.update(cx, |input, cx| {
                input.set_value(text.to_string(), window, cx);
                let end = text.len();
                input.set_selected_range(end..end, cx);
            });
            app.sync_command_menu(cx);
        });
    });
    cx.run_until_parked();
}

fn press_menu(app: &Entity<QuillApp>, cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        app.update(cx, |app, cx| app.toggle_button_command_menu(window, cx));
    });
    cx.run_until_parked();
}

fn menu_commands(app: &Entity<QuillApp>, cx: &mut VisualTestContext) -> Vec<String> {
    app.update(cx, |app, cx| {
        app.command_menu_state(cx)
            .map(|(_, items)| items.iter().map(|item| item.command_text(false)).collect())
            .unwrap_or_default()
    })
}

fn composer_text(app: &Entity<QuillApp>, cx: &mut VisualTestContext) -> String {
    app.read_with(cx, |app, cx| app.composer.read(cx).value().to_string())
}

fn last_sent(app: &Entity<QuillApp>, cx: &mut VisualTestContext) -> Option<String> {
    app.read_with(cx, |app, _| {
        let session = app.session()?;
        let history = session.histories.get(&session.open_chat?.0)?;
        history
            .messages
            .values()
            .filter(|message| message.is_outgoing)
            .filter_map(|message| match &message.content {
                quill::telegram::envelope::MessageContent::Text(text) => Some(text.text.clone()),
                _ => None,
            })
            .next_back()
    })
}

#[gpui_kit::test]
fn the_menu_button_lists_every_command_and_leaves_the_field_alone(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    set_text(&app, vcx, "hello");
    assert!(menu_commands(&app, vcx).is_empty());
    press_menu(&app, vcx);
    assert_eq!(menu_commands(&app, vcx), ["/start", "/help", "/ping"]);
    assert_eq!(composer_text(&app, vcx), "hello");
    // A second press hides it.
    press_menu(&app, vcx);
    assert!(menu_commands(&app, vcx).is_empty());
    assert_eq!(composer_text(&app, vcx), "hello");
}

#[gpui_kit::test]
fn a_command_picked_from_the_menu_button_is_sent_and_the_draft_stays(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    set_text(&app, vcx, "hello");
    press_menu(&app, vcx);
    app.update(vcx, |app, cx| assert!(app.step_command_menu(1, cx)));
    vcx.update(|window, cx| {
        app.update(cx, |app, cx| {
            assert!(app.pick_command_menu_selection(window, cx))
        });
    });
    vcx.run_until_parked();
    assert_eq!(last_sent(&app, vcx).as_deref(), Some("/help"));
    assert_eq!(composer_text(&app, vcx), "hello");
    assert!(menu_commands(&app, vcx).is_empty());
}

#[gpui_kit::test]
fn tab_on_the_menu_button_list_puts_the_command_in_front(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    set_text(&app, vcx, "hello");
    press_menu(&app, vcx);
    vcx.update(|window, cx| {
        app.update(cx, |app, cx| {
            assert!(app.insert_command_menu_selection(window, cx))
        });
    });
    vcx.run_until_parked();
    assert_eq!(composer_text(&app, vcx), "/start hello");
    assert!(menu_commands(&app, vcx).is_empty());
}

#[gpui_kit::test]
fn typing_closes_the_menu_button_list(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    press_menu(&app, vcx);
    assert_eq!(menu_commands(&app, vcx).len(), 3);
    vcx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.composer.update(cx, |input, cx| {
                input.replace_text_in_range(None, "h", window, cx);
            });
        });
    });
    vcx.run_until_parked();
    assert_eq!(composer_text(&app, vcx), "h");
    assert!(menu_commands(&app, vcx).is_empty());
    // Typing `/` opens the filtered list as before.
    set_text(&app, vcx, "/he");
    assert_eq!(menu_commands(&app, vcx), ["/help"]);
}
