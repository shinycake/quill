//! Messages privacy (tdesktop `EditMessagesPrivacyBox`): who can write to
//! the account, now with "Charge for messages", a price slider that shows
//! what the account receives, and the Remove Fee exceptions.
//!
//! Everybody and Contacts and Premium apply as soon as they are picked,
//! like before. Charging asks for a price first and starts only with Save:
//! nothing is charged by a stray click.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::chat_theme::{danger, text_muted, warning};
use super::paid_messages_state::PriceSlider;
use super::privacy::{PrivacyEditorTarget, PrivacyExceptionKind};
use crate::ui::status_note::StatusNote;
use crate::ui::user_error::user_error_for;
use gpui_kit::component::button::*;
use gpui_kit::component::slider::{Slider, SliderEvent, SliderState};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::lang::{keys, tr};
use quill::paid_messages::{
    DEFAULT_PRIVATE_PRICE, MessagesPolicy, price_about, price_steps, stars_text, step_for_price,
};
use quill::premium_hub::group_digits;
use quill::privacy::{NewChatPrivacy, NewChatPrivacyState};
use quill::telegram::requests_privacy::PrivacySettingKey;

/// A slider over `steps` (see `price_steps`) that reports each pick to
/// `apply`. The returned value keeps the steps and the current price.
pub(super) fn price_slider(
    cx: &mut Context<QuillApp>,
    steps: Vec<i64>,
    price: i64,
    apply: fn(&mut QuillApp, i64),
) -> PriceSlider {
    let last = steps.len().saturating_sub(1);
    let start = step_for_price(&steps, price);
    let slider = cx.new(|_| {
        SliderState::new()
            .min(0.)
            .max(last as f32)
            .step(1.)
            .default_value(start as f32)
    });
    let owned = steps.clone();
    cx.subscribe(&slider, move |this, _, event: &SliderEvent, cx| {
        let (SliderEvent::Change(value) | SliderEvent::Release(value)) = event;
        let index = (value.end().round().max(0.) as usize).min(owned.len().saturating_sub(1));
        if let Some(price) = owned.get(index) {
            apply(this, *price);
        }
        cx.notify();
    })
    .detach();
    PriceSlider {
        price: steps.get(start).copied().unwrap_or(price),
        steps,
        slider,
    }
}

impl QuillApp {
    /// The Messages row of the Privacy screen was pressed.
    pub(crate) fn open_new_chat_editor(&mut self, cx: &mut Context<Self>) {
        self.privacy.editor = Some(PrivacyEditorTarget::NewChat);
        self.privacy.exception_picker_open = false;
        self.paid_messages.charge = None;
        if let Some(live) = self.live.as_mut() {
            // The free-messages list behind "Remove fee".
            let _ = live
                .driver
                .fetch_privacy_rules(PrivacySettingKey::AllowUnpaidMessages);
        }
        cx.notify();
    }

    fn new_chat_settings(&self) -> Option<NewChatPrivacyState> {
        self.session()
            .and_then(|s| s.settings.privacy_data.new_chat)
    }

    fn can_restrict_new_chats(&self) -> bool {
        self.session().is_some_and(|s| {
            s.payments
                .paid_messages
                .can_set_privacy
                .unwrap_or_else(|| s.my_is_premium())
        })
    }

