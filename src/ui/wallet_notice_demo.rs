//! `ready-wallet-notice`: the wallet surfaces over the ready chat list
//! (injected, no live Telegram). `QUILL_DEMO_WALLET_NOTICE` picks one:
//! `money` (the Money page and its actions), `notice` (the Money page with
//! the notice over it), `details` (a transaction with an encrypted
//! comment), `chat` (a transfer card and a TON Connect request in the open
//! chat), `address` (a TON address menu) or `profile` (a profile with the
//! wallet address, balance and Send Money). The GPUI tests in
//! `wallet_notice_tests.rs` drive the same surfaces.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::screenshot_demo::{DemoSpec, register_demos};
use gpui_kit::*;
use quill::ids::{ChatId, MessageId};
use quill::state::InfoPanelTarget;
use quill::telegram::envelope::{MessageContent, ServiceAction};
use quill::text::LinkTarget;
use quill::ton_wallet::{
    OwnWallet, ProfileAddress, TonConnectRequest, TonConnectRequestState, TonWalletTransfer,
    TxKind, WalletNft, WalletSurface, WalletTx,
};

register_demos![
    DemoSpec::chats(
        "ready-wallet-notice",
        "screenshot demo — wallet surfaces and the not-supported notice (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_wallet_notice),
];

const OPEN: ChatId = ChatId(11);
const PROFILE_USER: i64 = 4_242;
const ADDRESS: &str = "EQAHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHB8Jh";

fn demo_mode() -> String {
    std::env::var("QUILL_DEMO_WALLET_NOTICE").unwrap_or_else(|_| "notice".into())
}

fn demo_tx() -> WalletTx {
    WalletTx {
        id: "t1".into(),
        peer_address: ADDRESS.into(),
        peer_user_id: 0,
        peer_domain: String::new(),
        date: 1_700_000_000,
        fee_amount: 0,
        tx_hash: Some("q83v".into()),
        kind: TxKind::Transfer {
            amount: 2_000_000_000,
            is_gasless: false,
            comment: "c2VjcmV0".into(),
            is_comment_encrypted: true,
        },
    }
}

fn add_service(session: &mut quill::state::Session, id: i64, action: ServiceAction) {
    session.open_chat(OPEN);
    let Some(history) = session.histories.get_mut(&OPEN.0) else {
        return;
    };
    let Some(mut message) = history.messages.values().next_back().cloned() else {
        return;
    };
    message.id = MessageId(id);
    message.is_outgoing = false;
    message.media_album_id = 0;
    message.forward_info = None;
    message.reply_to = None;
    message.content = MessageContent::Action(Box::new(action));
    history.messages.insert(id, message);
}

impl QuillApp {
    fn demo_wallet_notice(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mode = demo_mode();
        if let Some(session) = self.demo_session.as_mut() {
            let wallet = &mut session.ton_wallet;
            wallet.set_can_use(true);
            wallet.ton_usd_rate = Some(3.0);
            wallet.own = Some(OwnWallet {
                address: ADDRESS.into(),
                gram_amount: 12_500_000_000,
                is_being_created: false,
                is_backup_enabled: true,
            });
            wallet
                .money
                .history
                .apply(vec![demo_tx()], String::new(), false);
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
            if mode == "chat" {
                add_service(
                    session,
                    90_000_000_021,
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
                    session,
                    90_000_000_022,
                    ServiceAction::TonConnectRequest(TonConnectRequest {
                        session_id: 1,
                        state: TonConnectRequestState::Pending { expiration_date: 0 },
                        dapp_name: "Fragment".into(),
                        topic: String::new(),
                    }),
                );
            }
            if mode == "profile" {
                session.users.insert(
                    PROFILE_USER,
                    quill::telegram::envelope::ParsedUser {
                        id: PROFILE_USER,
                        first_name: "Dana".into(),
                        last_name: "Cole".into(),
                        ..Default::default()
                    },
                );
                session
                    .ton_wallet
                    .profile_addresses
                    .insert(PROFILE_USER, ProfileAddress::Found(ADDRESS.into()));
            }
        }
        match mode.as_str() {
            "money" => self.open_money(cx),
            "notice" => {
                self.open_money(cx);
                self.show_wallet_notice(WalletSurface::Send, cx);
            }
            "details" => {
                self.open_money(cx);
                self.select_wallet_tx(demo_tx(), cx);
            }
            "address" => {
                self.queue_link(LinkTarget::TonAddress(ADDRESS.into()), (OPEN.0, 1), cx);
            }
            "profile" => {
                self.open_info_panel_target(InfoPanelTarget::User(PROFILE_USER), window, cx);
            }
            _ => {}
        }
        cx.notify();
    }
}
