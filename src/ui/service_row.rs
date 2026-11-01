//! Service-message rows: the centered pill with Telegram Desktop's wording
//! (`quill::service_text`), linked names / pinned excerpts, and the new
//! photo under a chat-photo change.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::chat_theme::accent;
use super::message_media::photo_display_path;
use super::*;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::ids::MessageId;
use quill::lang::{keys, tr};
use quill::service_text::{ServiceLink, ServiceText};
use quill::state::{HistoryMessage, Session};
use quill::telegram::envelope::{MessageContent, PhotoContent, ServiceAction};
use std::path::PathBuf;

/// Side of the round photo shown under a photo-change row.
const PHOTO_SIDE: f32 = 96.0;
/// Widest a service pill grows before its text wraps.
/// Telegram Desktop `Service::performCountCurrentSize` in wide mode:
/// the bubble column (`msgMaxWidth` plus the avatar skips and margins)
/// minus the service margins.
const PILL_MAX_WIDTH: f32 = 522.0;

/// The photo a service action carries, if any.
fn action_photo(content: &MessageContent) -> Option<&PhotoContent> {
    match content {
        MessageContent::Action(action) => match action.as_ref() {
            ServiceAction::ChatPhoto { photo, .. }
            | ServiceAction::SuggestProfilePhoto { photo, .. } => photo.as_ref(),
            _ => None,
        },
        _ => None,
    }
}

/// Whether the content is drawn as a service row.
pub(super) fn is_service_row(content: &MessageContent) -> bool {
    !super::message_media::is_bubble_card(content)
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
                    ServiceLink::Url(url) => this.open_link(&url, cx),
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
    files: &quill::state::FileMap,
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
        .aria_label(tr(keys::Q_MISC_OPEN_PHOTO))
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
                img(super::mini_image::mini_image(mini))
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
                    .label(tr(keys::Q_MISC_VIEW))
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
    files: &quill::state::FileMap,
    media_roots: &[PathBuf],
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    if let Some(row) = super::history::sharing_request::sharing_request_row(message, session, cx) {
        return row;
    }
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
    // A story mention: the round story picture and a View Story button, or
    // the "no longer available" wording once the story is gone.
    let mention = quill::state::message_story_key(&message.content)
        .filter(|(_, _, via_mention)| *via_mention);
    let text = match (mention, session) {
        (Some((poster, story, _)), Some(s))
            if s.story_card_state(poster, story) == quill::state::StoryCardState::Expired =>
        {
            super::message_media::expired_mention_text(&text, message.is_outgoing)
        }
        _ => text,
    };
    let mention_extras = mention.and_then(|(poster, story, _)| {
        super::message_media::story_mention_extras(
            row_id,
            quill::ids::ChatId(poster),
            story,
            session,
            files,
            media_roots,
            cx,
        )
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
                    .label(tr(keys::Q_MISC_VIEW_PHOTO))
                    .small()
                    .ghost()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.open_media_viewer(chat_id, message_id, cx);
                    })),
            )
            .when(!message.is_outgoing && photo_id != 0, |this| {
                this.child(
                    Button::new(format!("suggested-set-{row_id}"))
                        .label(tr(keys::Q_MISC_SET_AS_MY_PHOTO))
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
    // "You joined this channel" hosts the similar-channels strip; the
    // notice itself collapses and expands it (tdesktop toggles
    // `SimilarExpanded` from the service text).
    let joined_channel = matches!(
        &message.content,
        MessageContent::Action(action) if **action == ServiceAction::YouJoinedChannel
    );
    let chat_id = message.chat_id;
    let similar_strip = joined_channel
        .then(|| super::similar_strip::similar_channels_strip(session, chat_id, media_roots, cx))
        .flatten();
    let card = super::premium_ui::gift_card_of(&message.content).map(|(inner, card)| {
        let roots = media_roots;
        super::premium_ui::gift_card_element(row_id, inner, card, session, files, roots, cx)
    });
    let learn_more = super::premium_ui::gift_card_of(&message.content).and_then(|(_, card)| {
        super::giveaway_info_ui::giveaway_learn_more(row_id, message.chat_id, message.id, card, cx)
    });
    let ton_transfer = super::ton_wallet_ui::transfer_of(&message.content).map(|transfer| {
        super::ton_wallet_ui::transfer_card(row_id, message, transfer, session, cx)
    });
    let connect_review =
        super::ton_wallet_ui::connect_review_button(row_id, &message.content, session, cx);
    let open_code = super::premium_ui::gift_card_of(&message.content)
        .and_then(|(_, card)| super::link_info_boxes::gift_code_open_button(row_id, card, cx));
    div()
        .id(("service-row", row_id))
        .flex()
        .flex_col()
        .items_center()
        .gap_1p5()
        .py_1()
        .child(if joined_channel {
            div()
                .id(("joined-notice", row_id))
                .cursor_pointer()
                .role(Role::Button)
                .aria_label(tr(keys::Q_MISC_SHOW_OR_HIDE_SIMILAR_CHANNELS))
                .tab_index(0)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.toggle_similar_strip(chat_id, cx);
                }))
                .child(pill(("service-pill", row_id), &text, cx))
                .into_any_element()
        } else {
            pill(("service-pill", row_id), &text, cx)
        })
        .when_some(similar_strip, |this, strip| this.child(strip))
        .when_some(photo, |this, photo| this.child(photo))
        .when_some(mention_extras, |this, extras| this.child(extras))
        .when_some(birthday, |this, birthday| this.child(birthday))
        .when_some(card, |this, card| this.child(card))
        .when_some(ton_transfer, |this, card| this.child(card))
        .when_some(connect_review, |this, button| this.child(button))
        .when_some(learn_more, |this, button| this.child(button))
        .when_some(open_code, |this, button| this.child(button))
        .when_some(buttons, |this, buttons| this.child(buttons))
        .into_any_element()
}
