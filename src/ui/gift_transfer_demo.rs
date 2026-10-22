//! `ready-gift-transfer`: the transfer picker of one of the user's
//! collectibles over the ready chat list (injected, no live Telegram;
//! nothing can be transferred or exported). `QUILL_DEMO_GIFT_TRANSFER`
//! picks the step: `picker` (default, export still locked), `locked`,
//! `confirm`, `probing`, `password`, `peer`, `open-chat` (the open chat's
//! user listed first), `wallet` (unlocked, wallet on: "Move to My Wallet")
//! or `action-locked`. The GPUI tests in
//! `gift_transfer_tests.rs` drive the same demo.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::gift_market_demo::{ME, listing, seed_gift_market_session};
use super::screenshot_demo::{DemoSpec, register_demos};
use gpui_kit::*;
use quill::diagnostics::MemorySink;
use quill::gift_manage::{ExportState, GiftFlow, ProbeOutcome, TransferPicker};
use quill::local_time::now_unix;
use quill::premium_hub::parse_received_gift;
use quill::state::Session;
use quill::telegram::envelope::MessageSender;
use serde_json::{Value, json};
use std::sync::Arc;

register_demos![
    DemoSpec::ready(
        "ready-gift-transfer",
        seed_gift_transfer_session,
        "screenshot demo — gift transfer picker and Fragment export steps (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_gift_transfer),
];

/// The received gift the demo transfers.
pub(super) const DEMO_GIFT: &str = "own-1204";
/// The demo's contacts (from the gift market fixtures).
pub(super) const CONTACTS: [i64; 4] = [502, 503, 504, 505];
/// Five days and a bit until the export unlocks.
const LOCKED_FOR: i64 = 5 * 86_400 + 600;

/// The step `QUILL_DEMO_GIFT_TRANSFER` asks for.
fn demo_mode() -> String {
    std::env::var("QUILL_DEMO_GIFT_TRANSFER").unwrap_or_default()
}

/// The export opens now for the steps after "Send via Blockchain".
fn export_date(mode: &str, now: i64) -> i64 {
    match mode {
        "confirm" | "password" | "probing" | "wallet" => now - 60,
        _ => now + LOCKED_FOR,
    }
}

/// One of the user's collectibles, `receivedGift` JSON.
pub(super) fn own_received_gift(export_date: i64, in_wallet: bool) -> Value {
    let mut upgraded = listing(1204, 0, 0, 0)
        .get("gift")
        .cloned()
        .unwrap_or(Value::Null);
    if let Some(object) = upgraded.as_object_mut() {
        object.insert("resale_parameters".into(), Value::Null);
        object.insert(
            "owner_id".into(),
            if in_wallet {
                Value::Null
            } else {
                json!({"@type": "messageSenderUser", "user_id": ME})
            },
        );
        if in_wallet {
            object.insert("owner_address".into(), json!("EQDemoWalletAddress"));
        }
    }
    json!({"@type": "receivedGift", "received_gift_id": DEMO_GIFT, "date": now_unix() - 86_400,
        "is_saved": true, "can_be_transferred": !in_wallet, "transfer_star_count": 0,
        "next_transfer_date": 0, "next_resale_date": 0, "export_date": export_date,
        "gift": {"@type": "sentGiftUpgraded", "gift": upgraded}})
}

pub(super) fn seed_gift_transfer_session(sink: Arc<MemorySink>) -> Session {
    let mut session = seed_gift_market_session(sink);
    let mode = demo_mode();
    let now = now_unix();
    session.users_state.contacts = Some(CONTACTS.to_vec());
    session.ton_wallet.set_can_use(mode == "wallet");
    let hub = &mut session.payments.hub;
    hub.gifts_open = true;
    hub.gifts_owner = Some(MessageSender::User { user_id: ME });
    hub.gifts = parse_received_gift(&own_received_gift(
        export_date(&mode, now),
        mode == "action-locked",
    ))
    .into_iter()
    .collect();
    hub.gifts_total = 1;
    hub.gifts_loaded = true;
    hub.gift_selected = Some(DEMO_GIFT.into());
    // The chat with Maya is open: her row comes first.
    if mode == "open-chat" {
        session.open_chat(quill::ids::ChatId(502));
    }
    session
}

impl QuillApp {
    fn demo_gift_transfer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mode = demo_mode();
        self.open_gift_transfer(DEMO_GIFT, window, cx);
        let export = self
            .session()
            .and_then(|s| s.payments.hub.gifts.first())
            .map_or(ExportState::Unavailable, |g| g.export_state(now_unix()));
        let edit = |picker: &mut TransferPicker| match mode.as_str() {
            "locked" | "confirm" => picker.tap_export(export),
            "password" => {
                picker.tap_export(export);
                picker.open_fragment();
                picker.probe_failed(ProbeOutcome::Other, Some(true));
            }
            // The password-less probe after "Open Fragment" is out.
            "probing" => {
                picker.tap_export(export);
                picker.open_fragment();
            }
            "peer" => picker.choose(502, "Maya Chen"),
            _ => {}
        };
        if let Some(Some(GiftFlow::Transfer(picker))) =
            self.gift_manage_mut().map(|m| m.flow.as_mut())
        {
            edit(picker);
        }
        cx.notify();
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "gift_transfer_tests.rs"]
mod tests;
