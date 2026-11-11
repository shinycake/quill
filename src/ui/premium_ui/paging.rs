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
        if !hub.stars.is_open()
            || hub.stars.selected().is_some()
            || (hub.stars.page.error().is_some() && !scrolled)
            || !load_due(
                scrolled_near_end(&self.payments.stars_scroll),
                !hub.stars.offset.is_empty(),
                hub.stars.page.is_loading(),
            )
            || !self
                .payments
                .stars_asked
                .allow(hub.stars.rows.len(), scrolled)
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
        if !hub.received.is_open()
            || hub.received.view.selected().is_some()
            || ((hub.received.page.error().is_some() || hub.received.notice.is_some()) && !scrolled)
            || !load_due(
                scrolled_near_end(&self.payments.gifts_scroll),
                !hub.received.offset.is_empty(),
                hub.received.page.is_loading(),
            )
            || !self
                .payments
                .gifts_asked
                .allow(hub.received.rows.len(), scrolled)
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
            || (grams.load.error().is_some() && !scrolled)
            || !load_due(
                scrolled_near_end(&self.payments.grams_scroll),
                grams.has_more(),
                grams.load.is_loading(),
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
