//! The video chat's share box: it opens once both links are in, the
//! checkbox picks the copied link, and a "Share to" chat gets the link in
//! its message box (needs the `demo-capture` feature for gpui-kit's test
//! support). Nothing reaches Telegram: the app has no connection.
use super::LINK_COPIED;
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::calls::share_links::{InviteRowAction, VideoChatShareLinks};
use quill::ids::ChatId;
use quill::state::{ActiveGroupCall, DeepLinkAction, VideoChatInfo};
use quill::telegram::envelope::ChatKind;

const LISTENER: &str = "https://t.me/quillchat?videochat";
const SPEAKER: &str = "https://t.me/quillchat?videochat=abc";

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(
                window,
                cx,
                None,
                Some(ScreenshotDemo::named("ready-privacy")),
            )
        });
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let app = slot.borrow().clone().unwrap();
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    (app, vcx)
}

/// A public supergroup's video chat with both links known. Returns the
/// chat a link can be shared to.
fn video_chat(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, speaker: &str) -> ChatId {
    app.update_in(vcx, |app, _, cx| {
        let session = app.demo_session.as_mut().expect("demo session");
        let chat_id = *session.chats.keys().min().expect("a chat");
        let chat = session.chats.get_mut(&chat_id).unwrap();
        chat.kind = ChatKind::Supergroup {
            supergroup_id: 4242,
            is_channel: false,
        };
        chat.video_chat = Some(VideoChatInfo {
            group_call_id: 77,
            has_participants: true,
            default_participant_id: None,
        });
        session.set_supergroup_username(4242, "quillchat".into());
        session.calls.active_group_call = Some(ActiveGroupCall {
            is_video_chat: true,
            can_be_managed: true,
            share_links: VideoChatShareLinks {
                listener: Some(LISTENER.into()),
                speaker: Some(speaker.into()),
                generating: false,
            },
            ..ActiveGroupCall::fresh(77)
        });
        cx.notify();
        ChatId(chat_id)
    })
}

fn overlay_shown(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> bool {
    app.update_in(vcx, |app, _, cx| {
        let call = app
            .session()
            .and_then(|s| s.calls.active_group_call.clone())
            .unwrap();
        app.group_call_share_overlay(&call, cx).is_some()
    })
}

#[gpui_kit::test]
fn the_box_copies_the_speaker_or_listener_link(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    video_chat(&app, vcx, SPEAKER);
    assert!(!overlay_shown(&app, vcx));
    app.update_in(vcx, |app, _, cx| {
        let call = app
            .session()
            .unwrap()
            .calls
            .active_group_call
            .clone()
            .unwrap();
        assert!(app.group_call_can_share_link(&call));
        app.open_group_call_share_links(cx);
    });
    vcx.run_until_parked();
    assert!(overlay_shown(&app, vcx));

    // Ticked by default: the speaker link.
    app.update_in(vcx, |app, _, cx| app.copy_group_call_share_link(cx));
    assert_eq!(
        vcx.read_from_clipboard().and_then(|item| item.text()),
        Some(SPEAKER.to_string())
    );
    app.read_with(vcx, |app, _| assert!(app.group_call.share_copied));

    // Unticked: the listener link; the box stays open.
    app.update_in(vcx, |app, _, cx| {
        app.group_call.share_speaker = false;
        app.copy_group_call_share_link(cx);
    });
    assert_eq!(
        vcx.read_from_clipboard().and_then(|item| item.text()),
        Some(LISTENER.to_string())
    );
    assert!(overlay_shown(&app, vcx));

    app.update_in(vcx, |app, _, cx| app.close_group_call_share_links(cx));
    assert!(!overlay_shown(&app, vcx));
    assert_eq!(LINK_COPIED, "Invite link copied to clipboard.");
}

#[gpui_kit::test]
fn without_a_speaker_link_the_listener_link_is_copied(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    video_chat(&app, vcx, "");
    app.update_in(vcx, |app, _, cx| {
        app.open_group_call_share_links(cx);
        app.copy_group_call_share_link(cx);
    });
    assert_eq!(
        vcx.read_from_clipboard().and_then(|item| item.text()),
        Some(LISTENER.to_string())
    );
}

#[gpui_kit::test]
fn a_chat_gets_the_link_in_its_message_box(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let chat = video_chat(&app, vcx, SPEAKER);
    app.update_in(vcx, |app, _, cx| {
        app.open_group_call_share_links(cx);
        app.share_group_call_link_to(chat, cx);
        // Taken before the next frame opens the chat.
        let pending = app.links.pending_deep_link_open.take();
        assert_eq!(
            pending,
            Some((
                chat,
                DeepLinkAction::ShareDraft {
                    text: SPEAKER.to_string()
                }
            ))
        );
        assert!(!app.group_call.share_open);
    });
}

#[gpui_kit::test]
fn the_invite_row_shares_the_link_in_a_public_channel(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let chat = video_chat(&app, vcx, SPEAKER);
    app.update_in(vcx, |app, _, cx| {
        let session = app.demo_session.as_mut().unwrap();
        session.chats.get_mut(&chat.0).unwrap().kind = ChatKind::Supergroup {
            supergroup_id: 4242,
            is_channel: true,
        };
        let call = session.calls.active_group_call.clone().unwrap();
        assert_eq!(
            app.group_call_invite_row(&call),
            Some(InviteRowAction::ShareLink)
        );
        app.group_call_invite_row_clicked(cx);
        assert!(app.group_call.share_open);
        assert!(!app.group_call.invite_open);
    });
}

#[gpui_kit::test]
fn the_box_waits_for_the_passcode_unlock(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    video_chat(&app, vcx, SPEAKER);
    app.update_in(vcx, |app, _, cx| {
        app.account.passcode.locked = true;
        app.open_group_call_share_links(cx);
    });
    assert!(!overlay_shown(&app, vcx));
    app.update_in(vcx, |app, _, _| app.account.passcode.locked = false);
    assert!(overlay_shown(&app, vcx));
}
