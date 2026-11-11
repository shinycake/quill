//! UI tests for the reply and edit bars and the field placeholder, over the
//! `ready-composer-bars` demo (gpui-kit's test support needs the
//! `demo-capture` feature).
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, Focusable, TestAppContext, VisualTestContext, px, size};
use quill::composer::{BarAction, BarKind, ComposerReplyTo};
use quill::ids::{ChatId, MessageId};

const GROUP: ChatId = ChatId(-2002);
const PAID_USER: ChatId = ChatId(51);
const CHANNEL: ChatId = ChatId(-1002000000001);
const SUPERGROUP: ChatId = ChatId(-1002000000002);

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
                Some(ScreenshotDemo::named("ready-composer-bars")),
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
    frames(&mut vcx);
    (app, vcx)
}

fn frames(cx: &mut VisualTestContext) {
    for _ in 0..2 {
        cx.update(|window, cx| window.render_frame(cx));
    }
    cx.run_until_parked();
}

fn open_chat(app: &Entity<QuillApp>, cx: &mut VisualTestContext, chat: ChatId) {
    app.update(cx, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session.open_chat(chat);
        }
        cx.notify();
    });
    frames(cx);
}

fn placeholder(app: &Entity<QuillApp>, cx: &mut VisualTestContext) -> String {
    app.read_with(cx, |app, cx| {
        app.composer
            .read(cx)
            .presentation()
            .placeholder()
            .to_string()
    })
}

fn begin_edit(app: &Entity<QuillApp>, cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            let edit = app
                .session()
                .and_then(|session| session.last_editable_message(GROUP));
            assert!(edit.is_some(), "the group has an own message to edit");
            if let Some(edit) = edit {
                app.begin_edit(edit, window, cx);
            }
        });
    });
    frames(cx);
}

fn editing(app: &Entity<QuillApp>, cx: &mut VisualTestContext) -> bool {
    app.read_with(cx, |app, _| app.chat_ui.is_editing())
}

fn confirm_open(app: &Entity<QuillApp>, cx: &mut VisualTestContext) -> bool {
    app.read_with(cx, |app, _| app.chat_ui.cancel_edit_confirm_open())
}

fn set_field(app: &Entity<QuillApp>, cx: &mut VisualTestContext, text: &str) {
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.composer.update(cx, |input, cx| {
                input.set_value(text.to_string(), window, cx)
            });
        });
    });
    frames(cx);
}

fn escape(_app: &Entity<QuillApp>, cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.press("escape", cx));
    frames(cx);
}

fn click(cx: &mut VisualTestContext, id: &'static str) {
    cx.update(|window, cx| window.click(id, cx));
    frames(cx);
}

#[gpui_kit::test]
fn placeholder_follows_the_chat(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    assert_eq!(placeholder(&app, vcx), "Write a message…");
    open_chat(&app, vcx, PAID_USER);
    assert_eq!(placeholder(&app, vcx), "Message for 25 Stars");
    open_chat(&app, vcx, CHANNEL);
    assert_eq!(placeholder(&app, vcx), "Broadcast a message…");
    app.update(vcx, |app, _| app.toggle_composer_silent());
    frames(vcx);
    assert_eq!(placeholder(&app, vcx), "Silent broadcast…");
    open_chat(&app, vcx, GROUP);
    assert_eq!(placeholder(&app, vcx), "Write a message…");
}

#[gpui_kit::test]
fn editing_changes_the_placeholder_and_cancelling_restores_it(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    begin_edit(&app, vcx);
    assert_eq!(placeholder(&app, vcx), "New message…");
    escape(&app, vcx);
    assert!(!editing(&app, vcx));
    assert_eq!(placeholder(&app, vcx), "Write a message…");
}

#[gpui_kit::test]
fn escape_cancels_an_unchanged_edit_at_once(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    begin_edit(&app, vcx);
    escape(&app, vcx);
    assert!(!confirm_open(&app, vcx));
    assert!(!editing(&app, vcx));
}

#[gpui_kit::test]
fn whitespace_only_is_not_a_change(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    begin_edit(&app, vcx);
    let text = app.read_with(vcx, |app, cx| app.composer.read(cx).value().to_string());
    set_field(&app, vcx, &format!("{text}  \n"));
    escape(&app, vcx);
    assert!(!editing(&app, vcx));
}

#[gpui_kit::test]
fn escape_asks_before_dropping_changed_text(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    begin_edit(&app, vcx);
    set_field(&app, vcx, "something else entirely");
    escape(&app, vcx);
    assert!(confirm_open(&app, vcx));
    assert!(editing(&app, vcx), "the edit stays until the user answers");
}

