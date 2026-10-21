//! The one-time "about this announcement" box (tdesktop
//! `HistoryWidget::showInfoTooltip`, a `HistoryView::InfoTooltip` toast
//! attached to the top of the conversation with the info icon). Whether it
//! shows, and its text, are session state ([`quill::psa_about`]); this file
//! draws it and runs the timer that takes it away.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use gpui_kit::assets::IconName;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::psa_about::{rich_runs, toast_duration};

/// The box's widest line (`historyInfoToast.maxWidth`).
const BOX_MAX_WIDTH: f32 = 380.;
/// Distance from the top of the window to the box: the chat header and the
/// toast margin (`st::defaultToast.margin.top`).
const BOX_TOP: f32 = 95.;

impl QuillApp {
    /// The box for the open chat, when there is one. Starts the timer that
    /// removes it the first frame it shows.
    pub(super) fn psa_about_layer(&mut self, cx: &mut Context<Self>) -> Option<Div> {
        let session = self.session()?;
        let toast = session.chat_list.psa_about.toast()?.clone();
        let promotion = session.chat_list.promoted_chats.get(&toast.chat_id.0)?;
        let text = session.chat_list.psa_labels.about(promotion);
        self.arm_psa_about_timer(&text, cx);
        let (left, right) = self.chat_insets();
        let mut plain = String::new();
        let mut highlights = Vec::new();
        for (run, bold) in rich_runs(&text) {
            let start = plain.len();
            plain.push_str(&run);
            if bold {
                highlights.push((
                    start..plain.len(),
                    HighlightStyle {
                        font_weight: Some(FontWeight::BOLD),
                        ..Default::default()
                    },
                ));
            }
        }
        Some(
            div()
                .absolute()
                .top(px(BOX_TOP))
                .left(left)
                .right(right)
                .px_4()
                .flex()
                .justify_center()
                .child(
                    div()
                        .id("psa-about-box")
                        .test_support()
                        .role(Role::Status)
                        .aria_label(plain.clone())
                        .w(px(BOX_MAX_WIDTH))
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
                        ),
                ),
        )
    }

    /// Takes the box away after its time (`CountToastDuration`), once per
    /// showing.
    fn arm_psa_about_timer(&mut self, text: &str, cx: &mut Context<Self>) {
        let Some(serial) = self.psa_about_mut().and_then(|about| about.arm()) else {
            return;
        };
        let duration = toast_duration(text);
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(duration).await;
            let _ = this.update(cx, |this, cx| {
                if let Some(about) = this.psa_about_mut() {
                    about.expire(serial);
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn psa_about_mut(&mut self) -> Option<&mut quill::psa_about::PsaAbout> {
        let session = match self.live.as_mut() {
            Some(live) => &mut live.driver.session,
            None => self.demo_session.as_mut()?,
        };
        Some(&mut session.chat_list.psa_about)
    }
}
