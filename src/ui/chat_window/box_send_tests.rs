//! The Add-bot and Share-game boxes wait for their own send only: a send
//! made by a box that closed is not waited on by the next box.
#![cfg(feature = "demo-capture")]

use super::tests::new_app;
use crate::ui::bots_box_send::{BoxKind, BoxSend};
use crate::ui::dialogs::ProfileDialog;
use gpui_kit::TestAppContext;
use quill::bot_invite::Invite;
use quill::ids::{ChatId, RequestId};
use quill::telegram::envelope::ChatAdminRights;

fn add_bot_box(target: i64) -> ProfileDialog {
    ProfileDialog::AddBot {
        bot_id: 7,
        invite: Invite::default(),
        target: Some(target),
        rights: ChatAdminRights::default(),
    }
}

fn sending(kind: BoxKind, chat: i64) -> BoxSend {
    BoxSend::Sending {
        kind,
        request: RequestId(1),
        chat: ChatId(chat),
        then_start: None,
    }
}

#[gpui_kit::test]
fn the_open_box_waits_only_for_its_own_send(cx: &mut TestAppContext) {
    let h = new_app(cx);
    h.app.update(cx, |app, _| {
        app.dialogs.box_send = sending(BoxKind::AddBot, 11);
        app.dialogs.profile_dialog = Some(add_bot_box(11));
        assert!(app.box_sending(), "its own box waits");
        app.dialogs.profile_dialog = Some(add_bot_box(12));
        assert!(!app.box_sending(), "another chat's box is not held");
        app.dialogs.profile_dialog = Some(ProfileDialog::ShareGame {
            bot_id: 7,
            game_short_name: "g".into(),
            target: Some(11),
        });
        assert!(!app.box_sending(), "another kind of box is not held");
    });
}

#[gpui_kit::test]
fn closing_the_box_forgets_its_send(cx: &mut TestAppContext) {
    let h = new_app(cx);
    h.app.update(cx, |app, cx| {
        app.dialogs.box_send = sending(BoxKind::AddBot, 11);
        app.dialogs.profile_dialog = Some(add_bot_box(11));
        app.close_profile_dialog(cx);
        assert_eq!(app.dialogs.box_send, BoxSend::Idle);
        app.dialogs.profile_dialog = Some(add_bot_box(12));
        assert!(!app.box_sending(), "the next box starts free");
    });
}