#[gpui_kit::test]
fn answering_no_keeps_the_edit_and_the_text(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    begin_edit(&app, vcx);
    set_field(&app, vcx, "something else entirely");
    escape(&app, vcx);
    assert!(confirm_open(&app, vcx));
    click(vcx, "cancel-edit-no");
    assert!(!confirm_open(&app, vcx));
    assert!(editing(&app, vcx));
    let text = app.read_with(vcx, |app, cx| app.composer.read(cx).value().to_string());
    assert_eq!(text, "something else entirely");
}

#[gpui_kit::test]
fn answering_yes_drops_the_edit(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    begin_edit(&app, vcx);
    set_field(&app, vcx, "something else entirely");
    escape(&app, vcx);
    click(vcx, "cancel-edit-yes");
    assert!(!editing(&app, vcx));
    assert!(!confirm_open(&app, vcx));
}

#[gpui_kit::test]
fn leaving_the_chat_with_the_box_open_leaves_no_box_and_no_edit(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    begin_edit(&app, vcx);
    set_field(&app, vcx, "something else entirely");
    escape(&app, vcx);
    assert!(confirm_open(&app, vcx));
    vcx.update(|window, cx| {
        app.update(cx, |app, cx| app.leave_open_chat(window, cx));
    });
    frames(vcx);
    assert!(!editing(&app, vcx));
    assert!(!confirm_open(&app, vcx));
}

#[test]
fn the_cancel_box_cannot_open_without_an_edit() {
    let mut ui = crate::ui::chat_ui_state::ChatUi::default();
    ui.set_cancel_edit_confirm(true);
    assert!(!ui.cancel_edit_confirm_open());
}

#[gpui_kit::test]
fn a_second_escape_does_not_reopen_or_cancel(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    begin_edit(&app, vcx);
    set_field(&app, vcx, "something else entirely");
    escape(&app, vcx);
    assert!(confirm_open(&app, vcx));
    escape(&app, vcx);
    assert!(
        editing(&app, vcx),
        "Escape on the box does not drop the edit"
    );
    escape(&app, vcx);
    assert!(editing(&app, vcx));
}

#[gpui_kit::test]
fn the_x_button_cancels_without_asking(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    begin_edit(&app, vcx);
    set_field(&app, vcx, "something else entirely");
    click(vcx, "cancel-edit");
    assert!(!editing(&app, vcx));
    assert!(!confirm_open(&app, vcx));
}

fn reply_to_ada(app: &Entity<QuillApp>, cx: &mut VisualTestContext) {
    app.update(cx, |app, cx| {
        app.chat_ui
            .set_reply(Some(ComposerReplyTo::new(GROUP, MessageId(10))));
        cx.notify();
    });
    frames(cx);
}

fn jump_ready(app: &Entity<QuillApp>, cx: &mut VisualTestContext, id: MessageId) -> bool {
    app.read_with(cx, |app, _| {
        app.session().is_some_and(|session| {
            matches!(
                session.search.chat_search.jump,
                quill::state::ChatSearchJump::Ready { message_id } if message_id == id
            )
        })
    })
}

fn clear_jump(app: &Entity<QuillApp>, cx: &mut VisualTestContext) {
    app.update(cx, |app, _| {
        if let Some(session) = app.demo_session.as_mut() {
            session.search.chat_search.jump = quill::state::ChatSearchJump::None;
        }
    });
}

#[gpui_kit::test]
fn clicking_the_reply_bar_opens_the_quote_picker(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    reply_to_ada(&app, vcx);
    let kind = app.read_with(vcx, |app, _| {
        app.chat_ui.reply().map(|reply| app.reply_bar_kind(reply))
    });
    assert!(matches!(
        kind,
        Some(BarKind::Reply {
            has_text: true,
            same_chat: true,
            ..
        })
    ));
    vcx.update(|window, cx| window.click(("composer-reply-quote", 1usize), cx));
    frames(vcx);
    assert!(app.read_with(vcx, |app, _| app.reply_quote_open()));
}

#[gpui_kit::test]
fn right_clicking_the_reply_bar_jumps_to_the_replied_message(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    reply_to_ada(&app, vcx);
    clear_jump(&app, vcx);
    vcx.update(|window, cx| window.right_click(("composer-reply-quote", 1usize), cx));
    frames(vcx);
    assert!(jump_ready(&app, vcx, MessageId(10)));
    assert!(
        !app.read_with(vcx, |app, _| app.reply_quote_open()),
        "a right click does not open the picker"
    );
}

#[gpui_kit::test]
fn clicking_the_edit_bar_jumps_to_the_edited_message(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    begin_edit(&app, vcx);
    let edited = app.read_with(vcx, |app, _| {
        app.chat_ui.pending_edit().map(|edit| edit.message_id)
    });
    assert_eq!(edited, Some(MessageId(11)));
    clear_jump(&app, vcx);
    vcx.update(|window, cx| window.click(("composer-edit-header", 1usize), cx));
    frames(vcx);
    assert!(jump_ready(&app, vcx, MessageId(11)));
    assert!(editing(&app, vcx), "jumping leaves the edit open");
}

