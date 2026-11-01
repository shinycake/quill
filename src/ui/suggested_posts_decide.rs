//! Deciding on a suggested post (Telegram Desktop
//! `history_view_suggest_decision.cpp`, `api_suggest_post.cpp`): Accept
//! with the Accept Terms box, Decline with a reason, Suggest Changes, Add
//! Offer, and the Grams Needed box.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::shell::{DialogKind, QuillShell};
use super::suggested_posts_state::{AcceptBox, BalanceFor, DeclineBox, SuggestTarget, TimeTarget};
use super::suggested_posts_ui::{channel_name, sender_short_name};
use crate::ui::status_note::StatusNote;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::*;
use quill::ids::{ChatId, MessageId};
use quill::lang::{keys, tr};
use quill::local_time::now_unix;
use quill::suggested_posts::{
    DECLINE_REASON_MAX, DirectMessagesRole, LOW_TON_BUTTON, LOW_TON_TEXT, LOW_TON_URL, SuggestMode,
    SuggestTerms, SuggestedPostInfo, SuggestedPostState, accept_text, decline_text, low_ton_title,
    time_value,
};
use std::cell::RefCell;
use std::rc::Rc;

/// What the boxes need to know about one suggestion.
struct Suggestion {
    info: SuggestedPostInfo,
    admin: bool,
    from: String,
    channel: String,
}

impl QuillApp {
    fn suggestion(&self, chat_id: ChatId, message_id: MessageId) -> Option<Suggestion> {
        let session = self.session()?;
        let message = session
            .histories
            .get(&chat_id.0)?
            .messages
            .get(&message_id.0)?;
        Some(Suggestion {
            info: message.extras.suggested_post?,
            admin: session.direct_messages_role(chat_id) == Some(DirectMessagesRole::Admin),
            from: sender_short_name(session, message.sender),
            channel: channel_name(session, chat_id),
        })
    }

    /// "Accept": a post without a time asks for one first.
    pub(super) fn begin_suggest_accept(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(suggestion) = self.suggestion(chat_id, message_id) else {
            return;
        };
        if suggestion.info.send_date <= 0 {
            self.open_suggest_time(
                TimeTarget::Accept {
                    chat_id,
                    message_id,
                },
                SuggestMode::Publish,
                0,
                window,
                cx,
            );
        } else {
            self.open_suggest_accept(chat_id, message_id, suggestion.info.send_date, window, cx);
        }
    }

    /// The Accept Terms box for publishing at `send_date` (0: now).
    pub(super) fn open_suggest_accept(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        send_date: i32,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(suggestion) = self.suggestion(chat_id, message_id) else {
            return;
        };
        let limits = self
            .session()
            .map(|session| session.payments.suggested_posts.limits)
            .unwrap_or_default();
        let date = (send_date > 0).then(|| time_value(send_date, now_unix()));
        let text = accept_text(
            suggestion.admin,
            &suggestion.from,
            &suggestion.channel,
            suggestion.info.price,
            date.as_deref(),
            &limits,
        );
        self.suggested_posts.accept = Some(AcceptBox {
            chat_id,
            message_id,
            send_date,
            text,
            pays: suggestion.info.price.filter(|_| !suggestion.admin),
        });
        cx.notify();
    }

    fn close_suggest_accept(&mut self, cx: &mut Context<Self>) {
        self.suggested_posts.accept = None;
        cx.notify();
    }

