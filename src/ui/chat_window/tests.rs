//! UI integration tests: a chat opened in its own window against the demo
//! `QuillApp` (gpui-kit's test support needs the `demo-capture` feature).
#![cfg(feature = "demo-capture")]

use crate::ui::actions::{CloseWindow, ReopenClosedWindow};
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::recording_state::{CaptureFor, CaptureMedia};
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
        let reply = slot.borrow().composer_ui.reply().cloned();
        (reply, app.composer_ui.reply().cloned())
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
fn a_chat_windows_reply_to_a_deleted_option_sends_a_plain_reply(cx: &mut TestAppContext) {
    use quill::composer::{ComposerReplyTo, ReplyItem};
    let h = new_app(cx);
    let _chat_window = h.open_in_new_window(OTHER, cx);
    // The window's message has no poll options, so a reply to option "gone"
    // points at an item that no longer exists.
    let (sent, draft) = h.app.update(cx, |app, cx| {
        app.in_window_scope(Some(OTHER), cx, |app, cx| {
            app.composer_ui.set_reply(Some(
                ComposerReplyTo::new(OTHER, OTHER_LAST)
                    .to_item(ReplyItem::PollOption("gone".into())),
            ));
            let sent = app.send_reply_for(OTHER, cx).map(|reply| reply.item);
            let draft = app
                .draft_reply(app.composer_ui.reply(), OTHER)
                .map(|reply| reply.item);
            (sent, draft)
        })
        .expect("the window's scope")
    });
    assert_eq!(sent, Some(ReplyItem::Message));
    assert_eq!(draft, Some(ReplyItem::Message));
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
    // (Accounts): the window the input happened in hosts it.
    h.app.update(cx, |app, _| {
        app.note_window_input(Some(OTHER));
        app.account.accounts.open = true;
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
        app.account.accounts.open = false;
    });
    cx.update_window(chat_window, |_, window, cx| window.render_frame(cx))
        .unwrap();
    assert!(!window_shell.read_with(cx, |shell, _| shell.dialog_open()));
    h.app.update(cx, |app, _| {
        app.note_window_input(None);
        app.account.accounts.open = true;
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
    let window_view = h.app.update(cx, |app, cx| {
        app.in_window_scope(Some(OTHER), cx, |app, _| {
            app.session().and_then(|session| session.open_chat)
        })
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
    vcx.simulate_click(center, crate::ui::host_os::secondary_key());
    vcx.run_until_parked();
    assert_eq!(h.other_windows(cx).len(), 1);
    assert_eq!(h.window_chats(cx), vec![OTHER]);
    assert_eq!(h.open_chat(cx), Some(OPEN), "a plain select did not happen");
}

#[gpui_kit::test]
fn a_chat_window_searches_and_forwards_with_fields_of_its_own(cx: &mut TestAppContext) {
    let h = new_app(cx);
    let chat_window = h.open_in_new_window(OTHER, cx);
    h.settle(cx);
    let (main_search, main_forward) = h.app.read_with(cx, |app, _| {
        (
            app.search_ui.chat_input.entity_id(),
            app.share.search_input.entity_id(),
        )
    });
    // With the window's state in place the fields are the window's own.
    let (window_search, window_forward) = h
        .app
        .update(cx, |app, cx| {
            app.in_window_scope(Some(OTHER), cx, |app, _| {
                (
                    app.search_ui.chat_input.entity_id(),
                    app.share.search_input.entity_id(),
                )
            })
        })
        .expect("the window is open");
    assert_ne!(main_search, window_search);
    assert_ne!(main_forward, window_forward);

    // Searching in the window's chat types into the window's field; the
    // main window's search stays closed and empty.
    cx.update_window(chat_window, |_, window, cx| {
        window.render_frame(cx);
        window.click("chat-find", cx);
        window.input("later", cx);
    })
    .unwrap();
    cx.run_until_parked();
    h.settle(cx);
    let (window_text, main_text, main_open) = h.app.read_with(cx, |app, cx| {
        let slot = app.windows.slot_for(OTHER).expect("slot");
        let slot = slot.borrow();
        (
            slot.chat_search_input.read(cx).value().to_string(),
            app.search_ui.chat_input.read(cx).value().to_string(),
            app.session().is_some_and(|s| s.search.chat_search.open),
        )
    });
    let window_open = h.app.update(cx, |app, cx| {
        app.in_window_scope(Some(OTHER), cx, |app, _| {
            app.session().is_some_and(|s| s.search.chat_search.open)
        })
    });
    assert_eq!(window_text, "later");
    assert_eq!(window_open, Some(true), "the window's chat search opened");
    assert_eq!(main_text, "");
    assert!(!main_open, "the main window's search stayed closed");
}

#[gpui_kit::test]
fn the_passcode_box_covers_a_chat_window_too(cx: &mut TestAppContext) {
    let h = new_app(cx);
    let chat_window = h.open_in_new_window(OTHER, cx);
    h.app
        .update(cx, |app, _| app.account.passcode.locked = true);
    cx.update_window(chat_window, |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window.try_find("lock-submit").is_some(),
            "the window shows the passcode box"
        );
        assert!(
            window.try_find("composer-send").is_none(),
            "nothing of the chat shows while locked"
        );
    })
    .unwrap();
    h.app
        .update(cx, |app, _| app.account.passcode.locked = false);
    cx.update_window(chat_window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("lock-submit").is_none());
        assert!(window.try_find("composer-send").is_some());
    })
    .unwrap();
}

