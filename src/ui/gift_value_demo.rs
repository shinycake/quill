//! `ready-gift-value`: the value of an upgraded gift over the ready chat
//! list, from the recorded `upgradedGiftValueInfo` fixtures (injected, no
//! live Telegram). `QUILL_DEMO_GIFT_VALUE` picks the view: unset or `box`
//! for the value box after a last sale on Fragment, `average` for the box
//! explained by the average price, `row` for the gift box with its Value
//! row, `loading` for the row while the value loads, `error` for the row
//! after a failed request.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::gift_market_demo::{ME, listing, seed_gift_market_session};
use super::screenshot_demo::{DemoSpec, register_demos};
use crate::ui::status_note::StatusNote;
use gpui_kit::*;
use quill::diagnostics::MemorySink;
use quill::premium_hub::gift_value::{GiftValueInfo, GiftValueLookup, parse_gift_value_info};
use quill::premium_hub::parse_received_gift;
use quill::state::Session;
use quill::telegram::envelope::MessageSender;
use serde_json::{Value, json};
use std::sync::Arc;

/// After a last sale on Fragment, above the average.
pub(super) const LAST: &str = include_str!("../../tests/fixtures/gift_value/value_info_last.json");
/// No resale yet: the average explains the price.
pub(super) const AVERAGE: &str =
    include_str!("../../tests/fixtures/gift_value/value_info_average.json");
/// The received gift the demo opens.
pub(super) const RECEIVED_ID: &str = "demo-value-gift";

register_demos![
    DemoSpec::ready(
        "ready-gift-value",
        seed_gift_value_session,
        "screenshot demo — gift value: the Value row and the value box (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_gift_value),
];

pub(super) fn fixture(text: &str) -> Option<GiftValueInfo> {
    serde_json::from_str::<Value>(text)
        .ok()
        .and_then(|value| parse_gift_value_info(&value))
}

/// "Plush Pepe #1,204" with its estimate, as the user's received gift.
fn received_gift() -> Value {
    let mut gift = listing(1204, 0, 0, 1_250)
        .get("gift")
        .cloned()
        .unwrap_or(Value::Null);
    if let Some(fields) = gift.as_object_mut() {
        fields.insert("resale_parameters".into(), Value::Null);
        fields.insert("value_currency".into(), json!("USD"));
        fields.insert("value_amount".into(), json!(248_750));
        fields.insert("value_usd_amount".into(), json!(248_750));
    }
    json!({"@type": "receivedGift", "received_gift_id": RECEIVED_ID,
        "sender_id": {"@type": "messageSenderUser", "user_id": 502},
        "text": {"@type": "formattedText", "text": "", "entities": []},
        "is_private": false, "is_saved": true, "is_pinned": false,
        "can_be_upgraded": false, "can_be_transferred": false, "was_refunded": false,
        "date": 1_758_110_400, "sell_star_count": 0,
        "gift": {"@type": "sentGiftUpgraded", "gift": gift}})
}

/// The gift market seed plus the user's gift box open on that gift.
pub(super) fn seed_gift_value_session(sink: Arc<MemorySink>) -> Session {
    let mut session = seed_gift_market_session(sink);
    let hub = &mut session.payments.hub;
    hub.gifts = parse_received_gift(&received_gift()).into_iter().collect();
    hub.gifts_total = 1;
    hub.gifts_loaded = true;
    hub.gifts_owner = Some(MessageSender::User { user_id: ME });
    hub.gifts_open = true;
    hub.gift_selected = Some(RECEIVED_ID.into());
    session
}

impl QuillApp {
    /// The demo's gift as a value lookup, filled with `info`.
    pub(super) fn demo_gift_value_lookup(
        &self,
        info: Option<GiftValueInfo>,
    ) -> Option<GiftValueLookup> {
        let session = self.session()?;
        let gift = session.payments.hub.selected_gift()?;
        let collectible = gift.gift.collectible.as_deref()?;
        Some(GiftValueLookup {
            info,
            ..GiftValueLookup::new(collectible)
        })
    }

    fn demo_gift_value(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let view = std::env::var("QUILL_DEMO_GIFT_VALUE").unwrap_or_default();
        let lookup = match view.as_str() {
            "row" => None,
            "loading" => self
                .demo_gift_value_lookup(None)
                .map(|lookup| GiftValueLookup {
                    loading: true,
                    ..lookup
                }),
            "error" => self
                .demo_gift_value_lookup(None)
                .map(|lookup| GiftValueLookup {
                    error: Some(quill::lang::tr_with(
                        quill::lang::keys::Q_GIFT_VALUE_COULDNT_LOAD,
                        &[("reason", "STARGIFT_SLUG_INVALID")],
                    )),
                    ..lookup
                }),
            "average" => {
                self.demo_gift_value_lookup(fixture(AVERAGE))
                    .map(|lookup| GiftValueLookup {
                        open: true,
                        ..lookup
                    })
            }
            _ => self
                .demo_gift_value_lookup(fixture(LAST))
                .map(|lookup| GiftValueLookup {
                    open: true,
                    ..lookup
                }),
        };
        if let Some(session) = self.demo_session.as_mut() {
            session.payments.hub.gift_value = lookup;
        }
        self.connection
            .set_note(StatusNote::silent("screenshot demo — gift value"));
        cx.notify();
    }
}
