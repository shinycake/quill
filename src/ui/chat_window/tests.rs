//! UI integration tests: a chat opened in its own window against the demo
//! `QuillApp` (gpui-kit's test support needs the `demo-capture` feature).
#![cfg(feature = "demo-capture")]

use crate::ui::actions::CloseWindow;
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::component::input::TextareaState;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, AppContext, Entity, Focusable, Modifiers, MouseButton, TestAppContext,
    VisualTestContext, px, size,
};
use quill::ids::{ChatId, MessageId};
use quill::telegram::envelope::MessageContent;

/// The demo's open chat and two more chats from its list.
const OPEN: ChatId = ChatId(11);
const OTHER: ChatId = ChatId(12);
const CHANNEL: ChatId = ChatId(13);
/// The one message in chat B.
const OTHER_LAST: MessageId = MessageId(40);

struct Harness {
    app: Entity<QuillApp>,
    shell: Entity<QuillShell>,
    main: AnyWindowHandle,
}

fn new_app(cx: &mut TestAppContext) -> Harness {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(700.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named("ready-chats")))
        });
        let focus = view.focus_handle(cx);
        window.focus(&focus, cx);
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some((view, shell.clone()));
        Root::new(shell, window, cx)
    });
    let (app, shell) = slot.borrow().clone().unwrap();
    let main: AnyWindowHandle = handle.into();
    cx.update_window(main, |_, window, cx| {
        for _ in 0..2 {
            window.render_frame(cx);
        }
    })
    .unwrap();
    cx.run_until_parked();
    Harness { app, shell, main }
}

impl Harness {
    fn open_in_new_window(&self, chat: ChatId, cx: &mut TestAppContext) -> AnyWindowHandle {
        let app = self.app.clone();
        cx.update_window(self.main, |_, window, cx| {
            app.update(cx, |app, cx| app.open_chat_in_new_window(chat, window, cx));
        })
        .unwrap();
        // The window opens in a deferred step.
        cx.run_until_parked();
        let windows = self.other_windows(cx);
        assert_eq!(windows.len(), 1, "one chat window opened");
        let window = windows[0];
        cx.update_window(window, |_, window, cx| {
            for _ in 0..2 {
                window.render_frame(cx);
            }
        })
        .unwrap();
        cx.run_until_parked();
        window
    }

    fn other_windows(&self, cx: &mut TestAppContext) -> Vec<AnyWindowHandle> {
        let main = self.main.window_id();
        cx.windows()
            .into_iter()
            .filter(|handle| handle.window_id() != main)
            .collect()
    }

    /// Put the main window's state in place, as the next main-window
    /// input, draw or poll would: a chat window's state stays in place
    /// after its input until then.
    fn settle(&self, cx: &mut TestAppContext) {
        self.app.update(cx, |app, _| app.set_window_scope(None));
    }

    fn open_chat(&self, cx: &mut TestAppContext) -> Option<ChatId> {
        self.settle(cx);
        self.app
            .read_with(cx, |app, _| app.session().and_then(|s| s.open_chat))
    }

    fn window_chats(&self, cx: &mut TestAppContext) -> Vec<ChatId> {
        self.settle(cx);
        self.app.read_with(cx, |app, _| {
            app.session()
                .map(|s| s.window_chats.iter().copied().collect())
                .unwrap_or_default()
        })
    }

    /// The newest message of `chat`'s loaded history: its text and the
    /// message it replies to.
    fn newest(&self, chat: ChatId, cx: &mut TestAppContext) -> Option<(String, Option<MessageId>)> {
        self.settle(cx);
        self.app.read_with(cx, |app, _| {
            let history = app.session()?.histories.get(&chat.0)?;
            let last = history.ordered().last().copied()?;
            let text = match &last.content {
                MessageContent::Text(text) => text.text.clone(),
                _ => String::new(),
            };
            Some((text, last.reply_to.as_ref().map(|reply| reply.message_id)))
        })
    }

