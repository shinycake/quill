use crate::ui::app::QuillApp;
use crate::ui::status_note::StatusNote;
use crate::ui::user_error::user_error_for;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::ids::{ChatId, MessageId};
use quill::lang::{keys, tr};
use quill::state::{HistoryMessage, Session};

const CARD_MAX_WIDTH: f32 = 272.;
const BULLET: f32 = 7.;

pub(in crate::ui) fn sharing_request_row(
    message: &HistoryMessage,
    session: Option<&Session>,
    cx: &mut Context<QuillApp>,
) -> Option<AnyElement> {
    let session = session?;
    let view = session.sharing_request_view(message)?;
    if !view.card {
        return None;
    }
    let row_id = message.id.0 as u64;
    let service_bg = crate::ui::chat_palette::service_bg(cx.theme().secondary.opacity(0.85));
    let fg = cx.theme().secondary_foreground;
    let title = if message.is_outgoing {
        StyledText::new(tr(keys::ACTION_NO_FORWARDS_REQUEST_YOU))
    } else {
        let name = crate::ui::suggested_posts_ui::sender_short_name(session, message.sender);
        named_title(tr(keys::ACTION_NO_FORWARDS_REQUEST), &name)
    };
    let features = [
        keys::ACTION_NO_FORWARDS_FEATURE_FORWARDING,
        keys::ACTION_NO_FORWARDS_FEATURE_SAVING,
        keys::ACTION_NO_FORWARDS_FEATURE_COPYING,
    ];
    let mut card = div()
        .id(("sharing-request", row_id))
        .debug_selector(|| "sharing-request-card".to_string())
        .flex()
        .flex_col()
        .gap_1()
        .w_full()
        .px_4()
        .py_4()
        .rounded_xl()
        .bg(service_bg)
        .text_xs()
        .font_medium()
        .text_color(fg)
        .child(
            div()
                .pb_0p5()
                .text_center()
                .whitespace_normal()
                .child(title),
        );
    for feature in features {
        card = card.child(
            div()
                .flex()
                .items_start()
                .gap_2()
                .pl_4()
                .child(
                    div()
                        .flex_none()
                        .mt(px(5.))
                        .size(px(BULLET))
                        .rounded_full()
                        .bg(fg),
                )
                .child(div().min_w_0().child(tr(feature))),
        );
    }
    let buttons = view.buttons.then(|| {
        let (chat_id, message_id) = (message.chat_id, message.id);
        let button = |id: &'static str, label: &'static str, approve: bool| {
            Button::new((id, row_id))
                .debug_selector(move || id.to_string())
                .ghost()
                .bg(service_bg)
                .text_color(fg)
                .rounded_xl()
                .label(label)
                .flex_1()
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.answer_sharing_request(chat_id, message_id, approve, cx);
                }))
        };
        div()
            .flex()
            .gap_1()
            .w_full()
            .child(button(
                "sharing-request-reject",
                tr(keys::ACTION_NO_FORWARDS_REJECT),
                false,
            ))
            .child(button(
                "sharing-request-accept",
                tr(keys::ACTION_NO_FORWARDS_ACCEPT),
                true,
            ))
    });
    Some(
        div()
            .id(("service-row", row_id))
            .flex()
            .justify_center()
            .py_1()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .w_full()
                    .max_w(px(CARD_MAX_WIDTH))
                    .child(card)
                    .when_some(buttons, |this, buttons| this.child(buttons)),
            )
            .into_any_element(),
    )
}

fn named_title(template: &str, name: &str) -> StyledText {
    let Some((before, after)) = template.split_once("{from}") else {
        return StyledText::new(template.to_string());
    };
    let text = format!("{before}{name}{after}");
    let bold = HighlightStyle {
        font_weight: Some(FontWeight::BOLD),
        ..Default::default()
    };
    StyledText::new(text).with_highlights([(before.len()..before.len() + name.len(), bold)])
}

impl QuillApp {
    pub(in crate::ui) fn answer_sharing_request(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        approve: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(live) = self.live.as_mut() else {
            self.connection
                .set_note(StatusNote::info(tr(keys::Q_PEOPLE_DEMO_MODE_THIS_NEEDS_A)));
            cx.notify();
            return;
        };
        if let Err(err) = live
            .driver
            .answer_sharing_request(chat_id, message_id, approve)
        {
            self.connection.set_note(StatusNote::error(user_error_for(
                keys::Q_PEOPLE_COULDNT_REACH_TELEGRAM_TRY_AGAIN,
                &err,
            )));
        }
        cx.notify();
    }
}
