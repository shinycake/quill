//! A switch replaces the account on screen whole: one of each surface kind
//! goes, the epoch moves, and nothing comes back on the way back.

use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::{Root, WindowExt};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, point, px, size};
use quill::composer::DeleteConfirm;
use quill::ids::{AccountKey, ChatId, MessageId};
use std::cell::Cell;
use std::rc::Rc;

const CHAT: ChatId = ChatId(11);

fn new_app(cx: &mut TestAppContext, demo: &'static str) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    let slot = Rc::new(std::cell::RefCell::new(None));
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

fn switch(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, to: &str) {
    app.update_in(vcx, |app, window, cx| {
        app.switch_account(&AccountKey(to.into()), "", window, cx);
    });
    vcx.run_until_parked();
}

fn dialog_up(vcx: &mut VisualTestContext) -> bool {
    vcx.update(|window, cx| window.has_active_dialog(cx))
}

/// What the screen shows of each surface kind, read in one place so the
/// before and after assertions cannot drift apart. The delete box is the
/// kit's dialog (`dialog_up`): it has no flag of its own until OK.
struct Surfaces {
    rich_editor: bool,
    context_menu: bool,
    selection: usize,
    player_chat: Option<ChatId>,
}

fn surfaces(app: &QuillApp) -> Surfaces {
    Surfaces {
        rich_editor: app.chat_ui.rich_editor_open,
        context_menu: app.chat_list.archive_menu.is_some(),
        selection: app.chat_list.selected.len(),
        player_chat: app.playback.player.chat,
    }
}

fn assert_clear(s: &Surfaces) {
    assert!(!s.rich_editor, "rich editor");
    assert!(!s.context_menu, "context menu");
    assert_eq!(s.selection, 0, "chat list selection");
    assert_eq!(s.player_chat, None, "player");
}

/// Open one surface of each kind: a kit dialog, a composer mode (the rich
/// editor, an Escape-stack layer), a context menu, a bulk selection and the
/// player bar. One dialog only: the shell shows one at a time, and a second
/// (the story composer, say) would close the first. Nothing the session
/// holds (the search's `open`): that follows the account, not the screen.
fn open_everything(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    let message_id = app.read_with(vcx, |app, _| {
        let history = app.session().and_then(|s| s.histories.get(&CHAT.0));
        *history
            .and_then(|h| h.messages.keys().next_back())
            .expect("a message")
    });
    // The delete box is the open chat's: another chat's would close it.
    app.update_in(vcx, |app, window, cx| {
        app.select_listed_chat(CHAT, window, cx);
    });
    frames(vcx);
    app.update(vcx, |app, cx| {
        app.chat_ui.rich_editor_open = true;
        app.chat_list.archive_menu = Some(point(px(10.), px(10.)));
        app.chat_list.selected.insert(CHAT.0);
        app.playback.playing_audio = Some(MessageId(1));
        app.playback.player.chat = Some(CHAT);
        cx.notify();
    });
    frames(vcx);
    // The dialog last: it is drawn over the rest, and closes nothing.
    app.update_in(vcx, |app, window, cx| {
        app.open_delete_dialog(
            DeleteConfirm {
                chat_id: CHAT,
                message_id: MessageId(message_id),
                revoke: true,
                can_revoke: true,
            },
            window,
            cx,
        );
    });
    frames(vcx);
}

#[gpui_kit::test]
fn a_switch_replaces_every_surface_and_nothing_comes_back(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    frames(&mut vcx);
    let first = app.read_with(&vcx, |app, _| app.screen_epoch());
    open_everything(&app, &mut vcx);
    assert!(dialog_up(&mut vcx));
    app.read_with(&vcx, |app, _| {
        let s = surfaces(app);
        assert!(s.rich_editor, "rich editor open");
        assert!(s.context_menu, "context menu open");
        assert_eq!(s.selection, 1);
        assert_eq!(s.player_chat, Some(CHAT));
    });

    // A task of the old screen, waiting.
    let (go, wait) = futures_channel::oneshot::channel::<()>();
    let outcome = Rc::new(Cell::new(None));
    let seen = outcome.clone();
    app.update(&mut vcx, |app, cx| {
        app.spawn_screen(cx, async move |this, cx| {
            let _ = wait.await;
            seen.set(Some(
                this.update(cx, |app, _| app.stories.composer.open = true)
                    .is_ok(),
            ));
        })
        .detach();
    });

    switch(&app, &mut vcx, "other");
    assert!(!dialog_up(&mut vcx), "closed before the next frame");
    frames(&mut vcx);
    assert!(!dialog_up(&mut vcx));
    let second = app.read_with(&vcx, |app, _| {
        assert_clear(&surfaces(app));
        assert!(QuillShell::open_dialog_names(app).is_empty());
        assert!(!app.context_menu_open());
        app.screen_epoch()
    });
    assert_ne!(first, second);
    let _ = go.send(());
    vcx.run_until_parked();
    assert_eq!(
        outcome.get(),
        Some(false),
        "the task's update came back Gone"
    );
    app.read_with(&vcx, |app, _| assert!(!app.stories.composer.open));

    // Back to the first account: a third screen, with nothing restored.
    switch(&app, &mut vcx, &AccountKey::primary().0);
    frames(&mut vcx);
    let third = app.read_with(&vcx, |app, _| {
        assert_clear(&surfaces(app));
        app.screen_epoch()
    });
    assert_ne!(third, first);
    assert_ne!(third, second);
}

/// The chat leave is the history's farewell: the typed text is the old
/// account's draft, saved through its own client, and the new screen's
/// composer is empty.
#[gpui_kit::test]
fn the_chat_leave_saves_the_draft_through_the_old_client(cx: &mut TestAppContext) {
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
    switch(&app, &mut vcx, "other");
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

/// The screen is one value: a field written on the old one is not on the
/// new one, whatever it is. Checked on a field no list ever named.
#[gpui_kit::test]
fn a_field_no_list_named_goes_too(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    frames(&mut vcx);
    app.update(&mut vcx, |app, _| {
        app.set_media_panel_mode(crate::ui::media_panel::PanelMode::Compose);
        app.folders.tab_menu = Some(crate::ui::folder_extras::FolderTabMenu {
            folder_id: Some(2),
            position: point(px(10.), px(10.)),
        });
    });
    switch(&app, &mut vcx, "other");
    app.read_with(&vcx, |app, _| {
        assert!(!app.pickers.media_panel.is_open());
        assert!(app.folders.tab_menu.is_none());
    });
}

/// Round 16 (area B, finding 3): the link flow an account was running
/// lives in its session, which stays alive in the background. It ends with
/// the screen, so a Join box left unanswered (or a chat still resolving)
/// does not pop up on the switch back.
#[gpui_kit::test]
fn a_switch_ends_the_link_flow_the_account_was_running(cx: &mut TestAppContext) {
    use quill::state::DeepLinkState;
    let (app, mut vcx) = new_app(cx, "ready-chats");
    frames(&mut vcx);
    app.update(&mut vcx, |app, _| {
        let session = app.demo_session.as_mut().expect("a demo session");
        session.chats_state.deep_link = Some(DeepLinkState::InvitePreview {
            hash: "AbCd".into(),
            title: "Book club".into(),
            member_count: 12,
            creates_join_request: false,
            is_channel: false,
            generation: 3,
        });
        session.chats_state.deep_link_from_agent = true;
    });
    switch(&app, &mut vcx, "other");
    frames(&mut vcx);
    app.read_with(&vcx, |app, _| {
        let session = app.session().expect("a session");
        assert!(session.chats_state.deep_link.is_none(), "the flow ended");
        assert!(!session.chats_state.deep_link_from_agent);
        assert!(app.links.deep_link_invite.is_none());
    });
    switch(&app, &mut vcx, &AccountKey::primary().0);
    frames(&mut vcx);
    app.read_with(&vcx, |app, _| {
        assert!(
            app.links.deep_link_invite.is_none(),
            "no Join box came back"
        );
        assert!(app.links.pending_deep_link_open.is_none());
    });
}
