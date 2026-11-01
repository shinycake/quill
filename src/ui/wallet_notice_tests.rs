//! Each wallet surface opens the shared notice; its button opens the official apps page.

use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::{DialogKind, QuillShell};
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, Modifiers, TestAppContext, VisualTestContext, px, size};
use quill::ids::{ChatId, MessageId};
use quill::state::InfoPanelTarget;
use quill::telegram::envelope::{MessageContent, ServiceAction};
use quill::text::LinkTarget;
use quill::ton_wallet::{
    OwnWallet, ProfileAddress, TonConnectRequest, TonConnectRequestState, TonWalletTransfer,
    TxKind, WalletNft, WalletSurface, WalletTx,
};

const OPEN: ChatId = ChatId(11);
const ADDRESS: &str = "EQAHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHB8Jh";

fn new_app(
    cx: &mut TestAppContext,
    demo: &'static str,
) -> (Entity<QuillApp>, Entity<QuillShell>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named(demo);
    let handle = cx.open_window(size(px(1100.), px(900.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
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

fn find(vcx: &mut VisualTestContext, selector: &str) -> bool {
    let selector: &'static str = Box::leak(selector.to_string().into_boxed_str());
    vcx.debug_bounds(selector).is_some()
}

fn click(vcx: &mut VisualTestContext, selector: &str) {
    let selector: &'static str = Box::leak(selector.to_string().into_boxed_str());
    let bounds = vcx.debug_bounds(selector).expect(selector);
    vcx.simulate_click(bounds.center(), Modifiers::default());
    frames(vcx);
}

fn notice(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Option<WalletSurface> {
    app.read_with(vcx, |app, _| {
        app.session().and_then(|s| s.ton_wallet.notice)
    })
}

fn dialog(shell: &Entity<QuillShell>, vcx: &mut VisualTestContext) -> Option<DialogKind> {
    shell.read_with(vcx, |shell, _| shell.open_dialog)
}

fn transfer(id: &str, encrypted: bool) -> WalletTx {
    WalletTx {
        id: id.into(),
        peer_address: ADDRESS.into(),
        peer_user_id: 0,
        peer_domain: String::new(),
        date: 1_700_000_000,
        fee_amount: 0,
        tx_hash: Some("q83v".into()),
        kind: TxKind::Transfer {
            amount: 2_000_000_000,
            is_gasless: false,
            comment: "secret".into(),
            is_comment_encrypted: encrypted,
        },
    }
}

fn seed_wallet(app: &mut QuillApp, with_wallet: bool) {
    let session = app.demo_session.as_mut().expect("demo session");
    let wallet = &mut session.ton_wallet;
    wallet.set_can_use(true);
    wallet.own = Some(OwnWallet {
        address: if with_wallet {
            ADDRESS.into()
        } else {
            String::new()
        },
        gram_amount: 12_500_000_000,
        is_being_created: false,
        is_backup_enabled: true,
    });
    wallet
        .money
        .history
        .apply(vec![transfer("t1", true)], String::new(), false);
    wallet.money.nfts.apply(
        vec![WalletNft {
            address: "EQnft".into(),
            name: "Cat #7".into(),
            index: "7".into(),
            ..WalletNft::default()
        }],
        String::new(),
        false,
    );
}

fn add_service(app: &mut QuillApp, id: i64, action: ServiceAction) {
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
    message.content = MessageContent::Action(Box::new(action));
    history.messages.insert(id, message);
}

#[gpui_kit::test]
fn the_button_opens_the_official_apps_page_and_ok_opens_nothing(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    frames(vcx);
    assert_eq!(dialog(&shell, vcx), None);
    app.update_in(vcx, |app, _, cx| {
        app.show_wallet_notice(WalletSurface::Send, cx)
    });
    frames(vcx);
    assert_eq!(dialog(&shell, vcx), Some(DialogKind::WalletNotice));
    assert!(find(vcx, "wallet-notice-text"));
    assert!(find(vcx, "wallet-notice-ok"));

    click(vcx, "wallet-notice-ok");
    assert_eq!(notice(&app, vcx), None);
    assert_eq!(dialog(&shell, vcx), None);
    assert_eq!(
        quill::platform::os_opened_last(),
        None,
        "OK never opens the browser"
    );

    app.update_in(vcx, |app, _, cx| {
        app.show_wallet_notice(WalletSurface::Receive, cx)
    });
    frames(vcx);
    click(vcx, "wallet-notice-open");
    assert_eq!(
        quill::platform::os_opened_last().as_deref(),
        Some("https://telegram.org/apps")
    );
    assert_eq!(notice(&app, vcx), None);
    assert_eq!(dialog(&shell, vcx), None);
}

#[gpui_kit::test]
fn the_money_page_actions_open_the_notice_and_the_page_returns(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    app.update_in(vcx, |app, _, cx| {
        seed_wallet(app, true);
        app.open_money(cx);
    });
    frames(vcx);
    assert_eq!(dialog(&shell, vcx), Some(DialogKind::Money));
    for (selector, surface) in [
        ("money-send", WalletSurface::Send),
        ("money-receive", WalletSurface::Receive),
        ("money-add-funds", WalletSurface::AddFunds),
        ("money-backup", WalletSurface::Backup),
    ] {
        click(vcx, selector);
        assert_eq!(notice(&app, vcx), Some(surface), "{selector}");
        assert_eq!(dialog(&shell, vcx), Some(DialogKind::WalletNotice));
        click(vcx, "wallet-notice-ok");
        assert_eq!(
            dialog(&shell, vcx),
            Some(DialogKind::Money),
            "the Money page is still open under the notice"
        );
    }
}

#[gpui_kit::test]
fn a_missing_wallet_offers_create_which_opens_the_notice(cx: &mut TestAppContext) {
    let (app, _shell, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    app.update_in(vcx, |app, _, cx| {
        seed_wallet(app, false);
        app.open_money(cx);
    });
    frames(vcx);
    assert!(!find(vcx, "money-send"), "no wallet, nothing to send from");
    click(vcx, "money-create");
    assert_eq!(notice(&app, vcx), Some(WalletSurface::CreateWallet));
}

#[gpui_kit::test]
fn details_open_the_notice_for_send_and_encrypted_comments(cx: &mut TestAppContext) {
    let (app, _shell, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    app.update_in(vcx, |app, _, cx| {
        seed_wallet(app, true);
        app.open_money(cx);
        app.set_money_tab(quill::ton_wallet::MoneyTab::Transactions, cx);
        app.select_wallet_tx(transfer("t1", true), cx);
    });
    frames(vcx);
    assert!(find(vcx, "money-tx-encrypted"));
    click(vcx, "money-tx-encrypted");
    assert_eq!(notice(&app, vcx), Some(WalletSurface::EncryptedComment));
    click(vcx, "wallet-notice-ok");

    app.update_in(vcx, |app, _, cx| {
        app.money_back(cx);
        app.select_wallet_nft(Some("EQnft".into()), cx);
    });
    frames(vcx);
    click(vcx, "money-nft-send");
    assert_eq!(notice(&app, vcx), Some(WalletSurface::SendNft));
}

#[gpui_kit::test]
fn the_first_grams_hint_send_opens_the_notice(cx: &mut TestAppContext) {
    let (app, _shell, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    app.update_in(vcx, |app, _, cx| {
        seed_wallet(app, true);
        app.open_money(cx);
        if let Some(wallet) = app.ton_wallet_mut() {
            wallet.money.hint_open = true;
        }
        cx.notify();
    });
    frames(vcx);
    click(vcx, "money-hint-send");
    assert_eq!(notice(&app, vcx), Some(WalletSurface::Send));
}

#[gpui_kit::test]
fn chat_surfaces_open_the_notice(cx: &mut TestAppContext) {
    let (app, _shell, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    let card = 90_000_000_021;
    let pending = 90_000_000_022;
    let answered = 90_000_000_023;
    app.update_in(vcx, |app, _, _| {
        seed_wallet(app, true);
        add_service(
            app,
            card,
            ServiceAction::TonWalletTransfer(Box::new(TonWalletTransfer {
                sender_user_id: OPEN.0,
                transaction_id: "t1".into(),
                peer_address: ADDRESS.into(),
                amount: 2_000_000_000,
                comment: "c2VjcmV0".into(),
                is_comment_encrypted: true,
            })),
        );
        let request = |state| TonConnectRequest {
            session_id: 1,
            state,
            dapp_name: "Fragment".into(),
            topic: String::new(),
        };
        add_service(
            app,
            pending,
            ServiceAction::TonConnectRequest(request(TonConnectRequestState::Pending {
                expiration_date: 0,
            })),
        );
        add_service(
            app,
            answered,
            ServiceAction::TonConnectRequest(request(TonConnectRequestState::Accepted)),
        );
    });
    frames(vcx);
    click(vcx, &format!("ton-transfer-encrypted-open-{card}"));
    assert_eq!(notice(&app, vcx), Some(WalletSurface::EncryptedComment));
    click(vcx, "wallet-notice-ok");

    assert!(
        !find(vcx, &format!("ton-connect-review-{answered}")),
        "an answered request has nothing to review"
    );
    click(vcx, &format!("ton-connect-review-{pending}"));
    assert_eq!(notice(&app, vcx), Some(WalletSurface::ConnectRequest));
    click(vcx, "wallet-notice-ok");

    app.update_in(vcx, |app, _, cx| {
        app.queue_link(LinkTarget::TonAddress(ADDRESS.into()), (OPEN.0, 1), cx);
    });
    frames(vcx);
    click(vcx, "link-popup-send-money");
    assert_eq!(notice(&app, vcx), Some(WalletSurface::SendMoney));
}

#[gpui_kit::test]
fn the_address_menu_offers_no_send_money_while_the_wallet_is_off(cx: &mut TestAppContext) {
    let (app, _shell, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    app.update_in(vcx, |app, _, cx| {
        app.queue_link(LinkTarget::TonAddress(ADDRESS.into()), (OPEN.0, 1), cx);
    });
    frames(vcx);
    assert!(find(vcx, "link-popup-explorer"));
    assert!(!find(vcx, "link-popup-send-money"));
}

#[gpui_kit::test]
fn a_profile_shows_send_money_and_the_card_balance(cx: &mut TestAppContext) {
    let (app, _shell, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    let user_id = 4_242;
    app.update_in(vcx, |app, window, cx| {
        let session = app.demo_session.as_mut().unwrap();
        session.users.insert(
            user_id,
            quill::telegram::envelope::ParsedUser {
                id: user_id,
                first_name: "Dana".into(),
                last_name: "Cole".into(),
                ..Default::default()
            },
        );
        session.ton_wallet.set_can_use(true);
        session
            .ton_wallet
            .profile_addresses
            .insert(user_id, ProfileAddress::Found(ADDRESS.into()));
        app.open_info_panel_target(InfoPanelTarget::User(user_id), window, cx);
    });
    frames(vcx);
    assert!(find(vcx, "info-ton-wallet"));
    assert!(!find(vcx, "info-ton-wallet-balance"), "no card, no balance");
    click(vcx, "info-ton-wallet-send");
    assert_eq!(notice(&app, vcx), Some(WalletSurface::SendMoney));
}

#[gpui_kit::test]
fn move_to_my_wallet_opens_the_notice_once_the_gift_is_unlocked(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = new_app(cx, "ready-gift-transfer");
    let vcx = &mut vcx;
    vcx.run_until_parked();
    assert_eq!(dialog(&shell, vcx), Some(DialogKind::GiftTransfer));
    app.update_in(vcx, |app, _, cx| {
        if let Some(wallet) = app.ton_wallet_mut() {
            wallet.set_can_use(true);
        }
        app.tap_transfer_export(cx);
    });
    assert_eq!(notice(&app, vcx), None);
    app.update_in(vcx, |app, window, cx| {
        app.transfer_step_back(window, cx);
        if let Some(session) = app.demo_session.as_mut() {
            for gift in &mut session.payments.hub.gifts {
                gift.manage.export_date = 1;
            }
        }
        app.tap_transfer_export(cx);
    });
    assert_eq!(notice(&app, vcx), Some(WalletSurface::GiftToWallet));
}

#[gpui_kit::test]
fn the_encrypted_comment_and_review_request_do_nothing_while_the_wallet_is_off(
    cx: &mut TestAppContext,
) {
    let (app, _shell, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    let card = 90_000_000_031;
    let pending = 90_000_000_032;
    app.update_in(vcx, |app, _, _| {
        add_service(
            app,
            card,
            ServiceAction::TonWalletTransfer(Box::new(TonWalletTransfer {
                sender_user_id: OPEN.0,
                transaction_id: "t1".into(),
                peer_address: ADDRESS.into(),
                amount: 2_000_000_000,
                comment: "c2VjcmV0".into(),
                is_comment_encrypted: true,
            })),
        );
        add_service(
            app,
            pending,
            ServiceAction::TonConnectRequest(TonConnectRequest {
                session_id: 1,
                state: TonConnectRequestState::Pending { expiration_date: 0 },
                dapp_name: "Fragment".into(),
                topic: String::new(),
            }),
        );
    });
    frames(vcx);
    assert!(find(vcx, &format!("ton-transfer-encrypted-{card}")));
    assert!(!find(vcx, &format!("ton-transfer-encrypted-open-{card}")));
    assert!(!find(vcx, &format!("ton-connect-review-{pending}")));
}

#[gpui_kit::test]
fn a_bot_profile_has_no_send_money(cx: &mut TestAppContext) {
    let (app, _shell, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    let user_id = 4_243;
    app.update_in(vcx, |app, window, cx| {
        let session = app.demo_session.as_mut().unwrap();
        let json = r#"{"@type":"updateUser","user":{"id":4243,"first_name":"Helper","type":{"@type":"userTypeBot"}}}"#;
        let envelope = quill::telegram::envelope::parse_envelope(json).unwrap();
        let seq = session.last_seq + 1;
        session.apply(quill::telegram::OwnedEnvelope {
            seq,
            client_id: None,
            envelope,
        });
        assert!(session.is_bot_user(user_id));
        session.ton_wallet.set_can_use(true);
        session
            .ton_wallet
            .profile_addresses
            .insert(user_id, ProfileAddress::Found(ADDRESS.into()));
        app.open_info_panel_target(InfoPanelTarget::User(user_id), window, cx);
    });
    frames(vcx);
    assert!(find(vcx, "info-ton-wallet"));
    assert!(!find(vcx, "info-ton-wallet-send"));
}
