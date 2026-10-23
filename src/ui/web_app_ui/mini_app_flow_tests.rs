//! The phone and invoice requests of a mini app window, driven with `cat`
//! standing in for the web view helper: what the window writes to the
//! helper's stdin comes back on its stdout, where the test reads the popups
//! and bridge replies.

use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::web_app_ui::helper_pipe::HelperWriter;
use crate::ui::web_app_ui::invoice::InvoiceWatch;
use crate::ui::web_app_ui::{MiniAppWindow, PopupPurpose, RenderAction, theme_params};
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::web_app::bridge::WebAppEvent;
use quill::web_app::{LaunchSource, WebAppLaunch};
use quill_webview_protocol::{HostCommand, ShellEvent, decode};
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

const BOT: i64 = 900;

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

fn open_window(app: &mut QuillApp, cx: &mut gpui_kit::App) -> mpsc::Receiver<HostCommand> {
    let mut child = Command::new("cat")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("cat");
    let stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let (sender, received) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            if let Ok(command) = decode::<HostCommand>(&line)
                && sender.send(command).is_err()
            {
                break;
            }
        }
    });
    app.mini_apps.window = Some(MiniAppWindow {
        child: Some(child),
        writer: HelperWriter::spawn(stdin),
        wedged: false,
        events: mpsc::channel().1,
        launch: WebAppLaunch {
            bot_user_id: BOT,
            bot_name: "Quillbot".into(),
            chat_id: None,
            source: LaunchSource::MenuButton,
            launch_id: 0,
            url: "https://example.com/app".into(),
        },
        theme: theme_params(cx),
        ready: true,
        closing_confirmation: false,
        settings_button: false,
        popups: Default::default(),
        popup_seq: 0,
        waiting_write_access: false,
        data_sent: false,
        privacy_policy_url: None,
        in_attachment_menu: false,
        emoji: Default::default(),
        phone: Default::default(),
        invoice: None,
    });
    received
}

fn next(received: &mpsc::Receiver<HostCommand>) -> HostCommand {
    received
        .recv_timeout(Duration::from_secs(5))
        .expect("the window wrote to the helper")
}

fn reply(received: &mpsc::Receiver<HostCommand>) -> (String, serde_json::Value) {
    match next(received) {
        HostCommand::Emit { event, data } => (event, data),
        other => panic!("expected a bridge reply, got {other:?}"),
    }
}

fn prompt(received: &mpsc::Receiver<HostCommand>) -> quill_webview_protocol::Popup {
    match next(received) {
        HostCommand::ShowPopup { popup } => popup,
        other => panic!("expected a prompt, got {other:?}"),
    }
}

fn answer(app: &mut QuillApp, id: &str, button: &str, cx: &mut gpui_kit::Context<QuillApp>) {
    app.handle_shell_event(
        ShellEvent::PopupClosed {
            id: id.into(),
            button: button.into(),
        },
        cx,
    );
}

#[gpui_kit::test]
fn the_phone_prompt_asks_and_cancel_answers_cancelled(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    app.update_in(&mut vcx, |app, _, cx| {
        let received = open_window(app, cx);
        app.handle_web_app_event(WebAppEvent::RequestPhone, cx);
        let popup = prompt(&received);
        assert_eq!(popup.title, "Phone number");
        assert_eq!(
            popup.message,
            "Do you want to share your phone number with this bot?"
        );
        assert_eq!(
            popup
                .buttons
                .iter()
                .map(|b| (b.id.as_str(), b.text.as_str()))
                .collect::<Vec<_>>(),
            [("cancel", "Cancel"), ("share", "Share")]
        );
        assert!(matches!(
            app.mini_apps.window.as_ref().unwrap().popups.get(&popup.id),
            Some(PopupPurpose::Phone)
        ));
        // A second request while the prompt is up is refused.
        app.handle_web_app_event(WebAppEvent::RequestPhone, cx);
        assert_eq!(reply(&received).1["status"], "cancelled");

        answer(app, &popup.id, "cancel", cx);
        let (event, data) = reply(&received);
        assert_eq!(event, "phone_requested");
        assert_eq!(data["status"], "cancelled");
    });
}

#[gpui_kit::test]
fn sharing_without_a_connection_answers_cancelled(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    app.update_in(&mut vcx, |app, _, cx| {
        let received = open_window(app, cx);
        app.handle_web_app_event(WebAppEvent::RequestPhone, cx);
        let popup = prompt(&received);
        answer(app, &popup.id, "share", cx);
        let (event, data) = reply(&received);
        assert_eq!(event, "phone_requested");
        assert_eq!(data["status"], "cancelled");
        // The request is over: a new one asks again.
        app.handle_web_app_event(WebAppEvent::RequestPhone, cx);
        prompt(&received);
    });
}

#[gpui_kit::test]
fn an_invoice_request_opens_the_checkout_at_render_time(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    app.update_in(&mut vcx, |app, _, cx| {
        let _received = open_window(app, cx);
        app.handle_web_app_event(WebAppEvent::OpenInvoice { slug: "abc".into() }, cx);
        assert_eq!(
            app.mini_apps.render_action,
            Some(RenderAction::OpenInvoice { slug: "abc".into() })
        );
    });
}

#[gpui_kit::test]
fn an_invoice_that_cannot_start_reports_failed(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    app.update_in(&mut vcx, |app, window, cx| {
        let received = open_window(app, cx);
        // The demo has no connection, so the form cannot be requested.
        app.open_mini_app_invoice("abc".into(), window, cx);
        let (event, data) = reply(&received);
        assert_eq!(event, "invoice_closed");
        assert_eq!(data["slug"], "abc");
        assert_eq!(data["status"], "failed");
        assert!(app.mini_apps.window.as_ref().unwrap().invoice.is_none());
    });
}

#[gpui_kit::test]
fn closing_the_checkout_reports_cancelled(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    app.update_in(&mut vcx, |app, window, cx| {
        let received = open_window(app, cx);
        app.open_payment_dialog(window, cx);
        app.mini_apps.window.as_mut().unwrap().invoice = Some(InvoiceWatch::new("abc"));
        app.poll_invoice(cx);
        assert!(app.mini_apps.window.as_ref().unwrap().invoice.is_some());
        app.close_payment_dialog(cx);
        app.poll_invoice(cx);
        let (event, data) = reply(&received);
        assert_eq!(event, "invoice_closed");
        assert_eq!(data["slug"], "abc");
        assert_eq!(data["status"], "cancelled");
        assert!(app.mini_apps.window.as_ref().unwrap().invoice.is_none());
    });
}

#[gpui_kit::test]
fn a_paid_checkout_reports_paid_and_closes(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    app.update_in(&mut vcx, |app, window, cx| {
        let received = open_window(app, cx);
        app.open_payment_dialog(window, cx);
        app.mini_apps.window.as_mut().unwrap().invoice = Some(InvoiceWatch::new("abc"));
        app.demo_session.as_mut().unwrap().payments.form_paid = true;
        app.poll_invoice(cx);
        let (event, data) = reply(&received);
        assert_eq!(event, "invoice_closed");
        assert_eq!(data["status"], "paid");
        assert!(app.payments.dialog.is_none(), "the checkout closes");
    });
}
