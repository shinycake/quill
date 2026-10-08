//! sponsored messages.

use super::app::QuillApp;
use super::demo::{demo_file_json, demo_thumb_png_path};
use super::message_media::{
    MediaCorners, animation_attachment, document_chip, photo_attachment, video_attachment,
};
use super::message_text::message_text_block;
use super::*;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::{ChatId, MessageId};
use quill::state::{RequestPurpose, Session, SponsoredReportFlight};
use quill::telegram::client::copy_and_parse;
use quill::telegram::envelope::{MessageContent, ParsedFile, SponsoredMessage};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use synthetic::BubbleLook;
/// `ReadySponsored` fixture: open the demo channel (id 13, now ungated) and
/// inject a `sponsoredMessages` response through the same reducer the live
/// `getChatSponsoredMessages` path uses — one Sponsored row, one Recommended.
/// The fixture still swaps the history pane for the sponsored rows pane.
pub(super) fn apply_ready_sponsored(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    session.open_chat(ChatId(13));
    let extra = session.request(RequestPurpose::GetChatSponsoredMessages, Some(ChatId(13)));
    let thumb = demo_file_json(61, &demo_thumb_png_path(), true);
    let json = format!(
        r#"{{"@type":"sponsoredMessages","@extra":"{}","messages_between":3,"messages":[{{"@type":"sponsoredMessage","message_id":9001,"is_recommended":false,"can_be_reported":true,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"Sponsored demo row — tap Report to open the option picker.","entities":[]}}}},"sponsor":{{"@type":"advertisementSponsor","url":"https://example.com/promo","photo":{{"@type":"photo","has_stickers":false,"sizes":[]}},"info":"Example Ads"}},"title":"Summer sale","button_text":"Shop now","accent_color_id":0,"background_custom_emoji_id":"0","additional_info":"Ad by Example"}},{{"@type":"sponsoredMessage","message_id":9002,"is_recommended":true,"can_be_reported":false,"content":{{"@type":"messagePhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"m","photo":{thumb},"width":240,"height":160,"progressive_sizes":[]}}]}},"caption":{{"@type":"formattedText","text":"Recommended demo photo row.","entities":[]}},"has_spoiler":false,"is_secret":false}},"sponsor":{{"@type":"advertisementSponsor","url":"https://example.com/pick","photo":{{"@type":"photo","has_stickers":false,"sizes":[]}},"info":"Curated"}},"title":"Editors' pick","button_text":"Learn more","accent_color_id":0,"background_custom_emoji_id":"0","additional_info":""}}]}}"#,
        extra.0,
    );
    if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
        session.apply(owned);
    }
}

/// One `sponsoredMessage` row: Sponsored / Recommended label, title, content,
/// sponsor button, and a Report button when `can_be_reported` is set.
/// Media reuses the history attachment helpers, so thumbs download at
/// priority 1 and a tap fetches the full file at priority 32.
pub(super) fn sponsored_message_row(
    chat_id: ChatId,
    message: &SponsoredMessage,
    files: &HashMap<i32, ParsedFile>,
    downloading: &std::collections::HashSet<i32>,
    failed: &std::collections::HashSet<i32>,
    media_roots: &[PathBuf],
    revealed: &std::collections::HashSet<(i64, u64, u64, bool)>,
    // Settings → Appearance: font size + bubble/plain style.
    look: BubbleLook,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let row_id = message.message_id as u64;
    let header = div()
        .id(("sponsored-row-header", row_id))
        .flex()
        .items_center()
        .gap_2()
        .child(
            div()
                .id(("sponsored-row-label", row_id))
                .text_xs()
                .px_2()
                .py_1()
                .rounded_md()
                .bg(accent_strong())
                .text_color(text_on_fill())
                .child(message.kind_label()),
        )
        .child(div().font_semibold().text_sm().child(message.title.clone()));
    let content: Option<AnyElement> = match &message.content {
        MessageContent::Text(text) => Some(message_text_block(
            (chat_id.0, row_id),
            text,
            files,
            downloading,
            media_roots,
            // Sponsored demo rows carry no custom emoji entities.
            &[],
            &HashMap::new(),
            revealed,
            // Settings → Appearance: message font size.
            look.font,
            false,
            None,
            cx,
        )),
        MessageContent::Photo(photo) => Some(
            div()
                .mt_2()
                .child(photo_attachment(
                    row_id,
                    photo,
                    files,
                    downloading,
                    media_roots,
                    Some((chat_id, message.message_id)),
                    None,
                    MediaCorners::small(),
                    cx,
                ))
                .into_any_element(),
        ),
        MessageContent::Animation(animation) => Some(
            div()
                .mt_2()
                .child(animation_attachment(
                    MessageId(message.message_id),
                    animation,
                    files,
                    downloading,
                    media_roots,
                    false,
                    None,
                    None,
                    Some((chat_id, message.message_id)),
                    MediaCorners::small(),
                    cx,
                ))
                .into_any_element(),
        ),
        MessageContent::Video(video) => Some(
            div()
                .mt_2()
                .child(video_attachment(
                    MessageId(message.message_id),
                    video,
                    files,
                    downloading,
                    media_roots,
                    false,
                    None,
                    None,
                    Some((chat_id, message.message_id)),
                    None,
                    MediaCorners::small(),
                    cx,
                ))
                .into_any_element(),
        ),
        MessageContent::Document(doc) => Some(document_chip(
            row_id,
            doc,
            false,
            files,
            downloading,
            failed,
            // Sponsored rows don't expose the list-API pause toggle (the
            // manager panel does); `None` hides it.
            None,
            Some((chat_id, message.message_id)),
            cx,
        )),
        _ => None,
    };
    let mut row = div()
        .id(("sponsored-row", row_id))
        .flex()
        .flex_col()
        .gap_1()
        .px_3()
        .py_2()
        .rounded_lg()
        .border_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().sidebar)
        .child(header);
    if let Some(content) = content {
        row = row.child(content);
    }
    if !message.sponsor.info.is_empty() {
        row = row.child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(message.sponsor.info.clone()),
        );
    }
    if !message.additional_info.is_empty() {
        row = row.child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(message.additional_info.clone()),
        );
    }
    let url = message.sponsor.url.clone();
    if !message.button_text.is_empty() && !url.is_empty() {
        let label = message.button_text.clone();
        let sponsored_id = message.message_id;
        row = row.child(
            Button::new(format!("sponsored-open-{row_id}"))
                .label(label)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.click_sponsored_message(chat_id, sponsored_id, false, cx);
                    this.open_message_url(&url, cx);
                })),
        );
    }
    if message.can_be_reported {
        let id = message.message_id;
        row = row.child(
            Button::new(format!("sponsored-report-{row_id}"))
                .label("Report")
                .ghost()
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.report_sponsored_message_ui(chat_id, id, cx);
                })),
        );
    }
    row.into_any_element()
}

