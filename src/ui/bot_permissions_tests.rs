//! The bot profile's "Emoji Status" switch: shown for a bot with the access,
//! kept after it is turned off, and absent for a bot without it.

use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::ScreenshotDemo;
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::telegram::envelope::BotInfo;

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named("ready-chats");
    let handle = cx.open_window(size(px(900.), px(600.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        *slot_in.borrow_mut() = Some(view.clone());
        Root::new(view, window, cx)
    });
    let app = slot.borrow().clone().unwrap();
    (app, VisualTestContext::from_window(handle.into(), cx))
}

fn seed_bot(app: &mut QuillApp, bot_id: i64, allowed: bool) {
    let session = app.demo_session.as_mut().expect("demo session");
    session.bots.bot_info.insert(
        bot_id,
        Some(BotInfo {
            can_manage_emoji_status: allowed,
            ..BotInfo::default()
        }),
    );
}

fn flag(app: &QuillApp, bot_id: i64) -> bool {
    app.demo_session.as_ref().unwrap().bots.bot_info[&bot_id]
        .as_ref()
        .unwrap()
        .can_manage_emoji_status
}

#[gpui_kit::test]
fn the_switch_shows_for_a_granted_bot_and_stays_when_turned_off(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    app.update_in(&mut vcx, |app, _, cx| {
        seed_bot(app, 900, true);
        seed_bot(app, 901, false);
        assert!(app.bot_permissions_section(900, cx).is_some());
        assert!(
            app.bot_permissions_section(901, cx).is_none(),
            "no access, no section"
        );
        assert!(
            app.bot_permissions_section(902, cx).is_none(),
            "unknown bot"
        );

        app.set_bot_emoji_status_access(900, false, cx);
        assert!(!flag(app, 900));
        assert!(
            app.bot_permissions_section(900, cx).is_some(),
            "the section stays so the access can be given again"
        );
        app.set_bot_emoji_status_access(900, true, cx);
        assert!(flag(app, 900));
    });
}
