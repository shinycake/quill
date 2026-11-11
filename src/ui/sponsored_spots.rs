//! Where a channel's ads sit in its history, the UI half (tdesktop
//! `Data::SponsoredMessages`, `docs/decisions/sponsored-placement.md`).
//!
//! The session keeps which ad follows which message
//! (`ChatSponsoredMessages::placed`). Here the ads are drawn under the row
//! of that message, as part of the history, so they scroll with the posts.
//! Ads join the history the way Telegram Desktop adds them: at the end
//! when the reader scrolls past the last message (`append`), or between
//! posts at least `messages_between` posts and two screens apart
//! (`inject`), which needs the rows' heights and so runs here.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::history_row::HistoryRow;
use gpui_kit::*;
use quill::ids::MessageId;
use quill::state::SponsoredLayout;
use std::hash::{Hash, Hasher};

/// One ad in the shown history and what hangs with it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct SponsoredSpot {
    /// The message the ad follows.
    pub(super) after: MessageId,
    pub(super) ad_id: i64,
    /// The ad card shows (not reported or hidden, media downloaded).
    pub(super) card: bool,
    /// The report's option picker is open for this ad.
    pub(super) report: bool,
    /// The outcome of a report or "Hide ads" shows here.
    pub(super) outcome: bool,
}

/// The main history's ads as drawn last, and the rows the placement
/// between posts last looked at.
#[derive(Default)]
pub(crate) struct SponsoredInHistory {
    pub(super) spots: Vec<SponsoredSpot>,
    /// What the last `inject_sponsored` pass found nothing new in: the
    /// rows' ends and count, the ads placed and the viewport's height.
    inject_seen: Option<u64>,
}

impl QuillApp {
    /// The open chat's ads in its main history (the session's open chat,
    /// no topic, comment thread or Saved Messages view).
    fn sponsored_spots_now(&self) -> Vec<SponsoredSpot> {
        let Some(session) = self.session() else {
            return Vec::new();
        };
        let Some(chat_id) = session.open_chat else {
            return Vec::new();
        };
        let Some(entry) = session.messages.sponsored.get(&chat_id.0) else {
            return Vec::new();
        };
        let shown: Vec<i64> = session
            .open_sponsored_in_history()
            .iter()
            .map(|(_, ad)| ad.message_id)
            .collect();
        let report = session
            .messages
            .sponsored_report
            .as_ref()
            .filter(|flight| flight.chat_id == chat_id)
            .map(|flight| flight.message_id);
        // "Hide ads" answers for no ad in particular (id 0): under the
        // last ad placed.
        let outcome = session
            .messages
            .last_sponsored_report
            .as_ref()
            .filter(|outcome| outcome.chat_id == chat_id)
            .map(|outcome| match outcome.message_id {
                0 => entry.placed.last().map_or(0, |spot| spot.ad_id),
                id => id,
            });
        entry
            .placed
            .iter()
            .map(|spot| SponsoredSpot {
                after: spot.after,
                ad_id: spot.ad_id,
                card: shown.contains(&spot.ad_id),
                report: report == Some(spot.ad_id),
                outcome: outcome == Some(spot.ad_id),
            })
            .filter(|spot| spot.card || spot.report || spot.outcome)
            .collect()
    }

    /// After the main history's rows are in: place ads between posts, and
    /// measure again the rows whose ads changed so the list makes room.
    /// Other lists carry no ads.
    pub(super) fn sync_sponsored_spots(&mut self, list: &'static str, cx: &mut Context<Self>) {
        let main = list == "session-history";
        if main {
            self.inject_sponsored(cx);
        }
        let spots = if main {
            self.sponsored_spots_now()
        } else {
            Vec::new()
        };
        if spots == self.history.sponsored.spots {
            return;
        }
        let changed: Vec<MessageId> = spots
            .iter()
            .filter(|spot| !self.history.sponsored.spots.contains(spot))
            .chain(
                self.history
                    .sponsored
                    .spots
                    .iter()
                    .filter(|spot| !spots.contains(spot)),
            )
            .map(|spot| spot.after)
            .collect();
        self.history.sponsored.spots = spots;
        let rows: Vec<usize> = self
            .history
            .rows
            .iter()
            .enumerate()
            .filter(|(_, row)| changed.iter().any(|id| row.contains(*id)))
            .map(|(ix, _)| ix)
            .collect();
        self.history.scroller.update(cx, |state, cx| {
            for ix in rows {
                let _ = state.remeasure_items(ix..ix + 1, cx);
            }
        });
    }

    /// The ads drawn under `row`, in the order they were placed.
    pub(super) fn sponsored_under_row(
        &self,
        row: &HistoryRow,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let spots: Vec<SponsoredSpot> = self
            .history
            .sponsored
            .spots
            .iter()
            .filter(|spot| row.contains(spot.after))
            .copied()
            .collect();
        if spots.is_empty() {
            return None;
        }
        Some(
            div()
                .flex()
                .flex_col()
                .children(
                    spots
                        .iter()
                        .filter_map(|spot| self.sponsored_spot(spot, cx)),
                )
                .into_any_element(),
        )
    }

