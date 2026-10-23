//! The next page of the long lists in the Stars, Gifts and Gram dialogs,
//! asked for when the reader scrolls near the end (`ui/scroll_load.rs`),
//! as Telegram Desktop does. A failed page is retried by scrolling again.
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use crate::ui::scroll_load::{load_due, scrolled_near_end};
use gpui_kit::*;

impl QuillApp {
    pub(super) fn stars_more(&mut self, cx: &mut Context<Self>, scrolled: bool) {
        // Paging needs a live connection; a demo session has nothing to ask.
        if self.live.is_none() {
            return;
        }
        let Some(hub) = self.session().map(|s| &s.payments.hub) else {
            return;
        };
        if !hub.stars_open
            || hub.tx_selected.is_some()
            || (hub.tx_error.is_some() && !scrolled)
            || !load_due(
                scrolled_near_end(&self.payments.stars_scroll),
                !hub.tx_offset.is_empty(),
                hub.tx_loading,
            )
            || !self
                .payments
                .stars_asked
                .allow(hub.transactions.len(), scrolled)
        {
            return;
        }
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.fetch_more_star_transactions();
        }
        cx.notify();
    }

    pub(super) fn gifts_more(&mut self, cx: &mut Context<Self>, scrolled: bool) {
        // Paging needs a live connection; a demo session has nothing to ask.
        if self.live.is_none() {
            return;
        }
        let Some(hub) = self.session().map(|s| &s.payments.hub) else {
            return;
        };
        if !hub.gifts_open
            || hub.gift_selected.is_some()
            || (hub.gifts_error.is_some() && !scrolled)
            || !load_due(
                scrolled_near_end(&self.payments.gifts_scroll),
                !hub.gifts_offset.is_empty(),
                hub.gifts_loading,
            )
            || !self.payments.gifts_asked.allow(hub.gifts.len(), scrolled)
        {
            return;
        }
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.fetch_more_received_gifts();
        }
        cx.notify();
    }

    pub(super) fn grams_more(&mut self, cx: &mut Context<Self>, scrolled: bool) {
        // Paging needs a live connection; a demo session has nothing to ask.
        if self.live.is_none() {
            return;
        }
        let Some(grams) = self.session().map(|s| &s.payments.hub.grams) else {
            return;
        };
        if !grams.open
            || grams.selected.is_some()
            || (grams.error.is_some() && !scrolled)
            || !load_due(
                scrolled_near_end(&self.payments.grams_scroll),
                grams.has_more(),
                grams.loading,
            )
            || !self
                .payments
                .grams_asked
                .allow(grams.entries.len(), scrolled)
        {
            return;
        }
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.fetch_more_grams();
        }
        cx.notify();
    }
}