#[gpui_kit::test]
fn the_bar_handler_for_a_media_reply_offers_another_chat(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    reply_to_ada(&app, vcx);
    let clicks = app.update(vcx, |app, cx| app.bar_clicks(BarKind::Edit, None, cx));
    vcx.update(|window, cx| (clicks.run)(BarAction::ReplyElsewhere, window, cx));
    assert!(app.read_with(vcx, |app, _| app.share.forward.reply_chooser_open()));
}

fn supergroup_status(app: &Entity<QuillApp>, cx: &mut VisualTestContext, status: &str) {
    app.update(cx, |app, _| {
        use std::sync::atomic::Ordering;
        let (sink, seq) = (app.demo_ui.sink.clone(), &app.demo_ui.seq);
        if let Some(session) = app.demo_session.as_mut() {
            seq.store(session.last_seq, Ordering::SeqCst);
            let json = format!(
                r#"{{"@type":"updateSupergroup","supergroup":{{"@type":"supergroup","id":2000000002,"status":{status}}}}}"#
            );
            let dyn_sink: std::sync::Arc<dyn quill::diagnostics::DiagnosticSink> = sink;
            if let Some(owned) = quill::telegram::client::copy_and_parse(&json, seq, &dyn_sink) {
                session.apply(owned);
            }
        }
    });
    frames(cx);
}

#[gpui_kit::test]
fn an_anonymous_admin_sends_anonymously_until_demoted_or_revealed(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    open_chat(&app, vcx, SUPERGROUP);
    assert_eq!(placeholder(&app, vcx), "Send anonymously…");
    // The owner turns "Remain anonymous" off.
    supergroup_status(
        &app,
        vcx,
        r#"{"@type":"chatMemberStatusAdministrator","can_be_edited":false,"rights":{"@type":"chatAdministratorRights","is_anonymous":false}}"#,
    );
    assert_eq!(placeholder(&app, vcx), "Write a message…");
    supergroup_status(
        &app,
        vcx,
        r#"{"@type":"chatMemberStatusAdministrator","can_be_edited":false,"rights":{"@type":"chatAdministratorRights","is_anonymous":true}}"#,
    );
    assert_eq!(placeholder(&app, vcx), "Send anonymously…");
    // The owner posts anonymously too; a demotion clears it at once.
    supergroup_status(
        &app,
        vcx,
        r#"{"@type":"chatMemberStatusCreator","is_anonymous":true,"is_member":true}"#,
    );
    assert_eq!(placeholder(&app, vcx), "Send anonymously…");
    supergroup_status(&app, vcx, r#"{"@type":"chatMemberStatusMember"}"#);
    assert_eq!(placeholder(&app, vcx), "Write a message…");
}

#[gpui_kit::test]
fn an_inline_bots_hint_follows_the_typed_name(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    open_chat(&app, vcx, PAID_USER);
    let hint = |app: &Entity<QuillApp>, vcx: &mut VisualTestContext| {
        app.read_with(vcx, |app, cx| app.inline_bot_hint_text(cx))
    };
    assert_eq!(hint(&app, vcx), None);
    app.update(vcx, |app, _| {
        if let Some(session) = app.demo_session.as_mut() {
            session.bots.inline_bot_resolve = Some(quill::state::InlineBotResolve::Resolved {
                username: "gifbot".into(),
                user_id: 61,
                is_inline: Some(true),
            });
        }
    });
    set_field(&app, vcx, "@gifbot ");
    assert_eq!(hint(&app, vcx).as_deref(), Some("Search GIFs"));
    set_field(&app, vcx, "@gifbot cats");
    assert_eq!(hint(&app, vcx), None);
    set_field(&app, vcx, "@gifbot");
    assert_eq!(hint(&app, vcx).as_deref(), Some("Search GIFs"));
    set_field(&app, vcx, "");
    assert_eq!(hint(&app, vcx), None);
}

const FORUM: ChatId = ChatId(-1002000000003);
const KEYBOARD_BOT: ChatId = ChatId(71);
const TOPIC_BOT: ChatId = ChatId(72);

fn select_topic(app: &Entity<QuillApp>, cx: &mut VisualTestContext, chat: ChatId, topic: i32) {
    app.update(cx, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session.select_topic(chat, topic);
        }
        cx.notify();
    });
    frames(cx);
}

fn reply_to(app: &Entity<QuillApp>, cx: &mut VisualTestContext, chat: ChatId, message: i64) {
    app.update(cx, |app, cx| {
        app.chat_ui
            .set_reply(Some(ComposerReplyTo::new(chat, MessageId(message))));
        cx.notify();
    });
    frames(cx);
}