#[gpui_kit::test]
fn switching_accounts_closes_the_chat_windows(cx: &mut TestAppContext) {
    let h = new_app(cx);
    h.open_in_new_window(OTHER, cx);
    assert_eq!(h.window_chats(cx), vec![OTHER]);
    h.app
        .update(cx, |app, cx| app.close_chat_windows_for_switch(cx));
    cx.run_until_parked();
    assert!(h.other_windows(cx).is_empty(), "the window is gone");
    assert!(h.window_chats(cx).is_empty(), "its chat is closed");
    assert!(h.app.read_with(cx, |app, _| !app.windows.has(OTHER)));
    // The main window keeps its chat.
    assert_eq!(h.open_chat(cx), Some(OPEN));
}

#[gpui_kit::test]
fn reopen_closed_window_brings_back_the_window_closed_last(cx: &mut TestAppContext) {
    let h = new_app(cx);
    // Nothing was closed yet: the command does not apply.
    let reopened = cx
        .update_window(h.main, |_, window, cx| {
            h.app
                .update(cx, |app, cx| app.reopen_closed_window_by_key(window, cx))
        })
        .unwrap();
    assert!(!reopened);

    let chat_window = h.open_in_new_window(OTHER, cx);
    cx.update_window(chat_window, |_, window, cx| {
        window.render_frame(cx);
        window.dispatch_action(Box::new(CloseWindow), cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert!(h.other_windows(cx).is_empty());

    // The chord's action opens it again, once.
    cx.update_window(h.main, |_, window, cx| {
        window.dispatch_action(Box::new(ReopenClosedWindow), cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert_eq!(h.other_windows(cx).len(), 1);
    assert_eq!(h.window_chats(cx), vec![OTHER]);
    let again = cx
        .update_window(h.main, |_, window, cx| {
            h.app
                .update(cx, |app, cx| app.reopen_closed_window_by_key(window, cx))
        })
        .unwrap();
    assert!(!again, "the list of closed windows is used up");
}

/// The media viewer's "saved" toast of `chat`'s window (`None`: the main
/// window's) is showing.
fn toast_shown(h: &Harness, chat: Option<ChatId>, cx: &mut TestAppContext) -> bool {
    h.settle(cx);
    h.app.read_with(cx, |app, _| match chat {
        Some(chat) => app
            .windows
            .slot_for(chat)
            .is_some_and(|slot| slot.borrow().viewer.extra.saved_toast.is_some()),
        None => app.viewer.extra.saved_toast.is_some(),
    })
}

/// Show the "saved" toast in `chat`'s window (`None`: the main window),
/// as a Save there would; its timer hides it later.
fn show_saved_toast(h: &Harness, chat: Option<ChatId>, cx: &mut TestAppContext) {
    let shown = h.app.update(cx, |app, cx| {
        app.in_window_scope(chat, cx, |app, cx| {
            app.show_saved_toast(std::path::PathBuf::from("/tmp/clip.mp4"), true, cx);
        })
    });
    assert_eq!(shown, Some(()), "the window is open");
    // The input that asked is over: the main window's state goes back in
    // place, as its next draw or the poll loop puts it.
    h.settle(cx);
}

const TOAST: std::time::Duration =
    std::time::Duration::from_millis(quill::viewer_extras::SAVED_TOAST_MS);
const A_MOMENT: std::time::Duration = std::time::Duration::from_millis(50);

#[gpui_kit::test]
fn a_timer_started_in_a_chat_window_finishes_in_that_window(cx: &mut TestAppContext) {
    let h = new_app(cx);
    h.open_in_new_window(OTHER, cx);
    // The window's toast first, the main window's half a lifetime later:
    // each is its viewer's first, so their counters match.
    show_saved_toast(&h, Some(OTHER), cx);
    cx.executor().advance_clock(TOAST / 2);
    show_saved_toast(&h, None, cx);
    assert!(toast_shown(&h, Some(OTHER), cx));
    assert!(toast_shown(&h, None, cx));

    // The window's timer ends with the main window's state in place: it
    // hides the window's toast, not the main window's younger one.
    cx.executor().advance_clock(TOAST / 2 + A_MOMENT);
    cx.run_until_parked();
    assert!(!toast_shown(&h, Some(OTHER), cx), "the window's toast went");
    assert!(toast_shown(&h, None, cx), "the main window's toast stays");

    cx.executor().advance_clock(TOAST / 2);
    cx.run_until_parked();
    assert!(!toast_shown(&h, None, cx), "then the main window's goes");
}

#[gpui_kit::test]
fn work_from_a_closed_chat_window_lands_nowhere(cx: &mut TestAppContext) {
    let h = new_app(cx);
    let chat_window = h.open_in_new_window(OTHER, cx);
    show_saved_toast(&h, Some(OTHER), cx);
    cx.executor().advance_clock(TOAST / 2);
    show_saved_toast(&h, None, cx);
    cx.update_window(chat_window, |_, window, cx| {
        window.render_frame(cx);
        window.dispatch_action(Box::new(CloseWindow), cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert!(h.other_windows(cx).is_empty());

    let ran = h.app.update(cx, |app, cx| {
        app.in_window_scope(Some(OTHER), cx, |_, _| ())
    });
    assert_eq!(ran, None, "a closed window's scope runs nothing");
    // The closed window's timer ends: the main window's toast, with the
    // same counter, is not the one it hides.
    cx.executor().advance_clock(TOAST / 2 + A_MOMENT);
    cx.run_until_parked();
    assert!(toast_shown(&h, None, cx), "the main window's toast stays");
}

/// How many voice messages the main window's chat and `OTHER` hold.
fn voice_notes(h: &Harness, cx: &mut TestAppContext) -> (usize, usize) {
    h.settle(cx);
    h.app.read_with(cx, |app, _| {
        let count = |chat: ChatId| {
            app.session()
                .and_then(|session| session.histories.get(&chat.0))
                .map_or(0, |history| {
                    history
                        .ordered()
                        .into_iter()
                        .filter(|message| matches!(message.content, MessageContent::VoiceNote(_)))
                        .count()
                })
        };
        (count(OPEN), count(OTHER))
    })
}

/// A recording in `chat`'s window (`None`: the main window; a fixture
/// voice capture, no microphone), started as `begin_voice_capture` or, for
/// `CaptureFor::StoryReply`, `begin_story_voice_capture` start one there.
fn record_in_window(
    h: &Harness,
    chat: Option<ChatId>,
    purpose: CaptureFor,
    cx: &mut TestAppContext,
) {
    h.app.update(cx, |app, cx| {
        app.in_window_scope(chat, cx, |app, _| {
            let path = std::env::temp_dir().join("quill-window-recording-test.ogg");
            let capture = quill::voice::VoiceCapture::preview(path, 3, vec![4, 8, 12]);
            app.hold_capture(CaptureMedia::Voice(capture), purpose);
        })
        .expect("the chat has a window");
    });
}

/// The recording reaches its time limit (`check_recording` saw the video
/// capture end), with the main window's state in place.
fn reach_time_limit(h: &Harness, cx: &mut TestAppContext) {
    h.app.update(cx, |app, cx| {
        app.set_window_scope(None);
        app.send_recording_at_time_limit(cx);
    });
}

#[gpui_kit::test]
fn a_recording_from_a_chat_window_is_never_sent_to_the_main_windows_chat(cx: &mut TestAppContext) {
    let h = new_app(cx);
    h.open_in_new_window(OTHER, cx);
    let (main, other) = voice_notes(&h, cx);
    record_in_window(&h, Some(OTHER), CaptureFor::Message, cx);
    // Its record bar is the window's, not the main window's.
    let here = h.app.update(cx, |app, cx| {
        let main = app.in_window_scope(None, cx, |app, _| app.recording_here());
        let window = app.in_window_scope(Some(OTHER), cx, |app, _| app.recording_here());
        (main, window)
    });
    assert_eq!(here, (Some(false), Some(true)));

    // The time limit is reached with the main window's state in place, and
    // the window it was recorded in does not draw (covered or minimized):
    // it goes to that window's chat all the same.
    reach_time_limit(&h, cx);
    cx.run_until_parked();
    assert_eq!(
        voice_notes(&h, cx),
        (main, other + 1),
        "the window's chat got it, the main window's did not"
    );
    assert!(h.app.read_with(cx, |app, _| !app.recording_active()));
}

#[gpui_kit::test]
fn a_main_window_recording_at_its_time_limit_is_sent_to_the_main_chat(cx: &mut TestAppContext) {
    let h = new_app(cx);
    h.open_in_new_window(OTHER, cx);
    let (main, other) = voice_notes(&h, cx);
    record_in_window(&h, None, CaptureFor::Message, cx);
    // Reached while the chat window's state is in place.
    h.app.update(cx, |app, cx| {
        app.in_window_scope(Some(OTHER), cx, |app, cx| {
            app.send_recording_at_time_limit(cx)
        })
        .expect("the chat has a window");
    });
    cx.run_until_parked();
    assert_eq!(voice_notes(&h, cx), (main + 1, other));
    assert!(h.app.read_with(cx, |app, _| !app.recording_active()));
}

#[gpui_kit::test]
fn the_send_button_sends_to_the_recording_window_whichever_window_asks(cx: &mut TestAppContext) {
    let h = new_app(cx);
    h.open_in_new_window(OTHER, cx);
    let (main, other) = voice_notes(&h, cx);
    record_in_window(&h, Some(OTHER), CaptureFor::Message, cx);
    // Asked with the main window's state in place (a shortcut, the Touch
    // Bar): it still goes to the window's chat.
    cx.update_window(h.main, |_, window, cx| {
        h.app.update(cx, |app, cx| {
            app.set_window_scope(None);
            app.send_recording(window, cx);
        });
    })
    .unwrap();
    cx.run_until_parked();
    assert_eq!(voice_notes(&h, cx), (main, other + 1));

    // One recorded in the main window still goes to its chat.
    record_in_window(&h, None, CaptureFor::Message, cx);
    cx.update_window(h.main, |_, window, cx| {
        h.app.update(cx, |app, cx| app.send_recording(window, cx));
    })
    .unwrap();
    cx.run_until_parked();
    assert_eq!(voice_notes(&h, cx), (main + 1, other + 1));
}

#[gpui_kit::test]
fn closing_a_chat_window_ends_the_recording_made_there(cx: &mut TestAppContext) {
    let h = new_app(cx);
    let chat_window = h.open_in_new_window(OTHER, cx);
    let (main, _) = voice_notes(&h, cx);
    record_in_window(&h, Some(OTHER), CaptureFor::Message, cx);
    cx.update_window(chat_window, |_, window, cx| {
        window.render_frame(cx);
        window.dispatch_action(Box::new(CloseWindow), cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert!(h.other_windows(cx).is_empty());
    let (active, note) = h.app.read_with(cx, |app, _| {
        (
            app.recording_active(),
            app.connection.status_note().to_string(),
        )
    });
    assert!(!active, "the recording ended with its window");
    assert_eq!(
        note,
        quill::lang::tr(quill::lang::keys::Q_MISC_RECORDING_DISCARDED_ITS_WINDOW_CLOSED)
    );
    // A time limit reported late sends nothing.
    reach_time_limit(&h, cx);
    cx.update_window(h.main, |_, window, cx| window.render_frame(cx))
        .unwrap();
    cx.run_until_parked();
    assert_eq!(
        voice_notes(&h, cx).0,
        main,
        "nothing went to the main window's chat"
    );
}

#[gpui_kit::test]
fn a_microphone_that_comes_up_after_its_window_closed_is_dropped(cx: &mut TestAppContext) {
    let h = new_app(cx);
    let chat_window = h.open_in_new_window(OTHER, cx);
    h.app.update(cx, |app, cx| {
        app.recording.open_voice = std::sync::Arc::new(|| {
            let path = std::env::temp_dir().join("quill-window-recording-test.ogg");
            Ok(quill::voice::VoiceCapture::preview(path, 3, vec![4]))
        });
        app.in_window_scope(Some(OTHER), cx, |app, cx| {
            app.start_voice_capture(CaptureFor::Message, cx);
        })
        .expect("the chat has a window");
    });
    cx.update_window(chat_window, |_, window, cx| {
        window.render_frame(cx);
        window.dispatch_action(Box::new(CloseWindow), cx);
    })
    .unwrap();
    cx.run_until_parked();
    let (active, busy) = h
        .app
        .read_with(cx, |app, _| (app.recording_active(), app.recording_busy()));
    assert!(!active, "no recording outlives its window");
    assert!(!busy, "and the next one may start");
}

#[gpui_kit::test]
fn a_sent_recording_leaves_no_window_behind_for_the_next_one(cx: &mut TestAppContext) {
    let h = new_app(cx);
    let chat_window = h.open_in_new_window(OTHER, cx);
    let (_, other) = voice_notes(&h, cx);
    record_in_window(&h, Some(OTHER), CaptureFor::Message, cx);
    cx.update_window(chat_window, |_, window, cx| {
        h.app.update(cx, |app, cx| app.send_recording(window, cx));
    })
    .unwrap();
    cx.run_until_parked();
    assert_eq!(voice_notes(&h, cx).1, other + 1);

    // A story reply, recorded from the main window's story viewer.
    record_in_window(&h, None, CaptureFor::StoryReply, cx);
    let note_before = h
        .app
        .read_with(cx, |app, _| app.connection.status_note().to_string());
    let in_other = h.app.update(cx, |app, cx| {
        app.in_window_scope(Some(OTHER), cx, |app, _| app.recording_here())
    });
    assert_eq!(in_other, Some(false), "its record bar is not in OTHER");

    // OTHER closes: the main window's recording carries on.
    cx.update_window(chat_window, |_, window, cx| {
        window.dispatch_action(Box::new(CloseWindow), cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert!(h.other_windows(cx).is_empty());
    let (active, here, note) = h.app.read_with(cx, |app, _| {
        (
            app.recording_active(),
            app.recording_here(),
            app.connection.status_note().to_string(),
        )
    });
    assert!(active, "the story reply still records");
    assert!(here, "and belongs to the main window");
    assert_eq!(note, note_before, "no \"discarded\" note");
}

#[gpui_kit::test]
fn a_chat_window_shows_the_placeholder_of_its_own_chat(cx: &mut TestAppContext) {
    let h = new_app(cx);
    // The user of chat B charges 25 Stars per message.
    h.app.update(cx, |app, _| {
        if let Some(session) = app.demo_session.as_mut() {
            session.payments.paid_messages.record_answer(
                OTHER.0,
                quill::paid_messages::CanSend::HasPaidMessages { stars: 25 },
                1,
            );
        }
    });
    let chat_window = h.open_in_new_window(OTHER, cx);
    cx.update_window(chat_window, |_, window, cx| window.render_frame(cx))
        .unwrap();
    let composer = h.window_composer(OTHER, cx);
    let window_text = composer.read_with(cx, |input, _| {
        input.presentation().placeholder().to_string()
    });
    assert_eq!(window_text, "Message for 25 Stars");
    // The main window shows chat A, which is free.
    let main_text = h.app.read_with(cx, |app, cx| {
        app.composer
            .read(cx)
            .presentation()
            .placeholder()
            .to_string()
    });
    assert_eq!(main_text, "Write a message…");
}

#[gpui_kit::test]
fn a_pending_chrome_notify_clears_when_the_window_lost_its_view(cx: &mut TestAppContext) {
    let h = new_app(cx);
    let _chat_window = h.open_in_new_window(OTHER, cx);
    let slot = h
        .app
        .read_with(cx, |app, _| app.windows.slot_for(OTHER))
        .expect("slot");
    slot.borrow().history.chrome_notify_pending.set(true);
    // The window is live, but its session view is gone.
    h.app.update(cx, |app, _| {
        if let Some(session) = app.demo_session.as_mut() {
            session.close_window_chat(OTHER);
        }
    });
    h.app.update(cx, |app, cx| {
        assert!(app.in_window_scope(Some(OTHER), cx, |_, _| ()).is_none());
        app.finish_chrome_notify(Some(OTHER), cx);
    });
    assert!(
        !slot.borrow().history.chrome_notify_pending.get(),
        "the flag cannot stick"
    );
}

/// A chat window's pictures go through a bounded cache of its own, the
/// window paints from it, and one the window no longer shows leaves at the
/// idle trim. They went to GPUI's asset cache, which keeps every picture
/// for good.
#[gpui_kit::test]
fn a_chat_windows_pictures_go_through_a_cache_of_its_own(cx: &mut TestAppContext) {
    let h = new_app(cx);
    let chat_window = h.open_in_new_window(OTHER, cx);
    let receive = |json: &str, cx: &mut TestAppContext| {
        h.settle(cx);
        h.app.update(cx, |app, _| {
            let sink: std::sync::Arc<dyn quill::diagnostics::DiagnosticSink> =
                app.demo_ui.sink.clone();
            let session = app.demo_session.as_mut().expect("the demo session");
            // `Session::apply` drops an envelope not newer than the last one.
            let seq = std::sync::atomic::AtomicU64::new(session.last_seq);
            let parsed = quill::telegram::client::copy_and_parse(json, &seq, &sink);
            session.apply(parsed.expect("the update parses"));
        });
        // Decodes land between frames; the frame after paints them.
        for _ in 0..3 {
            cx.update_window(chat_window, |_, window, cx| {
                window.refresh();
                window.render_frame(cx);
            })
            .unwrap();
            cx.run_until_parked();
        }
    };
    // A downloaded photo arrives in the window's chat.
    let file = crate::ui::demo::demo_file_json(91, &crate::ui::demo::demo_thumb_png_path(), true);
    receive(
        &format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":41,"chat_id":{chat},"is_outgoing":false,"date":1790640000,"content":{{"@type":"messagePhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"m","photo":{file},"width":240,"height":160,"progressive_sizes":[]}}]}},"caption":{{"@type":"formattedText","text":"","entities":[]}},"has_spoiler":false,"is_secret":false}}}}}}"#,
            chat = OTHER.0
        ),
        cx,
    );
    let images = h
        .app
        .read_with(cx, |app, cx| {
            app.windows.image_cache_for(chat_window.window_id(), cx)
        })
        .expect("the window's cache");
    let loaded = images.read_with(cx, |images, _| images.loaded_images());
    assert_eq!(loaded.len(), 1, "the photo is in the window's cache");
    let photo = loaded[0].clone();
    let in_atlas = |cx: &mut TestAppContext| {
        cx.update_window(chat_window, |_, window, _| {
            window.has_image_atlas_entry(&photo)
        })
        .unwrap()
    };
    assert!(in_atlas(cx), "painted from it");

    // The message goes; the window draws on without the photo.
    receive(
        &format!(
            r#"{{"@type":"updateDeleteMessages","chat_id":{chat},"message_ids":[41],"is_permanent":true,"from_cache":false}}"#,
            chat = OTHER.0
        ),
        cx,
    );
    images.update(cx, |images, cx| images.idle_trim(true, cx));
    let left = images.read_with(cx, |images, _| images.loaded_images());
    assert!(left.is_empty(), "no longer shown: gone");
    assert!(!in_atlas(cx), "and out of the atlas");
}

#[gpui_kit::test]
fn a_photo_edit_whose_chat_window_closed_still_lands_in_its_tray(cx: &mut TestAppContext) {
    use crate::ui::photo_editor::{EditorPurpose, PhotoEditor};
    use quill::composer::{AttachmentExtras, AttachmentKind, ComposerAttachment};
    let h = new_app(cx);
    let chat_window = h.open_in_new_window(OTHER, cx);
    let dir = quill::scratch::process_dir("quill-chat-window-edit");
    std::fs::create_dir_all(&dir).unwrap();
    let original = dir.join("closing.png");
    image::RgbaImage::from_pixel(53, 13, image::Rgba([1, 2, 3, 255]))
        .save(&original)
        .unwrap();
    let app = h.app.clone();
    let path = original.clone();
    cx.update_window(chat_window, |_, window, cx| {
        app.update(cx, |app, cx| {
            app.in_window_of(window, cx, |app, window, cx| {
                app.update_pending_attachments(|tray| {
                    tray.push(ComposerAttachment {
                        file_name: "closing.png".into(),
                        path: path.clone(),
                        kind: AttachmentKind::Photo,
                        spoiler: false,
                        extras: AttachmentExtras::default(),
                    })
                })
                .unwrap();
                let item = app.tray_item(0).unwrap();
                let pixels = image::open(&path).unwrap().to_rgba8();
                app.viewer.photo_editor =
                    Some(PhotoEditor::new(EditorPurpose::Attachment(item), pixels));
                app.photo_editor_mut(PhotoEditor::rotate, cx);
                app.save_photo_editor(window, cx);
            });
        });
        window.dispatch_action(Box::new(CloseWindow), cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert!(h.other_windows(cx).is_empty());
    let edits = quill::scratch::temp_work_dir("quill-edits");
    let rendered: Vec<_> = std::fs::read_dir(edits)
        .map(|entries| {
            entries
                .flatten()
                .filter_map(|entry| std::fs::canonicalize(entry.path()).ok())
                .filter(|path| image::image_dimensions(path).ok() == Some((13, 53)))
                .collect()
        })
        .unwrap_or_default();
    assert_eq!(rendered.len(), 1, "one PNG, not leaked twice");
    h.app.read_with(cx, |app, _| {
        assert!(
            app.composer_sends.holds(&rendered[0]),
            "the PNG is the tray's photo, not a leftover"
        );
        assert!(!app.composer_sends.holds(&original));
    });
    let _ = std::fs::remove_file(&rendered[0]);
}

#[gpui_kit::test]
fn a_message_photo_edit_whose_chat_window_closed_leaves_no_png(cx: &mut TestAppContext) {
    use crate::ui::photo_editor::{EditorPurpose, PhotoEditor};
    let h = new_app(cx);
    let chat_window = h.open_in_new_window(OTHER, cx);
    let app = h.app.clone();
    cx.update_window(chat_window, |_, window, cx| {
        app.update(cx, |app, cx| {
            app.in_window_of(window, cx, |app, window, cx| {
                let pixels = image::RgbaImage::from_pixel(59, 11, image::Rgba([4, 5, 6, 255]));
                app.viewer.photo_editor =
                    Some(PhotoEditor::new(EditorPurpose::MessageEdit, pixels));
                app.save_photo_editor(window, cx);
            });
        });
        window.dispatch_action(Box::new(CloseWindow), cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert!(h.other_windows(cx).is_empty());
    let edits = quill::scratch::temp_work_dir("quill-edits");
    let left = std::fs::read_dir(edits)
        .map(|entries| {
            entries
                .flatten()
                .filter(|entry| image::image_dimensions(entry.path()).ok() == Some((59, 11)))
                .count()
        })
        .unwrap_or_default();
    assert_eq!(left, 0, "the dropped edit's PNG was deleted");
}

/// Whether `window` drew the status toast.
fn status_toast_drawn(window: AnyWindowHandle, cx: &mut TestAppContext) -> bool {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.try_find("status-toast").is_some()
    })
    .unwrap()
}

#[gpui_kit::test]
fn a_note_from_a_chat_window_toasts_in_that_window(cx: &mut TestAppContext) {
    crate::ui::app_render::pin_toasts_for_tests();
    let h = new_app(cx);
    let chat_window = h.open_in_new_window(OTHER, cx);
    h.app.update(cx, |app, cx| {
        app.note_window_input(Some(OTHER));
        app.connection
            .set_note(crate::ui::status_note::StatusNote::error(
                "Couldn't open the link",
            ));
        app.set_window_scope(None);
        cx.notify();
    });
    assert!(status_toast_drawn(chat_window, cx), "the window shows it");
    assert!(!status_toast_drawn(h.main, cx), "the main window does not");

    // The next input in the main window brings the toast there.
    h.app.update(cx, |app, cx| {
        app.note_window_input(None);
        cx.notify();
    });
    assert!(status_toast_drawn(h.main, cx));
    assert!(!status_toast_drawn(chat_window, cx));
}
