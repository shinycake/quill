//! Forwarding and the share box.
#![deny(clippy::indexing_slicing)]

use super::*;
use gpui_kit::component::input::TextareaState;
use gpui_kit::*;
use quill::lang::{keys, tr};

pub(crate) struct ShareUi {
    /// The forward being built: the selection, the share box over it, the
    /// "Reply in Another Chat" chooser over it, or
    /// the forward bar it became (`ForwardStage`).
    pub(super) forward: super::forward_stage::ForwardStage,
    /// The share box's optional comment, sent before the forwards.
    pub(super) comment_input: Entity<TextareaState>,
    pub(super) search_input: Entity<TextareaState>,
    /// Text of a share link while its chat chooser is open.
    pub(super) link_text: Option<String>,
}

impl ShareUi {
    /// Destinations ticked in the open share box: they live in its stage
    /// (`ForwardStage::Picking`), so every way out of the box drops them.
    pub(super) fn is_ticked(&self, chat: quill::ids::ChatId) -> bool {
        self.forward
            .ticks()
            .is_some_and(|ticks| ticks.contains(chat))
    }

    pub(super) fn tick_count(&self) -> usize {
        self.forward
            .ticks()
            .map_or(0, quill::share_box::ShareSelection::len)
    }

    /// The one ticked destination, when exactly one is.
    pub(super) fn single_tick(&self) -> Option<quill::ids::ChatId> {
        self.forward
            .ticks()
            .and_then(quill::share_box::ShareSelection::single)
    }

    /// Tick or untick `chat` in the open box; nothing when no box is open.
    pub(super) fn toggle_tick(&mut self, chat: quill::ids::ChatId) {
        if let Some(ticks) = self.forward.ticks_mut() {
            ticks.toggle(chat);
        }
    }

    /// The box's inputs follow its stage: once it is gone, what was typed
    /// in it goes too (per frame, in every window: `per_window_upkeep`).
    pub(super) fn sync_inputs(&self, window: &mut Window, cx: &mut Context<QuillApp>) {
        if self.forward.picker_open() || self.forward.reply_chooser_open() {
            return;
        }
        for input in [&self.comment_input, &self.search_input] {
            if !input.read(cx).value().is_empty() {
                input.update(cx, |input, cx| input.set_value("", window, cx));
            }
        }
    }

    pub(super) fn new(
        window: &mut Window,
        cx: &mut Context<QuillApp>,
        forward_search_input: Entity<TextareaState>,
    ) -> Self {
        let share_comment_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder(tr(keys::Q_OPS_ADD_A_COMMENT))
                .auto_grow(1, 3)
                .submit_on_enter(false)
        });
        Self {
            forward: Default::default(),
            comment_input: share_comment_input,
            search_input: forward_search_input,
            link_text: None,
        }
    }
}

/// Nothing to say: its state is the account's only app-side.
impl super::on_screen::Farewell for ShareUi {}