#[gpui_kit::test]
fn a_forum_field_names_the_topic_it_sends_to(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    // Without a topic open a message lands in General.
    open_chat(&app, vcx, FORUM);
    assert_eq!(placeholder(&app, vcx), "Message in General");
    // A reply is sent to the topic its message is in, which no topic history
    // has to be loaded for.
    reply_to(&app, vcx, FORUM, 20);
    assert_eq!(placeholder(&app, vcx), "Reply in Releases");
    // Cancelling the reply brings General back.
    app.update(vcx, |app, cx| {
        app.chat_ui.set_reply(None);
        cx.notify();
    });
    frames(vcx);
    assert_eq!(placeholder(&app, vcx), "Message in General");
    // A topic that was renamed is read again.
    reply_to(&app, vcx, FORUM, 20);
    app.update(vcx, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session
                .threads
                .forum_topics
                .update_topic(FORUM.0, 5, |topic| topic.name = "Changelog".into());
        }
        cx.notify();
    });
    frames(vcx);
    assert_eq!(placeholder(&app, vcx), "Reply in Changelog");
    // Inside an open topic the field is plain, as in tdesktop's topic view.
    select_topic(&app, vcx, FORUM, 5);
    assert_eq!(placeholder(&app, vcx), "Write a message…");
    open_chat(&app, vcx, GROUP);
    assert_eq!(placeholder(&app, vcx), "Write a message…");
}

#[gpui_kit::test]
fn a_bot_keyboards_placeholder_shows_while_the_keyboard_does(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    open_chat(&app, vcx, KEYBOARD_BOT);
    assert_eq!(placeholder(&app, vcx), "Choose a size");
    // Hiding the keyboard takes its text with it.
    click(vcx, "composer-keyboard-toggle");
    assert_eq!(placeholder(&app, vcx), "Write a message…");
    click(vcx, "composer-keyboard-toggle");
    assert_eq!(placeholder(&app, vcx), "Choose a size");
    open_chat(&app, vcx, GROUP);
    assert_eq!(placeholder(&app, vcx), "Write a message…");
}

#[gpui_kit::test]
fn a_bot_that_makes_its_own_topics_is_off_thread_until_one_opens(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    open_chat(&app, vcx, TOPIC_BOT);
    assert_eq!(placeholder(&app, vcx), "Off-thread message");
    select_topic(&app, vcx, TOPIC_BOT, 2);
    assert_eq!(placeholder(&app, vcx), "Write a message…");
    open_chat(&app, vcx, KEYBOARD_BOT);
    assert_eq!(placeholder(&app, vcx), "Choose a size");
}

/// A reply-keyboard button pressed while editing sends its text as a
/// message of its own (tdesktop `sendBotCommand`): the edit stays open and
/// the edited message keeps its text.
#[gpui_kit::test]
fn a_keyboard_button_while_editing_sends_a_new_message(cx: &mut TestAppContext) {
    let (app, mut cx) = new_app(cx);
    let cx = &mut cx;
    open_chat(&app, cx, GROUP);
    begin_edit(&app, cx);
    assert!(editing(&app, cx));
    let state = |app: &Entity<QuillApp>, cx: &mut VisualTestContext, target: Option<MessageId>| {
        app.read_with(cx, |app, _| {
            let history = app.session().and_then(|s| s.histories.get(&GROUP.0));
            let count = history.map_or(0, |h| h.ordered().len());
            let content = history
                .zip(target)
                .and_then(|(h, id)| h.ordered().into_iter().find(|m| m.id == id))
                .map(|m| format!("{:?}", m.content));
            (count, content)
        })
    };
    let target = app.read_with(cx, |app, _| {
        app.chat_ui.pending_edit().map(|edit| edit.message_id)
    });
    assert!(target.is_some());
    let (count, content) = state(&app, cx, target);
    app.update(cx, |app, cx| {
        app.press_custom_keyboard_text(GROUP, MessageId(1), "Yes please", false, cx)
    });
    frames(cx);
    let (count_after, content_after) = state(&app, cx, target);
    assert_eq!(
        count_after,
        count + 1,
        "the button's text went out as a new message"
    );
    assert_eq!(content_after, content, "the edited message kept its text");
    assert!(editing(&app, cx), "the edit stays open");
}

