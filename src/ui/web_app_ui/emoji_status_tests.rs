//! The emoji status prompts of a mini app window, driven with `cat` standing in
//! for the web view helper: what the window writes to the helper's stdin comes
//! back on its stdout, where the test reads the popups and bridge replies.

use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::web_app_ui::helper_pipe::HelperWriter;
use crate::ui::web_app_ui::{MiniAppWindow, PopupPurpose, theme_params};
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::ids::FileId;
use quill::telegram::envelope::{BotInfo, StickerFormat, StickerItem};
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

/// Open a window for the bot and return what the helper would receive.
fn open_window(
    app: &mut QuillApp,
    allowed: bool,
    cx: &mut gpui_kit::App,
) -> mpsc::Receiver<HostCommand> {
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
    app.demo_session.as_mut().unwrap().bots.bot_info.insert(
        BOT,
        Some(BotInfo {
            can_manage_emoji_status: allowed,
            ..BotInfo::default()
        }),
    );
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

fn emoji_sticker(id: i64, alt: &str) -> StickerItem {
    StickerItem {
        custom_emoji_id: Some(id),
        id,
        set_id: 0,
        emoji: alt.into(),
        width: 100,
        height: 100,
        format: StickerFormat::Webp,
        file_id: FileId(0),
        thumb_file_id: None,
        thumb_width: 0,
        thumb_height: 0,
        requires_premium: false,
        premium_animation: None,
    }
}

#[gpui_kit::test]
fn access_is_granted_at_once_when_the_bot_already_has_it(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    app.update_in(&mut vcx, |app, _, cx| {
        let received = open_window(app, true, cx);
        app.handle_web_app_event(WebAppEvent::RequestEmojiStatusAccess, cx);
        let (event, data) = reply(&received);
        assert_eq!(event, "emoji_status_access_requested");
        assert_eq!(data["status"], "allowed");
    });
}

#[gpui_kit::test]
fn access_asks_and_a_cancel_answers_cancelled(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    app.update_in(&mut vcx, |app, _, cx| {
        let received = open_window(app, false, cx);
        app.handle_web_app_event(WebAppEvent::RequestEmojiStatusAccess, cx);
        let popup = prompt(&received);
        assert!(
            popup
                .message
                .contains("Quillbot requests access to set your emoji status")
        );
        assert_eq!(
            popup
                .buttons
                .iter()
                .map(|b| b.id.as_str())
                .collect::<Vec<_>>(),
            ["cancel", "allow"]
        );
        // A second request while the prompt is up is refused.
        app.handle_web_app_event(WebAppEvent::RequestEmojiStatusAccess, cx);
        assert_eq!(reply(&received).1["status"], "cancelled");

        answer(app, &popup.id, "cancel", cx);
        let (event, data) = reply(&received);
        assert_eq!(event, "emoji_status_access_requested");
        assert_eq!(data["status"], "cancelled");
        assert!(!app.bot_has_emoji_status_access(BOT), "nothing was granted");
    });
}

#[gpui_kit::test]
fn allow_without_premium_asks_again_instead_of_granting(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    app.update_in(&mut vcx, |app, _, cx| {
        let received = open_window(app, false, cx);
        app.demo_session.as_mut().unwrap().payments.premium_option = Some(false);
        app.handle_web_app_event(WebAppEvent::RequestEmojiStatusAccess, cx);
        let popup = prompt(&received);
        answer(app, &popup.id, "allow", cx);
        // The prompt comes back (tdesktop's box stays open behind the
        // Premium preview) and nothing was granted.
        let again = prompt(&received);
        assert_ne!(again.id, popup.id);
        assert!(!app.bot_has_emoji_status_access(BOT));
    });
}

#[gpui_kit::test]
fn a_suggested_emoji_is_confirmed_and_a_decline_says_so(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    app.update_in(&mut vcx, |app, _, cx| {
        let received = open_window(app, false, cx);
        app.demo_session
            .as_mut()
            .unwrap()
            .stickers
            .emoji
            .custom_emoji_stickers
            .push(emoji_sticker(777, "\u{1F31F}"));
        app.handle_web_app_event(
            WebAppEvent::SetEmojiStatus {
                custom_emoji_id: 777,
                duration_secs: 3600,
            },
            cx,
        );
        let popup = prompt(&received);
        assert_eq!(popup.title, "Set Emoji Status");
        assert!(popup.message.contains("suggested by Quillbot"));
        assert!(
            popup.message.contains('\u{1F31F}'),
            "the emoji is named in the prompt"
        );
        assert!(matches!(
            app.mini_apps.window.as_ref().unwrap().popups.get(&popup.id),
            Some(PopupPurpose::EmojiStatus)
        ));
        answer(app, &popup.id, "cancel", cx);
        let (event, data) = reply(&received);
        assert_eq!(event, "emoji_status_failed");
        assert_eq!(data["error"], "USER_DECLINED");
        assert!(app.mini_apps.window.as_ref().unwrap().emoji.set_is_idle());
    });
}

#[gpui_kit::test]
fn an_emoji_that_cannot_be_found_fails_as_invalid(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    app.update_in(&mut vcx, |app, _, cx| {
        let received = open_window(app, false, cx);
        // No session to look it up in (the demo has no driver): the lookup
        // cannot be sent, so the suggestion is invalid.
        app.handle_web_app_event(
            WebAppEvent::SetEmojiStatus {
                custom_emoji_id: 4242,
                duration_secs: 0,
            },
            cx,
        );
        let (event, data) = reply(&received);
        assert_eq!(event, "emoji_status_failed");
        assert_eq!(data["error"], "SUGGESTED_EMOJI_INVALID");
        // The request was dropped with the failure: a new one is looked at
        // afresh.
        app.handle_web_app_event(
            WebAppEvent::SetEmojiStatus {
                custom_emoji_id: 4242,
                duration_secs: 0,
            },
            cx,
        );
        assert_eq!(reply(&received).0, "emoji_status_failed");
    });
}
