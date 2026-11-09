//! The rights editor saves the rights its lookup brought in after it
//! opened, when no checkbox was touched.

use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::ScreenshotDemo;
use gpui_kit::TestAppContext;
use quill::ids::ChatId;
use quill::lang::{keys, tr};
use quill::state::AdminRightsFetch;
use quill::telegram::envelope::ChatAdminRights;

const CHAT: ChatId = ChatId(11);
const ADMIN: i64 = 77;

#[gpui_kit::test]
fn save_with_untouched_rights_uses_the_fetched_ones(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let window = cx.add_window(|window, cx| {
        QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named("ready-chats")))
    });
    window
        .update(cx, |app, _, cx| {
            app.open_rights_editor(CHAT, ADMIN, cx);
            if let Some(session) = app.demo_session.as_mut() {
                session.groups.admin_rights.insert(
                    (CHAT.0, ADMIN),
                    AdminRightsFetch::Loaded(ChatAdminRights::default()),
                );
            }
            app.submit_admin_dialog(cx);
            let note = app.connection.note().text().to_string();
            assert_ne!(note, tr(keys::Q_PEOPLE_RIGHTS_ARE_STILL_LOADING));
            assert_eq!(note, tr(keys::Q_PEOPLE_ADMIN_ACTIONS_NEED_A_LIVE));
        })
        .unwrap();
}
