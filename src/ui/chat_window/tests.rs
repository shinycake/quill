//! UI integration tests: a chat opened in its own window against the demo
//! `QuillApp` (gpui-kit's test support needs the `demo-capture` feature).
#![cfg(feature = "demo-capture")]

use crate::ui::actions::CloseWindow;
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, AppContext, Entity, Focusable, Modifiers, TestAppContext, VisualTestContext,
    px, size,
};
use quill::ids::ChatId;
use quill::telegram::envelope::MessageContent;

/// The demo's open chat and another private chat from its list.
const OPEN: ChatId = ChatId(11);
const OTHER: ChatId = ChatId(12);

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, AnyWindowHandle) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(700.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named("ready-chats")))
        });
        *slot_in.borrow_mut() = Some(view.clone());
        let focus = view.focus_handle(cx);
        window.focus(&focus, cx);
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().unwrap();
    let main: AnyWindowHandle = handle.into();
    cx.update_window(main, |_, window, cx| {
        for _ in 0..2 {
            window.render_frame(cx);
        }
    })
    .unwrap();
    cx.run_until_parked();
    (app, main)
}

fn open_in_new_window(
    app: &Entity<QuillApp>,
    main: AnyWindowHandle,
    chat: ChatId,
    cx: &mut TestAppContext,
) {
    cx.update_window(main, |_, window, cx| {
        app.update(cx, |app, cx| app.open_chat_in_new_window(chat, window, cx));
    })
    .unwrap();
    // The window opens in a deferred step.
    cx.run_until_parked();
}

fn other_windows(main: AnyWindowHandle, cx: &mut TestAppContext) -> Vec<AnyWindowHandle> {
    cx.windows()
        .into_iter()
        .filter(|handle| handle.window_id() != main.window_id())
        .collect()
}

fn open_chat(app: &Entity<QuillApp>, cx: &mut TestAppContext) -> Option<ChatId> {
    app.read_with(cx, |app, _| app.session().and_then(|s| s.open_chat))
}

fn window_chats(app: &Entity<QuillApp>, cx: &mut TestAppContext) -> Vec<ChatId> {
    app.read_with(cx, |app, _| {
        app.session()
            .map(|s| s.window_chats.iter().copied().collect())
            .unwrap_or_default()
    })
}

/// The newest text in `chat`'s loaded history.
fn newest_text(app: &Entity<QuillApp>, chat: ChatId, cx: &mut TestAppContext) -> Option<String> {
    app.read_with(cx, |app, _| {
        let history = app.session()?.histories.get(&chat.0)?;
        let last = history.ordered().last().copied()?;
        match &last.content {
            MessageContent::Text(text) => Some(text.text.clone()),
            _ => None,
        }
    })
}

