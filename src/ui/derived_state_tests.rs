//! The reply bar, the message selection and the chat-list selection are
//! derived from the history and the list they point into: a deleted or
//! hidden item leaves them without anything pruning a copy. Run on a demo
//! `QuillApp` (gpui-kit's test support needs the `demo-capture` feature).
#![cfg(test)]
#![cfg(feature = "demo-capture")]

use crate::ui::app::ChatListFilter;
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, Focusable, TestAppContext, VisualTestContext, px, size};
use quill::composer::ComposerEdit;
use quill::ids::{ChatId, MessageId};
use quill::telegram::envelope::MessageContent;

const CHAT: ChatId = ChatId(11);
const REPLIED: MessageId = MessageId(101);

fn new_app(cx: &mut TestAppContext, demo: &str) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named(demo);
    let handle = cx.open_window(size(px(1100.), px(700.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        let focus = view.focus_handle(cx);
        window.focus(&focus, cx);
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().expect("the window opened the app");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    frames(&mut vcx);
    (app, vcx)
}

fn frames(vcx: &mut VisualTestContext) {
    vcx.update(|window, cx| {
        for _ in 0..2 {
            window.render_frame(cx);
        }
    });
    vcx.run_until_parked();
}

fn reply_text(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Option<String> {
    app.read_with(vcx, |app, _| {
        let reply = app.composer_ui.reply()?;
        let session = app.session()?;
        session
            .reply_target(reply.chat_id, reply.message_id)
            .map(quill::state::effective_preview)
    })
}

#[gpui_kit::test]
fn the_reply_bar_follows_an_edit_of_the_message(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-reply");
    let vcx = &mut vcx;
    assert_eq!(
        reply_text(&app, vcx).as_deref(),
        Some("Hello from injected JSON.")
    );
    app.update(vcx, |app, _| {
        let edit = ComposerEdit::from_own_content(
            CHAT,
            REPLIED,
            true,
            false,
            &MessageContent::Text("x".into()),
        );
        app.apply_demo_edit(&edit.expect("text is editable"), "Moved to 6 pm.");
    });
    frames(vcx);
    assert_eq!(reply_text(&app, vcx).as_deref(), Some("Moved to 6 pm."));
    assert!(
        app.read_with(vcx, |app, _| app.composer_ui.has_reply()),
        "an edit keeps the reply"
    );
}

#[gpui_kit::test]
fn deleting_the_replied_message_cancels_the_reply(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-reply");
    let vcx = &mut vcx;
    assert!(app.read_with(vcx, |app, _| app.composer_ui.has_reply()));
    app.update(vcx, |app, _| app.demo_delete_message(CHAT, REPLIED));
    frames(vcx);
    assert!(
        app.read_with(vcx, |app, _| app.composer_ui.reply().is_none()),
        "the bar goes with the message"
    );
}

#[gpui_kit::test]
fn deleting_the_edited_message_cancels_the_edit(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-edit-delete");
    let vcx = &mut vcx;
    assert!(app.read_with(vcx, |app, _| app.composer_ui.pending_edit.is_some()));
    app.update(vcx, |app, _| app.demo_delete_message(CHAT, MessageId(102)));
    frames(vcx);
    assert!(
        app.read_with(vcx, |app, _| app.composer_ui.pending_edit.is_none()),
        "the edit bar goes with the message"
    );
}

#[gpui_kit::test]
fn a_deleted_selected_message_leaves_the_count_and_the_actions(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-selection");
    let vcx = &mut vcx;
    let ids = |app: &Entity<QuillApp>, vcx: &mut VisualTestContext| {
        app.read_with(vcx, |app, _| {
            app.forward_draft()
                .map(|draft| draft.message_ids)
                .unwrap_or_default()
        })
    };
    assert_eq!(ids(&app, vcx), vec![MessageId(101), MessageId(102)]);
    assert_eq!(
        app.read_with(vcx, |app, _| app.selection_shown_count(CHAT)),
        2
    );

    app.update(vcx, |app, _| app.demo_delete_message(CHAT, MessageId(102)));
    frames(vcx);
    assert_eq!(
        ids(&app, vcx),
        vec![MessageId(101)],
        "forward sends only the survivor"
    );
    assert_eq!(
        app.read_with(vcx, |app, _| app.selection_shown_count(CHAT)),
        1,
        "the bar counts the survivor"
    );
    assert!(app.read_with(vcx, |app, _| app.selecting_in(CHAT)));
    assert!(
        !app.read_with(vcx, |app, _| app.selection_shown(CHAT, MessageId(102))),
        "the deleted id is not drawn selected"
    );

    app.update(vcx, |app, _| app.demo_delete_message(CHAT, REPLIED));
    frames(vcx);
    assert!(ids(&app, vcx).is_empty());
    assert!(
        !app.read_with(vcx, |app, _| app.selecting_in(CHAT)),
        "no selected message left: selection mode ends"
    );
}

#[gpui_kit::test]
fn a_toggle_after_a_delete_does_not_revive_the_deleted_id(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-selection");
    let vcx = &mut vcx;
    app.update(vcx, |app, _| app.demo_delete_message(CHAT, MessageId(102)));
    app.update(vcx, |app, cx| {
        app.toggle_forward_select(CHAT, REPLIED, false, cx);
        app.toggle_forward_select(CHAT, MessageId(103), false, cx);
    });
    let ids = app.read_with(vcx, |app, _| {
        app.share
            .pending_forward
            .as_ref()
            .map(|draft| draft.message_ids.clone())
            .unwrap_or_default()
    });
    assert_eq!(ids, vec![MessageId(103)], "the stored set holds no ghost");
}

fn read_and_unread_chats(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> (ChatId, ChatId) {
    let (read, unread) = app.read_with(vcx, |app, _| {
        let session = app.session().expect("demo session");
        let listed = session.ordered_chats();
        let read = listed.iter().find(|c| !c.is_unread()).map(|c| c.id);
        let unread = listed.iter().find(|c| c.is_unread()).map(|c| c.id);
        (read, unread)
    });
    (
        read.expect("the demo lists a read chat"),
        unread.expect("the demo lists an unread chat"),
    )
}

#[gpui_kit::test]
fn changing_the_filter_or_folder_ends_the_chat_selection(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    let (read, unread) = read_and_unread_chats(&app, vcx);
    app.update(vcx, |app, cx| {
        app.enter_select_mode(read, cx);
        app.toggle_chat_selected(unread, cx);
    });
    frames(vcx);
    assert_eq!(
        app.read_with(vcx, |app, _| app.selected_chat_ids().len()),
        2,
        "both chats are listed on All"
    );

    // Unread, then back to All: the checks do not come back.
    app.update(vcx, |app, cx| {
        app.set_chat_list_view(ChatListFilter::Unread, None);
        cx.notify();
    });
    frames(vcx);
    assert!(!app.read_with(vcx, |app, _| app.selecting_chats()));
    app.update(vcx, |app, cx| {
        app.set_chat_list_view(ChatListFilter::All, None);
        cx.notify();
    });
    frames(vcx);
    assert!(
        !app.read_with(vcx, |app, _| app.selecting_chats()),
        "returning to All does not re-enter select mode"
    );
    assert!(app.read_with(vcx, |app, _| app.chat_list.selected.is_empty()));

    // The same view again keeps the checks.
    app.update(vcx, |app, cx| app.enter_select_mode(read, cx));
    app.update(vcx, |app, _| {
        app.set_chat_list_view(ChatListFilter::All, None)
    });
    assert!(app.read_with(vcx, |app, _| app.selecting_chats()));

    // A folder change ends it too.
    app.update(vcx, |app, _| {
        app.set_chat_list_view(ChatListFilter::All, Some(1))
    });
    assert!(!app.read_with(vcx, |app, _| app.selecting_chats()));
}

#[gpui_kit::test]
fn a_chat_that_leaves_the_list_drops_out_of_the_bulk_selection(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    let (read, unread) = read_and_unread_chats(&app, vcx);
    app.update(vcx, |app, cx| {
        app.enter_select_mode(read, cx);
        app.toggle_chat_selected(unread, cx);
        // A chat id that is not listed (archived or left) is not counted.
        app.chat_list.selected.insert(9_999);
    });
    frames(vcx);
    assert_eq!(
        app.read_with(vcx, |app, _| app.selected_chat_ids().len()),
        2
    );
    assert!(app.read_with(vcx, |app, _| app.selecting_chats()));
    let rows = app.read_with(vcx, |app, _| app.rows_selecting());
    assert!(rows, "the drawn rows agree with the count");
    // Unchecking the last listed one ends the mode and prunes the ghost.
    app.update(vcx, |app, cx| {
        app.toggle_chat_selected(read, cx);
        app.toggle_chat_selected(unread, cx);
    });
    assert!(!app.read_with(vcx, |app, _| app.selecting_chats()));
    assert!(app.read_with(vcx, |app, _| app.chat_list.selected.is_empty()));
}

fn history_len(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> usize {
    app.read_with(vcx, |app, _| {
        app.session()
            .and_then(|s| s.histories.get(&CHAT.0))
            .map_or(0, |h| h.messages.len())
    })
}

#[gpui_kit::test]
fn sending_drops_a_reply_to_a_message_deleted_since_the_last_frame(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-reply");
    let vcx = &mut vcx;
    assert!(app.read_with(vcx, |app, _| app.composer_ui.has_reply()));
    // Deleted, and no frame ran: the bar still holds the reply.
    app.update(vcx, |app, _| app.demo_delete_message(CHAT, REPLIED));
    assert!(app.read_with(vcx, |app, _| app.composer_ui.has_reply()));
    let ids = |app: &Entity<QuillApp>, vcx: &mut VisualTestContext| {
        app.read_with(vcx, |app, _| {
            app.session()
                .and_then(|s| s.histories.get(&CHAT.0))
                .map(|h| h.messages.keys().copied().collect::<Vec<_>>())
                .unwrap_or_default()
        })
    };
    let before = ids(&app, vcx);
    vcx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.submit_composer("sounds good".into(), window, cx)
        })
    });
    assert!(
        app.read_with(vcx, |app, _| app.composer_ui.reply().is_none()),
        "the reply is dropped at send time"
    );
    let replies_to_deleted = app.read_with(vcx, |app, _| {
        app.session()
            .and_then(|s| s.histories.get(&CHAT.0))
            .map(|h| {
                h.messages
                    .iter()
                    .filter(|(id, _)| !before.contains(id))
                    .filter(|(_, m)| m.reply_to.as_ref().is_some_and(|r| r.message_id == REPLIED))
                    .count()
            })
            .unwrap_or_default()
    });
    assert_eq!(
        replies_to_deleted, 0,
        "nothing new replies to the deleted id"
    );
    assert!(
        ids(&app, vcx).len() > before.len(),
        "the text was still sent"
    );
}

#[gpui_kit::test]
fn submitting_an_edit_of_a_deleted_message_sends_nothing(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-edit-delete");
    let vcx = &mut vcx;
    assert!(app.read_with(vcx, |app, _| app.composer_ui.pending_edit.is_some()));
    app.update(vcx, |app, _| app.demo_delete_message(CHAT, MessageId(102)));
    let before = history_len(&app, vcx);
    vcx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.submit_composer("new words".into(), window, cx)
        })
    });
    assert!(app.read_with(vcx, |app, _| app.composer_ui.pending_edit.is_none()));
    assert_eq!(history_len(&app, vcx), before, "no edit and no new message");
}

#[gpui_kit::test]
fn an_edit_of_a_message_that_is_not_loaded_does_not_begin(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    let unloaded = ComposerEdit::from_own_content(
        CHAT,
        MessageId(424_242),
        true,
        false,
        &MessageContent::Text("never loaded".into()),
    )
    .expect("text is editable");
    vcx.update(|window, cx| app.update(cx, |app, cx| app.begin_edit(unloaded, window, cx)));
    assert!(
        app.read_with(vcx, |app, _| app.composer_ui.pending_edit.is_none()),
        "no edit starts on an empty field"
    );
    assert_eq!(
        app.read_with(vcx, |app, _| app.connection.status_note().to_string()),
        quill::lang::tr(quill::lang::keys::EDIT_ERROR),
        "the refusal says so (tdesktop lng_edit_error)"
    );
}
