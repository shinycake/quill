//! The chat list's elastic overscroll and the pull that expands the stories
//! strip (tdesktop `dialogs_widget.cpp`, `quill::stories_pull`).
//!
//! The list is wrapped in the kit's `ScrollBounce`, which stretches either
//! end while a trackpad drags past it and springs back. The strip rests
//! collapsed; pulling the list down at its top reveals the strip in the gap,
//! and a pull past tdesktop's threshold expands it: the strip becomes the
//! list's first row and slides into view. Everything here reads the kit's
//! reported displacement and the list's scroll offset; nothing owns physics.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use gpui_base::{ScrollBounce, ScrollBounceMotion};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::stories_pull::{collapse_due, pull_expands, pull_progress};
use quill::stories_strip as metrics;
use std::time::Instant;

/// While the list rests at its top with a strip to pull open, the edge
/// follows the finger closely (tdesktop's pull side, `kPullBandStiffness`
/// 2.5 against 20 for a plain bounce) and may stretch past the strip's
/// height, so the pull can cross the expand threshold.
fn pull_motion() -> ScrollBounceMotion {
    ScrollBounceMotion::default()
        .with_tracking(0.5)
        .with_max_stretch(metrics::FULL_HEIGHT * 1.25)
}

impl QuillApp {
    /// Settle the strip before the list is built: slide it in, expand it for
    /// a finished pull, or drop it once it has scrolled out of sight.
    pub(super) fn follow_story_pull(&mut self, cx: &mut Context<Self>) {
        if self.stories.strip.tiles.is_empty() {
            self.stories.strip.pull.collapse();
            return;
        }
        let now = Instant::now();
        let scroll_top = self.chat_list_scroll_top();
        if let Some(top) = self.stories.strip.pull.reveal_scroll_top(now, scroll_top) {
            self.set_chat_list_scroll_top(top);
        } else if self.stories.strip.pull.expanded() {
            if collapse_due(scroll_top, self.stories.strip.pull.revealing()) {
                self.stories.strip.pull.collapse();
                self.set_chat_list_scroll_top(scroll_top - metrics::FULL_HEIGHT);
            }
        } else {
            let pulled = self.stories.strip.bounce.get();
            if pull_expands(pulled.offset, pulled.held) {
                let start = self.stories.strip.pull.expand(now);
                self.set_chat_list_scroll_top(start);
            }
        }
        if self.stories.strip.pull.revealing() {
            self.request_animation_tick(60, cx);
        }
    }

    fn set_chat_list_scroll_top(&self, top: f32) {
        let mut offset = self.chat_list.scroll.offset();
        offset.y = px(-top.max(0.));
        self.chat_list.scroll.set_offset(offset);
    }

    /// The compact stack's opacity: full while the strip is collapsed (fading
    /// as a pull grows the strip), and following the scroll once the strip is
    /// a row of the list.
    pub(super) fn story_stack_opacity(&self) -> f32 {
        if self.stories.strip.pull.expanded() {
            metrics::compact_opacity(metrics::collapse_progress(self.chat_list_scroll_top()))
        } else {
            1.0 - pull_progress(self.stories.strip.bounce.get().offset)
        }
    }

    /// A tap on the compact stack: bring the strip back, from the top of the
    /// list (tdesktop `toggleExpandedRequests`).
    pub(super) fn expand_story_strip(&mut self, cx: &mut Context<Self>) {
        if self.stories.strip.pull.expanded() {
            self.set_chat_list_scroll_top(0.);
        } else {
            let start = self.stories.strip.pull.expand(Instant::now());
            self.set_chat_list_scroll_top(start);
        }
        self.notify_sidebar(cx);
    }

    /// The strip drawn into the gap a pull opens above the list. It sits
    /// just above the list's top, so the list's own displacement reveals it.
    pub(super) fn story_pull_gap(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.stories.strip.pull.expanded()
            || self.stories.strip.tiles.is_empty()
            || self.stories.strip.bounce.get().offset <= 0.
        {
            return None;
        }
        Some(
            div()
                .absolute()
                .top(px(-metrics::FULL_HEIGHT))
                .left_0()
                .w_full()
                .child(self.story_strip_element(cx))
                .into_any_element(),
        )
    }

    /// Wrap the chat list in the kit's bounce. `content` is the list and
    /// whatever rides with it.
    pub(super) fn chat_list_bounce(&self, content: impl IntoElement) -> ScrollBounce {
        let has_strip = !self.stories.strip.tiles.is_empty();
        let at_top = self.chat_list_scroll_top() <= 0.5 || self.stories.strip.pull.revealing();
        ScrollBounce::new("chat-list-bounce", &self.chat_list.scroll, content)
            .enabled(true)
            .displacement(self.stories.strip.bounce.clone())
            .when(has_strip && at_top, |bounce| bounce.motion(pull_motion()))
    }
}
