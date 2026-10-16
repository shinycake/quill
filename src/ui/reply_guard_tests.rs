//! Every sender takes the composer's reply through `reply_for_send`, so a
//! reply to a message deleted since the last frame is dropped and the
//! content still goes out. One test per kind of sender (`ComposerUi` keeps
//! the reply private, so a sender cannot read it any other way).
#![cfg(test)]
#![cfg(feature = "demo-capture")]

use crate::ui::ChecklistDialog;
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, Focusable, TestAppContext, VisualTestContext, px, size};
use quill::ids::{ChatId, FileId, MessageId};

const CHAT: ChatId = ChatId(11);
const REPLIED: MessageId = MessageId(101);

/// The demo with a reply bar on `REPLIED`, which is then deleted without a
/// frame in between (the bar still holds the reply).
fn app_with_a_deleted_reply(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named("ready-reply");
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
    vcx.update(|window, cx| {
        for _ in 0..2 {
            window.render_frame(cx);
        }
    });
    vcx.run_until_parked();
    assert!(app.read_with(&vcx, |app, _| app.composer_ui.has_reply()));
    app.update(&mut vcx, |app, _| app.demo_delete_message(CHAT, REPLIED));
    assert!(
        app.read_with(&vcx, |app, _| app.composer_ui.has_reply()),
        "no frame ran: the bar still holds the reply"
    );
    (app, vcx)
}

fn reply_is_gone(app: &Entity<QuillApp>, vcx: &VisualTestContext) -> bool {
    app.read_with(vcx, |app, _| !app.composer_ui.has_reply())
}

fn rows(app: &Entity<QuillApp>, vcx: &VisualTestContext) -> Vec<i64> {
    app.read_with(vcx, |app, _| {
        app.session()
            .and_then(|s| s.histories.get(&CHAT.0))
            .map(|h| h.messages.keys().copied().collect())
            .unwrap_or_default()
    })
}

/// How many rows added since `before` reply to the deleted message.
fn new_rows_replying_to_the_deleted(
    app: &Entity<QuillApp>,
    vcx: &VisualTestContext,
    before: &[i64],
) -> usize {
    app.read_with(vcx, |app, _| {
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
    })
}

#[gpui_kit::test]
fn a_gif_drops_a_reply_to_a_deleted_message_and_is_still_sent(cx: &mut TestAppContext) {
    let (app, mut vcx) = app_with_a_deleted_reply(cx);
    let before = rows(&app, &vcx);
    app.update(&mut vcx, |app, cx| {
        app.send_gif_pick(FileId(7), 2, 120, 90, cx)
    });
    assert!(reply_is_gone(&app, &vcx));
    assert!(rows(&app, &vcx).len() > before.len(), "the GIF was sent");
    assert_eq!(new_rows_replying_to_the_deleted(&app, &vcx, &before), 0);
}

#[gpui_kit::test]
fn a_sticker_drops_a_reply_to_a_deleted_message_and_is_still_sent(cx: &mut TestAppContext) {
    let (app, mut vcx) = app_with_a_deleted_reply(cx);
    let before = rows(&app, &vcx);
    app.update(&mut vcx, |app, cx| {
        app.send_sticker_pick(FileId(8), "x".into(), 100, 100, None, cx)
    });
    assert!(reply_is_gone(&app, &vcx));
    assert!(
        rows(&app, &vcx).len() > before.len(),
        "the sticker was sent"
    );
    assert_eq!(new_rows_replying_to_the_deleted(&app, &vcx, &before), 0);
}

#[gpui_kit::test]
fn shared_content_drops_a_reply_to_a_deleted_message(cx: &mut TestAppContext) {
    // Contact, location, dice and venue all take the reply through
    // `send_reply_for`; the dice roll stands for them. The demo has no live
    // driver, so the check is on what the sender read.
    let (app, mut vcx) = app_with_a_deleted_reply(cx);
    let kept = app.update(&mut vcx, |app, cx| {
        app.roll_dice("🎲", cx);
        app.composer_ui.has_reply()
    });
    assert!(!kept, "the share sender dropped the reply itself");
}

#[gpui_kit::test]
fn a_poll_drops_a_reply_to_a_deleted_message(cx: &mut TestAppContext) {
    let (app, mut vcx) = app_with_a_deleted_reply(cx);
    // Read inside the update: the next frame would cancel it anyway.
    let kept = vcx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.open_poll_dialog(window, cx);
            if let Some(dialog) = app.composer_ui.poll_dialog.as_ref() {
                dialog
                    .question_input
                    .update(cx, |input, cx| input.set_value("Lunch?", window, cx));
                for (input, text) in dialog.option_inputs.iter().zip(["Tacos", "Soup"]) {
                    input.update(cx, |input, cx| input.set_value(text, window, cx));
                }
            }
            app.submit_poll_dialog(window, cx);
            app.composer_ui.has_reply()
        })
    });
    assert!(!kept, "the poll sender dropped the reply itself");
}

#[gpui_kit::test]
fn a_checklist_drops_a_reply_to_a_deleted_message(cx: &mut TestAppContext) {
    let (app, mut vcx) = app_with_a_deleted_reply(cx);
    let kept = vcx.update(|window, cx| {
        app.update(cx, |app, cx| {
            let dialog = ChecklistDialog::new(None, window, cx);
            dialog
                .title_input
                .update(cx, |input, cx| input.set_value("Trip", window, cx));
            if let Some(task) = dialog.task_inputs.first() {
                task.update(cx, |input, cx| input.set_value("Pack", window, cx));
            }
            app.composer_ui.checklist_dialog = Some(dialog);
            app.submit_checklist_dialog(cx);
            app.composer_ui.has_reply()
        })
    });
    assert!(!kept, "the checklist sender dropped the reply itself");
}

#[gpui_kit::test]
fn a_reply_to_a_message_that_is_there_passes_through(cx: &mut TestAppContext) {
    let (app, mut vcx) = app_with_a_deleted_reply(cx);
    app.update(&mut vcx, |app, cx| {
        app.composer_ui
            .set_reply(Some(quill::composer::ComposerReplyTo::new(
                CHAT,
                MessageId(102),
            )));
        let reply = app.reply_for_send(cx);
        assert_eq!(reply.map(|r| r.message_id), Some(MessageId(102)));
    });
    assert!(!reply_is_gone(&app, &vcx));
}