/// Round 14 (area B, finding 8): the quote picker belongs to the reply it
/// was opened for. Another reply comes without it, and a sent reply closes
/// it, so the next reply to the same message opens no picker either.
#[gpui_kit::test]
fn the_quote_picker_does_not_outlive_its_reply(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    reply_to_ada(&app, vcx);
    vcx.update(|window, cx| window.click(("composer-reply-quote", 1usize), cx));
    frames(vcx);
    assert!(app.read_with(vcx, |app, _| app.reply_quote_open()));
    app.update(vcx, |app, cx| {
        app.chat_ui
            .set_reply(Some(ComposerReplyTo::new(GROUP, MessageId(11))));
        cx.notify();
    });
    frames(vcx);
    assert!(
        !app.read_with(vcx, |app, _| app.reply_quote_open()),
        "another message's reply has no picker"
    );
    reply_to_ada(&app, vcx);
    vcx.update(|window, cx| window.click(("composer-reply-quote", 1usize), cx));
    frames(vcx);
    assert!(app.read_with(vcx, |app, _| app.reply_quote_open()));
    app.update(vcx, |app, cx| app.consume_sent_reply(GROUP, cx));
    frames(vcx);
    reply_to_ada(&app, vcx);
    assert!(
        !app.read_with(vcx, |app, _| app.reply_quote_open()),
        "the next reply to the same message opens no picker"
    );
}

/// Round 15 (area B, finding 6): a text send (Send without Done) closes
/// the picker with its reply too: the picker is part of the reply bar,
/// so every way a send drops the reply drops it.
#[gpui_kit::test]
fn a_text_send_closes_the_quote_picker(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    reply_to_ada(&app, vcx);
    vcx.update(|window, cx| window.click(("composer-reply-quote", 1usize), cx));
    frames(vcx);
    assert!(app.read_with(vcx, |app, _| app.reply_quote_open()));
    vcx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.set_composer_markup("thanks", window, cx);
            app.submit_from_send_button(window, cx);
        });
    });
    frames(vcx);
    assert!(
        app.read_with(vcx, |app, _| app.chat_ui.reply().is_none()),
        "the send took the reply"
    );
    reply_to_ada(&app, vcx);
    assert!(
        !app.read_with(vcx, |app, _| app.reply_quote_open()),
        "the next reply to the same message opens no picker"
    );
}

/// Switching from one edit to another keeps the real draft: cancelling
/// brings back what was typed before the first edit, not the first edit's
/// text (sweep 08-127).
#[gpui_kit::test]
fn a_second_edit_keeps_the_real_draft(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    set_field(&app, vcx, "my draft");
    begin_edit(&app, vcx);
    begin_edit(&app, vcx);
    assert!(editing(&app, vcx));
    cx_update_clear_edit(&app, vcx);
    assert!(!editing(&app, vcx));
    let text = app.read_with(vcx, |app, cx| app.composer.read(cx).value().to_string());
    assert_eq!(text, "my draft");
}

fn cx_update_clear_edit(app: &Entity<QuillApp>, cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        app.update(cx, |app, cx| app.clear_edit(window, cx));
    });
    frames(cx);
}

fn photo_in_tray(app: &Entity<QuillApp>, cx: &mut VisualTestContext) {
    use quill::composer::{AttachmentKind, ComposerAttachment};
    app.update(cx, |app, _| {
        app.update_pending_attachments(|tray| {
            tray.push(ComposerAttachment {
                path: std::env::temp_dir().join("sweep-tray.jpg"),
                kind: AttachmentKind::Photo,
                file_name: "sweep-tray.jpg".into(),
                spoiler: false,
                extras: Default::default(),
            })
        });
    });
}

fn field(app: &Entity<QuillApp>, cx: &mut VisualTestContext) -> String {
    app.read_with(cx, |app, cx| app.composer.read(cx).value().to_string())
}

/// Sweep 08-128: Edit does not throw away the tray the user built.
#[gpui_kit::test]
fn an_edit_is_refused_while_a_tray_waits(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    open_chat(&app, vcx, GROUP);
    photo_in_tray(&app, vcx);
    begin_edit(&app, vcx);
    assert!(!editing(&app, vcx), "the edit did not start");
    app.read_with(vcx, |app, _| {
        assert_eq!(app.pending_attachments().len(), 1, "the tray stays");
    });
}

/// Sweep 08-129: a cancelled edit and a new one on the same message are
/// two edits, so a picker's answer for the first is told apart.
#[gpui_kit::test]
fn a_cancelled_edit_and_a_new_one_are_two_edits(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    begin_edit(&app, vcx);
    let first = app.read_with(vcx, |app, _| app.chat_ui.edit_id());
    assert!(first.is_some());
    vcx.update(|window, cx| app.update(cx, |app, cx| app.clear_edit(window, cx)));
    assert_eq!(app.read_with(vcx, |app, _| app.chat_ui.edit_id()), None);
    begin_edit(&app, vcx);
    let second = app.read_with(vcx, |app, _| app.chat_ui.edit_id());
    assert!(second.is_some());
    assert_ne!(first, second);
}