    /// The compose field of `chat`'s window.
    fn window_composer(&self, chat: ChatId, cx: &mut TestAppContext) -> Entity<TextareaState> {
        self.settle(cx);
        self.app
            .read_with(cx, |app, _| {
                app.windows
                    .slot_for(chat)
                    .map(|slot| slot.borrow().composer.clone())
            })
            .expect("the chat has a window")
    }

    fn main_composer_text(&self, cx: &mut TestAppContext) -> String {
        self.settle(cx);
        self.app
            .read_with(cx, |app, cx| app.composer.read(cx).value().to_string())
    }
}

#[gpui_kit::test]
fn typing_in_a_chat_window_sends_to_that_chat(cx: &mut TestAppContext) {
    let h = new_app(cx);
    assert_eq!(h.open_chat(cx), Some(OPEN), "the demo opens chat A");
    let before_open = h.newest(OPEN, cx);

    let chat_window = h.open_in_new_window(OTHER, cx);
    assert_eq!(h.window_chats(cx), vec![OTHER]);
    // The main window keeps its chat.
    assert_eq!(h.open_chat(cx), Some(OPEN));
    let view = h
        .app
        .read_with(cx, |app, _| app.windows.view_for(OTHER))
        .expect("the window's view");
    assert_eq!(view.read_with(cx, |view, _| view.chat_id()), OTHER);
    let composer = h.window_composer(OTHER, cx);

    cx.update_window(chat_window, |_, window, cx| {
        // The window shows the chat's history and the compose field.
        assert!(window.try_find("chat-window-history").is_some());
        assert!(window.try_find("composer-send").is_some());
        window.click(("input", composer.entity_id()), cx);
        window.input("hello from the window", cx);
    })
    .unwrap();
    assert_eq!(
        composer.read_with(cx, |field, _| field.value().to_string()),
        "hello from the window",
        "the keystrokes reached the window's compose field"
    );
    cx.update_window(chat_window, |_, window, cx| window.press("enter", cx))
        .unwrap();
    cx.run_until_parked();

    assert_eq!(
        h.newest(OTHER, cx).map(|(text, _)| text).as_deref(),
        Some("hello from the window"),
        "the text went to the window's chat"
    );
    assert_eq!(h.newest(OPEN, cx), before_open, "chat A got nothing");
    assert_eq!(
        h.main_composer_text(cx),
        "",
        "the main composer is untouched"
    );
    assert_eq!(
        composer.read_with(cx, |field, _| field.value().to_string()),
        "",
        "Enter sent and cleared the window's field"
    );
}

