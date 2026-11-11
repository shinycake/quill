//! A switch ends what acts for the account on screen: the behaviour of
//! the boxes, the viewer and the chat leave. That every screen field is
//! gone is `on_screen_tests.rs`.

use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::{Root, WindowExt};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::composer::DeleteConfirm;
use quill::ids::{AccountKey, ChatId, MessageId};

const CHAT: ChatId = ChatId(11);

fn new_app(cx: &mut TestAppContext, demo: &'static str) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx
            .new(|cx| QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named(demo))));
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let app = slot.borrow().clone().expect("the app view");
    (app, VisualTestContext::from_window(handle.into(), cx))
}

fn frames(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    for _ in 0..2 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

fn switch(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    app.update_in(vcx, |app, window, cx| {
        app.switch_account(&AccountKey("other".into()), "", window, cx);
    });
}

fn dialog_up(vcx: &mut VisualTestContext) -> bool {
    vcx.update(|window, cx| window.has_active_dialog(cx))
}

/// The delete box holds the chat and message ids of the account it opened
/// on. Message ids are numbered per account, so its Delete pressed after a
/// switch would delete another message on the new account.
#[gpui_kit::test]
fn a_delete_box_open_across_a_switch_closes(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    frames(&mut vcx);
    let message_id = app.read_with(&vcx, |app, _| {
        let history = app.session().and_then(|s| s.histories.get(&CHAT.0));
        *history
            .and_then(|h| h.messages.keys().next_back())
            .expect("a message")
    });
    app.update_in(&mut vcx, |app, window, cx| {
        let confirm = DeleteConfirm {
            chat_id: CHAT,
            message_id: MessageId(message_id),
            revoke: true,
            can_revoke: true,
        };
        app.open_delete_dialog(confirm, window, cx);
    });
    frames(&mut vcx);
    assert!(dialog_up(&mut vcx));

    switch(&app, &mut vcx);
    assert!(!dialog_up(&mut vcx), "closed before the next frame");
    frames(&mut vcx);
    assert!(!dialog_up(&mut vcx));
    app.read_with(&vcx, |app, _| {
        assert!(app.chat_ui.pending_delete.is_none());
        let kept = app
            .session()
            .and_then(|s| s.histories.get(&CHAT.0))
            .is_some_and(|h| h.messages.contains_key(&message_id));
        assert!(kept, "nothing was deleted");
    });
}

/// The media viewer's Delete and Forward act on the item it shows.
#[gpui_kit::test]
fn a_switch_closes_the_media_viewer(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-video-inline");
    app.update_in(&mut vcx, |app, _, cx| {
        app.open_media_viewer(CHAT, MessageId(204), cx);
    });
    frames(&mut vcx);
    assert!(app.read_with(&vcx, |app, _| app.viewer.state.is_open()));
    switch(&app, &mut vcx);
    assert!(app.read_with(&vcx, |app, _| !app.viewer.state.is_open()));
}

/// A flagged dialog with a close of its own (the checkout) closes through
/// it, and the shell shows nothing stale after the switch.
#[gpui_kit::test]
fn a_switch_closes_the_checkout(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    frames(&mut vcx);
    app.update_in(&mut vcx, |app, window, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session.payments.form_loading = true;
        }
        app.open_payment_dialog(window, cx);
    });
    frames(&mut vcx);
    assert!(dialog_up(&mut vcx));
    switch(&app, &mut vcx);
    frames(&mut vcx);
    assert!(!dialog_up(&mut vcx));
    app.read_with(&vcx, |app, _| {
        assert!(QuillShell::open_dialog_names(app).is_empty());
    });
}

/// The switch leaves the open chat as any leave does: the typed text is
/// the old account's draft, and the composer the new account shows is
/// empty, so it cannot send the old account's text.
#[gpui_kit::test]
fn a_switch_saves_the_typed_text_and_empties_the_composer(cx: &mut TestAppContext) {
    use quill::composer::DraftTopic;
    let (app, mut vcx) = new_app(cx, "ready-chats");
    app.update_in(&mut vcx, |app, window, cx| {
        app.select_listed_chat(CHAT, window, cx);
    });
    frames(&mut vcx);
    app.update_in(&mut vcx, |app, window, cx| {
        app.set_composer_markup("meet at 5", window, cx);
        app.note_open_draft(true, cx);
    });
    switch(&app, &mut vcx);
    frames(&mut vcx);
    app.read_with(&vcx, |app, cx| {
        assert_eq!(app.composer_markup(cx), "", "nothing left to send");
        assert_eq!(app.open_chat_id(), None);
        let session = app.session().expect("session");
        let draft = session.draft_for(CHAT, DraftTopic::Chat).expect("saved");
        assert_eq!(draft.text, "meet at 5");
        assert!(!session.draft_is_dirty(CHAT, DraftTopic::Chat));
    });
}

