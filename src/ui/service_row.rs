//! Service-message rows: the centered pill with Telegram Desktop's wording
//! (`quill::service_text`), linked names / pinned excerpts, and the new
//! photo under a chat-photo change.

use super::app::QuillApp;
use super::chat_theme::accent;
use super::message_media::photo_display_path;
use super::*;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::ids::MessageId;
use quill::service_text::{ServiceLink, ServiceText};
use quill::state::{HistoryMessage, Session};
use quill::telegram::envelope::{MessageContent, ParsedFile, PhotoContent, ServiceAction};
use std::collections::HashMap;
use std::path::PathBuf;

/// Side of the round photo shown under a photo-change row.
const PHOTO_SIDE: f32 = 96.0;
/// Widest a service pill grows before its text wraps.
const PILL_MAX_WIDTH: f32 = 440.0;

/// The photo a service action carries, if any.
fn action_photo(content: &MessageContent) -> Option<&PhotoContent> {
    match content {
        MessageContent::Action(action) => match action.as_ref() {
            ServiceAction::ChatPhoto { photo }
            | ServiceAction::SuggestProfilePhoto { photo, .. } => photo.as_ref(),
            _ => None,
        },
        _ => None,
    }
}

/// Locked paid media draws as a bubble card, not a service row.
pub(super) fn locked_paid_media(
    content: &MessageContent,
) -> Option<(i64, &[quill::telegram::envelope::PaidMediaPreview], &str)> {
    match content {
        MessageContent::Action(action) => match action.as_ref() {
            ServiceAction::PaidMedia {
                stars,
                locked,
                caption,
            } if !locked.is_empty() => Some((*stars, locked.as_slice(), caption.as_str())),
            _ => None,
        },
        _ => None,
    }
}

/// Whether the content is drawn as a service row.
pub(super) fn is_service_row(content: &MessageContent) -> bool {
    locked_paid_media(content).is_none()
        && matches!(
            content,
            MessageContent::Action(_)
                | MessageContent::ChatTtlChanged { .. }
                | MessageContent::ScreenshotTaken
        )
}

/// The pill: wrapped, centered text whose linked parts (names, the pinned
/// excerpt, a game title) are accent-colored and clickable.
fn pill(id: (&'static str, u64), text: &ServiceText, cx: &mut Context<QuillApp>) -> AnyElement {
    let (plain, spans) = text.spans();
    let ranges: Vec<std::ops::Range<usize>> =
        spans.iter().map(|(range, _)| range.clone()).collect();
    let links: Vec<ServiceLink> = spans.into_iter().map(|(_, link)| link).collect();
    let link_style = HighlightStyle {
        color: Some(accent().into()),
        font_weight: Some(FontWeight::SEMIBOLD),
        ..Default::default()
    };
    let highlights: Vec<_> = ranges
        .iter()
        .map(|range| (range.clone(), link_style))
        .collect();
    let owner = cx.entity().downgrade();
    let body = if ranges.is_empty() {
        div().child(plain).into_any_element()
    } else {
        InteractiveText::new(id, StyledText::new(plain).with_highlights(highlights))
            .on_click(ranges, move |ix, window, cx| {
                let Some(link) = links.get(ix).cloned() else {
                    return;
                };
                let _ = owner.update(cx, |this, cx| match link {
                    ServiceLink::Sender(sender) => this.open_avatar_profile(sender, window, cx),
                    ServiceLink::Message(message_id) => {
                        this.jump_to_replied_message(MessageId(message_id), cx)
                    }
                    ServiceLink::Url(url) => this.open_message_url(&url, cx),
                });
            })
            .into_any_element()
    };
    div()
        .max_w(px(PILL_MAX_WIDTH))
        .px_3()
        .py_0p5()
        .rounded_xl()
        .bg(cx.theme().secondary.opacity(0.85))
        .text_xs()
        .font_medium()
        .text_center()
        .text_color(cx.theme().secondary_foreground)
        .child(body)
        .into_any_element()
}

/// The new photo of a photo-change row: a round tile that opens the viewer.
fn photo_tile(
    row_id: u64,
    chat_id: quill::ids::ChatId,
    message_id: MessageId,
    photo: &PhotoContent,
    files: &HashMap<i32, ParsedFile>,
    media_roots: &[PathBuf],
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let side = px(PHOTO_SIDE);
    let tile = div()
        .id(("service-photo", row_id))
        .size(side)
        .flex_none()
        .rounded_full()
        .overflow_hidden()
        .bg(fill_muted())
        .role(gpui_kit::Role::Button)
        .aria_label("Open photo")
        .tab_index(0)
        .cursor_pointer()
        .on_click(cx.listener(move |this, _, _, cx| {
            this.open_media_viewer(chat_id, message_id, cx);
        }));
    if let Some(path) = photo_display_path(photo, files, media_roots) {
        let dims = photo
            .largest_size()
            .or_else(|| photo.thumb_size())
            .map(|size| (size.width, size.height));
        return tile
            .child(
                img(super::image_budget::sized_media(
                    &path,
                    (side, side),
                    dims,
                    super::image_budget::Fit::Cover,
                ))
                .id(("service-photo-img", row_id))
                .size(side)
                .object_fit(ObjectFit::Cover),
            )
            .into_any_element();
    }
    match photo
        .minithumbnail
        .as_ref()
        .filter(|mini| !mini.data.is_empty())
    {
        Some(mini) => tile
            .child(
                img(ImageSource::Image(std::sync::Arc::new(
                    gpui_kit::Image::from_bytes(gpui_kit::ImageFormat::Jpeg, mini.data.clone()),
                )))
                .size(side)
                .object_fit(ObjectFit::Cover),
            )
            .into_any_element(),
        None => tile.into_any_element(),
    }
}

/// A suggested birthday: the Day / Month / Year table under the pill and,
/// on an incoming suggestion, a "View" button that opens the birthday form
/// filled with the suggested date (tdesktop `GenerateSuggetsBirthdayMedia`).
fn birthday_card(
    row_id: u64,
    day: i32,
    month: i32,
    year: i32,
    outgoing: bool,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let mut table = div().flex().gap_5().justify_center();
    for (label, value) in quill::service_text::birthday_table(day, month, year) {
        table = table.child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .child(div().text_xs().text_color(text_muted()).child(label))
                .child(div().text_sm().font_semibold().child(value)),
        );
    }
    let parts = quill::service_text::birthday_form_parts(day, month, year);
    div()
        .id(("service-birthday", row_id))
        .flex()
        .flex_col()
        .items_center()
        .gap_2()
        .px_4()
        .py_2()
        .rounded_xl()
        .bg(cx.theme().secondary.opacity(0.85))
        .text_color(cx.theme().secondary_foreground)
        .child(table)
        .when(!outgoing && parts.is_some(), |this| {
            this.child(
                Button::new(format!("suggested-birthday-view-{row_id}"))
                    .label("View")
                    .small()
                    .primary()
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if let Some(parts) = parts {
                            this.open_suggested_birthday(parts, window, cx);
                        }
                    })),
            )
        })
        .into_any_element()
}