impl QuillApp {
    /// `clickChatSponsoredMessage` for a sponsored row interaction. `is_media_click`
    /// is true when the user opened the row's media; false for the sponsor
    /// button/link. Demo sessions have no live driver, so the click is a no-op.
    pub(super) fn click_sponsored_message(
        &mut self,
        chat_id: ChatId,
        message_id: i64,
        is_media_click: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.click_chat_sponsored_message(
                chat_id,
                message_id,
                is_media_click,
                false,
            );
        }
        let _ = cx;
    }

    /// Fixture/proof surface for `ReadySponsored`: the demo channel renders
    /// its `getChatSponsoredMessages` rows with Sponsored / Recommended labels
    /// instead of history.
    pub(super) fn sponsored_rows_pane(&self, cx: &mut Context<Self>) -> impl IntoElement {
        // Settings → Appearance: font size + bubble/plain style.
        let look = self.bubble_look(cx);
        let session = self.session();
        let open = session.and_then(|s| s.open_chat);
        let rows: Vec<SponsoredMessage> = session
            .map(|s| s.open_sponsored_rows().into_iter().cloned().collect())
            .unwrap_or_default();
        let files: HashMap<i32, ParsedFile> = session.map(|s| s.files.clone()).unwrap_or_default();
        let downloading: std::collections::HashSet<i32> =
            session.map(|s| s.downloading.clone()).unwrap_or_default();
        let failed: std::collections::HashSet<i32> = session
            .map(|s| s.failed_downloads.clone())
            .unwrap_or_default();
        let media_roots = self.media_display_roots();
        let report = session.and_then(|s| s.sponsored_report.clone());
        let outcome = session.and_then(|s| s.last_sponsored_report.clone());
        let chat_id = open.unwrap_or(ChatId(0));
        let mut list = div()
            .id("sponsored-rows")
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .px_3()
            .pt_2()
            .gap_2();
        if let Some(outcome) = outcome {
            let message = outcome.user_message().to_string();
            list = list.child(
                div()
                    .id("sponsored-report-outcome")
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_3()
                    .py_2()
                    .rounded_md()
                    .border_1()
                    .border_color(cx.theme().border)
                    .bg(cx.theme().sidebar)
                    .child(div().text_sm().child(message))
                    .child(
                        Button::new("sponsored-outcome-dismiss")
                            .label("Dismiss")
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.dismiss_sponsored_outcome_ui(cx);
                            })),
                    ),
            );
        }
        if let Some(flight) = report {
            list = list.child(self.sponsored_report_panel(&flight, cx));
        }
        if rows.is_empty() {
            list = list.child(
                div()
                    .id("sponsored-empty")
                    .text_sm()
                    .child("No sponsored messages for this chat."),
            );
        }
        for message in &rows {
            list = list.child(sponsored_message_row(
                chat_id,
                message,
                &files,
                &downloading,
                &failed,
                &media_roots,
                &self.spoiler_revealed,
                // Settings → Appearance: font size + bubble/plain style.
                look,
                cx,
            ));
        }
        div()
            .id("sponsored-pane")
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .child(list)
    }

    /// `reportSponsoredResultOptionRequired` picker.
    pub(super) fn sponsored_report_panel(
        &self,
        flight: &SponsoredReportFlight,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let mut options = div()
            .id("sponsored-report-options")
            .flex()
            .flex_col()
            .gap_1();
        for option in &flight.options {
            let option_id = option.id.clone();
            options = options.child(
                Button::new(format!("sponsored-report-option-{}", option.id))
                    .label(option.text.clone())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.pick_sponsored_report_option(&option_id, cx);
                    })),
            );
        }
        div()
            .id("sponsored-report-picker")
            .flex()
            .flex_col()
            .gap_2()
            .px_3()
            .py_2()
            .rounded_md()
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().sidebar)
            .child(div().font_semibold().child(flight.title.clone()))
            .child(options)
            .child(
                Button::new("sponsored-report-cancel")
                    .label("Cancel")
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.dismiss_sponsored_report_ui(cx);
                    })),
            )
    }

    /// Start `reportChatSponsoredMessage` for a sponsored row. Live: the
    /// driver sends the request with an empty option id. Demo: inject
    /// `reportSponsoredResultOptionRequired` through the same reducer.
    pub(super) fn report_sponsored_message_ui(
        &mut self,
        chat_id: ChatId,
        message_id: i64,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            self.status_note = match live
                .driver
                .report_sponsored_message(chat_id, message_id, "")
            {
                Ok(Some(_)) => "reporting sponsored message…".into(),
                Ok(None) => "report not available for this row".into(),
                Err(_) => "could not send report".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            if session
                .begin_sponsored_report(chat_id, message_id)
                .is_some()
            {
                let extra =
                    session.request(RequestPurpose::ReportChatSponsoredMessage, Some(chat_id));
                let json = format!(
                    r#"{{"@type":"reportSponsoredResultOptionRequired","@extra":"{}","title":"Why are you reporting this ad?","options":[{{"@type":"reportOption","id":"bWlzLWxlYWQ=","text":"Misleading or scam"}},{{"@type":"reportOption","id":"c3BhbQ==","text":"Spam"}}]}}"#,
                    extra.0
                );
                let dyn_sink: Arc<dyn DiagnosticSink> = self.demo_sink.clone();
                if let Some(owned) = copy_and_parse(&json, &self.demo_seq, &dyn_sink) {
                    session.apply(owned);
                }
                self.status_note = "demo — report options injected".into();
            } else {
                self.status_note = "report not available for this row".into();
            }
        }
        cx.notify();
    }

    /// Send the chosen `reportOption` id for the in-flight sponsored report.
    /// Live: the driver sends the follow-up request. Demo: resolve with
    /// `reportSponsoredResultOk` through the same reducer.
    pub(super) fn pick_sponsored_report_option(&mut self, option_id: &str, cx: &mut Context<Self>) {
        let Some(flight) = self.session().and_then(|s| s.sponsored_report.clone()) else {
            return;
        };
        if let Some(live) = self.live.as_mut() {
            self.status_note = match live.driver.report_sponsored_message(
                flight.chat_id,
                flight.message_id,
                option_id,
            ) {
                Ok(_) => "report sent…".into(),
                Err(_) => "could not send report".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            let extra = session.request(
                RequestPurpose::ReportChatSponsoredMessage,
                Some(flight.chat_id),
            );
            let json = format!(
                r#"{{"@type":"reportSponsoredResultOk","@extra":"{}"}}"#,
                extra.0
            );
            let dyn_sink: Arc<dyn DiagnosticSink> = self.demo_sink.clone();
            if let Some(owned) = copy_and_parse(&json, &self.demo_seq, &dyn_sink) {
                session.apply(owned);
            }
            self.status_note = "demo — report sent".into();
        }
        cx.notify();
    }

    pub(super) fn dismiss_sponsored_report_ui(&mut self, cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            session.dismiss_sponsored_report();
        } else if let Some(live) = self.live.as_mut() {
            live.driver.session.dismiss_sponsored_report();
        }
        cx.notify();
    }

    pub(super) fn dismiss_sponsored_outcome_ui(&mut self, cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            session.clear_sponsored_report_outcome();
        } else if let Some(live) = self.live.as_mut() {
            live.driver.session.clear_sponsored_report_outcome();
        }
        cx.notify();
    }
}