    /// "Publish": a member is checked against the balance first.
    pub(super) fn confirm_suggest_accept(&mut self, cx: &mut Context<Self>) {
        let Some(accept) = self.suggested_posts.accept.clone() else {
            return;
        };
        // A second Publish while the first is on its way does nothing.
        let busy = self.session().is_some_and(|session| {
            session
                .payments
                .suggested_posts
                .busy
                .contains(&(accept.chat_id.0, accept.message_id.0))
        });
        if busy {
            self.suggested_posts.accept = None;
            cx.notify();
            return;
        }
        if let Some(short) = accept.pays.and_then(|price| self.suggest_shortfall(price)) {
            self.suggested_posts.accept = None;
            self.ask_for_balance(short, BalanceFor::Accept, cx);
            return;
        }
        self.suggested_posts.accept = None;
        match self.live.as_mut() {
            Some(live) => {
                if live
                    .driver
                    .approve_suggested_post(accept.chat_id, accept.message_id, accept.send_date)
                    .is_err()
                {
                    self.connection.set_note(StatusNote::error(tr(
                        keys::Q_MONEY_COULDNT_PUBLISH_THE_POST,
                    )));
                }
            }
            None => self.set_demo_suggestion_state(
                accept.chat_id,
                accept.message_id,
                SuggestedPostState::Approved,
            ),
        }
        cx.notify();
    }

    /// The demo has no TDLib to answer: the decision lands at once.
    fn set_demo_suggestion_state(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        state: SuggestedPostState,
    ) {
        if let Some(session) = self.demo_session.as_mut() {
            session.edit_suggested_post(chat_id, message_id, |info| {
                if let Some(info) = info.as_mut() {
                    info.state = state;
                    info.can_be_approved = false;
                    info.can_be_declined = false;
                }
            });
        }
    }

