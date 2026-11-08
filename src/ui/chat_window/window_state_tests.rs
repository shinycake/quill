//! Per-window state that work outside a window's own frame reaches: the
//! loop over every window, each composer's spelling, and the history's
//! middle-click autoscroll in a chat window.
#![cfg(feature = "demo-capture")]

use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, AppContext, Entity, Focusable, Modifiers, MouseButton, TestAppContext,
    VisualTestContext, point, px, size,
};
use quill::ids::ChatId;
use std::time::Duration;

/// The demo's open chat and the chat shown in its own window.
const OPEN: ChatId = ChatId(11);
const OTHER: ChatId = ChatId(12);

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, AnyWindowHandle, AnyWindowHandle) {
    cx.update(gpui_kit::init);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(700.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named("ready-chats")))
        });
        let focus = view.focus_handle(cx);
        window.focus(&focus, cx);
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let app = slot.borrow().clone().expect("the app view");
    let main: AnyWindowHandle = handle.into();
    cx.run_until_parked();
    cx.update_window(main, |_, window, cx| {
        app.update(cx, |app, cx| app.open_chat_in_new_window(OTHER, window, cx));
    })
    .expect("the main window");
    cx.run_until_parked();
    let chat_window = cx
        .windows()
        .into_iter()
        .find(|handle| handle.window_id() != main.window_id())
        .expect("the chat window opened");
    cx.update_window(chat_window, |_, window, cx| window.render_frame(cx))
        .expect("the chat window");
    cx.run_until_parked();
    (app, main, chat_window)
}

#[gpui_kit::test]
fn every_window_runs_with_its_own_state_in_place(cx: &mut TestAppContext) {
    let (app, _, _) = new_app(cx);
    let seen = app.update(cx, |app, cx| {
        app.set_window_scope(None);
        let mut seen = Vec::new();
        app.in_every_window(cx, |app, _| seen.push(app.chat_ui.chat()));
        assert_eq!(app.windows.scope, None, "the main window's state is back");
        seen
    });
    assert_eq!(seen, vec![Some(OPEN), Some(OTHER)]);
}

fn type_in(
    app: &Entity<QuillApp>,
    window: AnyWindowHandle,
    scope: Option<ChatId>,
    text: &'static str,
    cx: &mut TestAppContext,
) {
    cx.update_window(window, |_, window, cx| {
        app.update(cx, |app, cx| {
            app.in_window_scope(scope, cx, |app, cx| {
                let composer = app.composer.clone();
                composer.update(cx, |input, cx| input.set_value(text, window, cx));
                app.sync_spellcheck(text, cx);
            })
        })
    })
    .expect("the window");
}

fn underlined(app: &Entity<QuillApp>, scope: Option<ChatId>, cx: &mut TestAppContext) -> String {
    app.update(cx, |app, cx| {
        app.in_window_scope(scope, cx, |app, _| {
            let spelling = &app.composer_ui.spelling;
            if spelling.misspellings.is_empty() {
                String::new()
            } else {
                spelling.checked_text.clone()
            }
        })
    })
    .expect("the window is open")
}

/// Each composer keeps its own underlines: typing in a chat window leaves
/// the main window's, and the check's late result lands in the composer
/// it was made for.
#[gpui_kit::test]
fn each_windows_composer_keeps_its_own_spelling(cx: &mut TestAppContext) {
    let (app, main, chat_window) = new_app(cx);
    app.update(cx, |app, _| {
        app.chat_prefs.spellcheck_enabled = true;
        app.spell.checker = std::sync::Arc::new(quill::spellcheck::SpellChecker::wordlist());
    });
    type_in(&app, main, None, "qzxvb", cx);
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    assert_eq!(underlined(&app, None, cx), "qzxvb");

    type_in(&app, chat_window, Some(OTHER), "vbqzx", cx);
    app.update(cx, |app, _| app.set_window_scope(None));
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    assert_eq!(
        underlined(&app, None, cx),
        "qzxvb",
        "the main window's underlines stay"
    );
    assert_eq!(
        underlined(&app, Some(OTHER), cx),
        "vbqzx",
        "the chat window's check landed in its own composer"
    );
}

fn autoscrolling(app: &Entity<QuillApp>, cx: &mut TestAppContext) -> bool {
    app.update(cx, |app, cx| {
        app.in_window_scope(Some(OTHER), cx, |app, _| app.autoscroll_active())
    })
    .expect("the chat window is open")
}

/// A chat window draws the autoscroll anchor, and a left press ends the
/// mode there, as in the main window.
#[gpui_kit::test]
fn a_chat_window_marks_autoscroll_and_a_click_ends_it(cx: &mut TestAppContext) {
    let (app, _, chat_window) = new_app(cx);
    cx.update_window(chat_window, |_, window, cx| {
        app.update(cx, |app, cx| {
            app.in_window_scope(Some(OTHER), cx, |app, cx| {
                app.autoscroll_press(point(px(200.), px(300.)), window, cx);
            })
        })
    })
    .expect("the chat window");
    assert!(autoscrolling(&app, cx));
    let center = cx
        .update_window(chat_window, |_, window, cx| {
            window.render_frame(cx);
            assert!(
                window.try_find("autoscroll-mark").is_some(),
                "the mark shows"
            );
            let size = window.viewport_size();
            point(size.width / 2., size.height / 2.)
        })
        .expect("the chat window");

    let mut vcx = VisualTestContext::from_window(chat_window, cx);
    vcx.simulate_mouse_down(center, MouseButton::Left, Modifiers::default());
    vcx.simulate_mouse_up(center, MouseButton::Left, Modifiers::default());
    vcx.run_until_parked();
    assert!(!autoscrolling(&app, cx), "the click ended it");
}
