use crate::ui::app::QuillApp;
use crate::ui::dialogs::group_confirm::GroupConfirmAction;
use crate::ui::keybindings::bind_keys;
use crate::ui::menu_states::{ChatMenuState, ChatMenuSubmenu};
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, point, px, size};
use quill::ids::ChatId;
use quill::telegram::envelope::{
    ChatFolderInfo, ChatKind, NotificationSettingsScope, ScopeNotificationSettings,
};

fn frames(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    for _ in 0..3 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

fn open(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext, ChatId) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named("ready-chats");
    let handle = cx.open_window(size(px(1100.), px(900.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().expect("app");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    frames(&mut vcx);
    let chat_id = app.update(&mut vcx, |app, _| {
        if let Some(session) = app.demo_session.as_ref() {
            app.demo_ui
                .seq
                .store(session.last_seq, std::sync::atomic::Ordering::SeqCst);
        }
        let session = app.session().expect("session");
        let mut ids: Vec<i64> = session
            .chats
            .values()
            .filter(|chat| matches!(chat.kind, ChatKind::Private { .. }))
            .filter(|chat| !session.is_saved_messages(chat.id))
            .map(|chat| chat.id.0)
            .collect();
        ids.sort_unstable();
        ChatId(*ids.last().expect("a private chat"))
    });
    (app, vcx, chat_id)
}

fn show_menu(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, chat_id: ChatId) {
    app.update(vcx, |app, cx| {
        app.chat_list.menu = Some(ChatMenuState::at(chat_id, point(px(120.), px(200.))));
        cx.notify();
    });
    frames(vcx);
    frames(vcx);
}

fn present(vcx: &mut VisualTestContext, id: &'static str) -> bool {
    vcx.update(|window, _| window.try_find(id).is_some())
}

fn click(vcx: &mut VisualTestContext, id: &'static str) {
    vcx.update(|window, cx| window.click(id, cx));
    frames(vcx);
}

#[gpui_kit::test]
fn mute_submenu_applies_the_chosen_duration(cx: &mut TestAppContext) {
    let (app, mut vcx, chat_id) = open(cx);
    let vcx = &mut vcx;
    show_menu(&app, vcx, chat_id);
    assert!(present(vcx, "chat-menu-mute"));
    assert!(!present(vcx, "chat-mute-8h"), "submenu starts folded");
    click(vcx, "chat-menu-mute");
    assert!(present(vcx, "chat-mute-8h"));
    click(vcx, "chat-mute-8h");
    let (muted, closed) = app.update(vcx, |app, _| {
        (
            app.session()
                .and_then(|s| s.chats.get(&chat_id.0))
                .map(|chat| chat.notification_settings.mute_for),
            app.chat_list.menu.is_none(),
        )
    });
    assert_eq!(muted, Some(8 * 3600));
    assert!(closed);
}

#[gpui_kit::test]
fn a_chat_muted_by_the_scope_default_offers_a_direct_unmute(cx: &mut TestAppContext) {
    let (app, mut vcx, chat_id) = open(cx);
    let vcx = &mut vcx;
    // The chat keeps the default; only the private-chat scope is muted.
    app.update(vcx, |app, _| {
        if let Some(session) = app.demo_session.as_mut() {
            let scope = ScopeNotificationSettings {
                mute_for: 3600,
                ..Default::default()
            };
            session
                .settings
                .scope_notification_settings
                .insert(NotificationSettingsScope::PrivateChats, scope);
        }
    });
    let muted_before = app.update(vcx, |app, _| {
        app.session().is_some_and(|s| {
            s.chats
                .get(&chat_id.0)
                .is_some_and(|chat| s.effective_muted(chat))
        })
    });
    assert!(muted_before);
    show_menu(&app, vcx, chat_id);
    click(vcx, "chat-menu-mute");
    // Unmute acts at once: no submenu, the menu closes, and the chat
    // becomes an unmuted exception to the scope default.
    assert!(app.update(vcx, |app, _| app.chat_list.menu.is_none()));
    let muted_after = app.update(vcx, |app, _| {
        app.session().is_some_and(|s| {
            s.chats
                .get(&chat_id.0)
                .is_some_and(|chat| s.effective_muted(chat))
        })
    });
    assert!(!muted_after);
}

#[gpui_kit::test]
fn select_tone_opens_the_panel_for_the_row_chat_without_navigating(cx: &mut TestAppContext) {
    let (app, mut vcx, chat_id) = open(cx);
    let vcx = &mut vcx;
    let open_before = app.update(vcx, |app, _| app.session().and_then(|s| s.open_chat));
    assert_ne!(open_before, Some(chat_id));
    show_menu(&app, vcx, chat_id);
    click(vcx, "chat-menu-mute");
    click(vcx, "chat-mute-tone");
    let (open_after, target, panel, picker) = app.update(vcx, |app, _| {
        (
            app.session().and_then(|s| s.open_chat),
            app.notify.mute_chat,
            app.notify.mute_menu_open,
            app.notify.notif_sound_picker_open,
        )
    });
    assert_eq!(open_after, open_before);
    assert_eq!(target, Some(chat_id));
    assert!(panel && picker);
}

#[gpui_kit::test]
fn a_basic_group_offers_delete_and_leave(cx: &mut TestAppContext) {
    let (app, mut vcx, chat_id) = open(cx);
    let vcx = &mut vcx;
    app.update(vcx, |app, _| {
        if let Some(chat) = app
            .demo_session
            .as_mut()
            .and_then(|s| s.chats.get_mut(&chat_id.0))
        {
            chat.kind = ChatKind::BasicGroup { basic_group_id: 9 };
            chat.can_be_deleted_only_for_self = true;
        }
    });
    show_menu(&app, vcx, chat_id);
    assert!(!present(vcx, "chat-menu-leave"));
    click(vcx, "chat-menu-delete");
    let action = app.update(vcx, |app, _| {
        app.admin
            .group_confirm_dialog
            .as_ref()
            .map(|dialog| matches!(dialog.action, GroupConfirmAction::RemoveFromList))
    });
    assert_eq!(action, Some(true));
}

#[gpui_kit::test]
fn add_to_folder_needs_a_folder_and_lists_them(cx: &mut TestAppContext) {
    let (app, mut vcx, chat_id) = open(cx);
    let vcx = &mut vcx;
    app.update(vcx, |app, _| {
        if let Some(session) = app.demo_session.as_mut() {
            session.chat_list.chat_folders.clear();
        }
    });
    show_menu(&app, vcx, chat_id);
    assert!(!present(vcx, "chat-menu-folder"));
    app.update(vcx, |app, _| {
        if let Some(session) = app.demo_session.as_mut() {
            session.chat_list.chat_folders = vec![ChatFolderInfo {
                id: 7,
                name: "Work".into(),
                icon_name: String::new(),
                color_id: -1,
                is_shareable: false,
                has_my_invite_links: false,
            }];
        }
    });
    show_menu(&app, vcx, chat_id);
    assert!(present(vcx, "chat-menu-folder"));
    click(vcx, "chat-menu-folder");
    let open = app.update(vcx, |app, _| {
        app.chat_list.menu.and_then(|menu| menu.submenu)
    });
    assert_eq!(open, Some(ChatMenuSubmenu::Folders));
    assert!(present(vcx, "chat-menu-folder-7"));
    click(vcx, "chat-menu-folder-7");
    assert!(app.update(vcx, |app, _| app.chat_list.menu.is_none()));
}

#[gpui_kit::test]
fn exit_entry_follows_the_chat_kind(cx: &mut TestAppContext) {
    let (app, mut vcx, chat_id) = open(cx);
    let vcx = &mut vcx;
    app.update(vcx, |app, _| {
        if let Some(chat) = app
            .demo_session
            .as_mut()
            .and_then(|s| s.chats.get_mut(&chat_id.0))
        {
            chat.can_be_deleted_only_for_self = true;
        }
    });
    show_menu(&app, vcx, chat_id);
    assert!(present(vcx, "chat-menu-delete"));
    assert!(!present(vcx, "chat-menu-leave"));

    app.update(vcx, |app, _| {
        if let Some(chat) = app
            .demo_session
            .as_mut()
            .and_then(|s| s.chats.get_mut(&chat_id.0))
        {
            chat.kind = ChatKind::Supergroup {
                supergroup_id: 55,
                is_channel: true,
            };
        }
    });
    show_menu(&app, vcx, chat_id);
    assert!(present(vcx, "chat-menu-leave"));
    assert!(!present(vcx, "chat-menu-delete"));
    click(vcx, "chat-menu-leave");
    let action = app.update(vcx, |app, _| {
        app.admin
            .group_confirm_dialog
            .as_ref()
            .map(|dialog| matches!(dialog.action, GroupConfirmAction::LeaveChat))
    });
    assert_eq!(action, Some(true));
}

#[gpui_kit::test]
fn a_promotion_shapes_the_menu_like_use_top_promotion(cx: &mut TestAppContext) {
    use quill::promotion::Promotion;
    use quill::telegram::envelope::ChannelMemberStatus;
    let (app, mut vcx, chat_id) = open(cx);
    let vcx = &mut vcx;
    // A channel the account is not in, which is what a promotion is.
    let promote = |app: &Entity<QuillApp>,
                   vcx: &mut VisualTestContext,
                   p: Option<Promotion>,
                   status: ChannelMemberStatus| {
        app.update(vcx, |app, _| {
            if let Some(session) = app.demo_session.as_mut() {
                if let Some(chat) = session.chats.get_mut(&chat_id.0) {
                    chat.kind = ChatKind::Supergroup {
                        supergroup_id: 55,
                        is_channel: true,
                    };
                    chat.my_member_status = Some(status);
                }
                match p {
                    Some(p) => session.chat_list.promoted_chats.insert(chat_id.0, p),
                    None => session.chat_list.promoted_chats.remove(&chat_id.0),
                };
            }
        });
        show_menu(app, vcx, chat_id);
    };
    let psa = || Promotion::Psa {
        kind: "covid".into(),
        text: String::new(),
    };
    // Not promoted: the usual rows.
    promote(&app, vcx, None, ChannelMemberStatus::Left);
    assert!(!present(vcx, "chat-menu-hide-promotion"));
    assert!(present(vcx, "chat-menu-archive"));
    assert!(present(vcx, "chat-menu-pin"));
    assert!(present(vcx, "chat-menu-new-window"));
    // A proxy sponsor: no hide, and no Archive or Pin.
    promote(
        &app,
        vcx,
        Some(Promotion::MtprotoProxy),
        ChannelMemberStatus::Left,
    );
    assert!(!present(vcx, "chat-menu-hide-promotion"));
    assert!(!present(vcx, "chat-menu-archive"));
    assert!(!present(vcx, "chat-menu-pin"));
    assert!(present(vcx, "chat-menu-new-window"));
    // A joined channel is not promoted any more, whatever is left over.
    promote(&app, vcx, Some(psa()), ChannelMemberStatus::Member);
    assert!(!present(vcx, "chat-menu-hide-promotion"));
    assert!(present(vcx, "chat-menu-archive"));
    // A typed announcement: hide, but no Archive, Pin or new window.
    promote(&app, vcx, Some(psa()), ChannelMemberStatus::Left);
    assert!(present(vcx, "chat-menu-hide-promotion"));
    assert!(!present(vcx, "chat-menu-archive"));
    assert!(!present(vcx, "chat-menu-pin"));
    assert!(!present(vcx, "chat-menu-new-window"));
    click(vcx, "chat-menu-hide-promotion");
    assert!(app.update(vcx, |app, _| app.chat_list.menu.is_none()));
}

fn set_clear_flags(
    app: &Entity<QuillApp>,
    vcx: &mut VisualTestContext,
    chat_id: ChatId,
    only_for_self: bool,
    for_all: bool,
) {
    app.update(vcx, |app, cx| {
        let Some(chat) = app
            .demo_session
            .as_mut()
            .and_then(|s| s.chats.get_mut(&chat_id.0))
        else {
            panic!("chat missing");
        };
        chat.can_be_deleted_only_for_self = only_for_self;
        chat.can_be_deleted_for_all_users = for_all;
        cx.notify();
    });
}

#[gpui_kit::test]
fn clear_history_is_one_item_that_asks_once(cx: &mut TestAppContext) {
    let (app, mut vcx, chat_id) = open(cx);
    let vcx = &mut vcx;
    set_clear_flags(&app, vcx, chat_id, true, true);
    show_menu(&app, vcx, chat_id);
    assert!(present(vcx, "chat-menu-clear"));
    assert!(
        !present(vcx, "chat-menu-clear-all"),
        "no separate item for everyone"
    );
    click(vcx, "chat-menu-clear");
    assert!(app.update(vcx, |app, _| app.chat_list.menu.is_none()));
    assert!(
        present(vcx, "clear-history-revoke"),
        "the box offers the checkbox where the other side can be cleared"
    );
}

#[gpui_kit::test]
fn clear_history_has_no_checkbox_when_only_this_side_can_be_cleared(cx: &mut TestAppContext) {
    let (app, mut vcx, chat_id) = open(cx);
    let vcx = &mut vcx;
    set_clear_flags(&app, vcx, chat_id, true, false);
    show_menu(&app, vcx, chat_id);
    assert!(present(vcx, "chat-menu-clear"));
    click(vcx, "chat-menu-clear");
    frames(vcx);
    assert!(!present(vcx, "clear-history-revoke"));
}

#[gpui_kit::test]
fn clear_history_is_hidden_when_the_server_allows_none(cx: &mut TestAppContext) {
    let (app, mut vcx, chat_id) = open(cx);
    let vcx = &mut vcx;
    set_clear_flags(&app, vcx, chat_id, false, false);
    show_menu(&app, vcx, chat_id);
    assert!(!present(vcx, "chat-menu-clear"));
}