/// Sweep 08-131: an edit that was sent stays open until TDLib answers; an
/// error returns it to editing with the edited text in the field, an `ok`
/// leaves edit mode.
#[gpui_kit::test]
fn an_edit_leaves_edit_mode_only_on_its_answer(cx: &mut TestAppContext) {
    use quill::ids::RequestId;
    use quill::state::Answer;
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    begin_edit(&app, vcx);
    set_field(&app, vcx, "the edited text");
    app.update(vcx, |app, _| app.chat_ui.mark_edit_sending(RequestId(5)));
    assert_eq!(
        app.read_with(vcx, |app, _| app.chat_ui.edit_sending()),
        Some(RequestId(5))
    );
    vcx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.apply_edit_answer(
                &Answer::Failed("MESSAGE_EDIT_TIME_EXPIRED".into()),
                window,
                cx,
            )
        })
    });
    assert!(editing(&app, vcx), "an error keeps the edit open");
    assert_eq!(field(&app, vcx), "the edited text", "the text is not lost");
    assert_eq!(
        app.read_with(vcx, |app, _| app.chat_ui.edit_sending()),
        None
    );
    app.update(vcx, |app, _| app.chat_ui.mark_edit_sending(RequestId(6)));
    vcx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.apply_edit_answer(&Answer::Done, window, cx)
        })
    });
    assert!(!editing(&app, vcx), "the answer ends the edit");
    assert_ne!(field(&app, vcx), "the edited text", "the draft is back");
}

/// Sweep 08-135: a rich send that TDLib refuses gives its text back.
#[gpui_kit::test]
fn a_refused_rich_send_gives_the_text_back(cx: &mut TestAppContext) {
    use crate::ui::composer::SentText;
    use quill::ids::RequestId;
    use quill::state::Answer;
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let sent = |app: &Entity<QuillApp>, vcx: &mut VisualTestContext| {
        app.update(vcx, |app, _| {
            app.chat_ui.sent_text = SentText::Sent {
                request: RequestId(9),
                markup: "# Title\nbody".into(),
            };
        });
    };
    sent(&app, vcx);
    vcx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.apply_sent_text_answer(&Answer::Failed("FLOOD".into()), window, cx)
        })
    });
    assert_eq!(field(&app, vcx), "# Title\nbody");
    assert!(app.read_with(vcx, |app, _| app.chat_ui.rich_editor_open));
    assert_eq!(
        app.read_with(vcx, |app, _| app.chat_ui.sent_text.clone()),
        SentText::None
    );
    // An accepted send lets the text go; text typed since is not replaced.
    set_field(&app, vcx, "");
    sent(&app, vcx);
    vcx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.apply_sent_text_answer(&Answer::Done, window, cx)
        })
    });
    assert_eq!(field(&app, vcx), "");
    set_field(&app, vcx, "typed meanwhile");
    sent(&app, vcx);
    vcx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.apply_sent_text_answer(&Answer::Failed("FLOOD".into()), window, cx)
        })
    });
    assert_eq!(field(&app, vcx), "typed meanwhile");
}

/// Sweep 08-149/150: a refused rich send keeps no draft clear, so a later
/// send of the chat cannot delete the text it gave back. A refused inline
/// pick (10-116) gives the `@bot query` back too.
#[gpui_kit::test]
fn a_refused_send_drops_its_draft_clear_and_gives_back_the_text(cx: &mut TestAppContext) {
    use crate::ui::composer::SentText;
    use crate::ui::composer_state::DraftClear;
    use quill::ids::RequestId;
    use quill::state::Answer;
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    open_chat(&app, vcx, GROUP);
    let clear_waits = |app: &Entity<QuillApp>, vcx: &mut VisualTestContext| {
        app.read_with(vcx, |app, _| {
            app.send_slot()
                .is_some_and(|slot| app.composer_sends.has_draft_clear(&slot))
        })
    };
    app.update(vcx, |app, _| {
        app.expect_draft_clear(DraftClear::whole_composer(GROUP));
        app.chat_ui.sent_text = SentText::Sent {
            request: RequestId(9),
            markup: "rich".into(),
        };
    });
    assert!(clear_waits(&app, vcx));
    vcx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.apply_sent_text_answer(&Answer::Failed("FLOOD".into()), window, cx)
        })
    });
    assert!(!clear_waits(&app, vcx), "the refused send's clear is gone");
    // The inline pick: the query returns on an error answer, not on Done.
    set_field(&app, vcx, "");
    app.update(vcx, |app, _| {
        app.chat_ui.sent_text = SentText::InlinePick {
            request: RequestId(10),
            markup: "@bot cats".into(),
        };
    });
    vcx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.apply_sent_text_answer(&Answer::Failed("QUERY_ID_INVALID".into()), window, cx)
        })
    });
    assert_eq!(field(&app, vcx), "@bot cats");
    assert!(!app.read_with(vcx, |app, _| app.chat_ui.rich_editor_open));
}

