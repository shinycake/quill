//! The "About this giveaway" box (tdesktop `GiveawayInfoBox` in
//! `boxes/gift_premium_box.cpp`), opened by the Learn more button under a
//! giveaway card. It only reads: running or ended, whether the user takes
//! part, whether they won.

use super::app::QuillApp;
use super::link_info_boxes::finish;
use super::shell::{DialogKind, QuillShell};
use super::*;
use gpui_base::StyledExt;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::giveaway::{AboutText, GiveawayTextContext, about_text};
use quill::ids::{ChatId, MessageId};
use quill::local_time::{civil_local, day_label, hhmm, now_unix};
use quill::state::{GiveawayInfoLookup, Session};
use quill::telegram::envelope::{GiftCard, GiftCardKind, GiveawayInfoData, GiveawaySummary};

/// "October 6" for a Unix timestamp (tdesktop `langDayOfMonthFull`).
pub(super) fn day_text(unix: i32) -> String {
    day_label(&civil_local(i64::from(unix)), &civil_local(now_unix()))
}

/// "October 6 at 14:05" (tdesktop `langDateTime`).
pub(super) fn date_time_text(unix: i32) -> String {
    let at = civil_local(i64::from(unix));
    format!(
        "{} at {}",
        day_label(&at, &civil_local(now_unix())),
        hhmm(&at)
    )
}

/// The chat `getGiveawayInfo` is asked about: the original giveaway
/// message for a winners card, the card's own message otherwise.
pub(super) fn info_target(
    summary: &GiveawaySummary,
    chat_id: ChatId,
    message_id: MessageId,
) -> (i64, i64) {
    if summary.info_message_id != 0 {
        (summary.boosted_chat_id, summary.info_message_id)
    } else {
        (chat_id.0, message_id.0)
    }
}

/// The Learn more button under a giveaway card; `None` for other cards.
pub(super) fn giveaway_learn_more(
    row_id: u64,
    chat_id: ChatId,
    message_id: MessageId,
    card: &GiftCard,
    cx: &mut Context<QuillApp>,
) -> Option<AnyElement> {
    if !matches!(
        card.kind,
        GiftCardKind::Giveaway | GiftCardKind::GiveawayWinners
    ) {
        return None;
    }
    let summary = card.giveaway.clone()?;
    Some(
        Button::new(("giveaway-learn-more", row_id))
            .label("Learn more")
            .small()
            .ghost()
            .on_click(cx.listener(move |this, _, _, cx| {
                this.open_giveaway_info(summary.clone(), chat_id, message_id, cx);
            }))
            .into_any_element(),
    )
}

fn chat_title(session: Option<&Session>, id: i64) -> Option<String> {
    session
        .and_then(|s| s.chats.get(&id))
        .map(|chat| chat.title.clone())
        .filter(|title| !title.is_empty())
}

/// The wording of `lookup`'s box, once TDLib has answered.
pub(super) fn about_for(
    session: Option<&Session>,
    lookup: &GiveawayInfoLookup,
) -> Option<AboutText> {
    let info = lookup.info.as_ref()?;
    let boosted = lookup.summary.boosted_chat_id;
    let group = session
        .and_then(|s| s.chats.get(&boosted))
        .is_some_and(|chat| {
            matches!(
                chat.kind,
                quill::telegram::envelope::ChatKind::Supergroup {
                    is_channel: false,
                    ..
                }
            )
        });
    let title = |id: i64| chat_title(session, id).unwrap_or_else(|| "a channel".into());
    let ctx = GiveawayTextContext {
        first_channel: chat_title(session, boosted).unwrap_or_else(|| "channel".into()),
        group,
        chat_title: &title,
        day: &day_text,
        date_time: &date_time_text,
    };
    Some(about_text(&lookup.summary, info, &ctx))
}

impl QuillApp {
    fn giveaway_session_mut(&mut self) -> Option<&mut Session> {
        match self.live.as_mut() {
            Some(live) => Some(&mut live.driver.session),
            None => self.demo_session.as_mut(),
        }
    }