    /// "Decline": the box with the optional reason.
    pub(super) fn open_suggest_decline(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(suggestion) = self.suggestion(chat_id, message_id) else {
            return;
        };
        let reason = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(tr(keys::SUGGEST_DECLINE_REASON))
                .submit_on_enter(false)
        });
        cx.subscribe_in(
            &reason,
            window,
            |this, _, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    this.confirm_suggest_decline(cx);
                    this.close_kit_dialog_if_done(DialogKind::SuggestDecline, window, cx);
                }
            },
        )
        .detach();
        // The reason field takes the keyboard (tdesktop focuses it).
        reason.update(cx, |input, cx| input.focus(window, cx));
        self.suggested_posts.decline = Some(DeclineBox {
            chat_id,
            message_id,
            text: decline_text(suggestion.admin, &suggestion.from, &suggestion.channel),
            reason,
        });
        cx.notify();
    }

    fn close_suggest_decline(&mut self, cx: &mut Context<Self>) {
        self.suggested_posts.decline = None;
        cx.notify();
    }

    pub(super) fn confirm_suggest_decline(&mut self, cx: &mut Context<Self>) {
        let Some(decline) = self.suggested_posts.decline.take() else {
            return;
        };
        let reason: String = decline
            .reason
            .read(cx)
            .value()
            .chars()
            .take(DECLINE_REASON_MAX)
            .collect();
        match self.live.as_mut() {
            Some(live) => {
                if live
                    .driver
                    .decline_suggested_post(decline.chat_id, decline.message_id, &reason)
                    .is_err()
                {
                    self.connection.set_note(StatusNote::error(tr(
                        keys::Q_MONEY_COULDNT_DECLINE_THE_POST,
                    )));
                }
            }
            None => self.set_demo_suggestion_state(
                decline.chat_id,
                decline.message_id,
                SuggestedPostState::Declined,
            ),
        }
        cx.notify();
    }

    /// "Edit Price" of the Suggest Changes menu.
    pub(super) fn suggest_edit_price(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(suggestion) = self.suggestion(chat_id, message_id) else {
            return;
        };
        self.open_suggest_price(
            SuggestTarget::Offer {
                chat_id,
                message_id,
            },
            SuggestMode::Change,
            suggestion.info.terms(),
            suggestion.admin,
            window,
            cx,
        );
    }

    /// "Edit Time" of the Suggest Changes menu.
    pub(super) fn suggest_edit_time(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(suggestion) = self.suggestion(chat_id, message_id) else {
            return;
        };
        self.open_suggest_time(
            TimeTarget::Offer {
                chat_id,
                message_id,
                price: suggestion.info.price,
            },
            SuggestMode::Change,
            suggestion.info.send_date,
            window,
            cx,
        );
    }

    /// Whether the message menu offers "Add Offer": for a member, a sent
    /// regular message without a suggestion, outside an album, that isn't
    /// a service message (tdesktop `CanAddOfferToMessage`; the forward
    /// check is TDLib's on `addOffer`). The menu uses this only until
    /// `messageProperties.can_add_offer` arrives.
    pub(super) fn can_add_offer(&self, chat_id: ChatId, message_id: MessageId) -> bool {
        let Some(session) = self.session() else {
            return false;
        };
        if session.direct_messages_role(chat_id) != Some(DirectMessagesRole::Member) {
            return false;
        }
        session
            .histories
            .get(&chat_id.0)
            .and_then(|history| history.messages.get(&message_id.0))
            .is_some_and(|message| {
                use quill::telegram::envelope::MessageContent as C;
                !message.pending
                    && message.id.0 > 0
                    && message.media_album_id == 0
                    && message.extras.suggested_post.is_none()
                    && !matches!(
                        message.content,
                        C::Action(_)
                            | C::Unsupported { .. }
                            | C::ChatTtlChanged { .. }
                            | C::ScreenshotTaken
                            | C::Call { .. }
                            | C::GroupCallInvitation { .. }
                    )
            })
    }

    /// The delete banner's warning over a paid suggested post younger
    /// than `suggested_post_lifetime_min` ("Stars will be lost").
    pub(super) fn paid_post_delete_warning(&self) -> Option<(&'static str, &'static str)> {
        let pending = self.message_ui.pending_delete.as_ref()?;
        let session = self.session()?;
        let message = session
            .histories
            .get(&pending.chat_id.0)?
            .messages
            .get(&pending.message_id.0)?;
        let paid = message.extras.paid_suggested_post?;
        let limits = session.payments.suggested_posts.limits;
        limits
            .delete_loses_payment(now_unix(), message.date)
            .then(|| {
                quill::suggested_posts::delete_warning(
                    paid == quill::telegram::envelope::PaidSuggestedPost::Grams,
                )
            })
    }

    /// "Add Offer" from the message menu: the price box for a new offer.
    pub(super) fn begin_add_offer(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_suggest_price(
            SuggestTarget::Offer {
                chat_id,
                message_id,
            },
            SuggestMode::New,
            SuggestTerms::default(),
            false,
            window,
            cx,
        );
    }

    fn build_text_box(
        title: &'static str,
        body: AnyElement,
        dialog: Dialog,
        on_close: impl Fn(&ClickEvent, &mut Window, &mut App) + Clone + 'static,
    ) -> Dialog {
        let body = Rc::new(RefCell::new(Some(body)));
        dialog
            .overlay(true)
            .title(crate::ui::shell::dialog_title(title))
            .content(crate::ui::shell::scrollable_dialog_content(
                move |content, _, _| {
                    let body = body
                        .borrow_mut()
                        .take()
                        .unwrap_or_else(|| div().into_any_element());
                    content.child(body)
                },
            ))
            .on_close(on_close)
    }

    pub(super) fn build_suggest_accept_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::SuggestAccept, |this, _, cx| {
                this.close_suggest_accept(cx);
            });
        app.update(cx, |this, cx| {
            let Some(accept) = this.suggested_posts.accept.as_ref() else {
                return dialog.on_close(on_close);
            };
            let body = div()
                .flex()
                .flex_col()
                .gap_3()
                .child(
                    div()
                        .text_sm()
                        .whitespace_normal()
                        .child(accept.text.clone()),
                )
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap_2()
                        .child(
                            Button::new("suggest-accept-cancel")
                                .label(tr(keys::CANCEL))
                                .ghost()
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.close_suggest_accept(cx);
                                    this.close_kit_dialog_if_done(
                                        DialogKind::SuggestAccept,
                                        window,
                                        cx,
                                    );
                                })),
                        )
                        .child(
                            Button::new("suggest-accept-publish")
                                .label(tr(keys::Q_MONEY_PUBLISH))
                                .primary()
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.confirm_suggest_accept(cx);
                                    this.close_kit_dialog_if_done(
                                        DialogKind::SuggestAccept,
                                        window,
                                        cx,
                                    );
                                })),
                        ),
                )
                .into_any_element();
            Self::build_text_box(tr(keys::Q_MONEY_ACCEPT_TERMS), body, dialog, on_close)
        })
    }

    pub(super) fn build_suggest_decline_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::SuggestDecline, |this, _, cx| {
                this.close_suggest_decline(cx);
            });
        app.update(cx, |this, cx| {
            let Some(decline) = this.suggested_posts.decline.as_ref() else {
                return dialog.on_close(on_close);
            };
            let body = div()
                .flex()
                .flex_col()
                .gap_3()
                .child(
                    div()
                        .text_sm()
                        .whitespace_normal()
                        .child(decline.text.clone()),
                )
                .child(Input::new(&decline.reason).aria_label(tr(keys::SUGGEST_DECLINE_REASON)))
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap_2()
                        .child(
                            Button::new("suggest-decline-cancel")
                                .label(tr(keys::CANCEL))
                                .ghost()
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.close_suggest_decline(cx);
                                    this.close_kit_dialog_if_done(
                                        DialogKind::SuggestDecline,
                                        window,
                                        cx,
                                    );
                                })),
                        )
                        .child(
                            Button::new("suggest-decline-confirm")
                                .label(tr(keys::Q_MONEY_DECLINE))
                                .danger()
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.confirm_suggest_decline(cx);
                                    this.close_kit_dialog_if_done(
                                        DialogKind::SuggestDecline,
                                        window,
                                        cx,
                                    );
                                })),
                        ),
                )
                .into_any_element();
            Self::build_text_box(tr(keys::Q_MONEY_DECLINE), body, dialog, on_close)
        })
    }

    fn close_grams_needed(&mut self, cx: &mut Context<Self>) {
        self.suggested_posts.grams_needed = None;
        cx.notify();
    }

    pub(super) fn build_grams_needed_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::SuggestGramsNeeded, |this, _, cx| {
                this.close_grams_needed(cx);
            });
        app.update(cx, |this, cx| {
            let Some(price) = this.suggested_posts.grams_needed else {
                return dialog.on_close(on_close);
            };
            let title = low_ton_title(price);
            let body = div()
                .flex()
                .flex_col()
                .gap_3()
                .child(div().text_sm().child(LOW_TON_TEXT))
                .child(
                    Button::new("suggest-grams-fragment")
                        .label(LOW_TON_BUTTON)
                        .primary()
                        .w_full()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.open_link(LOW_TON_URL, cx);
                            this.close_grams_needed(cx);
                            this.close_kit_dialog_if_done(
                                DialogKind::SuggestGramsNeeded,
                                window,
                                cx,
                            );
                        })),
                )
                .into_any_element();
            let body = Rc::new(RefCell::new(Some(body)));
            dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title(title))
                .content(crate::ui::shell::scrollable_dialog_content(
                    move |content, _, _| {
                        let body = body
                            .borrow_mut()
                            .take()
                            .unwrap_or_else(|| div().into_any_element());
                        content.child(body)
                    },
                ))
                .on_close(on_close)
        })
    }
}

crate::ui::shell::register_dialogs! {
    /// "{amount} Grams Needed" before paying for a suggested post.
    SuggestGramsNeeded => DialogSpec::new(
        3962,
        |app| app.suggested_posts.grams_needed.is_some(),
        QuillApp::build_grams_needed_dialog,
    )
    .closes(|app, _, cx| app.close_grams_needed(cx)),
    /// "Accept Terms" of a suggested post.
    SuggestAccept => DialogSpec::new(
        3964,
        |app| app.suggested_posts.accept.is_some(),
        QuillApp::build_suggest_accept_dialog,
    ),
    /// "Decline" of a suggested post, with the reason.
    SuggestDecline => DialogSpec::new(
        3966,
        |app| app.suggested_posts.decline.is_some(),
        QuillApp::build_suggest_decline_dialog,
    ),
}
