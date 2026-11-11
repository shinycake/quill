//! A channel's Direct Messages (tdesktop `EditDirectMessagesPriceBox`):
//! the owner lets people write to the channel and sets what each message
//! costs. The row sits in the channel settings dialog; the box is its own
//! dialog.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::chat_theme::{danger, text_muted};
use super::paid_messages_privacy::price_slider;
use super::paid_messages_state::{DirectMessagesBox, DirectStage, PriceSlider};
use super::pressable::action_row;
use super::shell::{DialogKind, QuillShell};
use crate::ui::status_note::StatusNote;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::slider::Slider;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::lang::{keys, tr, tr_with};
use quill::paid_messages::{
    DEFAULT_CHANNEL_PRICE, DirectMessagesChoice, channel_price_about, price_steps, stars_text,
};
use quill::premium_hub::group_digits;
use quill::state::Session;
use quill::telegram::envelope::ChatKind;
use std::cell::RefCell;
use std::rc::Rc;

/// What the channel has now: on or off, and the price when known.
#[derive(Clone, Copy)]
struct DirectState {
    enabled: bool,
    price: Option<i64>,
    /// `supergroupFullInfo.can_enable_paid_messages`.
    can_charge: bool,
}

fn direct_state(session: &Session, chat_id: ChatId) -> Option<DirectState> {
    let supergroup = session.chat_supergroup(chat_id)?;
    let info = session.supergroup_full_info(supergroup)?;
    let enabled = info.direct_messages_chat_id != 0;
    let price = (enabled)
        .then(|| session.chat_supergroup(ChatId(info.direct_messages_chat_id)))
        .flatten()
        .and_then(|group| session.payments.paid_messages.dm_price.get(&group).copied());
    Some(DirectState {
        enabled,
        price: if enabled { price } else { Some(0) },
        can_charge: info.admin.can_enable_paid_messages,
    })
}

