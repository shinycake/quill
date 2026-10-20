//! UI tests for the `/` menu in a group with bots, over the
//! `ready-group-commands` demo (gpui-kit's test support needs the
//! `demo-capture` feature).
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, Focusable, TestAppContext, VisualTestContext, px, size};

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
                Some(ScreenshotDemo::named("ready-group-commands")),
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

fn type_text(app: &Entity<QuillApp>, cx: &mut VisualTestContext, text: &str, caret: usize) {
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.composer.update(cx, |input, cx| {
                input.set_value(text.to_string(), window, cx);
                input.set_selected_range(caret..caret, cx);
            });
            app.sync_command_menu(cx);
        });
    });
    cx.run_until_parked();
}

fn menu_commands(app: &Entity<QuillApp>, cx: &mut VisualTestContext) -> Vec<String> {
    app.update(cx, |app, cx| {
        app.command_menu_state(cx)
            .map(|(prefix, items)| {
                let with_username = quill::composer::commands_with_username(&items, &prefix);
                items
                    .iter()
                    .map(|item| item.command_text(with_username))
                    .collect()
            })
            .unwrap_or_default()
    })
}

fn composer_text(app: &Entity<QuillApp>, cx: &mut VisualTestContext) -> (String, usize) {
    app.read_with(cx, |app, cx| {
        let input = app.composer.read(cx);
        (input.value().to_string(), input.selected_range().start)
    })
}

#[gpui_kit::test]
fn a_group_lists_every_bots_commands_with_the_bot_name(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    // The demo types `/`; the bot that wrote last is listed first.
    assert_eq!(
        menu_commands(&app, vcx),
        [
            "/start@weatherbot",
            "/forecast@weatherbot",
            "/start@pollbot",
            "/poll@pollbot",
            "/results@pollbot"
        ]
    );
    type_text(&app, vcx, "/start@po", 9);
    assert_eq!(menu_commands(&app, vcx), ["/start@pollbot"]);
}

#[gpui_kit::test]
fn tab_completes_the_command_before_the_caret(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    type_text(&app, vcx, "/st and more", 3);
    assert_eq!(
        menu_commands(&app, vcx),
        ["/start@weatherbot", "/start@pollbot"]
    );
    vcx.update(|window, cx| {
        app.update(cx, |app, cx| {
            assert!(app.insert_command_menu_selection(window, cx));
        });
    });
    // The text after the caret stays and its space is reused.
    assert_eq!(
        composer_text(&app, vcx),
        ("/start@weatherbot and more".to_string(), 18)
    );
    assert!(menu_commands(&app, vcx).is_empty());
}

#[gpui_kit::test]
fn a_slash_after_other_text_opens_nothing(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    type_text(&app, vcx, "hi /st", 6);
    assert!(menu_commands(&app, vcx).is_empty());
}

#[gpui_kit::test]
fn enter_sends_the_command_alone_and_keeps_the_rest_of_the_field(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    // A photo waits in the tray.
    let photo = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("docs/screenshots/fixtures/demo-thumb.png");
    app.update(vcx, |app, _| {
        let attachment = quill::composer::ComposerAttachment::pick(
            &photo,
            quill::composer::AttachmentKind::Photo,
        );
        app.seed_demo_attachments(attachment.into_iter().collect());
    });
    type_text(&app, vcx, "/st and more", 3);
    assert_eq!(
        menu_commands(&app, vcx),
        ["/start@weatherbot", "/start@pollbot"]
    );
    let before = sent_texts(&app, vcx).len();
    vcx.update(|window, cx| {
        app.update(cx, |app, cx| {
            assert!(app.pick_command_menu_selection(window, cx));
        });
    });
    // Tab would have put `/start@weatherbot` in the field. Enter sends it as
    // its own text message, with no caption on the tray's photo.
    let sent = sent_texts(&app, vcx);
    assert_eq!(sent.len(), before + 1);
    assert_eq!(sent.last().map(String::as_str), Some("/start@weatherbot"));
    app.read_with(vcx, |app, _| {
        assert_eq!(app.pending_attachments().len(), 1, "the tray is untouched");
        assert!(app.composer_ui.clear_draft_on_success.is_none());
    });
    // What followed the caret stays, the caret at its end, and it is saved
    // as the chat's draft.
    assert_eq!(composer_text(&app, vcx), (" and more".to_string(), 9));
    let draft = app.read_with(vcx, |app, _| {
        let chat = app.open_chat_id()?;
        let topic = app.open_draft_topic(chat);
        app.session()?
            .draft_for(chat, topic)
            .map(|d| d.text.clone())
    });
    assert_eq!(draft.as_deref(), Some(" and more"));
    assert!(menu_commands(&app, vcx).is_empty());
}

fn sent_texts(app: &Entity<QuillApp>, cx: &mut VisualTestContext) -> Vec<String> {
    app.read_with(cx, |app, _| {
        let Some(session) = app.session() else {
            return Vec::new();
        };
        let Some(chat) = session.open_chat else {
            return Vec::new();
        };
        session
            .histories
            .get(&chat.0)
            .map(|history| {
                history
                    .messages
                    .values()
                    .filter(|message| message.is_outgoing)
                    .filter_map(|message| match &message.content {
                        quill::telegram::envelope::MessageContent::Text(text) => {
                            Some(text.text.clone())
                        }
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default()
    })
}

#[gpui_kit::test]
fn enter_without_a_menu_is_not_consumed(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    type_text(&app, vcx, "hello", 5);
    vcx.update(|window, cx| {
        app.update(cx, |app, cx| {
            assert!(!app.pick_command_menu_selection(window, cx));
        });
    });
    assert_eq!(composer_text(&app, vcx).0, "hello");
}

#[gpui_kit::test]
fn the_slash_menu_stays_closed_while_a_message_is_edited(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    // An own message to edit, in the open group.
    app.update(vcx, |app, _| {
        use std::sync::atomic::Ordering;
        let (sink, seq) = (app.demo_ui.sink.clone(), &app.demo_ui.seq);
        if let Some(session) = app.demo_session.as_mut() {
            seq.store(session.last_seq, Ordering::SeqCst);
            let json = r#"{"@type":"updateNewMessage","message":{"id":99,"chat_id":-2001,"sender_id":{"@type":"messageSenderUser","user_id":1},"is_outgoing":true,"date":1700000099,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"please hurry","entities":[]}}}}"#;
            if let Some(owned) = quill::telegram::client::copy_and_parse(json, seq, &{
                let dyn_sink: std::sync::Arc<dyn quill::diagnostics::DiagnosticSink> = sink;
                dyn_sink
            }) {
                session.apply(owned);
            }
        }
    });
    vcx.update(|window, cx| {
        app.update(cx, |app, cx| {
            let edit = app
                .session()
                .and_then(|session| session.last_editable_message(quill::ids::ChatId(-2001)));
            assert!(edit.is_some(), "the group has an own message to edit");
            if let Some(edit) = edit {
                app.begin_edit(edit, window, cx);
            }
        });
    });
    // The menu the demo opened is gone, and typing a command opens nothing.
    assert!(menu_commands(&app, vcx).is_empty());
    type_text(&app, vcx, "/st hurry", 3);
    assert!(menu_commands(&app, vcx).is_empty());
    vcx.update(|window, cx| {
        app.update(cx, |app, cx| {
            assert!(!app.pick_command_menu_selection(window, cx));
        });
    });
    assert_eq!(composer_text(&app, vcx).0, "/st hurry");
}
