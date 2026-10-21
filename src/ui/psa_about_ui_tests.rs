use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::ids::ChatId;
use quill::telegram::client::copy_and_parse;
use quill::telegram::envelope::ChannelMemberStatus;

fn frames(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    for _ in 0..3 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

fn open(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
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
    app.update(&mut vcx, |app, _| {
        let Some(session) = app.demo_session.as_mut() else {
            return;
        };
        session.chat_list.psa_about.restore(Default::default());
        let seq = std::sync::atomic::AtomicU64::new(session.last_seq);
        let sink: std::sync::Arc<dyn quill::diagnostics::DiagnosticSink> = app.demo_ui.sink.clone();
        let source = r#"{"@type":"chatSourcePublicServiceAnnouncement","type":"covid","text":""}"#;
        for id in [91, 92] {
            let chat = format!(
                r#"{{"@type":"updateNewChat","chat":{{"id":{id},"title":"Chat {id}","type":{{"@type":"chatTypeSupergroup","supergroup_id":{id},"is_channel":true}},"last_read_inbox_message_id":0,"last_read_outbox_message_id":0}}}}"#
            );
            let source = if id == 91 { source } else { "null" };
            let position = format!(
                r#"{{"@type":"updateChatPosition","chat_id":{id},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"900000000000000{id}","is_pinned":false,"source":{source}}}}}"#
            );
            for json in [chat, position] {
                if let Some(owned) = copy_and_parse(&json, &seq, &sink) {
                    session.apply(owned);
                }
            }
            if let Some(chat) = session.chats.get_mut(&id) {
                chat.my_member_status = Some(ChannelMemberStatus::Left);
            }
        }
    });
    frames(&mut vcx);
    (app, vcx)
}

fn open_chat(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, id: i64) {
    app.update(vcx, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session.open_chat(ChatId(id));
        }
        cx.notify();
    });
    frames(vcx);
}

fn shown(vcx: &mut VisualTestContext) -> bool {
    vcx.update(|window, _| window.try_find("psa-about-box").is_some())
}

#[gpui_kit::test]
fn the_box_shows_for_a_promoted_chat_once_and_goes_with_the_chat(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    let vcx = &mut vcx;
    assert!(!shown(vcx), "nothing is open");
    open_chat(&app, vcx, 91);
    assert!(shown(vcx));
    open_chat(&app, vcx, 92);
    assert!(!shown(vcx), "another chat hides it");
    open_chat(&app, vcx, 91);
    assert!(!shown(vcx), "it is one-time");
}

#[gpui_kit::test]
fn the_box_goes_when_the_chat_leaves_the_window(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    let vcx = &mut vcx;
    open_chat(&app, vcx, 91);
    assert!(shown(vcx));
    app.update(vcx, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session.leave_open_chat();
        }
        cx.notify();
    });
    frames(vcx);
    assert!(!shown(vcx), "popped out: no box over the empty window");
}
