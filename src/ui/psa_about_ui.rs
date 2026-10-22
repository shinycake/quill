//! The one-time "about this announcement" box (tdesktop
//! `HistoryWidget::showInfoTooltip`, a `HistoryView::InfoTooltip` toast
//! attached to the top of the conversation with the info icon). Whether it
//! shows, and its text, are session state ([`quill::psa_about`]): it is
//! drawn while the open chat is its chat and its time has not run out. This
//! file only draws it; the frame clock repaints once its time is up.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use gpui_kit::assets::IconName;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::psa_about::{plain_text, rich_runs};

/// The box's width (`historyInfoToast.maxWidth`).
const BOX_WIDTH: f32 = 380.;
/// Gap between the bars above the conversation and the box
/// (`st::defaultToast.margin.top`).
const BOX_MARGIN_TOP: f32 = 13.;
/// How often the clock repaints while the box is up, to catch its end.
const WATCH_FPS: u32 = 2;

impl QuillApp {
    /// The box for `chat_id`, as a zero-height anchor right under the
    /// header and bars so the box floats over the top of the history.
    pub(super) fn psa_about_view(
        &self,
        chat_id: ChatId,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let session = self.session()?;
        session
            .chat_list
            .psa_about
            .visible(session.open_chat, std::time::Instant::now())
            .filter(|toast| toast.chat_id == chat_id)?;
        self.psa_about_view_body(chat_id, cx)
    }

    /// `psa_about_view` past its cheap check. A debug build reserves this
    /// builder's whole frame on entry, so frames that show nothing skip
    /// it (docs/decisions/main-thread-stack.md).
    /// The outer function repeats this body's first check; keep the two in step.
    #[inline(never)]
    fn psa_about_view_body(&self, chat_id: ChatId, cx: &mut Context<Self>) -> Option<AnyElement> {
        let session = self.session()?;
        let about = &session.chat_list.psa_about;
        about
            .visible(session.open_chat, std::time::Instant::now())
            .filter(|toast| toast.chat_id == chat_id)?;
        let promotion = session.chat_list.promoted_chats.get(&chat_id.0)?;
        let text = session.chat_list.psa_labels.about(promotion);
        self.request_animation_tick(WATCH_FPS, cx);
        let plain = plain_text(&text);
        let mut highlights = Vec::new();
        let mut start = 0;
        for (run, bold) in rich_runs(&text) {
            let end = start + run.len();
            if bold {
                highlights.push((
                    start..end,
                    HighlightStyle {
                        font_weight: Some(FontWeight::BOLD),
                        ..Default::default()
                    },
                ));
            }
            start = end;
        }
        let card = div()
            .id("psa-about-box")
            .test_support()
            .role(Role::Status)
            .aria_label(plain.clone())
            .w(px(BOX_WIDTH))
            .max_w_full()
            .flex()
            .items_start()
            .gap_3()
            .px_4()
            .py_3()
            .rounded_lg()
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().popover)
            .text_color(cx.theme().popover_foreground)
            .shadow_md()
            .text_sm()
            .child(
                div().flex_none().pt(px(2.)).child(
                    Icon::new(IconName::Info)
                        .size_4()
                        .text_color(cx.theme().popover_foreground),
                ),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(StyledText::new(plain).with_highlights(highlights)),
            );
        Some(
            div()
                .id("psa-about-anchor")
                .relative()
                .flex_none()
                .w_full()
                .h(px(0.))
                .child(deferred(
                    div()
                        .absolute()
                        .top(px(BOX_MARGIN_TOP))
                        .left_0()
                        .right_0()
                        .px_4()
                        .flex()
                        .justify_center()
                        .child(card),
                ))
                .into_any_element(),
        )
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "psa_about_ui_tests.rs"]
mod tests;
