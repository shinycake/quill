//! `ready-gift-craft`: crafting "Plush Pepe #1,204" over the ready chat
//! list (injected, no live Telegram). The craft box opens with three gifts
//! in the slots; `QUILL_DEMO_MARKET` opens another box instead: `info`,
//! `select`, `forge`, `success`, `failed` or `later`.

#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::gift_craft_ui::gate_notice;
use super::gift_market_demo::{ME, RESALE_GIFT, listing, resale_page, seed_gift_market_session};
use super::gift_market_state::MarketBox;
use super::screenshot_demo::{DemoSpec, register_demos};
use gpui_kit::*;
use quill::diagnostics::MemorySink;
use quill::gift_market::{
    CraftGate, CraftGift, CraftResult, CraftSession, ResaleFilter, ResaleSearch, parse_resale_page,
};
use quill::local_time::now_unix;
use quill::premium_hub::upgraded::parse_collectible;
use quill::state::Session;
use serde_json::Value;
use std::sync::Arc;
use std::time::{Duration, Instant};

register_demos![
    DemoSpec::ready(
        "ready-gift-craft",
        seed_gift_craft_session,
        "screenshot demo — gift crafting: slots, chances, select, forge and result (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_gift_craft),
];

/// (number, model, backdrop, chance per mille) of the user's own gifts.
const OWN: [(i32, usize, usize, i64); 5] = [
    (1204, 0, 3, 120),
    (77, 1, 3, 150),
    (19, 1, 2, 80),
    (640, 0, 3, 100),
    (5120, 0, 1, 90),
];

/// One of the user's collectibles of the collection.
fn own_gift(number: i32, model: usize, backdrop: usize, per_mille: i64) -> Option<CraftGift> {
    let mut value = listing(number, model, backdrop, 0);
    let gift = value.get_mut("gift")?;
    let object = gift.as_object_mut()?;
    object.insert("craft_probability_per_mille".into(), Value::from(per_mille));
    object.insert("resale_parameters".into(), Value::Null);
    object.insert(
        "owner_id".into(),
        serde_json::json!({"@type": "messageSenderUser", "user_id": ME}),
    );
    Some(CraftGift {
        gift: parse_collectible(gift)?,
        received_gift_id: format!("own-{number}"),
        craft_date: 0,
    })
}

/// The gift market session with a craft of three gifts open.
pub(super) fn seed_gift_craft_session(sink: Arc<MemorySink>) -> Session {
    let mut session = seed_gift_market_session(sink);
    let mut gifts: Vec<CraftGift> = OWN
        .iter()
        .filter_map(|(number, model, backdrop, chance)| {
            own_gift(*number, *model, *backdrop, *chance)
        })
        .collect();
    // The last one can't be crafted for another day.
    if let Some(last) = gifts.last_mut() {
        last.craft_date = i32::try_from(now_unix().saturating_add(86_400)).unwrap_or(i32::MAX);
    }
    let Some(primary) = gifts.first().cloned() else {
        return session;
    };
    let mut craft = CraftSession::new(primary);
    for gift in gifts.iter().skip(1).take(2) {
        craft.add(gift.clone());
    }
    craft.candidates = gifts.iter().skip(1).cloned().collect();
    craft.candidates_loaded = true;
    craft.probabilities = vec![
        vec![1000],
        vec![600, 1000],
        vec![400, 700, 1000],
        vec![300, 550, 800, 1000],
    ];
    let mut market = ResaleSearch::new(RESALE_GIFT, ResaleFilter::default(), true);
    market.apply(parse_resale_page(&resale_page()), Some(ME), false);
    craft.market = market;
    session.payments.gift_market.craft = Some(craft);
    session
}

impl QuillApp {
    fn demo_gift_craft(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let mode = std::env::var("QUILL_DEMO_MARKET").unwrap_or_default();
        self.gift_market.craft_primary = self
            .craft_session()
            .and_then(|craft| craft.slots.first().cloned());
        self.gift_market.stack = vec![MarketBox::Craft];
        match mode.as_str() {
            "info" => self.gift_market.stack = vec![MarketBox::CraftInfo],
            "select" => self.gift_market.stack.push(MarketBox::CraftSelect),
            "forge" => {
                // Half way through the forge, still waiting for the answer.
                if let Some(craft) = self.craft_mut() {
                    craft.crafting = true;
                }
                self.gift_market.forge_started =
                    Instant::now().checked_sub(Duration::from_millis(1_400));
            }
            "success" | "failed" => {
                let crafted = self.craft_session().and_then(|craft| {
                    let mut gift = craft.slots.get(1)?.gift.clone();
                    gift.number = 12;
                    gift.model.name = "Crystal Pepe".into();
                    Some(gift)
                });
                let result = match (mode.as_str(), crafted) {
                    ("success", Some(gift)) => CraftResult::Success {
                        gift: Box::new(gift),
                        received_gift_id: "crafted-12".into(),
                    },
                    _ => CraftResult::Fail,
                };
                if let Some(craft) = self.craft_mut() {
                    craft.result = Some(result);
                }
                self.gift_market.stack = vec![MarketBox::CraftResult];
            }
            "later" => {
                let date = i32::try_from(now_unix().saturating_add(86_400)).unwrap_or(i32::MAX);
                if let Some(notice) = gate_notice(CraftGate::Later(date)) {
                    self.gift_market.stack = vec![notice];
                }
            }
            _ => {}
        }
        cx.notify();
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "gift_craft_tests.rs"]
mod tests;
