//! Selecting albums as Telegram Desktop does (`HistoryInner`
//! `changeSelectionAsGroup`, `PointState::GroupPart`): a press on one tile
//! of an album (a photo, a video, a file of a file album) picks that one
//! message; a press anywhere else on the album's row, the menu opened
//! there, the keyboard and range selections pick the whole album.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::ids::{ChatId, MessageId};
use std::cell::RefCell;
use std::rc::Rc;

/// Where each album tile was painted, so a press can tell a tile from the
/// rest of its row. Tiles note themselves while the history paints; the
/// list, painted after its rows, makes that frame's notes the ones presses
/// read until its next paint (as `HistoryState::hit_rows`).
#[derive(Clone, Default)]
pub(crate) struct TileHits(Rc<RefCell<TileFrames>>);

#[derive(Default)]
struct TileFrames {
    painting: Vec<(Bounds<Pixels>, MessageId)>,
    painted: Vec<(Bounds<Pixels>, MessageId)>,
}

impl TileHits {
    /// An element that notes where the tile of `id` that holds it is
    /// painted. A re-entrant borrow drops the note for one frame.
    pub(crate) fn probe(&self, id: MessageId) -> impl IntoElement + use<> {
        let hits = self.clone();
        canvas(
            |_, _, _| {},
            move |bounds, _, _, _| {
                if let Ok(mut frames) = hits.0.try_borrow_mut() {
                    frames.painting.push((bounds, id));
                }
            },
        )
        .absolute()
        .inset_0()
        .size_full()
    }

    /// The history list painted: this frame's tiles are the ones presses
    /// hit from now on.
    pub(crate) fn frame_painted(&self) {
        if let Ok(mut frames) = self.0.try_borrow_mut() {
            let frames = &mut *frames;
            std::mem::swap(&mut frames.painted, &mut frames.painting);
            frames.painting.clear();
        }
    }

    /// The tile painted under `point` in the last frame.
    pub(crate) fn at(&self, point: Point<Pixels>) -> Option<MessageId> {
        let frames = self.0.try_borrow().ok()?;
        frames
            .painted
            .iter()
            .rev()
            .find(|(bounds, _)| bounds.contains(&point))
            .map(|(_, id)| *id)
    }

    #[cfg(test)]
    #[cfg(feature = "demo-capture")]
    pub(crate) fn center_of(&self, id: MessageId) -> Option<Point<Pixels>> {
        let frames = self.0.try_borrow().ok()?;
        frames
            .painted
            .iter()
            .find(|(_, tile)| *tile == id)
            .map(|(bounds, _)| bounds.center())
    }
}

/// The check an album item shows in its corner while selecting: an empty
/// ring, or filled with a tick once the item is selected (the row check's
/// look, `selection_mode::selection_row`).
pub(crate) fn tile_check(selected: bool, cx: &App) -> AnyElement {
    let size = super::motion::CHECK_SIZE;
    div()
        .size(px(size))
        .rounded_full()
        .border_2()
        .border_color(gpui_kit::white())
        .bg(if selected {
            cx.theme().success
        } else {
            gpui_kit::black().opacity(0.3)
        })
        .flex()
        .items_center()
        .justify_center()
        .when(selected, |this| {
            this.child(
                gpui_kit::component::Icon::new(gpui_kit::assets::IconName::Check)
                    .size(px(12.))
                    .text_color(gpui_kit::white()),
            )
        })
        .into_any_element()
}

impl QuillApp {
    /// The selectable messages of the album `id` belongs to in the view's
    /// history (ascending), or just `id` when it is in none.
    pub(super) fn album_of(&self, chat: ChatId, id: MessageId) -> Vec<MessageId> {
        let Some(messages) = self.shown_history(chat) else {
            return vec![id];
        };
        let album = messages.get(&id.0).map_or(0, |m| m.media_album_id);
        if album == 0 {
            return vec![id];
        }
        let mut ids: Vec<MessageId> = messages
            .values()
            .filter(|m| m.media_album_id == album && m.can_pin())
            .map(|m| m.id)
            .collect();
        ids.sort_by_key(|id| id.0);
        if ids.is_empty() { vec![id] } else { ids }
    }

    /// `ids` with the rest of every album one of them belongs to
    /// (ascending): a range never ends in the middle of an album.
    pub(super) fn with_whole_albums(&self, chat: ChatId, ids: Vec<MessageId>) -> Vec<MessageId> {
        let mut all: Vec<MessageId> = Vec::with_capacity(ids.len());
        for id in ids {
            if !all.contains(&id) {
                all.extend(self.album_of(chat, id));
            }
        }
        all.sort_by_key(|id| id.0);
        all.dedup();
        all
    }

    /// What a press at `position` on the row of `id` picks: the tile under
    /// it alone, else the row's whole album (just `id` outside albums).
    pub(super) fn press_pick(
        &self,
        chat: ChatId,
        id: MessageId,
        position: Point<Pixels>,
    ) -> Vec<MessageId> {
        match self.history.tile_hits.at(position) {
            Some(tile) if self.album_of(chat, id).contains(&tile) => vec![tile],
            _ => self.album_of(chat, id),
        }
    }

    /// Select or deselect `group` as one (`changeSelectionAsGroup`); one id
    /// toggles like any message.
    pub(super) fn toggle_select_group(
        &mut self,
        chat: ChatId,
        group: Vec<MessageId>,
        cx: &mut Context<Self>,
    ) {
        if let [one] = group[..] {
            let pending = self.message_still_sending(chat, one);
            self.toggle_forward_select(chat, one, pending, cx);
            return;
        }
        self.drop_deleted_selection();
        let topic = self.open_topic_id(chat);
        let selected: Vec<MessageId> = self
            .forward_draft()
            .filter(|draft| draft.is_from(chat, topic))
            .map(|draft| draft.message_ids)
            .unwrap_or_default();
        let next = quill::selection_pin::toggle_group(&selected, &group);
        self.set_selection(chat, next, cx);
    }

    /// Add `group` to the selection ("Select" in the menu of an album).
    pub(super) fn select_group(
        &mut self,
        chat: ChatId,
        group: Vec<MessageId>,
        cx: &mut Context<Self>,
    ) {
        let topic = self.open_topic_id(chat);
        let all_in = self
            .forward_draft()
            .filter(|draft| draft.is_from(chat, topic))
            .is_some_and(|draft| group.iter().all(|id| draft.contains(*id)));
        if !all_in {
            self.toggle_select_group(chat, group, cx);
        }
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "album_select_tests.rs"]
mod tests;
