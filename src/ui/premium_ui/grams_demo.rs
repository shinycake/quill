//! `ready-grams`: Settings → My Grams over the ready chat list, filled from
//! the recorded `tonTransactions` fixture (injected, no live Telegram).
//! `QUILL_DEMO_GRAMS` picks the view: unset or `overview` for the screen,
//! `receipt` for a gift sale's receipt, `settings` for the Settings list
//! with the "My Grams" row, `withdraw` for Collect's password box and
//! `withdraw-check` for its security check (no password set) and
//! `loading-more` for the history with the next page on its way.
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use crate::ui::demo::seed_ready_chats_session;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use crate::ui::status_note::StatusNote;
use gpui_kit::*;
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::premium_hub::ton::{GramRevenue, GramWithdraw, parse_ton_transactions};
use quill::state::Session;
use quill::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

/// The people and the channel the fixture's rows name, so the capture
/// shows names, not placeholders.
const PEERS: [&str; 3] = [
    r#"{"@type":"updateUser","user":{"id":11,"first_name":"Maya","last_name":"Chen","type":{"@type":"userTypeRegular"}}}"#,
    r#"{"@type":"updateUser","user":{"id":12,"first_name":"Leo","last_name":"Park","type":{"@type":"userTypeRegular"}}}"#,
    r#"{"@type":"updateChatTitle","chat_id":13,"title":"Design Weekly"}"#,
];

/// The recorded first page of the history (every transaction type).
pub(in crate::ui) const FIXTURE: &str =
    include_str!("../../../tests/fixtures/grams/ton_transactions.json");

register_demos![
    DemoSpec::ready(
        "ready-grams",
        seed_grams_session,
        "screenshot demo — My Grams: balance, Fragment top-up, withdrawal and history (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_grams),
];

/// The fixture page and a withdrawable revenue on the ready chat list.
pub(in crate::ui) fn seed_grams_session(sink: Arc<MemorySink>) -> Session {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let mut session = seed_ready_chats_session(sink);
    // Continue the seed's envelope sequence; older ones are ignored.
    let seq = AtomicU64::new(session.last_seq);
    for json in PEERS {
        if let Some(owned) = copy_and_parse(json, &seq, &dyn_sink) {
            session.apply(owned);
        }
    }
    let page = serde_json::from_str(FIXTURE)
        .map(|value| parse_ton_transactions(&value))
        .unwrap_or_default();
    let grams = &mut session.payments.hub.grams;
    grams.probed = true;
    grams.apply_page(page, false);
    grams.revenue = Some(GramRevenue {
        available_nano: 18_500_000_000,
        balance_nano: 18_500_000_000,
        withdrawal_enabled: true,
        usd_rate: 0.000_000_35,
    });
    session.ton_wallet.ton_usd_rate = Some(3.5);
    session
}

impl QuillApp {
    fn demo_grams(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        match std::env::var("QUILL_DEMO_GRAMS")
            .unwrap_or_default()
            .as_str()
        {
            "settings" => self.open_settings(Some(quill::settings_catalog::Page::Telegram)),
            "withdraw" => {
                self.open_grams(cx);
                self.start_grams_withdraw(cx);
            }
            "withdraw-check" => {
                self.open_grams(cx);
                if let Some(grams) = self.grams_mut() {
                    grams.withdraw = Some(GramWithdraw::SecurityCheck { later: false });
                }
            }
            "loading-more" => {
                self.open_grams(cx);
                if let Some(grams) = self.grams_mut() {
                    grams.next_offset = "more".into();
                    grams.loading = true;
                }
            }
            "receipt" => {
                self.open_grams(cx);
                self.select_grams_tx(Some("tx-sale".into()), cx);
            }
            _ => self.open_grams(cx),
        }
        self.connection
            .set_note(StatusNote::silent("screenshot demo — My Grams"));
        cx.notify();
    }
}
