//! `ready-gift-delete`: one of the user's regular gifts open in the gifts
//! dialog with "Delete Gift" offered (injected, no live Telegram).
//! `QUILL_DEMO_GIFT_DELETE=confirm` also opens tdesktop's confirmation.
//! The GPUI tests in `gift_delete_tests.rs` drive the same demo.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::gift_market_demo::{ME, seed_gift_market_session};
use super::screenshot_demo::{DemoSpec, register_demos};
use gpui_kit::*;
use quill::diagnostics::MemorySink;
use quill::local_time::now_unix;
use quill::premium_hub::parse_received_gift;
use quill::state::Session;
use quill::telegram::envelope::MessageSender;
use serde_json::{Value, json};
use std::sync::Arc;

register_demos![
    DemoSpec::ready(
        "ready-gift-delete",
        seed_gift_delete_session,
        "screenshot demo — delete a received gift (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_gift_delete),
];

/// The gift that can be deleted; the second one cannot.
pub(super) const DELETABLE: &str = "own-gift-1";
pub(super) const KEPT: &str = "own-gift-2";

fn regular_gift(id: &str, stars: i64, can_be_deleted: bool) -> Value {
    json!({"@type": "receivedGift", "received_gift_id": id, "date": now_unix() - 86_400,
        "is_saved": true, "can_be_deleted": can_be_deleted,
        "gift": {"@type": "sentGiftRegular", "gift": {
            "@type": "gift", "id": "9", "star_count": stars}}})
}

pub(super) fn seed_gift_delete_session(sink: Arc<MemorySink>) -> Session {
    let mut session = seed_gift_market_session(sink);
    let hub = &mut session.payments.hub;
    hub.gifts_open = true;
    hub.gifts_owner = Some(MessageSender::User { user_id: ME });
    hub.gifts = [
        regular_gift(DELETABLE, 100, true),
        regular_gift(KEPT, 250, false),
    ]
    .iter()
    .filter_map(parse_received_gift)
    .collect();
    hub.gifts_total = 2;
    hub.gifts_loaded = true;
    hub.gift_selected = Some(DELETABLE.into());
    session
}

impl QuillApp {
    fn demo_gift_delete(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if std::env::var("QUILL_DEMO_GIFT_DELETE").is_ok_and(|v| v == "confirm") {
            // The window's dialog layer exists only after the first frame.
            cx.defer_in(window, |this, window, cx| {
                this.ask_delete_gift(DELETABLE, window, cx)
            });
        }
        cx.notify();
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "gift_delete_tests.rs"]
mod tests;