/// Sweep 08-134: an AI answer replaces the draft it was asked about, not
/// text typed since.
#[gpui_kit::test]
fn an_ai_answer_applies_only_to_the_draft_it_was_asked_about(cx: &mut TestAppContext) {
    use crate::ui::composer::AiAsk;
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    open_chat(&app, vcx, GROUP);
    let answer = |app: &Entity<QuillApp>, vcx: &mut VisualTestContext, text: &str| {
        app.update(vcx, |app, _| {
            if let Some(session) = app.demo_session.as_mut() {
                session.messages.ai_composer_text = Some((None, GROUP, text.to_string()));
            }
        });
        vcx.update(|window, cx| app.update(cx, |app, cx| app.apply_ai_composer_answer(window, cx)));
    };
    set_field(&app, vcx, "teh draft");
    app.update(vcx, |app, _| {
        app.chat_ui.ai_ask = AiAsk::Asked {
            markup: "teh draft".into(),
        };
    });
    answer(&app, vcx, "the draft");
    assert_eq!(field(&app, vcx), "the draft", "the asked draft is replaced");
    // Asked about "the draft", then the user typed on before the answer.
    app.update(vcx, |app, _| {
        app.chat_ui.ai_ask = AiAsk::Asked {
            markup: "the draft".into(),
        };
    });
    set_field(&app, vcx, "the draft and more");
    answer(&app, vcx, "better draft");
    assert_eq!(
        field(&app, vcx),
        "the draft and more",
        "typed text survives"
    );
}

/// Sweep 10-115: a window reads its own bot, not the one another window's
/// trigger put in the session's single slot.
#[gpui_kit::test]
fn a_window_reads_its_own_inline_bot(cx: &mut TestAppContext) {
    use quill::state::InlineBotResolve;
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update(vcx, |app, _| {
        if let Some(session) = app.demo_session.as_mut() {
            session.bots.inline_bot_resolve = Some(InlineBotResolve::Resolved {
                username: "botb".into(),
                user_id: 2,
                is_inline: Some(true),
            });
        }
        app.chat_ui.inline_bot = Some(InlineBotResolve::Resolved {
            username: "bota".into(),
            user_id: 1,
            is_inline: Some(true),
        });
    });
    app.read_with(vcx, |app, _| {
        assert!(matches!(
            app.inline_bot_for("bota"),
            Some(InlineBotResolve::Resolved { user_id: 1, .. })
        ));
        assert!(matches!(
            app.inline_bot_for("botb"),
            Some(InlineBotResolve::Resolved { user_id: 2, .. })
        ));
        assert!(
            app.inline_bot_for("botc").is_none(),
            "unknown: not resolved"
        );
    });
}

/// Sweep 10-115: window A copies the session slot while it is still
/// `Resolving`, then window B's lookup replaces the slot. A must not keep
/// the stale `Resolving` (it would wait for an answer that never comes): it
/// reads nothing for its bot, so it asks again.
#[gpui_kit::test]
fn a_resolving_slot_replaced_by_another_window_is_not_kept(cx: &mut TestAppContext) {
    use quill::state::InlineBotResolve;
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update(vcx, |app, _| {
        if let Some(session) = app.demo_session.as_mut() {
            session.bots.inline_bot_resolve = Some(InlineBotResolve::Resolving {
                username: "bota".into(),
                generation: 1,
            });
        }
        app.capture_inline_bot("bota");
        if let Some(session) = app.demo_session.as_mut() {
            session.bots.inline_bot_resolve = Some(InlineBotResolve::Resolved {
                username: "botb".into(),
                user_id: 2,
                is_inline: Some(true),
            });
        }
        assert!(app.chat_ui.inline_bot.is_none());
        assert!(app.inline_bot_for("bota").is_none());
    });
}

fn apply_json(app: &mut QuillApp, json: &str) {
    let last = app
        .demo_session
        .as_ref()
        .map_or(0, |session| session.last_seq);
    app.demo_ui
        .seq
        .fetch_max(last, std::sync::atomic::Ordering::Relaxed);
    let sink: std::sync::Arc<dyn quill::diagnostics::DiagnosticSink> = app.demo_ui.sink.clone();
    let owned = quill::telegram::client::copy_and_parse(json, &app.demo_ui.seq, &sink)
        .expect("a valid update");
    if let Some(session) = app.demo_session.as_mut() {
        session.apply(owned);
    }
}