#[gpui_kit::test]
fn typing_in_a_chat_window_sends_to_that_chat(cx: &mut TestAppContext) {
    let (app, main) = new_app(cx);
    assert_eq!(open_chat(&app, cx), Some(OPEN), "the demo opens chat A");
    let before_other = newest_text(&app, OTHER, cx);
    let before_open = newest_text(&app, OPEN, cx);

    open_in_new_window(&app, main, OTHER, cx);
    let windows = other_windows(main, cx);
    assert_eq!(windows.len(), 1, "one chat window opened");
    let chat_window = windows[0];
    assert_eq!(window_chats(&app, cx), vec![OTHER]);
    // The main window keeps its chat.
    assert_eq!(open_chat(&app, cx), Some(OPEN));
    let view = app
        .read_with(cx, |app, _| app.windows.view_for(OTHER))
        .expect("the window's view");
    assert_eq!(view.read_with(cx, |view, _| view.chat_id()), OTHER);

    cx.update_window(chat_window, |_, window, cx| {
        window.render_frame(cx);
        // The window shows the chat's history and a compose field.
        assert!(window.try_find("chat-window-history").is_some());
        assert!(window.try_find("chat-window-composer").is_some());
        window.click("chat-window-composer", cx);
        window.input("hello from the window", cx);
    })
    .unwrap();
    assert_eq!(
        view.read_with(cx, |view, cx| view.composer.read(cx).value().to_string()),
        "hello from the window",
        "the keystrokes reached the window's compose field"
    );
    cx.update_window(chat_window, |_, window, cx| {
        window.press("enter", cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert_eq!(
        view.read_with(cx, |view, _| view.note.clone()),
        None,
        "the send was accepted"
    );
    assert_eq!(
        view.read_with(cx, |view, cx| view.composer.read(cx).value().to_string()),
        "",
        "Enter sent and cleared the field"
    );

    assert_eq!(
        newest_text(&app, OTHER, cx).as_deref(),
        Some("hello from the window"),
        "the text went to the window's chat"
    );
    assert_ne!(before_other.as_deref(), Some("hello from the window"));
    assert_eq!(
        newest_text(&app, OPEN, cx),
        before_open,
        "the main window's chat got nothing"
    );
    // The main composer stayed empty; the window's field was cleared.
    assert_eq!(
        app.read_with(cx, |app, cx| app.composer.read(cx).value().to_string()),
        ""
    );
    cx.update_window(chat_window, |_, window, cx| {
        window.render_frame(cx);
        let field = window.find("chat-window-composer");
        assert!(field.visible());
    })
    .unwrap();
    assert_eq!(
        view.read_with(cx, |view, cx| view.composer.read(cx).value().to_string()),
        ""
    );
}

#[gpui_kit::test]
fn the_active_chat_moves_to_its_window_and_the_main_window_shows_no_chat(cx: &mut TestAppContext) {
    let (app, main) = new_app(cx);
    open_in_new_window(&app, main, OPEN, cx);
    assert_eq!(other_windows(main, cx).len(), 1);
    assert_eq!(
        open_chat(&app, cx),
        None,
        "tdesktop clears the main window's section"
    );
    assert_eq!(window_chats(&app, cx), vec![OPEN]);
    // The main window's composer let go of the chat's text (it is the
    // chat's draft now) and the pane draws without a chat.
    assert_eq!(
        app.read_with(cx, |app, cx| app.composer.read(cx).value().to_string()),
        ""
    );
    cx.update_window(main, |_, window, cx| window.render_frame(cx))
        .unwrap();

    // Picking the chat in the main window's list shows its window instead.
    cx.update_window(main, |_, window, cx| {
        app.update(cx, |app, cx| app.select_listed_chat(OPEN, window, cx));
    })
    .unwrap();
    cx.run_until_parked();
    assert_eq!(open_chat(&app, cx), None);
    assert_eq!(other_windows(main, cx).len(), 1);

    // Another chat still opens in the main window as before.
    cx.update_window(main, |_, window, cx| {
        app.update(cx, |app, cx| app.select_listed_chat(OTHER, window, cx));
    })
    .unwrap();
    assert_eq!(open_chat(&app, cx), Some(OTHER));
}

#[gpui_kit::test]
fn asking_again_reuses_the_window_and_closing_it_frees_the_chat(cx: &mut TestAppContext) {
    let (app, main) = new_app(cx);
    open_in_new_window(&app, main, OTHER, cx);
    open_in_new_window(&app, main, OTHER, cx);
    let windows = other_windows(main, cx);
    assert_eq!(
        windows.len(),
        1,
        "the second request activates the first window"
    );

    // Cmd/Ctrl+W closes the chat window, not the app.
    cx.update_window(windows[0], |_, window, cx| {
        window.render_frame(cx);
        window.dispatch_action(Box::new(CloseWindow), cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert!(other_windows(main, cx).is_empty());
    assert!(window_chats(&app, cx).is_empty());
    assert!(app.read_with(cx, |app, _| !app.windows.has(OTHER)));
    // The main window is untouched.
    assert_eq!(open_chat(&app, cx), Some(OPEN));
}

#[gpui_kit::test]
fn ctrl_or_cmd_click_on_a_chat_row_opens_its_window(cx: &mut TestAppContext) {
    let (app, main) = new_app(cx);
    let center = cx
        .update_window(main, |_, window, cx| {
            window.render_frame(cx);
            window.find(("chat-row", OTHER.0 as u64)).bounds().center()
        })
        .unwrap();
    let mut vcx = VisualTestContext::from_window(main, cx);
    vcx.simulate_click(center, Modifiers::secondary_key());
    vcx.run_until_parked();
    assert_eq!(other_windows(main, cx).len(), 1);
    assert_eq!(window_chats(&app, cx), vec![OTHER]);
    assert_eq!(
        open_chat(&app, cx),
        Some(OPEN),
        "a plain select did not happen"
    );
}
