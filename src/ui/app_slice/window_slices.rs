//! The slices each window owns: its conversation, its composer and the
//! media panel's layer, as a chat window hosts the same history widget the
//! main window does. `Slices::window` holds those of the window whose state
//! is in place (`chat_window::slot`), so the code that reads them serves
//! every window.

use super::super::anim_layer::{AnimationLayer, Layer};
use super::super::app::QuillApp;
use super::{AppSlice, SliceKind, SliceParts};
use gpui_kit::*;
use quill::ids::ChatId;
use std::cell::Cell;
use std::rc::Rc;

#[derive(Default)]
pub(in crate::ui) struct WindowSlices {
    pub(super) conversation: Option<SliceParts>,
    /// The composer slice, below the conversation slice.
    pub(super) composer: Option<Entity<AppSlice>>,
    /// The media panel's layer: over the composer, whose slice draws the
    /// panel, or the selector, which the window draws.
    pub(super) panel: Option<SliceParts>,
    /// The composer's height as laid out last time: a cached slice's box
    /// is sized before its content renders, so the slot takes the measured
    /// height (a change redraws the slot next frame).
    pub(super) composer_height: Rc<Cell<Pixels>>,
    /// The conversation slice rendered since the window's last render.
    pub(super) conversation_rendered: Cell<bool>,
    /// The conversation slot was part of the window's last render.
    pub(super) conversation_shown: Cell<bool>,
}

impl WindowSlices {
    /// The slices of `window` (`None`: the main window).
    pub(in crate::ui) fn new(window: Option<ChatId>, cx: &mut Context<QuillApp>) -> Self {
        let app = cx.entity().downgrade();
        let layer = Layer::default();
        let conversation = SliceParts {
            slice: cx.new(|_| AppSlice::new(app.clone(), SliceKind::Conversation, window)),
            painter: cx.new(|_| AnimationLayer::new(layer.clone(), app.clone(), window)),
            layer,
        };
        let composer = cx.new(|_| AppSlice::new(app.clone(), SliceKind::Composer, window));
        let layer = Layer::default();
        let panel = SliceParts {
            slice: composer.clone(),
            painter: cx.new(|_| AnimationLayer::new(layer.clone(), app.clone(), window)),
            layer,
        };
        Self {
            conversation: Some(conversation),
            composer: Some(composer),
            panel: Some(panel),
            ..Self::default()
        }
    }

    /// Start at `height`, as the composer's box until it is measured.
    pub(in crate::ui) fn with_composer_height(self, height: Pixels) -> Self {
        self.composer_height.set(height);
        self
    }

    /// As the window renders: whether the conversation rendered since its
    /// last render, or that render showed none. A conversation replayed
    /// from its cache still shows its clips: only one that rendered (or a
    /// frame without one) and swept no player orphans them.
    pub(in crate::ui) fn history_drawn(&self) -> bool {
        self.conversation_rendered.replace(false) || !self.conversation_shown.replace(false)
    }

    /// Whether the window's last render showed the conversation.
    pub(in crate::ui) fn conversation_shown(&self) -> bool {
        self.conversation_shown.get()
    }

    /// The slices that show the conversation.
    pub(in crate::ui) fn slice_ids(&self) -> impl Iterator<Item = EntityId> {
        let conversation = self.conversation.as_ref().map(|p| p.slice.entity_id());
        let composer = self.composer.as_ref().map(Entity::entity_id);
        conversation.into_iter().chain(composer)
    }

    /// The window's views: its slices and layer painters.
    pub(in crate::ui) fn view_ids(&self) -> impl Iterator<Item = EntityId> {
        let painters = [&self.conversation, &self.panel]
            .into_iter()
            .flatten()
            .map(|parts| parts.painter.entity_id());
        self.slice_ids()
            .chain(painters)
            .collect::<Vec<_>>()
            .into_iter()
    }
}
