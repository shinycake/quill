//! A switch ends what acts for the account on screen.

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
        assert!(app.message_ui.pending_delete.is_none());
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

/// Boxes that keep the old account's ids in app-side state and act through
/// `self.live` all close on a switch: none reopens over the new account to
/// delete, leave, terminate or call there.
#[gpui_kit::test]
fn a_switch_closes_boxes_holding_the_old_accounts_ids(cx: &mut TestAppContext) {
    use crate::ui::dialogs::confirm::SessionsConfirm;
    use crate::ui::dialogs::folder::FolderDeleteConfirm;
    use crate::ui::dialogs::group_confirm::GroupConfirmAction;

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
        app.calls.confirm = Some((7, false));
        cx.notify();
    });
    frames(&mut vcx);
    assert!(dialog_up(&mut vcx));
    let opened = [
        "GroupConfirm",
        "FolderDelete",
        "SessionsConfirm",
        "ClearCalls",
        "CallConfirm",
    ];
    app.read_with(&vcx, |app, _| {
        let open = QuillShell::open_dialog_names(app);
        for name in opened {
            assert!(open.contains(&name), "{name} open before the switch");
        }
    });

    switch(&app, &mut vcx);
    frames(&mut vcx);
    assert!(!dialog_up(&mut vcx));
    app.read_with(&vcx, |app, _| {
        let open = QuillShell::open_dialog_names(app);
        assert!(open.is_empty(), "still open: {open:?}");
        assert!(app.admin.group_confirm_dialog.is_none());
        assert!(app.folders.delete_confirm.is_none());
        assert!(app.privacy.sessions_confirm.is_none());
        assert!(!app.chat_list.global.clear_calls_open);
        assert!(app.calls.confirm.is_none());
    });
}

/// Every dialog says what a switch does to it (the registry will not take
/// one that does not), and only these few stay open across one. A new
/// app-wide dialog joins this list on purpose.
#[test]
fn only_app_wide_dialogs_stay_open_across_a_switch() {
    use crate::ui::shell::OnSwitch;

    let mut kept: Vec<_> = QuillShell::dialog_switch_scopes()
        .into_iter()
        .filter(|(_, on_switch)| *on_switch == OnSwitch::AppWide)
        .map(|(name, _)| name)
        .collect();
    kept.sort_unstable();
    assert_eq!(
        kept,
        ["BackgroundCall", "EmojiSetStyle", "Passcode", "ThemeEditor"]
    );
}
