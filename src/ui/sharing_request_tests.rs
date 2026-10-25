use super::{ADA_LIVE_REQUEST, CHAT};
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::{DialogKind, QuillShell};
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, Modifiers, TestAppContext, VisualTestContext, px, size};
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::ChatId;
use quill::state::EnableSharing;
use quill::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, Entity<QuillShell>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(900.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(
                window,
                cx,
                None,
                Some(ScreenshotDemo::named("ready-sharing-request")),
            )
        });
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some((view, shell.clone()));
        Root::new(shell, window, cx)
    });
    let (app, shell) = slot.borrow().clone().expect("the app view");
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    (app, shell, vcx)
}

fn frames(vcx: &mut VisualTestContext) {
    vcx.update(|_, cx| cx.set_reduce_motion(true));
    vcx.run_until_parked();
    for _ in 0..2 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

fn find(vcx: &mut VisualTestContext, selector: &'static str) -> bool {
    vcx.debug_bounds(selector).is_some()
}

fn click(vcx: &mut VisualTestContext, selector: &'static str) {
    let bounds = vcx.debug_bounds(selector).expect(selector);
    vcx.simulate_click(bounds.center(), Modifiers::default());
    frames(vcx);
}

fn apply(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, updates: &[String]) {
    app.update_in(vcx, |app, _, cx| {
        let Some(session) = app.demo_session.as_mut() else {
            return;
        };
        let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
        let seq = AtomicU64::new(session.last_seq);
        for json in updates {
            if let Some(owned) = copy_and_parse(json, &seq, &sink) {
                session.apply(owned);
            }
        }
        cx.notify();
    });
    frames(vcx);
}

#[gpui_kit::test]
fn an_incoming_request_offers_reject_and_accept(cx: &mut TestAppContext) {
    let (app, _shell, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    frames(vcx);
    assert!(find(vcx, "sharing-request-card"));
    assert!(find(vcx, "sharing-request-reject"));
    assert!(find(vcx, "sharing-request-accept"));

    click(vcx, "sharing-request-accept");
    let note = app.read_with(vcx, |app, _| app.connection.status_note().to_string());
    assert!(!note.is_empty(), "the press leaves a note");
}

#[gpui_kit::test]
fn an_answered_request_keeps_its_card_without_buttons(cx: &mut TestAppContext) {
    let (app, _shell, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    frames(vcx);
    assert!(find(vcx, "sharing-request-accept"));
    apply(
        &app,
        vcx,
        &[
            format!(
                r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":{CHAT},"is_outgoing":false,"sender_id":{{"@type":"messageSenderUser","user_id":{CHAT}}},"date":1790632300,"content":{{"@type":"messageChatHasProtectedContentToggled","request_message_id":{ADA_LIVE_REQUEST},"old_has_protected_content":true,"new_has_protected_content":false}}}}}}"#,
                id = ADA_LIVE_REQUEST + 1_048_576,
            ),
            format!(
                r#"{{"@type":"updateMessageContent","chat_id":{CHAT},"message_id":{ADA_LIVE_REQUEST},"new_content":{{"@type":"messageChatHasProtectedContentDisableRequested","is_expired":true}}}}"#
            ),
        ],
    );
    assert!(!find(vcx, "sharing-request-accept"));
    assert!(!find(vcx, "sharing-request-reject"));
    let answered = app.read_with(vcx, |app, _| {
        app.session()
            .and_then(|s| {
                let message = s.histories.get(&CHAT)?.messages.get(&ADA_LIVE_REQUEST)?;
                s.sharing_request_view(message)
            })
            .is_some_and(|view| view.card && !view.buttons)
    });
    assert!(answered);
}

#[gpui_kit::test]
fn the_request_box_opens_from_state_and_cancel_closes_it(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    frames(vcx);
    let ask = |app: &Entity<QuillApp>, vcx: &mut VisualTestContext| {
        app.update_in(vcx, |app, _, cx| {
            if let Some(session) = app.demo_session.as_mut() {
                session.groups.private_sharing.enable =
                    Some(EnableSharing::AskToRequest(ChatId(CHAT)));
            }
            cx.notify();
        });
        frames(vcx);
    };
    let open = |shell: &Entity<QuillShell>, vcx: &mut VisualTestContext| {
        shell.read_with(vcx, |shell, _| shell.open_dialog)
    };
    let request_box = |app: &Entity<QuillApp>, vcx: &mut VisualTestContext| {
        app.read_with(vcx, |app, _| {
            app.session()
                .and_then(|s| s.groups.private_sharing.request_box_chat())
        })
    };

    ask(&app, vcx);
    assert_eq!(open(&shell, vcx), Some(DialogKind::SharingRequest));
    click(vcx, "sharing-request-cancel");
    assert_eq!(request_box(&app, vcx), None);
    assert_ne!(open(&shell, vcx), Some(DialogKind::SharingRequest));

    ask(&app, vcx);
    click(vcx, "sharing-request-send");
    assert_eq!(request_box(&app, vcx), None);
    assert_ne!(open(&shell, vcx), Some(DialogKind::SharingRequest));
}