    /// tdesktop `SponsoredMessages::inject`: ads of a list with
    /// `messages_between` go between posts, counted from the first unread
    /// message or the last ad placed. Each waits for at least that many
    /// messages and two screens of rows after its start; when the loaded
    /// rows end first, the chat's first ad goes after the last message and
    /// a later one waits for more rows.
    fn inject_sponsored(&mut self, cx: &mut Context<Self>) {
        let Some(SponsoredLayout::BetweenPosts(between)) =
            self.session().and_then(|s| s.open_sponsored_layout())
        else {
            return;
        };
        let Some(viewport) = self.history.viewport.get().map(|view| view.size.height) else {
            return;
        };
        let seen = self.inject_key(viewport);
        if self.history.sponsored.inject_seen == Some(seen) {
            return;
        }
        let mut placed_any = false;
        while let Some((start, injected_any)) =
            self.session().and_then(|s| s.sponsored_inject_start())
        {
            let Some(after) = self.inject_target(start, injected_any, between, viewport, cx) else {
                break;
            };
            let placed = self
                .session_mut()
                .is_some_and(|session| session.place_sponsored_after(after));
            if !placed {
                break;
            }
            placed_any = true;
        }
        // Rows not measured yet: look again on the next render.
        if placed_any || self.history.scroller.read(cx).distance_to_edges().is_some() {
            self.history.sponsored.inject_seen = Some(self.inject_key(viewport));
        }
    }

    /// Where the next ad between posts goes: after the row that brings at
    /// least `between` messages and two viewports of height after `start`.
    fn inject_target(
        &self,
        start: MessageId,
        injected_any: bool,
        between: u32,
        viewport: Pixels,
        cx: &App,
    ) -> Option<MessageId> {
        let rows = &self.history.rows;
        let start_ix = rows.iter().position(|row| row.contains(start))?;
        let after_start = rows.get(start_ix + 1..)?;
        let heights = self
            .history
            .scroller
            .read(cx)
            .estimated_row_heights(start_ix + 1..rows.len())?;
        let (mut count, mut height) = (0usize, px(0.));
        let need = between as usize;
        for (row, row_height) in after_start.iter().zip(heights) {
            count += row.message_ids().len();
            height += row_height;
            if count >= need && height >= viewport * 2. {
                return row.last_id();
            }
        }
        // `appendAtLeastToEnd`: the first ad goes at the end of what is
        // loaded; a later one waits for more rows.
        (!injected_any)
            .then(|| rows.last().and_then(HistoryRow::last_id))
            .flatten()
    }

    fn inject_key(&self, viewport: Pixels) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        let placed = self
            .session()
            .and_then(|s| {
                s.open_chat
                    .and_then(|chat| s.messages.sponsored.get(&chat.0))
            })
            .map_or(0, |entry| entry.placed.len());
        (
            self.history.ends.map(|(first, last)| (first.0, last.0)),
            self.history.rows.len(),
            placed,
            f32::from(viewport) as i64,
        )
            .hash(&mut hasher);
        hasher.finish()
    }

    /// tdesktop `setBottomContentRequest`: a scroll toward newer messages
    /// that would go past the end of the main history adds the next ad at
    /// its end (`SponsoredMessages::append`). `down` is how far the scroll
    /// goes. Returns whether an ad joined: the scroll then runs into it
    /// instead of pulling to the next channel.
    pub(super) fn sponsored_bottom_request(
        &mut self,
        down: Pixels,
        cx: &mut Context<Self>,
    ) -> bool {
        if down <= px(0.) || !self.main_history_shown() {
            return false;
        }
        let below = self
            .history
            .scroller
            .read(cx)
            .distance_to_edges()
            .map_or(px(0.), |(_, below, _)| below);
        // One ad per drawn history: the last one placed must be in the
        // rows before the next joins.
        if below >= down || self.sponsored_spots_now() != self.history.sponsored.spots {
            return false;
        }
        let placed = self
            .session_mut()
            .is_some_and(|session| session.place_sponsored_at_end(std::time::Instant::now()));
        if placed {
            self.notify_conversation(cx);
            cx.notify();
        }
        placed
    }

    /// The history on screen is the open chat's own: no topic, comment
    /// thread or Saved Messages view, in the window of that chat.
    fn main_history_shown(&self) -> bool {
        let Some(session) = self.session() else {
            return false;
        };
        session.open_chat.is_some()
            && self.viewed_chat() == session.open_chat
            && session.open_topic.is_none()
            && !self.thread_active()
            && session.threads.saved.sublist.is_none()
            && session.threads.saved.tag_search.is_none()
    }
}

/// A wheel or trackpad scroll's distance toward newer messages; a mostly
/// sideways one moves nothing.
pub(super) fn scroll_down_distance(delta: ScrollDelta, line_height: Pixels) -> Pixels {
    let (dx, dy) = match delta {
        ScrollDelta::Pixels(delta) => (delta.x, delta.y),
        ScrollDelta::Lines(lines) => (line_height * lines.x, line_height * lines.y),
    };
    if dx.abs() >= dy.abs() { px(0.) } else { -dy }
}
