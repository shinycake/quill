//! Boxes whose open flag lives in the `Session` close on a switch. The
//! session goes to the background with the account and comes back with
//! it, so a box left open there would be open again on switch-back.

use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::boost_box::BoostBoxState;
use quill::calls::rtmp_start::RtmpStart;
use quill::collectible_info::CollectibleLookup;
use quill::ids::{AccountKey, ChatId};
use quill::state::LanguageLinkLookup;

const CHAT: ChatId = ChatId(11);

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named("ready-chats")))
        });
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

#[gpui_kit::test]
fn a_switch_closes_the_boxes_whose_flag_is_the_sessions(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    frames(vcx);
    app.update_in(vcx, |app, _, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session.open_chat(CHAT);
            session.stickers.masks.open = true;
            session.stickers.emoji.open = true;
            session.chat_list.archive_settings_open = true;
            session.calls.rtmp_start = Some(RtmpStart::new(CHAT));
            session.groups.boost_box = Some(BoostBoxState::new(CHAT.0, false));
            session.settings.language_link = Some(LanguageLinkLookup::new("de".into()));
            session.payments.hub.collectible = Some(CollectibleLookup::new(
                "durov",
                "Pavel",
                None,
                String::new(),
            ));
        }
        app.open_location_picker(cx);
    });
    frames(vcx);
    assert!(app.read_with(vcx, |app, _| app.location_picker_open()));

    app.update_in(vcx, |app, window, cx| {
        app.switch_account(&AccountKey("other".into()), "", window, cx);
    });
    frames(vcx);
    app.read_with(vcx, |app, _| {
        assert!(!app.location_picker_open(), "Send Location");
        let session = app.session().expect("the demo session");
        assert!(!session.stickers.masks.open, "Manage masks");
        assert!(!session.stickers.emoji.open, "Emoji sets");
        assert!(!session.chat_list.archive_settings_open, "Archive settings");
        assert!(session.calls.rtmp_start.is_none(), "RTMP start");
        assert!(session.groups.boost_box.is_none(), "Boost box");
        assert!(session.settings.language_link.is_none(), "Language link");
        assert!(session.payments.hub.collectible.is_none(), "Collectible");
    });
}
