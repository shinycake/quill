//! Suggested posts in a channel's direct messages, composer side
//! (Telegram Desktop `HistoryWidget::refreshSuggestPostToggle`,
//! `SuggestOptionsBar`): the toggle in the input row, the suggest bar above
//! the field, sending with the bar's terms and "Edit Message" of a
//! suggestion. The boxes are in `suggested_posts_boxes.rs` and
//! `suggested_posts_decide.rs`.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::suggested_posts_state::{BalanceFor, SuggestTarget};
use crate::ui::status_note::StatusNote;
use crate::ui::wall_clock::WallClock;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::composer::SendOptions;
use quill::ids::{ChatId, MessageId};
use quill::lang::{keys, tr};
use quill::state::Session;
use quill::suggested_posts::{
    BAR_PRICED_TAIL, BAR_PROMPT, BarLine, DirectMessagesRole, SuggestBar, SuggestMode,
    SuggestTerms, SuggestedPostPurpose, SuggestedPrice, bar_both_tail, bar_dated, time_value,
};
use quill::telegram::envelope::MessageSender;

/// The first name of a sender, as tdesktop's `shortName()`; a chat's
/// title.
pub(super) fn sender_short_name(session: &Session, sender: Option<MessageSender>) -> String {
    match sender {
        Some(MessageSender::User { user_id }) => session
            .user(user_id)
            .map(|user| {
                if user.first_name.trim().is_empty() {
                    user.display_name()
                } else {
                    user.first_name.clone()
                }
            })
            .unwrap_or_default(),
        Some(MessageSender::Chat { chat_id }) => session
            .chats
            .get(&chat_id)
            .map(|chat| chat.title.clone())
            .unwrap_or_default(),
        None => String::new(),
    }
}

/// The channel a direct messages chat belongs to, by name: the linked
/// channel's title when its full info is known, else the chat's own.
pub(super) fn channel_name(session: &Session, chat_id: ChatId) -> String {
    session
        .chat_direct_messages_chat(chat_id)
        .and_then(|channel| session.chats.get(&channel.0))
        .or_else(|| session.chats.get(&chat_id.0))
        .map(|chat| chat.title.clone())
        .unwrap_or_default()
}

/// The amount as tdesktop draws it: the currency icon, then the number.
/// A post's time as the time row shows it, on the clock the pickers read.
pub(super) fn time_label(send_date: i32, cx: &App) -> String {
    let clock = WallClock::of(cx);
    time_value(send_date, clock.now(), |unix| clock.civil(unix))
}

pub(super) fn price_amount(price: SuggestedPrice, size: f32, cx: &App) -> AnyElement {
    let icon = match price {
        SuggestedPrice::Stars(_) => super::earn::star_icon(size),
        SuggestedPrice::TonCents(_) => super::earn::ton_icon(size, cx.theme().foreground),
    };
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap_0p5()
        .child(icon)
        .child(price.number_text())
        .into_any_element()
}

impl QuillApp {
    /// The session to change: the live driver's, or the demo's.
    pub(super) fn suggest_session_mut(&mut self) -> Option<&mut Session> {
        match self.live.as_mut() {
            Some(live) => Some(&mut live.driver.session),
            None => self.demo_session.as_deref_mut(),
        }
    }

    pub(super) fn suggest_role(&self, chat_id: ChatId) -> Option<DirectMessagesRole> {
        self.session()?.direct_messages_role(chat_id)
    }

    /// The composer's options with the suggest bar's terms: a send while
    /// the bar is up is a suggested post.
    pub(in crate::ui) fn composer_post_options(&self) -> SendOptions {
        let mut options = self.composer_send_options();
        options.suggested_post = self
            .open_chat_id()
            .and_then(|chat| self.session()?.suggest_bar(chat))
            .filter(|bar| bar.edit_of.is_none())
            .map(|bar| bar.terms);
        options
    }

    /// Puts the bar up, changes it or takes it down, and saves the draft
    /// with the new terms right away (tdesktop `saveDraftWithTextNow`).
    pub(super) fn set_suggest_bar(
        &mut self,
        chat_id: ChatId,
        bar: Option<SuggestBar>,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self.suggest_session_mut() else {
            return;
        };
        let bars = &mut session.payments.suggested_posts.bars;
        match bar {
            Some(bar) => {
                bars.insert(chat_id.0, bar);
            }
            None => {
                bars.remove(&chat_id.0);
            }
        }
        if self.open_chat_id() == Some(chat_id) && bar.is_none_or(|bar| bar.edit_of.is_none()) {
            self.note_open_draft(false, cx);
        }
        cx.notify();
    }

    /// A send went out: the bar's terms rode it, so the bar goes.
    pub(in crate::ui) fn clear_suggest_bar_after_send(&mut self, chat_id: ChatId) {
        if let Some(session) = self.suggest_session_mut() {
            session.payments.suggested_posts.bars.remove(&chat_id.0);
        }
    }