impl QuillApp {
    /// The "Direct messages" row of the channel settings dialog, for the
    /// owner of a broadcast channel.
    pub(super) fn direct_messages_settings_row(
        &self,
        chat_id: ChatId,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let session = self.session()?;
        let chat = session.chats.get(&chat_id.0)?;
        let channel = matches!(
            chat.kind,
            ChatKind::Supergroup {
                is_channel: true,
                ..
            }
        );
        if !channel || !session.chat_is_owner(chat_id) {
            return None;
        }
        let state = direct_state(session, chat_id)?;
        let summary = DirectMessagesChoice {
            enabled: state.enabled,
            stars: state.price.unwrap_or(0),
        }
        .summary();
        let muted = cx.theme().muted_foreground;
        Some(
            action_row(
                "direct-messages-row",
                Some(IconName::MessagesSquare),
                tr(keys::PROFILE_DIRECT_MESSAGES),
                false,
                cx,
            )
            .child(div().flex_1())
            .child(div().text_sm().text_color(muted).child(summary))
            .child(
                Icon::new(IconName::ChevronRight)
                    .size(px(14.))
                    .text_color(muted),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.open_direct_messages(chat_id, cx);
            }))
            .into_any_element(),
        )
    }

    pub(super) fn open_direct_messages(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        let Some(session) = self.session() else {
            return;
        };
        let Some(state) = direct_state(session, chat_id) else {
            return;
        };
        let max = session.payments.paid_messages.max();
        if let Some(live) = self.live.as_mut() {
            // A refusal of an earlier Save is not this box's.
            let paid = &mut live.driver.session.payments.paid_messages;
            if !paid.save.is_saving() {
                paid.save = quill::paid_messages::PaidSave::Idle;
            }
            // The price of an enabled group may not have arrived yet.
            let _ = live.driver.fetch_direct_messages_price(chat_id);
        }
        let seeded = state.price.is_some();
        let price = state.price.unwrap_or(DEFAULT_CHANNEL_PRICE);
        let slider = direct_messages_slider(cx, price, max);
        self.paid_messages.direct = Some(DirectMessagesBox {
            chat_id: chat_id.0,
            enabled: state.enabled,
            price: slider,
            touched: false,
            seeded,
            stage: DirectStage::Editing,
        });
        self.open_box(DialogKind::DirectMessages);
        cx.notify();
    }

    /// The channel's price arrived after the box opened on the default:
    /// the slider moves to it, unless the owner already moved it.
    pub(super) fn seed_direct_messages_price(&mut self, cx: &mut Context<Self>) {
        let Some(boxed) = self.paid_messages.direct.as_ref() else {
            return;
        };
        if boxed.seeded || boxed.touched {
            return;
        }
        let Some(session) = self.session() else {
            return;
        };
        let Some(price) = direct_state(session, ChatId(boxed.chat_id)).and_then(|s| s.price) else {
            return;
        };
        let max = session.payments.paid_messages.max();
        let slider = direct_messages_slider(cx, price, max);
        if let Some(boxed) = self.paid_messages.direct.as_mut() {
            boxed.price = slider;
            boxed.seeded = true;
        }
    }

    fn close_direct_messages(&mut self, cx: &mut Context<Self>) {
        self.paid_messages.direct = None;
        cx.notify();
    }

    fn save_direct_messages(&mut self, cx: &mut Context<Self>) {
        let Some(boxed) = self.paid_messages.direct.as_ref() else {
            return;
        };
        if boxed.stage == DirectStage::Saving {
            return;
        }
        let chat_id = ChatId(boxed.chat_id);
        let stars = if boxed.enabled { boxed.price.price } else { 0 };
        let enabled = boxed.enabled;
        if let Some(live) = self.live.as_mut() {
            if live
                .driver
                .set_direct_messages_group(chat_id, enabled, stars)
                .is_ok()
            {
                if let Some(boxed) = self.paid_messages.direct.as_mut() {
                    boxed.stage = DirectStage::Saving;
                }
            } else {
                self.set_note(StatusNote::error(tr(
                    keys::Q_MONEY_COULDNT_CHANGE_THE_DIRECT_MESSAGES,
                )));
            }
        } else {
            self.paid_messages.direct = None;
            self.set_note(StatusNote::info(tr(
                keys::Q_MONEY_DIRECT_MESSAGES_NEED_A_LIVE,
            )));
        }
        cx.notify();
    }

    /// Per-frame, in the window that hosts the box: Save is out until its
    /// answer. An accepted change closes the box; a refusal opens it up
    /// again with the reason drawn in it.
    pub(in crate::ui) fn direct_messages_upkeep(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use quill::paid_messages::{PaidSave, PaidTarget};
        let Some(boxed) = self
            .paid_messages
            .direct
            .as_ref()
            .filter(|boxed| boxed.stage == DirectStage::Saving)
        else {
            return;
        };
        let target = PaidTarget::Channel(boxed.chat_id);
        let save = self
            .session()
            .map(|session| session.payments.paid_messages.save.clone())
            .unwrap_or_default();
        match save {
            // Another target's save (a Remove Fee) holds the slot: it says
            // nothing about this channel's request, which is not known to
            // have ended. The box stays as it is, and closable.
            PaidSave::Saving(other) | PaidSave::Failed { target: other, .. } if other != target => {
            }
            PaidSave::Saving(_) => {}
            PaidSave::Failed { .. } => {
                if let Some(boxed) = self.paid_messages.direct.as_mut() {
                    boxed.stage = DirectStage::Editing;
                }
                cx.notify();
            }
            // Idle: the change went through (or the answer was lost with
            // its account).
            PaidSave::Idle => {
                self.close_direct_messages(cx);
                self.close_kit_dialog_if_done(DialogKind::DirectMessages, window, cx);
            }
        }
    }

    pub(super) fn build_direct_messages_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::DirectMessages, |this, _, cx| {
                this.close_direct_messages(cx);
            });
        app.update(cx, |this, cx| {
            this.seed_direct_messages_price(cx);
            let (Some(session), Some(boxed)) = (this.session(), this.paid_messages.direct.as_ref())
            else {
                return dialog.overlay(true).on_close(on_close);
            };
            let chat_id = ChatId(boxed.chat_id);
            let current = direct_state(session, chat_id);
            let muted = cx.theme().muted_foreground;
            let paid = &session.payments.paid_messages;
            let can_charge = current.is_some_and(|s| s.can_charge);
            let (min, max) = (
                boxed.price.steps.first().copied().unwrap_or(0),
                boxed.price.steps.last().copied().unwrap_or(0),
            );
            let price = boxed.price.price;
            let saving = boxed.stage == DirectStage::Saving || paid.save.is_saving();
            let error = paid
                .save
                .error_for(quill::paid_messages::PaidTarget::Channel(chat_id.0))
                .map(str::to_string);
            let known = current.and_then(|s| s.price);
            let was_enabled = current.is_some_and(|s| s.enabled);
            // Without the price the channel has now, only a deliberate
            // slider move may write one.
            let changed = boxed.enabled != was_enabled
                || (boxed.enabled && (boxed.touched || known.is_some_and(|k| k != price)));
            let link = session
                .chat_usernames(chat_id)
                .and_then(|lists| lists.active.first().cloned())
                .map(|name| format!("https://t.me/{name}?direct"));
            let mut body = div().flex().flex_col().gap_3().child(
                div()
                    .text_sm()
                    .text_color(muted)
                    .child(tr(keys::MANAGE_MONOFORUM_ABOUT)),
            );
            body = body.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        Switch::new("direct-messages-enabled")
                            .checked(boxed.enabled)
                            .accessibility_label(tr(keys::Q_MONEY_ALLOW_CHANNEL_MESSAGES))
                            .on_click(cx.listener(|this, &on: &bool, _, cx| {
                                if let Some(boxed) = this.paid_messages.direct.as_mut() {
                                    boxed.enabled = on;
                                }
                                cx.notify();
                            })),
                    )
                    .child(
                        div()
                            .text_sm()
                            .child(tr(keys::Q_MONEY_ALLOW_CHANNEL_MESSAGES)),
                    ),
            );
            if boxed.enabled && can_charge {
                body = body
                    .child(
                        div()
                            .text_xs()
                            .font_semibold()
                            .text_color(muted)
                            .child(tr(keys::MANAGE_MONOFORUM_PRICE)),
                    )
                    .child(div().flex().justify_center().child(
                        div().text_lg().font_semibold().child(if price == 0 {
                            tr(keys::Q_MONEY_FREE).to_string()
                        } else {
                            format!("\u{2B50} {}", group_digits(price))
                        }),
                    ))
                    .child(Slider::new(&boxed.price.slider).bg(cx.theme().primary))
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .text_xs()
                            .text_color(muted)
                            .child(tr(keys::Q_MONEY_FREE))
                            .child(group_digits(max)),
                    )
                    .when(price > 0, |this| {
                        this.child(
                            div()
                                .text_xs()
                                .text_color(muted)
                                .child(channel_price_about(paid.per_mille())),
                        )
                    });
                let _ = min;
            } else if boxed.enabled {
                body = body.child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child(tr(keys::Q_MONEY_MESSAGES_TO_THIS_CHANNEL_ARE)),
                );
            }
            if let Some(link) = link.filter(|_| boxed.enabled && was_enabled) {
                let shown = link.trim_start_matches("https://").to_string();
                body = body.child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap_2()
                        .child(div().text_sm().text_color(text_muted()).child(shown))
                        .child(
                            Button::new("direct-messages-copy")
                                .label(tr(keys::BOOST_CHANNEL_ASK_BUTTON))
                                .ghost()
                                .small()
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(link.clone()));
                                    this.set_note(StatusNote::info(tr(keys::Q_MONEY_LINK_COPIED)));
                                    cx.notify();
                                })),
                        ),
                );
            }
            if was_enabled && known.is_none() && !boxed.touched {
                body = body.child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child(tr(keys::Q_MONEY_THE_CURRENT_PRICE_IS_STILL)),
                );
            }
            if let Some(line) = error {
                body = body.child(div().text_xs().text_color(danger()).child(line));
            }
            if boxed.enabled && price > 0 && can_charge {
                body = body.child(div().text_xs().text_color(muted).child(tr_with(
                    keys::Q_MONEY_EACH_MESSAGE_COSTS_SENDERS,
                    &[("stars_text", &(stars_text(price)).to_string())],
                )));
            }
            let body = body.into_any_element();
            let footer = div()
                .flex()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("direct-messages-cancel")
                        .label(tr(keys::CANCEL))
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.close_direct_messages(cx);
                            this.close_kit_dialog_if_done(DialogKind::DirectMessages, window, cx);
                        })),
                )
                .child(
                    Button::new("direct-messages-save")
                        .label(tr(keys::SETTINGS_SAVE))
                        .primary()
                        .disabled(!changed || saving)
                        .on_click(cx.listener(|this, _, _, cx| this.save_direct_messages(cx))),
                );
            dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title(tr(
                    keys::PROFILE_DIRECT_MESSAGES,
                )))
                .content(crate::ui::shell::scrollable_dialog_content({
                    let body = Rc::new(RefCell::new(Some(body)));
                    move |content, _, _| {
                        let body = body
                            .borrow_mut()
                            .take()
                            .unwrap_or_else(|| div().into_any_element());
                        content.child(body)
                    }
                }))
                .footer(footer)
                .on_close(on_close)
        })
    }
}

crate::ui::shell::register_dialogs! {
    /// A channel's direct messages: on or off, and the price per message.
    DirectMessages => DialogSpec::new(
        4895,
        |app| app.paid_messages.direct.is_some(),
        QuillApp::build_direct_messages_dialog,
    )
    .close(|app, _, cx| app.close_direct_messages(cx)),
}

/// The box's price slider, starting at `price`.
fn direct_messages_slider(cx: &mut Context<QuillApp>, price: i64, max: i64) -> PriceSlider {
    let price = price.clamp(0, max);
    price_slider(cx, price_steps(price, 0, max), price, |this, price| {
        if let Some(boxed) = this.paid_messages.direct.as_mut() {
            boxed.price.price = price;
            boxed.touched = true;
        }
    })
}
