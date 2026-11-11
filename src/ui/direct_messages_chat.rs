//! A channel's direct messages as their writer sees them (tdesktop
//! `AboutView::makeStarsPerMessage`, the monoforum `TopBarWidget`): the
//! header's way back to the channel and its "Direct messages" line, and
//! the card an empty chat shows with the price and "Buy Stars". The same
//! card stands in an empty private chat with a user who charges.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::lang::{keys, tr};
use quill::locked_intro::{IntroPiece, LockedIntro};
use quill::premium_hub::group_digits;
use quill::state::{ChatSummary, Session};
use quill::suggested_posts::DirectMessagesRole;
use quill::telegram::envelope::ChatKind;

/// The channel behind `chat_id` when it is that channel's direct messages.
fn writer_channel(session: &Session, chat_id: ChatId) -> Option<ChatId> {
    session.direct_messages_role(chat_id)?;
    session.chat_direct_messages_chat(chat_id)
}

impl QuillApp {
    /// The channel to go back to from its open direct messages.
    pub(super) fn direct_messages_channel(&self) -> Option<ChatId> {
        let session = self.session()?;
        writer_channel(session, session.open_chat_id()?)
    }

    /// The header's back arrow in a channel's direct messages.
    pub(super) fn direct_messages_back_button(&self, cx: &mut Context<Self>) -> Option<Button> {
        let channel = self.direct_messages_channel()?;
        Some(
            Button::new("direct-messages-back")
                .icon(IconName::ChevronLeft)
                .ghost()
                .tooltip(tr(keys::Q_CHAT_BACK))
                .accessibility_label(tr(keys::Q_CHAT_BACK))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.select_listed_chat(channel, window, cx);
                })),
        )
    }

    /// The card of an empty chat that costs Stars or is a channel's direct
    /// messages; `None` for every other chat.
    pub(super) fn locked_intro(
        &self,
        chat: Option<&ChatSummary>,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let chat = chat?;
        let session = self.session()?;
        // The channel's admins answer from here; the card is for writers.
        let role = session.direct_messages_role(chat.id);
        if role == Some(DirectMessagesRole::Admin) {
            return None;
        }
        let direct = role.is_some();
        let private = matches!(chat.kind, ChatKind::Private { .. });
        if !direct && !private {
            return None;
        }
        let intro = LockedIntro::for_chat(direct, session.paid_send_price(chat.id))?;
        let name = if direct {
            super::suggested_posts_ui::channel_name(session, chat.id)
        } else {
            chat.title.clone()
        };
        let sentence = intro_sentence(intro.pieces(&name));
        let card = div()
            .id("locked-intro")
            .flex()
            .flex_col()
            .items_center()
            .gap_3()
            .w(px(260.))
            .px_4()
            .py_4()
            .rounded_xl()
            .bg(super::chat_palette::service_bg(cx.theme().secondary))
            .text_color(cx.theme().secondary_foreground)
            .child(
                div()
                    .size(px(64.))
                    .rounded_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(cx.theme().secondary_foreground.opacity(0.12))
                    .child(Icon::new(IconName::MessagesSquare).size(px(32.))),
            )
            .child(sentence)
            .when(intro.buys_stars(), |card| {
                card.child(
                    Button::new("locked-intro-buy-stars")
                        .label(tr(keys::SEND_CHARGES_STARS_GO))
                        .rounded(ButtonRounded::Large)
                        .on_click(cx.listener(|this, _, _, cx| this.open_stars(cx))),
                )
            });
        Some(
            div()
                .id("locked-intro-pane")
                .flex()
                .flex_1()
                .items_center()
                .justify_center()
                .child(card)
                .into_any_element(),
        )
    }
}

/// One run inside a word of the card's sentence.
enum Run {
    Text { text: String, bold: bool },
    Stars(i64),
}

/// The sentence as words, so it wraps between them and centres each line;
/// a star and its number stay with the text they touch.
fn intro_words(pieces: Vec<IntroPiece>) -> Vec<Vec<Run>> {
    let mut words: Vec<Vec<Run>> = vec![Vec::new()];
    let push_text = |words: &mut Vec<Vec<Run>>, text: &str, bold: bool| {
        for (ix, part) in text.split(' ').enumerate() {
            if ix > 0 {
                words.push(Vec::new());
            }
            if !part.is_empty()
                && let Some(word) = words.last_mut()
            {
                word.push(Run::Text {
                    text: part.to_string(),
                    bold,
                });
            }
        }
    };
    for piece in pieces {
        match piece {
            IntroPiece::Text(text) => push_text(&mut words, &text, false),
            IntroPiece::Name(name) => push_text(&mut words, &name, true),
            IntroPiece::Stars(stars) => {
                if let Some(word) = words.last_mut() {
                    word.push(Run::Stars(stars));
                }
            }
        }
    }
    words.retain(|word| !word.is_empty());
    words
}

fn intro_sentence(pieces: Vec<IntroPiece>) -> Div {
    div()
        .flex()
        .flex_wrap()
        .justify_center()
        .gap_x(px(4.))
        .text_sm()
        .children(intro_words(pieces).into_iter().map(|word| {
            div()
                .flex()
                .items_center()
                .children(word.into_iter().map(|run| {
                    match run {
                        Run::Text { text, bold } => {
                            div().when(bold, |run| run.font_semibold()).child(text)
                        }
                        Run::Stars(stars) => div()
                            .flex()
                            .items_center()
                            .gap_0p5()
                            .font_semibold()
                            .child(super::earn::star_icon(14.))
                            .child(group_digits(stars)),
                    }
                }))
        }))
}

/// The header's status line in a channel's direct messages.
pub(super) fn direct_messages_line() -> String {
    tr(keys::CHAT_STATUS_DIRECT).to_string()
}
