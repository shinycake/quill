//! Telegram's delete animation: a message you delete doesn't just vanish.
//! It stays in place for a moment, fading while it breaks into dust that
//! drifts up and away. The row is kept as a "ghost" (a snapshot of the
//! message) until the animation ends.

use super::app::QuillApp;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::ids::{ChatId, MessageId};
use quill::state::HistoryMessage;
use std::time::{Duration, Instant};

/// How long a deleted message takes to turn to dust.
const VANISH: Duration = Duration::from_millis(600);

/// A delete that never lands (failed, offline) stops waiting after this.
const VANISH_WAIT: Duration = Duration::from_secs(10);

pub(super) struct Vanishing {
    message: HistoryMessage,
    requested: Instant,
    /// When the message actually left the history (the animation clock).
    started: Option<Instant>,
}

impl QuillApp {
    /// Remember the messages about to be deleted, so they can dissolve
    /// once the deletion lands.
    pub(super) fn begin_vanish(&self, chat_id: ChatId, ids: &[MessageId]) {
        let Some(history) = self.session().and_then(|s| s.histories.get(&chat_id.0)) else {
            return;
        };
        let now = Instant::now();
        let mut vanishing = self.vanishing.borrow_mut();
        for id in ids {
            if let Some(message) = history.messages.get(&id.0) {
                vanishing.push(Vanishing {
                    message: message.clone(),
                    requested: now,
                    started: None,
                });
            }
        }
    }

    /// Put the dissolving messages back among `messages` (oldest first)
    /// while their animation runs; drop the finished ones.
    pub(super) fn merge_vanishing(
        &self,
        messages: &mut Vec<HistoryMessage>,
        cx: &mut Context<Self>,
    ) {
        let now = Instant::now();
        let mut vanishing = self.vanishing.borrow_mut();
        vanishing.retain(|ghost| match ghost.started {
            Some(started) => now.duration_since(started) < VANISH,
            None => now.duration_since(ghost.requested) < VANISH_WAIT,
        });
        let chat = messages.first().map(|m| m.chat_id);
        let mut added = false;
        for ghost in vanishing.iter_mut() {
            if Some(ghost.message.chat_id) != chat
                || messages.iter().any(|m| m.id == ghost.message.id)
            {
                continue;
            }
            ghost.started.get_or_insert(now);
            messages.push(ghost.message.clone());
            added = true;
        }
        if added {
            messages.sort_by_key(|m| m.id.0);
        }
        if !vanishing.is_empty() {
            self.request_animation_tick(60, cx);
        }
    }

    /// The animation progress (0..1) of a dissolving message.
    pub(super) fn vanish_progress(&self, id: MessageId) -> Option<f32> {
        let vanishing = self.vanishing.borrow();
        let ghost = vanishing.iter().find(|ghost| ghost.message.id == id)?;
        let started = ghost.started?;
        Some((started.elapsed().as_secs_f32() / VANISH.as_secs_f32()).min(1.0))
    }

    /// For the history rows key: a fresh value every frame while a
    /// message dissolves.
    pub(super) fn vanish_rows_hash(&self) -> Option<u128> {
        let vanishing = self.vanishing.borrow();
        (!vanishing.is_empty()).then(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |elapsed| elapsed.as_nanos())
        })
    }
}

/// The dust over a dissolving row: grains scattered over the bubble's side
/// of the row, drifting up and outward and fading as `progress` runs.
pub(super) fn vanish_dust(row_id: u64, outgoing: bool, progress: f32, color: Hsla) -> AnyElement {
    let grains = (0..90u64).map(|index| {
        let mut seed = row_id.wrapping_add(index.wrapping_mul(0x9E37_79B9_7F4A_7C15)) | 1;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed % 10_000) as f32 / 10_000.0
        };
        let (x, y, speed, size) = (next(), next(), next(), 1.5 + next() * 2.5);
        // Grains leave in a wave across the bubble: later ones start later.
        let local = ((progress - x * 0.35) / 0.65).clamp(0.0, 1.0);
        let x = if outgoing {
            0.55 + x * 0.43
        } else {
            0.05 + x * 0.43
        };
        let direction = if outgoing { 1.0 } else { -1.0 };
        div()
            .absolute()
            .left(relative(x))
            .top(relative(y))
            .ml(px(direction * local * (30.0 + speed * 90.0)))
            .mt(px(-local * (15.0 + speed * 70.0)))
            .size(px(size))
            .rounded_full()
            .bg(color.opacity((1.0 - local) * (0.4 + speed * 0.6)))
            .when(local <= 0.0, |grain| grain.invisible())
    });
    div()
        .absolute()
        .inset_0()
        .children(grains)
        .into_any_element()
}
