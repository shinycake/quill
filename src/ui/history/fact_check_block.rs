//! The "Fact Check" block under a message (Telegram Desktop
//! `WebPageType::Factcheck` in `history_view_web_page.cpp`): a red-barred
//! quote with the title, the clarification text collapsed to three lines,
//! a chevron to expand, and, expanded, the footer naming the assigning
//! country. "what's this?" explains the block in the status line.

use super::super::app::QuillApp;
use super::super::message_text::rich_text_line;
use gpui_kit::assets::IconName;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::ids::{ChatId, MessageId};
use quill::state::{HistoryMessage, Session};
use std::collections::HashMap;

/// tdesktop `kFactcheckCollapsedLines`.
const COLLAPSED_LINES: f32 = 3.;
/// Line height factor the collapsed box assumes for the message font.
const LINE_HEIGHT: f32 = 1.4;

/// tdesktop `lng_factcheck_about` / `lng_factcheck_bottom`.
pub(crate) fn about_text(country: &str) -> String {
    format!(
        "This clarification was provided by a fact checking agency assigned by the department of the government of your country ({country}) responsible for combatting misinformation."
    )
}

/// The country's English name from the sign-in country list when it is
/// loaded, else the ISO code (tdesktop `LookupFactcheckCountryName`).
pub(crate) fn country_name(session: Option<&Session>, iso2: &str) -> String {
    session
        .and_then(|session| session.auth_state.countries.as_ref())
        .and_then(|countries| {
            countries
                .iter()
                .find(|country| country.iso.eq_ignore_ascii_case(iso2))
        })
        .map(|country| country.english_name.clone())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| iso2.to_string())
}

/// The block for `message`, or nothing when it has no fact check.
pub(in crate::ui) fn fact_check_block(
    message: &HistoryMessage,
    expanded: bool,
    revealed: &std::collections::HashSet<(i64, u64, u64, bool)>,
    font: Pixels,
    session: Option<&Session>,
    cx: &mut Context<QuillApp>,
) -> Option<AnyElement> {
    let check = message.extras.fact_check.as_ref()?;
    let (chat_id, message_id) = (message.chat_id, message.id);
    let row_id = message.id.0 as u64;
    let country = country_name(session, &check.country_code);
    let bar = cx.theme().danger;
    let text = rich_text_line(
        &check.text,
        &check.entities,
        (chat_id.0, row_id),
        true,
        revealed,
        font,
        &HashMap::new(),
        cx,
    );
    let title = div()
        .flex()
        .items_center()
        .gap_2()
        .child(
            div()
                .text_size(font)
                .font_semibold()
                .text_color(bar)
                .child("Fact Check"),
        )
        .when(!check.country_code.is_empty(), |this| {
            let about = about_text(&country);
            this.child(
                div()
                    .id(("fact-check-about", row_id))
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .cursor_pointer()
                    .role(Role::Button)
                    .aria_label("About fact checks")
                    .tab_index(0)
                    .child("what's this?")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.connection.status_note = about.clone();
                        cx.notify();
                    })),
            )
        });
    let body = div()
        .id(("fact-check-text", row_id))
        .when(!expanded, |this| {
            this.max_h(px(f32::from(font) * LINE_HEIGHT * COLLAPSED_LINES))
                .overflow_hidden()
        })
        .child(text);
    let chevron = div()
        .id(("fact-check-toggle", row_id))
        .absolute()
        .right_1()
        .bottom_1()
        .size_5()
        .flex()
        .items_center()
        .justify_center()
        .rounded_sm()
        .cursor_pointer()
        .role(Role::Button)
        .aria_label(if expanded {
            "Collapse fact check"
        } else {
            "Expand fact check"
        })
        .tab_index(0)
        .hover(|style| style.bg(bar.opacity(0.12)))
        .child(
            Icon::new(if expanded {
                IconName::ChevronUp
            } else {
                IconName::ChevronDown
            })
            .size_4()
            .text_color(bar),
        )
        .on_click(cx.listener(move |this, _, _, cx| {
            this.toggle_fact_check(chat_id, message_id, cx);
        }));
    Some(
        div()
            .id(("fact-check", row_id))
            .relative()
            .mt_2()
            .pl_2()
            .pr_6()
            .py_1()
            .rounded_md()
            .border_l_2()
            .border_color(bar)
            .bg(bar.opacity(0.08))
            .flex()
            .flex_col()
            .gap_0p5()
            .child(title)
            .child(body)
            .when(expanded, |this| {
                // tdesktop `factcheckFooterSkip` and a 30% separator.
                this.child(div().mt_2().h(px(1.)).w_full().bg(bar.opacity(0.3)))
                    .child(
                        div()
                            .pt_1()
                            .text_xs()
                            .text_color(bar)
                            .child(about_text(&country)),
                    )
            })
            .child(chevron)
            .into_any_element(),
    )
}

impl QuillApp {
    /// The chevron: expand or collapse one message's fact check.
    pub(in crate::ui) fn toggle_fact_check(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        cx: &mut Context<Self>,
    ) {
        let key = (chat_id.0, message_id.0);
        if !self.message_ui.fact_check_expanded.remove(&key) {
            self.message_ui.fact_check_expanded.insert(key);
        }
        cx.notify();
    }
}