    /// The toggle in the input row: only for a member of a channel's
    /// direct messages, hidden while the bar is up or a message is being
    /// edited.
    pub(super) fn suggest_post_toggle(&self, cx: &mut Context<Self>) -> Option<Button> {
        let chat_id = self.open_chat_id()?;
        let session = self.session()?;
        if self.suggest_role(chat_id) != Some(DirectMessagesRole::Member)
            || session.suggest_bar(chat_id).is_some()
            || self.composer_ui.pending_edit.is_some()
        {
            return None;
        }
        Some(
            Button::new("composer-suggest-post")
                .icon(IconName::CircleDollarSign)
                .ghost()
                .tooltip(tr(keys::Q_MONEY_SUGGEST_A_POST))
                .accessibility_label(tr(keys::Q_MONEY_SUGGEST_A_POST))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.toggle_suggest_post(chat_id, cx);
                })),
        )
    }

    /// The toggle pressed: the bar goes up with empty terms, in the
    /// reply's place (tdesktop clears the reply; the box doesn't open).
    pub(super) fn toggle_suggest_post(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        self.composer_ui.set_reply(None);
        self.set_suggest_bar(chat_id, Some(SuggestBar::new(SuggestTerms::default())), cx);
    }

    /// The suggest bar above the field, in the reply bar's place.
    pub(super) fn suggest_bar_panel(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let chat_id = self.open_chat_id()?;
        self.session()?.suggest_bar(chat_id)?;
        self.suggest_bar_panel_body(cx)
    }

    /// `suggest_bar_panel` past its cheap check. A debug build reserves this
    /// builder's whole frame on entry, so frames that show nothing skip
    /// it (docs/decisions/main-thread-stack.md).
    /// The outer function repeats this body's first check; keep the two in step.
    #[inline(never)]
    fn suggest_bar_panel_body(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let chat_id = self.open_chat_id()?;
        let session = self.session()?;
        let bar = session.suggest_bar(chat_id)?;
        let admin = self.suggest_role(chat_id) == Some(DirectMessagesRole::Admin);
        let accent: Hsla = super::chat_theme::accent().into();
        let muted = cx.theme().muted_foreground;
        let date = |send_date: i32| time_label(send_date, cx);
        let line = match bar.terms.bar_line() {
            BarLine::Prompt => div().child(BAR_PROMPT),
            BarLine::Priced(price) => div()
                .flex()
                .items_center()
                .child(price_amount(price, 13., cx))
                .child(BAR_PRICED_TAIL),
            BarLine::Dated(send_date) => div().child(bar_dated(&date(send_date))),
            BarLine::PricedDated(price, send_date) => div()
                .flex()
                .items_center()
                .child(price_amount(price, 13., cx))
                .child(bar_both_tail(&date(send_date))),
        };
        let mode = bar.mode;
        let terms = bar.terms;
        Some(
            div()
                .id("composer-suggest-bar")
                .flex()
                .items_center()
                .gap_3()
                .px_2()
                .py_1()
                .child(
                    Icon::new(IconName::CircleDollarSign)
                        .size(px(18.))
                        .text_color(accent),
                )
                .child(
                    div()
                        .id("composer-suggest-bar-edit")
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_w_0()
                        .pl_2()
                        .border_l_2()
                        .border_color(accent)
                        .cursor_pointer()
                        .role(Role::Button)
                        .aria_label(tr(keys::Q_MONEY_EDIT_THE_OFFER))
                        .child(
                            div()
                                .text_sm()
                                .font_semibold()
                                .text_color(accent)
                                .truncate()
                                .child(bar.title()),
                        )
                        .child(div().text_sm().text_color(muted).truncate().child(line))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.open_suggest_price(
                                SuggestTarget::Bar(chat_id),
                                mode,
                                terms,
                                admin,
                                window,
                                cx,
                            );
                        })),
                )
                .child(
                    Button::new("composer-suggest-bar-close")
                        .icon(IconName::X)
                        .ghost()
                        .small()
                        .tooltip(tr(keys::CANCEL))
                        .accessibility_label(tr(keys::Q_MONEY_CANCEL_THE_SUGGESTION))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.cancel_suggest_bar(chat_id, window, cx);
                        })),
                )
                .into_any_element(),
        )
    }

    /// The bar's close: drops the suggestion; an "Edit Message" also
    /// drops the text it put in the field.
    fn cancel_suggest_bar(&mut self, chat_id: ChatId, window: &mut Window, cx: &mut Context<Self>) {
        let editing = self
            .session()
            .and_then(|session| session.suggest_bar(chat_id))
            .is_some_and(|bar| bar.edit_of.is_some());
        if editing {
            self.composer
                .update(cx, |input, cx| input.set_value("", window, cx));
        }
        self.set_suggest_bar(chat_id, None, cx);
    }

    /// "Edit Message" from the Suggest Changes menu: the text (or caption)
    /// goes into the field and the bar comes up as "Suggest Changes" with
    /// the current terms. Saving sends a new message.
    pub(super) fn begin_suggest_edit(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((text, terms)) = self.session().and_then(|session| {
            let message = session
                .histories
                .get(&chat_id.0)?
                .messages
                .get(&message_id.0)?;
            let terms = message.extras.suggested_post?.terms();
            let (text, _) = quill::suggested_posts::edit_source(&message.content)?;
            Some((text, terms))
        }) else {
            return;
        };
        self.composer_ui.set_reply(None);
        self.composer
            .update(cx, |input, cx| input.set_value(text, window, cx));
        self.set_suggest_bar(
            chat_id,
            Some(SuggestBar {
                terms,
                mode: SuggestMode::Change,
                edit_of: Some(message_id),
            }),
            cx,
        );
    }

    /// The send of an "Edit Message": a new message replying to the
    /// suggestion, never an edit. Returns whether it took the send.
    pub(in crate::ui) fn submit_suggested_change(
        &mut self,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(chat_id) = self.open_chat_id() else {
            return false;
        };
        let Some((message_id, terms, is_media)) = self.session().and_then(|session| {
            let bar = session.suggest_bar(chat_id)?;
            let message_id = bar.edit_of?;
            let is_media = session
                .histories
                .get(&chat_id.0)
                .and_then(|history| history.messages.get(&message_id.0))
                .and_then(|message| quill::suggested_posts::edit_source(&message.content))
                .is_some_and(|(_, media)| media);
            Some((message_id, bar.terms, is_media))
        }) else {
            return false;
        };
        if text.trim().is_empty() && !is_media {
            self.connection.set_note(StatusNote::silent(tr(
                keys::Q_MONEY_TYPE_A_MESSAGE_OR_ATTACH,
            )));
            cx.notify();
            return true;
        }
        let sent = match self.live.as_mut() {
            Some(live) => live
                .driver
                .send_suggested_change(chat_id, message_id, text, is_media, terms)
                .is_ok(),
            None => true,
        };
        if sent {
            self.clear_suggest_bar_after_send(chat_id);
            self.composer
                .update(cx, |input, cx| input.set_value("", window, cx));
            self.connection
                .set_note(StatusNote::silent(tr(keys::Q_MONEY_SENDING_2)));
        } else {
            self.connection
                .set_note(StatusNote::error(tr(keys::Q_MONEY_COULDNT_SEND_THE_OFFER)));
        }
        cx.notify();
        true
    }

    /// A suggested post's history row with the terms block above it.
    pub(in crate::ui) fn with_suggested_post_block(
        &self,
        message: &quill::state::HistoryMessage,
        row: impl IntoElement,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let block = self.session().and_then(|session| {
            super::history::suggested_post_block::suggested_post_block(session, message, cx)
        });
        match block {
            Some(block) => div()
                .flex()
                .flex_col()
                .child(block)
                .child(row)
                .into_any_element(),
            None => row.into_any_element(),
        }
    }

    /// Errors of approve, decline and offers become status toasts.
    pub(super) fn drain_suggested_post_notes(&mut self, cx: &mut Context<Self>) {
        let Some(live) = self.live.as_mut() else {
            return;
        };
        let notes = live.driver.session.take_suggested_post_notes();
        let balance_low = live.driver.session.take_suggested_post_balance_low();
        if let Some(note) = notes.into_iter().last() {
            self.connection.set_note(StatusNote::info(note));
        }
        if let Some(purpose) = balance_low {
            self.ask_after_balance_error(purpose, cx);
        }
    }

    /// TDLib refused a payment with `BALANCE_TOO_LOW` (the balance Quill
    /// knew was stale or missing): ask for a top-up as the local check
    /// would have.
    fn ask_after_balance_error(&mut self, purpose: SuggestedPostPurpose, cx: &mut Context<Self>) {
        let (price, purpose_for) = match purpose {
            SuggestedPostPurpose::Approve {
                chat_id,
                message_id,
            } => (
                self.session().and_then(|session| {
                    session
                        .histories
                        .get(&chat_id)?
                        .messages
                        .get(&message_id)?
                        .extras
                        .suggested_post?
                        .price
                }),
                BalanceFor::Accept,
            ),
            SuggestedPostPurpose::AddOffer { price, .. }
            | SuggestedPostPurpose::SendChange { price, .. } => (price, BalanceFor::Offer),
            SuggestedPostPurpose::Decline { .. } => (None, BalanceFor::Offer),
        };
        let Some(price) = price else {
            self.connection.set_note(StatusNote::error(tr(
                keys::Q_MONEY_COULDNT_PAY_FOR_THE_POST,
            )));
            return;
        };
        let short = self.suggest_shortfall(price).unwrap_or(price);
        self.ask_for_balance(short, purpose_for, cx);
    }
}
