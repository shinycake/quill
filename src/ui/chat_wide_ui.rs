//! "Adaptive layout for wide screens", the UI half (`quill::chat_wide`):
//! the history's width and width class measured at paint, the wide column for
//! service rows and dates, and the Appearance checkbox.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::*;
use quill::bubble_layout::HistoryWidth;
use quill::chat_wide::{ChatLayout, WIDE_CHAT_WIDTH, is_chat_wide};
use quill::lang::{keys, tr};

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

/// The width bubble layout sees for a history `width` wide: floored to
/// whole [`WIDTH_STEP`]s, so dragging a window edge re-lays the rows out
/// every few pixels instead of every pixel, and the bubbles never
/// outgrow the pane.
pub(super) fn history_pane_width(width: f32) -> f32 {
    (width / WIDTH_STEP).floor() * WIDTH_STEP
}

/// Resize granularity of [`history_pane_width`], in logical pixels.
const WIDTH_STEP: f32 = 8.;

impl QuillApp {
    /// Until the history has painted, lays its bubbles out for `pane`, the
    /// width of the conversation pane it is about to fill, so the first
    /// frame is not the wide layout shrinking a frame later. The paint
    /// probe is the source of truth from then on.
    pub(in crate::ui) fn seed_history_width(&self, pane: f32) {
        if !self.history.width_probed.get() {
            self.history
                .width
                .set(HistoryWidth::new(history_pane_width(pane)));
        }
    }

    /// `Adaptive::isChatWide` for the history being built.
    pub(super) fn chat_wide(&self) -> bool {
        is_chat_wide(
            self.appearance.adaptive_for_wide,
            self.history.chat_layout.get(),
        )
    }

    /// Notes the history's width at paint (`HistoryWidget::recountChatWidth`)
    /// and renders again when it crosses `adaptiveChatWideWidth`, or when
    /// it changes how wide the bubbles may be (`HistoryWidth`, which reads
    /// the same for every wide history).
    pub(super) fn history_width_probe(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let layout = self.history.chat_layout.clone();
        let history_width = self.history.width.clone();
        let probed = self.history.width_probed.clone();
        let app = cx.weak_entity();
        canvas(
            |_, _, _| {},
            move |bounds, _, window, cx| {
                let width = f32::from(bounds.size.width);
                let now = ChatLayout::for_width(width);
                let layout_changed = layout.replace(now) != now;
                let measured = HistoryWidth::new(history_pane_width(width));
                probed.set(true);
                let width_changed = history_width.replace(measured) != measured;
                if layout_changed || width_changed {
                    let app = app.clone();
                    window.defer(cx, move |window, cx| {
                        let _ = app.update(cx, |this, cx| {
                            // The history painted is this window's.
                            this.in_window_of(window, cx, |this, _, cx| {
                                // Rows change height (photos join, dates
                                // move). A new bubble width only changes
                                // what the rows draw, not the rows: their
                                // heights are measured again below.
                                if layout_changed {
                                    this.history.rows_key = None;
                                }
                                this.history
                                    .scroller
                                    .update(cx, |state, cx| state.remeasure(cx));
                                cx.notify();
                            });
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
                .label(tr(keys::SETTINGS_ADAPTIVE_WIDE))
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

#[cfg(test)]
mod tests {
    use super::history_pane_width;
    use quill::bubble_layout::HistoryWidth;

    #[test]
    fn the_pane_width_steps_down_and_never_exceeds_the_pane() {
        assert_eq!(history_pane_width(380.), 376.);
        assert_eq!(history_pane_width(383.9), 376.);
        assert_eq!(history_pane_width(384.), 384.);
        assert_eq!(history_pane_width(640.), 640.);
        let mut width = 0.;
        while width < 1300. {
            let stepped = history_pane_width(width);
            assert!(stepped <= width && width - stepped < 8., "{width}");
            width += 0.37;
        }
    }

    #[test]
    fn a_resize_drag_changes_the_width_once_per_step() {
        let seen = (380..640)
            .map(|px| HistoryWidth::new(history_pane_width(px as f32)))
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(seen.len(), 33);
        assert_eq!(
            HistoryWidth::new(history_pane_width(f32::NAN)),
            HistoryWidth::WIDE
        );
        assert_eq!(
            HistoryWidth::new(history_pane_width(f32::INFINITY)),
            HistoryWidth::WIDE
        );
    }
}