#[gpui_kit::test]
fn a_reply_started_in_a_chat_window_stays_in_that_window(cx: &mut TestAppContext) {
    let h = new_app(cx);
    let chat_window = h.open_in_new_window(OTHER, cx);
    // Right-click the window's one message (its bubble sits at the left
    // of the row, level with the row's reaction button): the menu belongs
    // to the window.
    let point = cx
        .update_window(chat_window, |_, window, cx| {
            window.render_frame(cx);
            let react = window
                .within(("row", OTHER_LAST.0 as u64))
                .find(format!("message-react-{}", OTHER_LAST.0))
                .bounds();
            gpui_kit::point(px(70.), react.center().y)
        })
        .unwrap();
    let mut vcx = VisualTestContext::from_window(chat_window, cx);
    vcx.simulate_mouse_down(point, MouseButton::Right, Modifiers::default());
    vcx.simulate_mouse_up(point, MouseButton::Right, Modifiers::default());
    vcx.run_until_parked();
    h.settle(cx);
    let (window_menu, main_menu) = h.app.read_with(cx, |app, _| {
        let slot = app.windows.slot_for(OTHER).expect("slot");
        let window_menu = slot.borrow().message_ui.menu;
        (window_menu, app.message_ui.menu)
    });
    assert_eq!(
        window_menu.map(|menu| (menu.chat_id, menu.message_id)),
        Some((OTHER, OTHER_LAST)),
        "the menu opened for the window's message"
    );
    assert!(main_menu.is_none(), "the main window shows no menu");

    // "Reply" from that menu arms the window's compose field, not the
    // main window's.
    cx.update_window(chat_window, |_, window, cx| {
        h.app.update(cx, |app, cx| {
            app.in_window_scope(Some(OTHER), cx, |app, cx| {
                app.begin_reply_from_message(OTHER, OTHER_LAST, window, cx);
            });
        });
    })
    .unwrap();
    h.settle(cx);
    let (window_reply, main_reply) = h.app.read_with(cx, |app, _| {
        let slot = app.windows.slot_for(OTHER).expect("slot");
        let reply = slot.borrow().composer_ui.pending_reply.clone();
        (reply, app.composer_ui.pending_reply.clone())
    });
    assert_eq!(window_reply.map(|reply| reply.message_id), Some(OTHER_LAST));
    assert!(main_reply.is_none());

    // The sent message carries the reply.
    let composer = h.window_composer(OTHER, cx);
    cx.update_window(chat_window, |_, window, cx| {
        window.render_frame(cx);
        window.click(("input", composer.entity_id()), cx);
        window.input("replying", cx);
        window.press("enter", cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert_eq!(
        h.newest(OTHER, cx),
        Some(("replying".to_string(), Some(OTHER_LAST)))
    );
}

#[gpui_kit::test]
fn the_emoji_panel_opens_in_the_window_that_asked(cx: &mut TestAppContext) {
    let h = new_app(cx);
    let chat_window = h.open_in_new_window(OTHER, cx);
    cx.update_window(chat_window, |_, window, cx| {
        window.render_frame(cx);
        window.click("composer-emoji", cx);
    })
    .unwrap();
    cx.run_until_parked();
    h.settle(cx);
    let (window_panel, main_panel) = h.app.read_with(cx, |app, _| {
        let slot = app.windows.slot_for(OTHER).expect("slot");
        let open = slot.borrow().pickers.media_panel.open;
        (open, app.pickers.media_panel.open)
    });
    assert!(window_panel, "the window's picker opened");
    assert!(!main_panel, "the main window's picker stayed closed");
}

#[gpui_kit::test]
fn a_dialog_asked_for_in_a_chat_window_opens_there(cx: &mut TestAppContext) {
    let h = new_app(cx);
    let chat_window = h.open_in_new_window(OTHER, cx);
    let view = h
        .app
        .read_with(cx, |app, _| app.windows.view_for(OTHER))
        .expect("view");
    // Input in the chat window, then a flag that opens a kit dialog
    // (Settings): the window the input happened in hosts it.
    h.app.update(cx, |app, _| {
        app.note_window_input(Some(OTHER));
        app.settings.open = true;
        app.set_window_scope(None);
    });
    cx.update_window(h.main, |_, window, cx| window.render_frame(cx))
        .unwrap();
    cx.update_window(chat_window, |_, window, cx| window.render_frame(cx))
        .unwrap();
    cx.run_until_parked();
    let window_shell = view.read_with(cx, |view, _| view.shell.clone());
    assert!(window_shell.read_with(cx, |shell, _| shell.dialog_open()));
    assert!(
        !h.shell.read_with(cx, |shell, _| shell.dialog_open()),
        "the main window did not open it too"
    );
    // While the window shows it, the main window keeps out of the way.
    cx.update_window(h.main, |_, window, cx| window.render_frame(cx))
        .unwrap();
    assert!(!h.shell.read_with(cx, |shell, _| shell.dialog_open()));

    // The flag cleared: the window's dialog goes, and the next one asked
    // for from the main window opens in the main window.
    h.app.update(cx, |app, _| {
        app.settings.open = false;
    });
    cx.update_window(chat_window, |_, window, cx| window.render_frame(cx))
        .unwrap();
    assert!(!window_shell.read_with(cx, |shell, _| shell.dialog_open()));
    h.app.update(cx, |app, _| {
        app.note_window_input(None);
        app.settings.open = true;
    });
    cx.update_window(chat_window, |_, window, cx| window.render_frame(cx))
        .unwrap();
    cx.update_window(h.main, |_, window, cx| window.render_frame(cx))
        .unwrap();
    assert!(!window_shell.read_with(cx, |shell, _| shell.dialog_open()));
    assert!(h.shell.read_with(cx, |shell, _| shell.dialog_open()));
}

#[gpui_kit::test]
fn a_chat_picked_inside_a_chat_window_opens_in_the_main_window(cx: &mut TestAppContext) {
    let h = new_app(cx);
    let chat_window = h.open_in_new_window(OTHER, cx);
    // As a link or search hit in the window would: the pick happens with
    // the window's state in place.
    cx.update_window(chat_window, |_, window, cx| {
        h.app.update(cx, |app, cx| {
            app.in_window_scope(Some(OTHER), cx, |app, cx| {
                app.select_listed_chat(CHANNEL, window, cx);
            });
        });
    })
    .unwrap();
    cx.run_until_parked();
    assert_eq!(h.open_chat(cx), Some(CHANNEL), "the main window switched");
    assert_eq!(h.window_chats(cx), vec![OTHER], "the window kept its chat");
    h.settle(cx);
    let window_view = h.app.read_with(cx, |app, _| {
        app.windows
            .slot_for(OTHER)
            .map(|slot| slot.borrow().session.open_chat)
    });
    assert_eq!(window_view, Some(Some(OTHER)));
}

#[gpui_kit::test]
fn the_active_chat_moves_to_its_window_and_the_main_window_shows_no_chat(cx: &mut TestAppContext) {
    let h = new_app(cx);
    h.open_in_new_window(OPEN, cx);
    assert_eq!(
        h.open_chat(cx),
        None,
        "tdesktop clears the main window's section"
    );
    assert_eq!(h.window_chats(cx), vec![OPEN]);
    // The main window's composer let go of the chat's text (it is the
    // chat's draft now) and the pane draws without a chat.
    assert_eq!(h.main_composer_text(cx), "");

    // Picking the chat in the main window's list shows its window instead.
    cx.update_window(h.main, |_, window, cx| {
        h.app
            .update(cx, |app, cx| app.select_listed_chat(OPEN, window, cx));
    })
    .unwrap();
    cx.run_until_parked();
    assert_eq!(h.open_chat(cx), None);
    assert_eq!(h.other_windows(cx).len(), 1);

    // Another chat still opens in the main window as before.
    cx.update_window(h.main, |_, window, cx| {
        h.app
            .update(cx, |app, cx| app.select_listed_chat(OTHER, window, cx));
    })
    .unwrap();
    assert_eq!(h.open_chat(cx), Some(OTHER));
}

#[gpui_kit::test]
fn asking_again_reuses_the_window_and_closing_it_frees_the_chat(cx: &mut TestAppContext) {
    let h = new_app(cx);
    let chat_window = h.open_in_new_window(OTHER, cx);
    cx.update_window(h.main, |_, window, cx| {
        h.app
            .update(cx, |app, cx| app.open_chat_in_new_window(OTHER, window, cx));
    })
    .unwrap();
    cx.run_until_parked();
    assert_eq!(
        h.other_windows(cx).len(),
        1,
        "the second request activates the first window"
    );

    // Cmd/Ctrl+W closes the chat window, not the app; its state leaves
    // the app's place with it.
    cx.update_window(chat_window, |_, window, cx| {
        window.render_frame(cx);
        window.dispatch_action(Box::new(CloseWindow), cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert!(h.other_windows(cx).is_empty());
    assert!(h.window_chats(cx).is_empty());
    assert!(h.app.read_with(cx, |app, _| !app.windows.has(OTHER)));
    assert_eq!(h.open_chat(cx), Some(OPEN));
}

#[gpui_kit::test]
fn ctrl_or_cmd_click_on_a_chat_row_opens_its_window(cx: &mut TestAppContext) {
    let h = new_app(cx);
    let center = cx
        .update_window(h.main, |_, window, cx| {
            window.render_frame(cx);
            window.find(("chat-row", OTHER.0 as u64)).bounds().center()
        })
        .unwrap();
    let mut vcx = VisualTestContext::from_window(h.main, cx);
    vcx.simulate_click(center, Modifiers::secondary_key());
    vcx.run_until_parked();
    assert_eq!(h.other_windows(cx).len(), 1);
    assert_eq!(h.window_chats(cx), vec![OTHER]);
    assert_eq!(h.open_chat(cx), Some(OPEN), "a plain select did not happen");
}
