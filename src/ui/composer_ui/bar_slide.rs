//! The edit, reply and forward bars above the composer sliding open and
//! shut (`surface_reveal`), so the composer grows and shrinks instead of
//! jumping. Telegram Desktop shows its field header at once
//! (`FieldHeader::updateVisible`, `history_view_compose_controls.cpp`);
//! its pinned and sponsored bars slide (`Ui::SlideWrap`,
//! `slideWrapDuration` 150 ms), and the bars here follow those.
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use crate::ui::surface_reveal::{self, Attached, Presence, slide_open};
use gpui_kit::*;

/// A bar's height until it was laid out once: icon row plus two lines.
const BAR_GUESS: Pixels = px(46.);
/// The forward bar: a title, two lines and the option checkboxes.
const FORWARD_BAR_GUESS: Pixels = px(110.);
/// The composer column's `gap_2`.
const COLUMN_GAP: Pixels = px(8.);

impl QuillApp {
    /// The surface's scope: the open chat, so a bar never slides out of
    /// one chat into another.
    fn bar_scope(&self) -> u64 {
        self.open_chat_id().map_or(0, |chat| chat.0 as u64)
    }

    /// Follows `state` on `presence` and asks for frames while it moves.
    fn follow_bar<T: Clone + PartialEq>(
        &self,
        presence: &std::cell::RefCell<Presence<T>>,
        state: Option<T>,
        cx: &mut Context<Self>,
    ) -> Option<surface_reveal::Drawn<T>> {
        let now = surface_reveal::now();
        let animate = surface_reveal::animate(cx);
        let (drawn, moving) = {
            let mut presence = presence.borrow_mut();
            let drawn = presence.follow(state, self.bar_scope(), animate, now);
            (drawn, presence.moving(now))
        };
        if moving {
            self.request_animation_tick(60, cx);
        }
        drawn
    }

    /// The edit bar, sliding open over the composer when an edit starts
    /// and shut when it ends.
    pub(in crate::ui) fn composer_edit_bar_slot(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let drawn = self.follow_bar(
            &self.frame.surfaces.edit_bar,
            self.chat_ui.pending_edit().cloned(),
            cx,
        )?;
        let content = self
            .composer_edit_banner(&drawn.value, cx)
            .into_any_element();
        Some(slide_open(
            content,
            drawn.shown,
            self.frame.surfaces.edit_bar_size.height.clone(),
            BAR_GUESS,
            COLUMN_GAP,
            Attached::Bottom,
        ))
    }

    /// The reply bar, likewise.
    pub(in crate::ui) fn composer_reply_bar_slot(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let drawn = self.follow_bar(
            &self.frame.surfaces.reply_bar,
            self.chat_ui.reply().cloned(),
            cx,
        )?;
        let content = self
            .composer_reply_banner(&drawn.value, cx)
            .into_any_element();
        Some(slide_open(
            content,
            drawn.shown,
            self.frame.surfaces.reply_bar_size.height.clone(),
            BAR_GUESS,
            COLUMN_GAP,
            Attached::Bottom,
        ))
    }

    /// The forward bar, likewise. Its content comes from the forward
    /// draft, which is still there while the bar slides shut.
    pub(in crate::ui) fn forward_bar_slot(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let drawn = self.follow_bar(
            &self.frame.surfaces.forward_bar,
            self.forward_bar_here().then_some(()),
            cx,
        )?;
        let content = self.forward_bar(cx).into_any_element();
        Some(slide_open(
            content,
            drawn.shown,
            self.frame.surfaces.forward_bar_size.height.clone(),
            FORWARD_BAR_GUESS,
            COLUMN_GAP,
            Attached::Bottom,
        ))
    }
}