    /// Open the box and ask TDLib about the giveaway. Only reads.
    pub(super) fn open_giveaway_info(
        &mut self,
        summary: GiveawaySummary,
        chat_id: ChatId,
        message_id: MessageId,
        cx: &mut Context<Self>,
    ) {
        let (chat, message) = info_target(&summary, chat_id, message_id);
        match self.live.as_mut() {
            Some(live) => {
                if live
                    .driver
                    .request_giveaway_info(chat, message, summary)
                    .is_err()
                {
                    self.connection.status_note = "could not load the giveaway".into();
                }
            }
            None => {
                self.connection.status_note = "giveaway details need a signed-in account".into();
            }
        }
        cx.notify();
    }

    fn close_giveaway_info(&mut self, cx: &mut Context<Self>) {
        if let Some(session) = self.giveaway_session_mut() {
            session.payments.giveaway.info = None;
        }
        cx.notify();
    }

    fn giveaway_info_body(session: Option<&Session>, lookup: &GiveawayInfoLookup) -> AnyElement {
        let mut body = div().flex().flex_col().gap_2();
        if lookup.loading {
            return body
                .child(
                    div()
                        .text_sm()
                        .text_color(text_muted())
                        .child("Loading the giveaway…"),
                )
                .into_any_element();
        }
        let Some(text) = about_for(session, lookup) else {
            let reason = lookup
                .error
                .clone()
                .unwrap_or_else(|| "This giveaway is not available any more.".into());
            return body
                .child(div().id("giveaway-info-error").text_sm().child(reason))
                .into_any_element();
        };
        if let Some(result) = text.result.clone() {
            body = body.child(
                div()
                    .id("giveaway-result")
                    .px_3()
                    .py_2()
                    .rounded_md()
                    .bg(text_muted().opacity(0.12))
                    .text_sm()
                    .when(text.won, |d| d.font_semibold())
                    .child(result),
            );
        }
        for (index, paragraph) in text
            .paragraphs
            .iter()
            .flat_map(|p| p.split("\n\n"))
            .enumerate()
        {
            body = body.child(
                div()
                    .id(("giveaway-paragraph", index as u64))
                    .text_sm()
                    .child(paragraph.to_string()),
            );
        }
        if let Some(notice) = text.refund_notice.clone() {
            body = body.child(
                div()
                    .id("giveaway-refunded")
                    .px_3()
                    .py_2()
                    .rounded_md()
                    .bg(danger_dark().opacity(0.12))
                    .text_sm()
                    .text_color(danger_dark())
                    .child(notice),
            );
        }
        body.into_any_element()
    }

    pub(super) fn build_giveaway_info_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::GiveawayInfo, |this, _, cx| {
                this.close_giveaway_info(cx);
            });
        app.update(cx, |this, cx| {
            let session = this.session();
            let lookup = session.and_then(|s| s.payments.giveaway.info.clone());
            let Some(lookup) = lookup else {
                return dialog
                    .overlay(true)
                    .title(crate::ui::shell::dialog_title("About this giveaway"))
                    .on_close(on_close.clone());
            };
            let title = about_for(session, &lookup).map_or("About this giveaway", |t| t.title);
            let prize_code = match &lookup.info {
                Some(GiveawayInfoData::Completed { gift_code, .. }) if !gift_code.0.is_empty() => {
                    Some(gift_code.0.clone())
                }
                _ => None,
            };
            let body = Self::giveaway_info_body(session, &lookup);
            let footer = div()
                .flex()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("giveaway-info-close")
                        .label("Close")
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.close_giveaway_info(cx);
                            this.close_kit_dialog_if_done(DialogKind::GiveawayInfo, window, cx);
                        })),
                )
                .when_some(prize_code, |footer, code| {
                    footer.child(
                        Button::new("giveaway-view-prize")
                            .label("View my prize")
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.close_giveaway_info(cx);
                                this.close_kit_dialog_if_done(DialogKind::GiveawayInfo, window, cx);
                                this.open_gift_code_link(code.clone(), cx);
                            })),
                    )
                })
                .into_any_element();
            finish(dialog, title, body, footer, on_close)
        })
    }
}

crate::ui::shell::register_dialogs! {
    /// "About this giveaway": running or ended, the user's part, the result.
    GiveawayInfo => DialogSpec::new(
        2937,
        |app| app.session().is_some_and(|s| s.payments.giveaway.info.is_some()),
        QuillApp::build_giveaway_info_dialog,
    ),
}
