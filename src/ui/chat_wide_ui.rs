//! "Adaptive layout for wide screens", the UI half (`quill::chat_wide`):
//! the history's width class measured at paint, the wide column for
//! service rows and dates, and the Appearance checkbox.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::*;
use quill::chat_wide::{ChatLayout, WIDE_CHAT_WIDTH, is_chat_wide};

/// The scroller's row inset (`MessageScroller` wraps each row in `px_3`):
/// rows start this far from the history's left edge, so a column that is
/// [`WIDE_CHAT_WIDTH`] wide from that edge is this much narrower on each
/// side inside a row.
pub(super) const ROW_INSET: f32 = 12.;

/// The wide column inside a history row: centred content in it lands
/// where tdesktop centres it in `WideChatWidth()` from the history's edge.
pub(super) fn wide_column(wide: bool, content: impl IntoElement) -> AnyElement {
    if !wide {
        return content.into_any_element();
    }
    div()
        .w_full()
        .child(
            div()
                .w(px(WIDE_CHAT_WIDTH - 2. * ROW_INSET))
                .max_w_full()
                .child(content),
        )
        .into_any_element()
}

impl QuillApp {
    /// `Adaptive::isChatWide` for the history being built.
    pub(super) fn chat_wide(&self) -> bool {
        is_chat_wide(
            self.appearance.adaptive_for_wide,
            self.history.chat_layout.get(),
        )
    }

    /// Notes the history's width at paint (`HistoryWidget::recountChatWidth`)
    /// and renders again when it crosses `adaptiveChatWideWidth`.
    pub(super) fn chat_layout_probe(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let layout = self.history.chat_layout.clone();
        let app = cx.weak_entity();
        canvas(
            |_, _, _| {},
            move |bounds, _, window, cx| {
                let now = ChatLayout::for_width(f32::from(bounds.size.width));
                if layout.replace(now) != now {
                    let app = app.clone();
                    window.defer(cx, move |_, cx| {
                        let _ = app.update(cx, |this, cx| {
                            // Rows change height (photos join, dates move).
                            this.history.rows_key = None;
                            this.history
                                .scroller
                                .update(cx, |state, cx| state.remeasure(cx));
                            cx.notify();
                        });
                    });
                }
            },
        )
        .absolute()
        .inset_0()
        .size_full()
    }

    /// Settings → Appearance: tdesktop's "Adaptive layout for wide
    /// screens" checkbox, under "Tile background". Like tdesktop it shows
    /// only while the chat is wide enough for it to matter.
    pub(super) fn adaptive_wide_checkbox(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        (self.history.chat_layout.get() == ChatLayout::Wide).then(|| {
            Checkbox::new("appearance-adaptive-wide")
                .checked(self.appearance.adaptive_for_wide)
                .label("Adaptive layout for wide screens")
                .on_click(cx.listener(|this, &on, _, cx| {
                    this.set_appearance(cx, |a| a.adaptive_for_wide = on);
                    this.history.rows_key = None;
                    this.history
                        .scroller
                        .update(cx, |state, cx| state.remeasure(cx));
                }))
                .into_any_element()
        })
    }
}
