//! TON wallet, phase 1, on a demo `QuillApp`: transfer cards draw with the
//! public comment or the hidden placeholder, a TON address opens the
//! Copy Address / View in Explorer menu, and nothing is sent.

use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::ids::{ChatId, MessageId};
use quill::telegram::envelope::{MessageContent, ServiceAction};
use quill::text::LinkTarget;
use quill::ton_wallet::{TonWalletTransfer, is_valid_ton_address};

const OPEN: ChatId = ChatId(11);
/// A checksummed address (flag 0x11, workchain 0, hash of 7s).
const ADDRESS: &str = "EQAHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHB8Jh";

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
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
    let app: Entity<QuillApp> = slot.borrow().clone().unwrap();
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    (app, vcx)
}

fn frames(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    for _ in 0..2 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

fn find(vcx: &mut VisualTestContext, selector: String) -> bool {
    let selector: &'static str = Box::leak(selector.into_boxed_str());
    vcx.debug_bounds(selector).is_some()
}

/// Adds a transfer as the newest message of the open chat.
fn add_transfer(app: &mut QuillApp, id: i64, comment: &str, encrypted: bool) {
    let session = app.demo_session.as_mut().expect("demo session");
    session.open_chat(OPEN);
    let history = session.histories.get_mut(&OPEN.0).expect("open history");
    let mut message = history
        .messages
        .values()
        .next_back()
        .expect("a message")
        .clone();
    message.id = MessageId(id);
    message.is_outgoing = false;
    message.media_album_id = 0;
    message.forward_info = None;
    message.reply_to = None;
    message.content = MessageContent::Action(Box::new(ServiceAction::TonWalletTransfer(Box::new(
        TonWalletTransfer {
            sender_user_id: OPEN.0,
            transaction_id: "tx".into(),
            peer_address: ADDRESS.into(),
            amount: 2_500_000_000,
            comment: comment.into(),
            is_comment_encrypted: encrypted,
        },
    ))));
    history.messages.insert(id, message);
}

#[gpui_kit::test]
fn transfer_cards_show_the_comment_or_hide_an_encrypted_one(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    assert!(
        is_valid_ton_address(ADDRESS),
        "the fixture is a real address"
    );
    app.update_in(vcx, |app, _, _| {
        add_transfer(app, 90_000_000_001, "for lunch", false);
        add_transfer(app, 90_000_000_002, "c2VjcmV0", true);
    });
    frames(vcx);
    assert!(find(vcx, "ton-transfer-card-90000000001".into()));
    assert!(find(vcx, "ton-transfer-comment-90000000001".into()));
    assert!(!find(vcx, "ton-transfer-encrypted-90000000001".into()));
    assert!(find(vcx, "ton-transfer-card-90000000002".into()));
    assert!(find(vcx, "ton-transfer-encrypted-90000000002".into()));
    assert!(
        !find(vcx, "ton-transfer-comment-90000000002".into()),
        "an encrypted comment is never printed"
    );
    let line = app.read_with(vcx, |app, _| {
        let session = app.demo_session.as_ref().unwrap();
        let message = session
            .histories
            .get(&OPEN.0)
            .and_then(|h| h.messages.get(&90_000_000_001))
            .unwrap();
        session.service_text(message).unwrap().plain()
    });
    assert!(line.ends_with(" sent you 2.5 Grams"), "{line}");
}

#[gpui_kit::test]
fn a_ton_address_opens_copy_and_explorer(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    frames(vcx);
    app.update_in(vcx, |app, _, cx| {
        app.queue_link(LinkTarget::TonAddress(ADDRESS.into()), (OPEN.0, 1), cx);
    });
    frames(vcx);
    let popup = app.read_with(vcx, |app, _| {
        app.message_ui.link_popup.as_ref().map(|p| p.link.clone())
    });
    assert_eq!(popup, Some(LinkTarget::TonAddress(ADDRESS.into())));
    assert!(find(vcx, "link-popup-explorer".into()), "View in Explorer");
    let url = app.read_with(vcx, |app, _| {
        app.demo_session
            .as_ref()
            .unwrap()
            .ton_wallet
            .explorer_url_for(ADDRESS)
    });
    assert_eq!(url, format!("https://tonviewer.com/{ADDRESS}"));
    // Copy Address puts the address on the clipboard and closes the menu.
    app.update_in(vcx, |app, _, cx| {
        app.message_ui.link_popup = None;
        app.copy_ton_address(ADDRESS.to_string(), cx);
    });
    let copied = vcx.read_from_clipboard().and_then(|item| item.text());
    assert_eq!(copied.as_deref(), Some(ADDRESS));
    app.read_with(vcx, |app, _| {
        assert_eq!(app.connection.status_note, "address copied to clipboard");
    });
}