/// A service message as a centered row: the pill, plus the photo for
/// photo-change actions.
pub(super) fn service_message_row(
    message: &HistoryMessage,
    session: Option<&Session>,
    files: &HashMap<i32, ParsedFile>,
    media_roots: &[PathBuf],
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let row_id = message.id.0 as u64;
    let text = session
        .and_then(|session| session.service_text(message))
        .unwrap_or_else(|| {
            let plain = message.content.preview();
            ServiceText {
                segs: vec![quill::service_text::Seg {
                    text: plain,
                    link: None,
                }],
            }
        });
    let photo = action_photo(&message.content).map(|photo| {
        photo_tile(
            row_id,
            message.chat_id,
            message.id,
            photo,
            files,
            media_roots,
            cx,
        )
    });
    // An incoming suggestion offers View / Set as My Photo (tdesktop's
    // suggested-photo service box buttons).
    let suggestion = match &message.content {
        MessageContent::Action(action) => match action.as_ref() {
            ServiceAction::SuggestProfilePhoto {
                photo: Some(_),
                photo_id,
            } => Some(*photo_id),
            _ => None,
        },
        _ => None,
    };
    let buttons = suggestion.map(|photo_id| {
        let (chat_id, message_id) = (message.chat_id, message.id);
        div()
            .flex()
            .gap_2()
            .child(
                Button::new(format!("suggested-view-{row_id}"))
                    .label("View Photo")
                    .small()
                    .ghost()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.open_media_viewer(chat_id, message_id, cx);
                    })),
            )
            .when(!message.is_outgoing && photo_id != 0, |this| {
                this.child(
                    Button::new(format!("suggested-set-{row_id}"))
                        .label("Set as My Photo")
                        .small()
                        .primary()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.accept_suggested_photo(photo_id, cx);
                        })),
                )
            })
    });
    let birthday = match &message.content {
        MessageContent::Action(action) => match action.as_ref() {
            ServiceAction::SuggestBirthdate { day, month, year } => Some(birthday_card(
                row_id,
                *day,
                *month,
                *year,
                message.is_outgoing,
                cx,
            )),
            _ => None,
        },
        _ => None,
    };
    let card = super::premium_ui::gift_card_of(&message.content).map(|(inner, card)| {
        let roots = media_roots;
        super::premium_ui::gift_card_element(row_id, inner, card, session, files, roots, cx)
    });
    div()
        .id(("service-row", row_id))
        .flex()
        .flex_col()
        .items_center()
        .gap_1p5()
        .py_1()
        .child(pill(("service-pill", row_id), &text, cx))
        .when_some(photo, |this, photo| this.child(photo))
        .when_some(birthday, |this, birthday| this.child(birthday))
        .when_some(card, |this, card| this.child(card))
        .when_some(buttons, |this, buttons| this.child(buttons))
        .into_any_element()
}