    /// Messages privacy editor.
    pub(super) fn new_chat_editor(&self, cx: &mut Context<Self>) -> AnyElement {
        let session = self.session();
        let can_restrict = self.can_restrict_new_chats();
        let error = session.and_then(|s| s.settings.privacy_data.error.clone());
        let paid_error = session.and_then(|s| s.payments.paid_messages.error.clone());
        let muted = cx.theme().muted_foreground;
        let mut body = div().flex().flex_col().gap_1();
        body = body.child(
            div()
                .text_xs()
                .font_semibold()
                .px_1()
                .text_color(muted)
                .child(tr(keys::MESSAGES_PRIVACY_SUBTITLE)),
        );
        match self.new_chat_settings() {
            Some(NewChatPrivacyState::Ready(settings)) => {
                body = self.messages_choices(body, settings, can_restrict, cx);
            }
            Some(NewChatPrivacyState::Failed) => {
                body = body.child(
                    div()
                        .text_xs()
                        .px_2()
                        .text_color(muted)
                        .child(tr(keys::Q_MONEY_COULDNT_LOAD_THIS_SETTING)),
                );
            }
            _ => {
                body = body.child(
                    div()
                        .text_xs()
                        .px_2()
                        .text_color(muted)
                        .child(tr(keys::Q_MONEY_LOADING)),
                );
            }
        }
        if !can_restrict {
            body = body
                .child(
                    div()
                        .text_xs()
                        .px_2()
                        .text_color(muted)
                        .child(tr(keys::Q_MONEY_RESTRICTING_NEW_CHATS_NEEDS_TELEGRAM)),
                )
                .child(
                    Button::new("messages-privacy-premium")
                        .label(tr(keys::MESSAGES_PRIVACY_PREMIUM_BUTTON))
                        .ghost()
                        .small()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.open_premium_from("noncontact_peers_require_premium", cx);
                        })),
                );
        }
        for line in [error, paid_error].into_iter().flatten() {
            body = body.child(div().text_xs().px_2().text_color(danger()).child(line));
        }
        self.privacy_shell(
            cx,
            "editor",
            tr(keys::Q_MONEY_WHO_CAN_MESSAGE_ME),
            body.into_any_element(),
        )
    }

    fn messages_choices(
        &self,
        mut body: Div,
        settings: NewChatPrivacy,
        can_restrict: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let policy = MessagesPolicy::from_settings(&settings);
        let charging = self.paid_messages.charge.is_some();
        let paid = self.session().map(|s| &s.payments.paid_messages);
        let charge_offered = paid.is_some_and(|p| p.charging_available());
        body = body
            .child(self.choice_row(
                cx,
                "privacy-new-chat-everyone".into(),
                tr(keys::MESSAGES_PRIVACY_EVERYONE),
                None,
                matches!(policy, MessagesPolicy::Everybody) && !charging,
                |this, cx| this.pick_messages_policy(MessagesPolicy::Everybody, cx),
            ))
            .child(self.choice_row(
                cx,
                "privacy-new-chat-restricted".into(),
                tr(keys::MESSAGES_PRIVACY_RESTRICTED),
                Some(tr(keys::Q_MONEY_NON_CONTACTS_NEED_TELEGRAM_PREMIUM).into()),
                matches!(policy, MessagesPolicy::ContactsAndPremium) && !charging,
                |this, cx| this.pick_messages_policy(MessagesPolicy::ContactsAndPremium, cx),
            ));
        if charge_offered || matches!(policy, MessagesPolicy::Charge(_)) {
            body = body.child(self.choice_row(
                cx,
                "privacy-new-chat-charge".into(),
                tr(keys::MESSAGES_PRIVACY_CHARGE),
                Some(tr(keys::Q_MONEY_NON_CONTACTS_PAY_STARS_FOR).into()),
                charging || matches!(policy, MessagesPolicy::Charge(_)),
                |this, cx| this.open_charge_draft(cx),
            ));
        }
        let per_mille = paid.map_or(quill::paid_messages::FALLBACK_PER_MILLE, |p| p.per_mille());
        let current_price = match policy {
            MessagesPolicy::Charge(stars) => Some(stars),
            _ => None,
        };
        if let Some(draft) = self.paid_messages.charge.as_ref() {
            body =
                body.child(self.charge_editor(draft, current_price, per_mille, can_restrict, cx));
        } else if let Some(stars) = current_price {
            body = body.child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_2()
                    .py_1()
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!(
                                "{} per message. {}",
                                stars_text(stars),
                                price_about(per_mille)
                            )),
                    )
                    .child(
                        Button::new("privacy-new-chat-change-price")
                            .label(tr(keys::Q_MONEY_CHANGE_PRICE))
                            .ghost()
                            .small()
                            .on_click(cx.listener(|this, _, _, cx| this.open_charge_draft(cx))),
                    ),
            );
        }
        if charge_offered || current_price.is_some() {
            body = body.child(self.remove_fee_row(cx));
        }
        body
    }

    /// The price slider, what it earns, and Save.
    fn charge_editor(
        &self,
        draft: &PriceSlider,
        saved: Option<i64>,
        per_mille: i64,
        can_restrict: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let muted = cx.theme().muted_foreground;
        let (min, max) = (
            draft.steps.first().copied().unwrap_or(1),
            draft.steps.last().copied().unwrap_or(1),
        );
        let changed = saved != Some(draft.price);
        div()
            .flex()
            .flex_col()
            .gap_1()
            .px_2()
            .py_1()
            .child(
                div()
                    .text_xs()
                    .font_semibold()
                    .text_color(muted)
                    .child(tr(keys::MESSAGES_PRIVACY_PRICE)),
            )
            .child(
                div().flex().justify_center().child(
                    div()
                        .text_lg()
                        .font_semibold()
                        .child(format!("\u{2B50} {}", group_digits(draft.price))),
                ),
            )
            .child(Slider::new(&draft.slider).bg(cx.theme().primary))
            .child(
                div()
                    .flex()
                    .justify_between()
                    .text_xs()
                    .text_color(muted)
                    .child(group_digits(min))
                    .child(group_digits(max)),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child(price_about(per_mille)),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap_2()
                    .child(
                        Button::new("privacy-new-chat-charge-cancel")
                            .label(tr(keys::CANCEL))
                            .ghost()
                            .small()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.paid_messages.charge = None;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("privacy-new-chat-charge-save")
                            .label(if saved.is_some() {
                                tr(keys::Q_MONEY_SAVE_PRICE)
                            } else {
                                tr(keys::Q_MONEY_START_CHARGING)
                            })
                            .primary()
                            .small()
                            .disabled(!changed || !can_restrict)
                            .on_click(cx.listener(|this, _, _, cx| this.save_charge_price(cx))),
                    ),
            )
            .when(!can_restrict, |this| {
                this.child(
                    div()
                        .text_xs()
                        .text_color(warning())
                        .child(tr(keys::Q_MONEY_CHARGING_NEEDS_TELEGRAM_PREMIUM)),
                )
            })
    }

    /// "Remove fee": who writes for free, opening the exceptions list.
    fn remove_fee_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let key = PrivacySettingKey::AllowUnpaidMessages;
        let count = self
            .session()
            .and_then(|s| s.settings.privacy.get(&key))
            .map(|state| match state {
                quill::privacy::PrivacyKeyState::Ready(detail) => {
                    detail.always.len() + detail.always_chats.len()
                }
                _ => 0,
            })
            .unwrap_or(0);
        let target = PrivacyEditorTarget::Rule(key);
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .id("privacy-new-chat-exceptions")
                    .role(gpui_kit::Role::Button)
                    .aria_label(tr(keys::Q_MONEY_EXCEPTIONS_REMOVE_FEE))
                    .tab_index(0)
                    .cursor_pointer()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .child(
                        div()
                            .text_sm()
                            .child(tr(keys::Q_MONEY_EXCEPTIONS_REMOVE_FEE_2)),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(text_muted())
                            .child(if count == 0 {
                                tr(keys::Q_MONEY_ADD).to_string()
                            } else {
                                format!("{count} added")
                            }),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.privacy.exceptions = Some((target, PrivacyExceptionKind::Always));
                        this.privacy.exception_picker_open = false;
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .text_xs()
                    .px_2()
                    .text_color(cx.theme().muted_foreground)
                    .child(PrivacySettingKey::AllowUnpaidMessages.exceptions_note()),
            )
            .into_any_element()
    }

    /// Everybody or Contacts and Premium: applied at once. Restricting
    /// needs Premium (or the server's free allowance).
    pub(super) fn pick_messages_policy(&mut self, policy: MessagesPolicy, cx: &mut Context<Self>) {
        self.paid_messages.charge = None;
        if !matches!(policy, MessagesPolicy::Everybody) && !self.can_restrict_new_chats() {
            self.connection.set_note(StatusNote::info(tr(
                keys::Q_MONEY_RESTRICTING_NEW_CHATS_NEEDS_TELEGRAM,
            )));
            cx.notify();
            return;
        }
        self.apply_new_chat_settings(policy.to_settings(), cx);
    }

    /// Opens the price slider on the saved price, or the default one.
    pub(super) fn open_charge_draft(&mut self, cx: &mut Context<Self>) {
        if !self.can_restrict_new_chats() {
            self.connection.set_note(StatusNote::info(tr(
                keys::Q_MONEY_CHARGING_FOR_MESSAGES_NEEDS_TELEGRAM,
            )));
            cx.notify();
            return;
        }
        let saved = match self.new_chat_settings() {
            Some(NewChatPrivacyState::Ready(settings)) => {
                match MessagesPolicy::from_settings(&settings) {
                    MessagesPolicy::Charge(stars) => Some(stars),
                    _ => None,
                }
            }
            _ => return,
        };
        let max = self
            .session()
            .map_or(quill::paid_messages::FALLBACK_MAX_STARS, |s| {
                s.payments.paid_messages.max()
            });
        let price = saved.unwrap_or(DEFAULT_PRIVATE_PRICE).clamp(1, max);
        let steps = price_steps(price, 1, max);
        self.paid_messages.charge = Some(price_slider(cx, steps, price, |this, price| {
            if let Some(draft) = this.paid_messages.charge.as_mut() {
                draft.price = price;
            }
        }));
        cx.notify();
    }

    /// Save: charge the staged price from now on.
    pub(super) fn save_charge_price(&mut self, cx: &mut Context<Self>) {
        let Some(draft) = self.paid_messages.charge.take() else {
            return;
        };
        if !self.can_restrict_new_chats() {
            return;
        }
        self.apply_new_chat_settings(MessagesPolicy::Charge(draft.price).to_settings(), cx);
    }

    fn apply_new_chat_settings(&mut self, settings: NewChatPrivacy, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            if let Err(err) = live.driver.set_new_chat_settings(settings) {
                self.connection.set_note(StatusNote::error(user_error_for(
                    keys::Q_NOTES_FAILED_PRIVACY_UPDATE,
                    &err,
                )));
            }
        } else if let Some(demo) = self.demo_session.as_mut()
            && matches!(
                demo.settings.privacy_data.new_chat,
                Some(NewChatPrivacyState::Ready(_))
            )
        {
            demo.set_new_chat_privacy_local(settings);
        }
        cx.notify();
    }
}
