//! TON wallet, the own side, on a demo `QuillApp`: the Money page with its
//! balance, fiat value, history and collectibles; the profile's wallet
//! address with copy; a transfer card opening its transaction; and the
//! "Your first Grams!" hint. Nothing shows while the wallet option is off.

use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, Modifiers, TestAppContext, VisualTestContext, px, size};
use quill::ids::{ChatId, MessageId};
use quill::state::InfoPanelTarget;
use quill::telegram::envelope::{MessageContent, ServiceAction};
use quill::ton_wallet::{
    FIRST_TRANSFER_HINT, OwnWallet, ProfileAddress, TonWalletTransfer, TxKind, WalletNft, WalletTx,
};

const OPEN: ChatId = ChatId(11);
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

fn find(vcx: &mut VisualTestContext, selector: &'static str) -> bool {
    vcx.debug_bounds(selector).is_some()
}

fn click(vcx: &mut VisualTestContext, selector: &'static str) {
    let bounds = vcx.debug_bounds(selector).expect(selector);
    vcx.simulate_click(bounds.center(), Modifiers::default());
    frames(vcx);
}

fn transfer(id: &str, amount: i64) -> WalletTx {
    WalletTx {
        id: id.into(),
        peer_address: ADDRESS.into(),
        peer_user_id: 0,
        peer_domain: String::new(),
        date: 1_700_000_000,
        fee_amount: 0,
        tx_hash: Some("q83v".into()),
        kind: TxKind::Transfer {
            amount,
            is_gasless: false,
            comment: "secret".into(),
            is_comment_encrypted: true,
        },
    }
}

/// Turns the wallet on with a balance, a rate, two transfers and an NFT.
fn seed_wallet(app: &mut QuillApp) {
    let session = app.demo_session.as_mut().expect("demo session");
    let wallet = &mut session.ton_wallet;
    wallet.set_can_use(true);
    wallet.ton_usd_rate = Some(3.0);
    wallet.own = Some(OwnWallet {
        address: ADDRESS.into(),
        gram_amount: 12_500_000_000,
        is_being_created: false,
        is_backup_enabled: true,
    });
    wallet.money.history.apply(
        vec![transfer("t1", 2_000_000_000), transfer("t2", -500_000_000)],
        String::new(),
        false,
    );
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

#[gpui_kit::test]
fn money_page_shows_balance_history_and_collectibles(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, _, cx| {
        // Off: opening does nothing and Settings has no Money row.
        app.open_money(cx);
        assert!(!app.demo_session.as_ref().unwrap().ton_wallet.money.open);
        seed_wallet(app);
        app.open_money(cx);
    });
    frames(vcx);
    assert!(find(vcx, "money-balance"));
    let fiat = app.read_with(vcx, |app, _| {
        app.demo_session
            .as_ref()
            .unwrap()
            .ton_wallet
            .fiat_text(12_500_000_000)
    });
    assert_eq!(fiat, "$37.50");
    assert!(find(vcx, "money-address"));
    assert!(find(vcx, "money-tx-0") && find(vcx, "money-tx-1"));

    click(vcx, "money-tx-0");
    assert!(find(vcx, "money-tx-details"));
    assert!(
        find(vcx, "money-tx-encrypted"),
        "encrypted comment is a placeholder"
    );
    assert!(!find(vcx, "money-tx-comment"));

    app.update_in(vcx, |app, _, cx| {
        app.money_back(cx);
        app.set_money_tab(quill::ton_wallet::MoneyTab::Collectibles, cx);
    });
    frames(vcx);
    assert!(find(vcx, "money-nft-0"));
    click(vcx, "money-nft-0");
    assert!(find(vcx, "money-nft-details"));

    // Turning the option off closes the page and hides everything.
    app.update_in(vcx, |app, _, cx| {
        app.ton_wallet_mut().unwrap().set_can_use(false);
        cx.notify();
    });
    frames(vcx);
    assert!(!find(vcx, "money-balance") && !find(vcx, "money-nft-details"));
}

#[gpui_kit::test]
fn profile_shows_the_wallet_address_and_copies_it(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
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
        session
            .ton_wallet
            .profile_addresses
            .insert(user_id, ProfileAddress::Found(ADDRESS.into()));
        app.open_info_panel_target(InfoPanelTarget::User(user_id), window, cx);
    });
    frames(vcx);
    assert!(
        !find(vcx, "info-ton-wallet"),
        "hidden while the option is off"
    );

    app.update_in(vcx, |app, _, cx| {
        app.ton_wallet_mut().unwrap().can_use = true;
        cx.notify();
    });
    frames(vcx);
    assert!(find(vcx, "info-ton-wallet"));
    click(vcx, "info-ton-wallet");
    let copied = vcx.read_from_clipboard().and_then(|item| item.text());
    assert_eq!(copied.as_deref(), Some(ADDRESS));
}

#[gpui_kit::test]
fn a_transfer_card_opens_the_transaction_and_the_first_grams_hint(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let row = 90_000_000_011;
    app.update_in(vcx, |app, _, _| {
        seed_wallet(app);
        let session = app.demo_session.as_mut().unwrap();
        session
            .chat_list
            .suggestions
            .actions
            .insert(FIRST_TRANSFER_HINT.into());
        session.open_chat(OPEN);
        let history = session.histories.get_mut(&OPEN.0).expect("open history");
        let mut message = history
            .messages
            .values()
            .next_back()
            .expect("a message")
            .clone();
        message.id = MessageId(row);
        message.is_outgoing = false;
        message.media_album_id = 0;
        message.forward_info = None;
        message.reply_to = None;
        message.content = MessageContent::Action(Box::new(ServiceAction::TonWalletTransfer(
            Box::new(TonWalletTransfer {
                sender_user_id: OPEN.0,
                transaction_id: "t1".into(),
                peer_address: ADDRESS.into(),
                amount: 2_000_000_000,
                comment: String::new(),
                is_comment_encrypted: false,
            }),
        )));
        history.messages.insert(row, message);
    });
    frames(vcx);
    click(vcx, "ton-transfer-card-90000000011");
    assert!(find(vcx, "money-first-grams"), "first incoming transfer");
    click(vcx, "money-hint-got-it");
    assert!(!find(vcx, "money-first-grams"));
    assert!(find(vcx, "money-tx-details"), "the transaction stays open");
    let suggested = app.read_with(vcx, |app, _| {
        app.demo_session
            .as_ref()
            .unwrap()
            .chat_list
            .suggestions
            .actions
            .contains(FIRST_TRANSFER_HINT)
    });
    assert!(!suggested, "dismissed for good");
}
