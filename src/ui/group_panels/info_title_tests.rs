//! The info panel's title follows the peer (tdesktop `Info::Profile::Widget::title`)
//! and a verified channel's header carries its badge.

use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::diagnostics::DiagnosticSink;
use quill::lang::{keys, tr};
use quill::peer_badge::TitleBadge;
use quill::state::InfoPanelTarget;
use quill::telegram::client::copy_and_parse;

fn frames(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    for _ in 0..3 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

fn open_demo(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named("ready-info-channel");
    let handle = cx.open_window(size(px(1200.), px(1000.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let app = slot.borrow().clone().expect("app");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    frames(&mut vcx);
    (app, vcx)
}

fn inject(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, jsons: &[&str]) {
    app.update(vcx, |app, _| {
        let sink = app.demo_ui.sink.clone();
        let dyn_sink: std::sync::Arc<dyn DiagnosticSink> = sink;
        if let Some(session) = app.demo_session.as_mut() {
            for json in jsons {
                if let Some(owned) = copy_and_parse(json, &app.demo_ui.seq, &dyn_sink) {
                    session.apply(owned);
                }
            }
            session.my_user_id = Some(777);
        }
    });
}

fn title_of(
    app: &Entity<QuillApp>,
    vcx: &mut VisualTestContext,
    target: InfoPanelTarget,
) -> Option<&'static str> {
    vcx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.open_info_panel_target(target, window, cx);
        });
    });
    frames(vcx);
    app.update(vcx, |app, cx| {
        app.info_panel_parts(cx).map(|(title, _)| title)
    })
}

#[gpui_kit::test]
fn title_follows_the_peer_and_a_verified_channel_shows_its_badge(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_demo(cx);
    let vcx = &mut vcx;
    inject(
        &app,
        vcx,
        &[
            r#"{"@type":"updateNewChat","chat":{"id":74,"title":"A group","type":{"@type":"chatTypeSupergroup","supergroup_id":74,"is_channel":false},"unread_count":0}}"#,
            r#"{"@type":"updateUser","user":{"id":21,"first_name":"Demo","type":{"@type":"userTypeBot","can_be_edited":false,"can_join_groups":false,"can_read_all_group_messages":false,"has_main_web_app":false,"has_topics":false,"allows_users_to_create_topics":false,"can_manage_bots":false,"is_inline":false,"inline_query_placeholder":"","supports_guest_queries":false,"is_guard":false,"need_location":false,"can_connect_to_business":false,"can_be_added_to_attachment_menu":false,"active_user_count":0}}}"#,
            r#"{"@type":"updateUser","user":{"id":22,"first_name":"Ada","type":{"@type":"userTypeRegular"}}}"#,
            r#"{"@type":"updateUser","user":{"id":777,"first_name":"Me","type":{"@type":"userTypeRegular"}}}"#,
        ],
    );
    let titled = |target| title_of(&app, vcx, target);
    assert_eq!(
        titled(InfoPanelTarget::Supergroup(71)),
        Some(tr(keys::INFO_CHANNEL_TITLE))
    );
    assert_eq!(
        titled(InfoPanelTarget::Supergroup(74)),
        Some(tr(keys::INFO_GROUP_TITLE))
    );
    assert_eq!(
        titled(InfoPanelTarget::User(21)),
        Some(tr(keys::INFO_BOT_TITLE))
    );
    assert_eq!(
        titled(InfoPanelTarget::User(22)),
        Some(tr(keys::INFO_USER_TITLE))
    );
    assert_eq!(
        titled(InfoPanelTarget::User(777)),
        Some(tr(keys::SAVED_MESSAGES))
    );
    let badges = app.read_with(vcx, |app, _| {
        let session = app.session()?;
        let chat = session.supergroup_chat(71)?;
        Some(session.chat_header_badges(chat))
    });
    assert_eq!(badges, Some(vec![TitleBadge::Verified]));
}