/// The round-4 table: boxes that keep the old account's ids and act
/// through `self.live`. Each one's flag sits in a struct the screen owns,
/// so the switch closes them all: none reopens over the new account to
/// delete, leave, terminate, block or call there.
#[gpui_kit::test]
fn a_switch_closes_boxes_holding_the_old_accounts_ids(cx: &mut TestAppContext) {
    use crate::ui::chat_bars::BlockBarDialog;
    use crate::ui::dialogs::confirm::SessionsConfirm;
    use crate::ui::dialogs::folder::FolderDeleteConfirm;
    use crate::ui::dialogs::group_confirm::GroupConfirmAction;
    use crate::ui::on_screen::OnScreen;

    struct HeldBox {
        dialog: &'static str,
        /// The `OnScreen` field whose struct holds the flag.
        field: &'static str,
        open: fn(&QuillApp) -> bool,
    }
    let boxes = [
        HeldBox {
            dialog: "GroupConfirm",
            field: "admin",
            open: |app| app.admin.group_confirm_dialog.is_some(),
        },
        HeldBox {
            dialog: "FolderDelete",
            field: "folders",
            open: |app| app.folders.delete_confirm.is_some(),
        },
        HeldBox {
            dialog: "SessionsConfirm",
            field: "privacy",
            open: |app| app.privacy.sessions_confirm.is_some(),
        },
        HeldBox {
            dialog: "ClearCalls",
            field: "chat_list",
            open: |app| app.chat_list.global.clear_calls_open,
        },
        HeldBox {
            dialog: "BlockBar",
            field: "dialogs",
            open: |app| app.dialogs.block_bar_dialog.is_some(),
        },
        HeldBox {
            dialog: "CallConfirm",
            field: "call_boxes",
            open: |app| app.call_boxes.confirm.is_some(),
        },
        HeldBox {
            dialog: "Translate",
            field: "translate",
            open: |app| app.translate.dialog.is_some(),
        },
    ];

    let (app, mut vcx) = new_app(cx, "ready-chats");
    frames(&mut vcx);
    app.update_in(&mut vcx, |app, _, cx| {
        app.open_group_confirm(CHAT, GroupConfirmAction::DeleteChat, cx);
        app.folders.delete_confirm = Some(FolderDeleteConfirm {
            folder_id: 2,
            name: "Work".into(),
            has_links: false,
            shared: false,
            keep: Default::default(),
        });
        app.privacy.sessions_confirm = Some(SessionsConfirm::TerminateAll);
        app.chat_list.global.clear_calls_open = true;
        app.dialogs.block_bar_dialog = Some(BlockBarDialog {
            chat_id: CHAT,
            user_id: 5,
            report: false,
            delete_chat: false,
        });
        app.call_boxes.confirm = Some((7, false));
        cx.notify();
    });
    app.update_in(&mut vcx, |app, window, cx| {
        app.open_translate_selection(CHAT, "hola".into(), window, cx);
    });
    frames(&mut vcx);
    assert!(dialog_up(&mut vcx));
    app.read_with(&vcx, |app, _| {
        let open = QuillShell::open_dialog_names(app);
        for held in &boxes {
            assert!(
                (held.open)(app),
                "{} flagged before the switch",
                held.dialog
            );
            assert!(
                open.contains(&held.dialog),
                "{} open before the switch",
                held.dialog
            );
            assert!(
                OnScreen::FIELD_NAMES.contains(&held.field),
                "{}'s flag lives in `{}`, which is not on the screen",
                held.dialog,
                held.field
            );
        }
    });

    switch(&app, &mut vcx);
    frames(&mut vcx);
    assert!(!dialog_up(&mut vcx));
    app.read_with(&vcx, |app, _| {
        let open = QuillShell::open_dialog_names(app);
        assert!(open.is_empty(), "still open: {open:?}");
        for held in &boxes {
            assert!(
                !(held.open)(app),
                "{} flagged after the switch",
                held.dialog
            );
        }
    });
}