/// A group where the viewer is a plain member and a 30 s slow-mode wait is
/// running (25 s left).
fn waiting_group(app: &mut QuillApp) -> ChatId {
    const ID: i64 = 7702;
    for json in [
        format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{ID},"title":"Slow Club","type":{{"@type":"chatTypeSupergroup","supergroup_id":{ID},"is_channel":false}},"unread_count":0}}}}"#
        ),
        format!(
            r#"{{"@type":"updateChatPosition","chat_id":{ID},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"1","is_pinned":false}}}}"#
        ),
        format!(
            r#"{{"@type":"updateSupergroup","supergroup":{{"@type":"supergroup","id":{ID},"is_forum":false,"member_count":3,"status":{{"@type":"chatMemberStatusMember"}}}}}}"#
        ),
        format!(
            r#"{{"@type":"updateSupergroupFullInfo","supergroup_id":{ID},"supergroup_full_info":{{"@type":"supergroupFullInfo","description":"","member_count":3,"slow_mode_delay":30,"slow_mode_delay_expires_in":25,"linked_chat_id":0}}}}"#
        ),
    ] {
        apply_json(app, &json);
    }
    ChatId(ID)
}

/// Sweep 09-015: a one-time keyboard goes only when its button's text did.
#[gpui_kit::test]
fn a_one_time_keyboard_goes_only_when_its_text_went(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    open_chat(&app, vcx, GROUP);
    let dismissed = |app: &Entity<QuillApp>, vcx: &mut VisualTestContext, chat: ChatId| {
        app.read_with(vcx, |app, _| {
            app.message_ui.dismissed_keyboards.contains(&(chat.0, 1))
        })
    };
    // Held back by slow mode: the keyboard stays for another press.
    let slow = app.update(vcx, |app, _| waiting_group(app));
    open_chat(&app, vcx, slow);
    app.update(vcx, |app, cx| {
        app.press_custom_keyboard_text(slow, MessageId(1), "Yes", true, cx)
    });
    assert!(!dismissed(&app, vcx, slow), "the text was held back");
    // Sent: the keyboard goes.
    open_chat(&app, vcx, GROUP);
    app.update(vcx, |app, cx| {
        app.press_custom_keyboard_text(GROUP, MessageId(1), "Yes", true, cx)
    });
    assert!(
        dismissed(&app, vcx, GROUP),
        "the text went, the keyboard goes"
    );
}

/// Sweep 08-132: a command a gate refuses reports `Refused`, so the menu
/// keeps the typed token in the field; one that goes reports `Sent`.
#[gpui_kit::test]
fn a_menu_command_says_whether_it_went(cx: &mut TestAppContext) {
    use crate::ui::composer::send_bot_command::MenuSend;
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let slow = app.update(vcx, |app, _| waiting_group(app));
    open_chat(&app, vcx, slow);
    let refused = app.update(vcx, |app, cx| app.send_menu_command("/start", cx));
    assert_eq!(refused, MenuSend::Refused, "slow mode holds it back");
    open_chat(&app, vcx, GROUP);
    let sent = app.update(vcx, |app, cx| app.send_menu_command("/start", cx));
    assert_eq!(sent, MenuSend::Sent);
}

/// Sweep 09-112: a forced reply waits for the window that shows its chat.
#[gpui_kit::test]
fn a_forced_reply_waits_for_its_chat(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    open_chat(&app, vcx, GROUP);
    app.update(vcx, |app, _| {
        apply_json(
            app,
            r#"{"@type":"updateNewMessage","message":{"id":88,"chat_id":71,"sender_id":{"@type":"messageSenderUser","user_id":71},"is_outgoing":false,"date":1700000088,"reply_markup":{"@type":"replyMarkupForceReply","input_field_placeholder":"Name?"},"content":{"@type":"messageText","text":{"@type":"formattedText","text":"What is your name?","entities":[]}}}}"#,
        );
    });
    let waiting = |app: &Entity<QuillApp>, vcx: &mut VisualTestContext| {
        app.read_with(vcx, |app, _| {
            app.session()
                .is_some_and(|s| s.bots.pending_force_replies.contains_key(&KEYBOARD_BOT))
        })
    };
    assert!(waiting(&app, vcx), "armed for chat 71");
    // Another chat is open: nothing is armed there, the target waits.
    vcx.update(|window, cx| app.update(cx, |app, cx| app.arm_force_reply_here(window, cx)));
    assert!(
        app.read_with(vcx, |app, _| app
            .chat_ui
            .reply()
            .is_none_or(|r| r.chat_id != KEYBOARD_BOT)),
        "not armed in the other chat"
    );
    assert!(waiting(&app, vcx), "still waiting for its own chat");
    // Its chat opens: the composer replies to the message.
    open_chat(&app, vcx, KEYBOARD_BOT);
    vcx.update(|window, cx| app.update(cx, |app, cx| app.arm_force_reply_here(window, cx)));
    assert!(!waiting(&app, vcx), "taken once armed");
    let reply = app.read_with(vcx, |app, _| {
        app.chat_ui.reply().map(|r| (r.chat_id, r.message_id))
    });
    assert_eq!(reply, Some((KEYBOARD_BOT, MessageId(88))));
}
