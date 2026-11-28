//! An album's delete box keeps the moderator's checkboxes and deletes the
//! whole album (tdesktop `deleteAsGroup` with `CreateModerateMessagesBox`).

use crate::ui::app::QuillApp;
use crate::ui::message_menu_ui::ModerationOffer;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::composer::DeleteConfirm;
use quill::ids::{ChatId, MessageId};
use quill::moderation::ModerateOptions;

const CHAT: ChatId = ChatId(11);

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named("ready-albums");
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app = slot.borrow().clone().expect("the app view");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    frames(&mut vcx);
    (app, vcx)
}

fn frames(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    for _ in 0..2 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

#[gpui_kit::test]
fn an_album_delete_offers_moderation_and_deletes_every_item(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let album = app.read_with(vcx, |app, _| app.album_of(CHAT, MessageId(801)));
    assert!(album.len() > 1, "801 is in an album");
    app.update_in(vcx, |app, window, cx| {
        let confirm = DeleteConfirm {
            chat_id: CHAT,
            message_id: MessageId(801),
            revoke: false,
            can_revoke: false,
        };
        let offer = ModerationOffer {
            chat_id: CHAT,
            user_id: 11,
            user_name: "Ada".into(),
            options: ModerateOptions {
                report_spam: true,
                delete_all: true,
                delete_reactions: false,
                ban_or_restrict: true,
            },
            can_restrict_instead: false,
        };
        app.open_delete_dialog_with(confirm, album.clone(), Some(offer), window, cx);
    });
    frames(vcx);
    // The admin's checkboxes are in the box (a missing one fails the click).
    vcx.update(|window, cx| window.click("delete-moderate-spam", cx));
    vcx.update(|window, cx| window.click("delete-moderate-ban", cx));
    frames(vcx);
    vcx.update(|window, cx| window.click("ok", cx));
    frames(vcx);
    app.read_with(vcx, |app, _| {
        let session = app.session().expect("demo session");
        for id in &album {
            assert!(session.message_deleted(CHAT, *id), "{id:?} is deleted");
        }
    });
}
