//! sponsored messages.

use super::app::QuillApp;
use super::demo::{demo_file_json, demo_thumb_png_path};
use super::message_media::{
    MediaCorners, animation_attachment, document_chip, photo_attachment, video_attachment,
};
use super::message_text::message_text_block;
use super::*;
use gpui_kit::component::button::*;
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
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
/// The demo channel then renders its normal history with the live-path
/// sponsored footer (the first row) below it.
pub(super) fn apply_ready_sponsored(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    session.open_chat(ChatId(13));
    for (id, text) in [
        (201, "Welcome to the channel. New posts land here."),
        (202, "Tonight's release notes are out - read them below."),
        (203, "Poll results: dark mode wins by a mile."),
    ] {
        let post = format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":13,"sender_id":{{"@type":"messageSenderChat","chat_id":13}},"is_outgoing":false,"is_channel_post":true,"date":1700000000,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"{text}","entities":[]}}}}}}}}"#
        );
        if let Some(owned) = copy_and_parse(&post, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
    let extra = session.request(RequestPurpose::GetChatSponsoredMessages, Some(ChatId(13)));
    let thumb = demo_file_json(61, &demo_thumb_png_path(), true);
    let json = format!(
        r#"{{"@type":"sponsoredMessages","@extra":"{}","messages_between":3,"messages":[{{"@type":"sponsoredMessage","message_id":9001,"is_recommended":false,"can_be_reported":true,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"Sponsored demo ad — open the menu for About, Report and Hide.","entities":[]}}}},"sponsor":{{"@type":"advertisementSponsor","url":"https://example.com/promo","photo":{{"@type":"photo","has_stickers":false,"sizes":[]}},"info":"Example Ads"}},"title":"Summer sale","button_text":"Shop now","accent_color_id":0,"background_custom_emoji_id":"0","additional_info":"Ad by Example"}},{{"@type":"sponsoredMessage","message_id":9002,"is_recommended":true,"can_be_reported":false,"content":{{"@type":"messagePhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"m","photo":{thumb},"width":240,"height":160,"progressive_sizes":[]}}]}},"caption":{{"@type":"formattedText","text":"Recommended demo photo row.","entities":[]}},"has_spoiler":false,"is_secret":false}},"sponsor":{{"@type":"advertisementSponsor","url":"https://example.com/pick","photo":{{"@type":"photo","has_stickers":false,"sizes":[]}},"info":"Curated"}},"title":"Editors' pick","button_text":"Learn more","accent_color_id":0,"background_custom_emoji_id":"0","additional_info":""}}]}}"#,
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
    let can_report = message.can_be_reported;
    let owner = cx.entity().downgrade();
    let about_message = message.clone();
    let menu_id = message.message_id;
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
                .child(message.badge_label()),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .font_semibold()
                .text_sm()
                .child(message.title.clone()),
        )
        // tdesktop's "Ad" menu: About this ad / Report ad / Hide ads.
        .child(
            Button::new(format!("sponsored-menu-{row_id}"))
                .icon(IconName::Ellipsis)
                .ghost()
                .tooltip("About this ad")
                .accessibility_label("Ad options")
                .dropdown_menu(move |menu, _, _| {
                    let about = owner.clone();
                    let about_message = about_message.clone();
                    let report = owner.clone();
                    let hide = owner.clone();
                    let mut menu = menu.item(PopupMenuItem::new("About this ad").on_click(
                        move |_, _, cx| {
                            let _ = about.update(cx, |this, cx| {
                                this.open_sponsored_about(&about_message, cx)
                            });
                        },
                    ));
                    if can_report {
                        menu =
                            menu.item(PopupMenuItem::new("Report ad").on_click(move |_, _, cx| {
                                let _ = report.update(cx, |this, cx| {
                                    this.report_sponsored_message_ui(chat_id, menu_id, cx);
                                });
                            }));
                    }
                    menu.item(
                        PopupMenuItem::new("Hide ads (Premium)").on_click(move |_, _, cx| {
                            let _ = hide.update(cx, |this, cx| {
                                this.hide_sponsored_messages_ui(chat_id, menu_id, cx);
                            });
                        }),
                    )
                }),
        );
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
                    0,
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
                    None,
                    MediaCorners::small(),
                    0,
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
                    0,
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
            false,
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

    /// The sponsored message tdesktop shows after the last message of a
    /// channel, only while the history is scrolled to the bottom: the ad card
    /// (Ad label, "About this ad / Report ad / Hide ads" menu, sponsor
    /// button) plus the report picker and outcome banner. The ids it paints
    /// are recorded so `report_visible_sponsored` can send the TDLib view
    /// once they are actually on screen.
    pub(super) fn sponsored_footer(
        &self,
        scrolled_up: bool,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if scrolled_up {
            return None;
        }
        let session = self.session()?;
        let chat_id = session.open_chat?;
        let ad = session.open_sponsored_tail().cloned();
        let report = session
            .messages
            .sponsored_report
            .clone()
            .filter(|flight| flight.chat_id == chat_id);
        let outcome = session
            .messages
            .last_sponsored_report
            .clone()
            .filter(|outcome| outcome.chat_id == chat_id);
        if ad.is_none() && report.is_none() && outcome.is_none() {
            return None;
        }
        let look = self.bubble_look(cx);
        let files: HashMap<i32, ParsedFile> = session.media.files.clone();
        let downloading = session.media.downloading.clone();
        let failed = session.media.failed_downloads.clone();
        let media_roots = self.media_display_roots();
        let mut list = div()
            .id("sponsored-footer")
            .flex()
            .flex_col()
            .flex_none()
            .min_w_0()
            .px_3()
            .py_2()
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
        if let Some(ad) = ad {
            self.message_ui
                .rendered_sponsored
                .borrow_mut()
                .push(ad.message_id);
            list = list.child(sponsored_message_row(
                chat_id,
                &ad,
                &files,
                &downloading,
                &failed,
                &media_roots,
                &self.message_ui.spoiler_revealed,
                look,
                cx,
            ));
        }
        Some(list.into_any_element())
    }

    /// Frame-start hook (next to `report_visible_history`): the ad cards the
    /// last frame painted are on screen, so tell TDLib. The session counts
    /// each ad once, so this is cheap to call every frame.
    pub(super) fn report_visible_sponsored(&mut self, window_active: bool) {
        let shown = std::mem::take(&mut *self.message_ui.rendered_sponsored.borrow_mut());
        if !window_active || shown.is_empty() {
            return;
        }
        let Some(chat_id) = self.session().and_then(|s| s.open_chat) else {
            return;
        };
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.view_sponsored_messages(chat_id, &shown);
        }
    }

    /// "Hide ads" (tdesktop `HideSponsoredClickHandler`): Premium accounts
    /// turn sponsored messages off (`toggleHasSponsoredMessagesEnabled`);
    /// others get the "needs Telegram Premium" banner without any request.
    /// Demo: the same reducer calls, with `ok` injected for Premium.
    pub(super) fn hide_sponsored_messages_ui(
        &mut self,
        chat_id: ChatId,
        message_id: i64,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            self.connection.status_note =
                match live.driver.hide_sponsored_messages(chat_id, message_id) {
                    Ok(Some(_)) => "hiding ads…".into(),
                    Ok(None) => self.connection.status_note.clone(),
                    Err(_) => "could not hide ads".into(),
                };
        } else if let Some(session) = self.demo_session.as_mut()
            && session.begin_sponsored_hide(chat_id, message_id) == Some(true)
        {
            session.accept_sponsored_hidden(chat_id);
        }
        cx.notify();
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
            self.connection.status_note = match live
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
                let dyn_sink: Arc<dyn DiagnosticSink> = self.demo_ui.sink.clone();
                if let Some(owned) = copy_and_parse(&json, &self.demo_ui.seq, &dyn_sink) {
                    session.apply(owned);
                }
                self.connection.status_note = "demo — report options injected".into();
            } else {
                self.connection.status_note = "report not available for this row".into();
            }
        }
        cx.notify();
    }

    /// Send the chosen `reportOption` id for the in-flight sponsored report.
    /// Live: the driver sends the follow-up request. Demo: resolve with
    /// `reportSponsoredResultOk` through the same reducer.
    pub(super) fn pick_sponsored_report_option(&mut self, option_id: &str, cx: &mut Context<Self>) {
        let Some(flight) = self
            .session()
            .and_then(|s| s.messages.sponsored_report.clone())
        else {
            return;
        };
        if let Some(live) = self.live.as_mut() {
            self.connection.status_note = match live.driver.report_sponsored_message(
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
            let dyn_sink: Arc<dyn DiagnosticSink> = self.demo_ui.sink.clone();
            if let Some(owned) = copy_and_parse(&json, &self.demo_ui.seq, &dyn_sink) {
                session.apply(owned);
            }
            self.connection.status_note = "demo — report sent".into();
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