/// An app-wide dialog's flag sits in a struct the app keeps, so the switch
/// leaves it: the emoji set style box is open for the next account too.
#[gpui_kit::test]
fn an_app_wide_dialogs_flag_survives_a_switch(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    frames(&mut vcx);
    app.update(&mut vcx, |app, cx| {
        app.preferences.emoji_style.open = true;
        cx.notify();
    });
    frames(&mut vcx);
    app.read_with(&vcx, |app, _| {
        assert!(QuillShell::open_dialog_names(app).contains(&"EmojiSetStyle"));
    });
    switch(&app, &mut vcx);
    frames(&mut vcx);
    app.read_with(&vcx, |app, _| {
        assert!(app.preferences.emoji_style.open, "the flag is the app's");
        assert!(QuillShell::open_dialog_names(app).contains(&"EmojiSetStyle"));
    });
}

/// Files handed to Quill from outside wait in the app's own `start_files`,
/// so a switch leaves them: the chooser is up for the next account too
/// (sweep s4-payments-2, 13-037).
#[gpui_kit::test]
fn files_waiting_for_a_chat_survive_a_switch(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    frames(&mut vcx);
    let file = quill::scratch::process_dir("quill-start-switch").join("file.txt");
    let _ = std::fs::write(&file, b"hello");
    app.update(&mut vcx, |app, cx| {
        app.begin_start_files(vec![file.clone()], cx);
    });
    frames(&mut vcx);
    app.read_with(&vcx, |app, _| {
        assert!(QuillShell::open_dialog_names(app).contains(&"StartFiles"));
    });
    switch(&app, &mut vcx);
    frames(&mut vcx);
    app.read_with(&vcx, |app, _| {
        assert_eq!(
            app.start_files.choosing,
            vec![file.clone()],
            "the files wait"
        );
        assert!(QuillShell::open_dialog_names(app).contains(&"StartFiles"));
    });
}

/// "Reply in another chat" shows only while the composer has a reply, and
/// the chat leave clears it: the box still closes, so the new account's
/// next Reply does not open the chooser.
#[gpui_kit::test]
fn a_switch_closes_reply_in_another_chat(cx: &mut TestAppContext) {
    use quill::composer::ComposerReplyTo;
    let (app, mut vcx) = new_app(cx, "ready-chats");
    app.update_in(&mut vcx, |app, window, cx| {
        app.select_listed_chat(CHAT, window, cx);
    });
    frames(&mut vcx);
    app.update(&mut vcx, |app, cx| {
        let message = app
            .session()
            .and_then(|s| s.histories.get(&CHAT.0))
            .and_then(|h| h.messages.keys().next_back().copied())
            .expect("a message");
        app.chat_ui
            .set_reply(Some(ComposerReplyTo::new(CHAT, MessageId(message))));
        app.share.forward.open_reply_chooser();
        cx.notify();
    });
    frames(&mut vcx);
    app.read_with(&vcx, |app, _| {
        assert!(QuillShell::open_dialog_names(app).contains(&"ReplyElsewhere"));
    });
    switch(&app, &mut vcx);
    app.read_with(&vcx, |app, _| {
        assert!(!app.share.forward.reply_chooser_open());
        assert!(!app.reply_quote_open());
    });
}

/// A dialog whose gate is false while another shows (the giveaway form
/// hides behind its own checkout) still closes once that one has, so it
/// does not come back over the new account.
#[gpui_kit::test]
fn a_switch_closes_a_dialog_hidden_behind_another(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    frames(&mut vcx);
    app.update_in(&mut vcx, |app, window, cx| {
        app.open_giveaway_form(CHAT, None, window, cx);
        if let Some(session) = app.demo_session.as_mut() {
            session.payments.form_loading = true;
        }
        app.open_payment_dialog(window, cx);
    });
    frames(&mut vcx);
    app.read_with(&vcx, |app, _| {
        let open = QuillShell::open_dialog_names(app);
        assert!(!open.contains(&"Giveaway"), "hidden behind the checkout");
        assert!(app.giveaway.form.is_some());
    });
    switch(&app, &mut vcx);
    frames(&mut vcx);
    assert!(!dialog_up(&mut vcx));
    app.read_with(&vcx, |app, _| {
        assert!(app.giveaway.form.is_none());
        assert!(QuillShell::open_dialog_names(app).is_empty());
    });
}

#[path = "on_screen_session_box_tests.rs"]
mod session_box_tests;
