//! The price and time boxes of suggested posts (Telegram Desktop
//! `ChooseSuggestPriceBox`, `ChooseSuggestTimeBox`).
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::shell::{DialogKind, QuillShell};
use super::suggested_posts_state::{BalanceFor, PriceBox, SuggestTarget, TimeBox, TimeTarget};
use super::suggested_posts_ui::price_amount;
use crate::ui::status_note::StatusNote;
use gpui_kit::component::button::*;
use gpui_kit::component::date_picker::DatePicker;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::tab::{Tab, TabBar};
use gpui_kit::component::*;
use gpui_kit::*;
use quill::lang::{keys, tr, tr_with};
use quill::local_time::now_unix;
use quill::suggested_posts::{
    DATE_ABOUT, PriceCheck, STARS_WARNING, SuggestBar, SuggestMode, SuggestTerms, SuggestedPrice,
    currency_tabs, field_text, price_about_member, price_section_title, price_submit_label,
    shortfall, time_value,
};
use std::cell::RefCell;
use std::rc::Rc;

impl QuillApp {
    /// Opens the "Suggest a Message" box with `terms`.
    pub(super) fn open_suggest_price(
        &mut self,
        target: SuggestTarget,
        mode: SuggestMode,
        terms: SuggestTerms,
        admin: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(tr(keys::Q_MONEY_PRICE))
                .submit_on_enter(false)
        });
        let text = field_text(terms.price);
        if !text.is_empty() {
            input.update(cx, |input, cx| input.set_value(text, window, cx));
        }
        cx.subscribe_in(
            &input,
            window,
            |this, _, event: &gpui_kit::component::input::InputEvent, window, cx| {
                this.in_window_of(window, cx, |this, window, cx| match event {
                    gpui_kit::component::input::InputEvent::PressEnter { .. } => {
                        if this.submit_suggest_price(cx) {
                            this.close_kit_dialog_if_done(DialogKind::SuggestPrice, window, cx);
                        }
                    }
                    gpui_kit::component::input::InputEvent::Change => {
                        if let Some(price) = this.suggested_posts.price.as_mut() {
                            price.error = None;
                        }
                        cx.notify();
                    }
                    _ => {}
                });
            },
        )
        .detach();
        self.suggested_posts.price = Some(PriceBox {
            target,
            mode,
            admin,
            ton: terms.price.is_some_and(SuggestedPrice::is_ton),
            input,
            send_date: terms.send_date,
            error: None,
        });
        cx.notify();
    }

    fn close_suggest_price(&mut self, cx: &mut Context<Self>) {
        self.suggested_posts.price = None;
        cx.notify();
    }

    /// The tabs switch the currency; the typed amount carries over.
    fn set_suggest_currency(&mut self, ton: bool, cx: &mut Context<Self>) {
        if let Some(price) = self.suggested_posts.price.as_mut() {
            price.ton = ton;
            price.error = None;
        }
        cx.notify();
    }

    fn suggest_limits(&self) -> quill::suggested_posts::SuggestedPostLimits {
        self.session()
            .map(|session| session.payments.suggested_posts.limits)
            .unwrap_or_default()
    }

    /// What a member's balance lacks for `price`, when the balance is known.
    pub(super) fn suggest_shortfall(&self, price: SuggestedPrice) -> Option<SuggestedPrice> {
        let session = self.session()?;
        let stars = session.payments.hub.balance.map(|balance| balance.stars);
        let grams = session.payments.suggested_posts.gram_balance.or_else(|| {
            session
                .ton_wallet
                .own
                .as_ref()
                .map(|wallet| wallet.gram_amount)
        });
        shortfall(price, stars, grams)
    }

    /// A member can't pay: Stars open the Stars screen (Quill can't buy
    /// Stars in-app yet), Grams show the Grams Needed box. `price` is the
    /// full price in Stars and the missing part in Grams
    /// (`quill::suggested_posts::shortfall`).
    pub(super) fn ask_for_balance(
        &mut self,
        price: SuggestedPrice,
        purpose: BalanceFor,
        cx: &mut Context<Self>,
    ) {
        match price {
            SuggestedPrice::Stars(_) => {
                let what = match purpose {
                    BalanceFor::Offer => tr(keys::Q_MONEY_OFFER_THIS_PRICE),
                    BalanceFor::Accept => tr(keys::Q_MONEY_ACCEPT_THIS_POST),
                };
                self.connection.set_note(StatusNote::info(tr_with(
                    keys::Q_MONEY_YOU_NEED_TO,
                    &[("label", &(price.label()).to_string()), ("what", what)],
                )));
                self.open_stars(cx);
            }
            SuggestedPrice::TonCents(_) => {
                self.suggested_posts.grams_needed = Some(price);
            }
        }
        cx.notify();
    }

    /// Checks the field and applies the terms. Returns whether the box
    /// closed.
    pub(super) fn submit_suggest_price(&mut self, cx: &mut Context<Self>) -> bool {
        let limits = self.suggest_limits();
        let Some(price_box) = self.suggested_posts.price.as_ref() else {
            return true;
        };
        let text = price_box.input.read(cx).value().to_string();
        let ton = price_box.ton;
        let error = match limits.check_price(&text, ton) {
            PriceCheck::Ok(price) => {
                let terms = SuggestTerms {
                    price,
                    send_date: price_box.send_date,
                };
                let (target, mode, admin) = (price_box.target, price_box.mode, price_box.admin);
                if !admin && let Some(short) = price.and_then(|p| self.suggest_shortfall(p)) {
                    self.ask_for_balance(short, BalanceFor::Offer, cx);
                    return false;
                }
                self.suggested_posts.price = None;
                self.apply_suggest_terms(target, mode, terms, cx);
                return true;
            }
            PriceCheck::TooSmall => limits.min_price_toast(ton),
            PriceCheck::TooLarge => {
                let max = if ton {
                    SuggestedPrice::TonCents(limits.ton_cent_max)
                } else {
                    SuggestedPrice::Stars(limits.star_max)
                };
                tr_with(
                    keys::Q_MONEY_MAXIMUM_PRICE_IS,
                    &[("label", &(max.label()).to_string())],
                )
            }
            PriceCheck::Invalid => tr(keys::Q_MONEY_ENTER_A_NUMBER).into(),
        };
        if let Some(price_box) = self.suggested_posts.price.as_mut() {
            price_box.error = Some(error);
        }
        cx.notify();
        false
    }

    /// New terms from a box: into the bar, or out as `addOffer`.
    pub(super) fn apply_suggest_terms(
        &mut self,
        target: SuggestTarget,
        mode: SuggestMode,
        terms: SuggestTerms,
        cx: &mut Context<Self>,
    ) {
        match target {
            SuggestTarget::Bar(chat_id) => {
                let edit_of = self
                    .session()
                    .and_then(|session| session.suggest_bar(chat_id))
                    .and_then(|bar| bar.edit_of);
                self.set_suggest_bar(
                    chat_id,
                    Some(SuggestBar {
                        terms,
                        mode,
                        edit_of,
                    }),
                    cx,
                );
            }
            SuggestTarget::Offer {
                chat_id,
                message_id,
            } => {
                self.connection.set_note(match self.live.as_mut() {
                    Some(live) => match live.driver.add_offer(chat_id, message_id, terms) {
                        Ok(_) => StatusNote::silent(tr(keys::Q_MONEY_SENDING_THE_OFFER)),
                        Err(_) => StatusNote::error(tr(keys::Q_MONEY_COULDNT_SEND_THE_OFFER)),
                    },
                    None => StatusNote::silent(tr(keys::Q_MONEY_OFFER_SENT_DEMO)),
                });
                cx.notify();
            }
        }
    }

    /// Opens the time box at `current` (0: a day from now).
    pub(super) fn open_suggest_time(
        &mut self,
        target: TimeTarget,
        mode: SuggestMode,
        current: i32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let limits = self.suggest_limits();
        let now = now_unix();
        let picker = super::scheduled::new_date_time_picker(
            window,
            cx,
            limits.initial_time(now, current),
            limits.send_delay_max,
        );
        self.suggested_posts.time = Some(TimeBox {
            target,
            mode,
            picker,
            error: None,
        });
        cx.notify();
    }

    fn close_suggest_time(&mut self, cx: &mut Context<Self>) {
        self.suggested_posts.time = None;
        cx.notify();
    }

    /// The time box's answer: `Some(0)` for "Anytime" / "Publish Now",
    /// `None` to read the picker.
    pub(super) fn submit_suggest_time(
        &mut self,
        anytime: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(time_box) = self.suggested_posts.time.as_ref() else {
            return;
        };
        let send_date = if anytime {
            0
        } else {
            let Some(picked) = super::scheduled::picked_unix(&time_box.picker, cx) else {
                if let Some(time_box) = self.suggested_posts.time.as_mut() {
                    time_box.error = Some(tr(keys::Q_MONEY_PICK_A_DATE_AND_TIME).into());
                }
                cx.notify();
                return;
            };
            let clamped = self.suggest_limits().clamp_time(now_unix(), picked);
            i32::try_from(clamped).unwrap_or(i32::MAX)
        };
        let target = time_box.target;
        self.suggested_posts.time = None;
        match target {
            TimeTarget::PriceBox => {
                if let Some(price_box) = self.suggested_posts.price.as_mut() {
                    price_box.send_date = send_date;
                }
            }
            TimeTarget::Offer {
                chat_id,
                message_id,
                price,
            } => self.apply_suggest_terms(
                super::suggested_posts_state::SuggestTarget::Offer {
                    chat_id,
                    message_id,
                },
                SuggestMode::Change,
                SuggestTerms { price, send_date },
                cx,
            ),
            TimeTarget::Accept {
                chat_id,
                message_id,
            } => self.open_suggest_accept(chat_id, message_id, send_date, window, cx),
        }
        cx.notify();
    }

    fn suggest_price_body(&self, price_box: &PriceBox, cx: &mut Context<Self>) -> AnyElement {
        let limits = self.suggest_limits();
        let muted = cx.theme().muted_foreground;
        let ton = price_box.ton;
        let (stars_tab, grams_tab) = currency_tabs(price_box.admin);
        let owner = cx.entity().downgrade();
        let tabs = TabBar::new("suggest-price-tabs")
            .pill()
            .child(Tab::new().label(stars_tab))
            .child(Tab::new().label(grams_tab))
            .selected_index(usize::from(ton))
            .on_click(move |ix, _, cx| {
                let ton = *ix == 1;
                let _ = owner.update(cx, |this, cx| this.set_suggest_currency(ton, cx));
            });
        let typed = price_box.input.read(cx).value().to_string();
        let price = match limits.check_price(&typed, ton) {
            PriceCheck::Ok(price) => price,
            _ => None,
        };
        let balance = (!price_box.admin)
            .then(|| self.suggest_balance(ton, cx))
            .flatten();
        let usd = price
            .filter(|price| price.is_ton())
            .zip(self.session().and_then(|s| s.ton_wallet.ton_usd_rate))
            .map(|(price, rate)| format!("\u{2248}${:.2}", price.nanos() as f64 / 1e9 * rate));
        // tdesktop: the admin always sees their share, "0 Stars" for free.
        let about = if price_box.admin {
            let shown = price.unwrap_or(if ton {
                SuggestedPrice::TonCents(0)
            } else {
                SuggestedPrice::Stars(0)
            });
            let mut text = limits.you_get(shown);
            if !ton {
                text.push_str("\n\n");
                text.push_str(STARS_WARNING);
            }
            text
        } else {
            price_about_member(ton).to_string()
        };
        let prefix = if ton {
            super::earn::ton_icon(16., cx.theme().foreground)
        } else {
            super::earn::star_icon(16.)
        };
        let mode = price_box.mode;
        let send_date = price_box.send_date;
        let submit_label = price_submit_label(price_box.mode, price);
        let mut submit = Button::new("suggest-price-submit")
            .primary()
            .w_full()
            .label(submit_label)
            .on_click(cx.listener(|this, _, window, cx| {
                if this.submit_suggest_price(cx) {
                    this.close_kit_dialog_if_done(DialogKind::SuggestPrice, window, cx);
                }
            }));
        if let Some(price) = price.filter(|_| mode != SuggestMode::Change) {
            submit = submit.child(price_amount(price, 14., cx));
        }
        div()
            .flex()
            .flex_col()
            .gap_3()
            .children(balance)
            .child(div().flex().justify_center().child(tabs))
            .child(
                div()
                    .text_sm()
                    .font_semibold()
                    .child(price_section_title(ton)),
            )
            .child(
                Input::new(&price_box.input)
                    .prefix(prefix)
                    .aria_label(price_section_title(ton)),
            )
            .children(usd.map(|usd| div().text_sm().text_color(muted).child(usd)))
            .children(
                price_box
                    .error
                    .clone()
                    .map(|error| div().text_sm().text_color(cx.theme().danger).child(error)),
            )
            .child(div().text_sm().text_color(muted).child(about))
            .child(
                Button::new("suggest-price-time")
                    .ghost()
                    .w_full()
                    .accessibility_label(tr(keys::Q_MONEY_TIME))
                    .child(
                        div()
                            .flex()
                            .w_full()
                            .justify_between()
                            .child(tr(keys::Q_MONEY_TIME))
                            .child(
                                div()
                                    .text_color(super::chat_theme::accent())
                                    .child(time_value(send_date, now_unix())),
                            ),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_suggest_time(TimeTarget::PriceBox, mode, send_date, window, cx);
                    })),
            )
            .child(div().text_sm().text_color(muted).child(DATE_ABOUT))
            .child(submit)
            .into_any_element()
    }

    /// The member's balance in the active currency, when known.
    fn suggest_balance(&self, ton: bool, cx: &App) -> Option<AnyElement> {
        let session = self.session()?;
        let price = if ton {
            let nanos = session.payments.suggested_posts.gram_balance.or_else(|| {
                session
                    .ton_wallet
                    .own
                    .as_ref()
                    .map(|wallet| wallet.gram_amount)
            })?;
            SuggestedPrice::TonCents(nanos.checked_div(10_000_000).unwrap_or(0))
        } else {
            SuggestedPrice::Stars(session.payments.hub.balance?.stars)
        };
        Some(
            div()
                .flex()
                .items_center()
                .gap_1()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(tr(keys::Q_MONEY_BALANCE))
                .child(price_amount(price, 13., cx))
                .into_any_element(),
        )
    }

    pub(super) fn build_suggest_price_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::SuggestPrice, |this, _, cx| {
                this.close_suggest_price(cx);
            });
        app.update(cx, |this, cx| {
            let Some(price_box) = this.suggested_posts.price.take() else {
                return dialog.on_close(on_close);
            };
            let title = price_box.mode.price_title();
            let body = this.suggest_price_body(&price_box, cx);
            this.suggested_posts.price = Some(price_box);
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

    pub(super) fn build_suggest_time_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::SuggestTime, |this, _, cx| {
                this.close_suggest_time(cx);
            });
        app.update(cx, |this, cx| {
            let Some(time_box) = this.suggested_posts.time.as_ref() else {
                return dialog.on_close(on_close);
            };
            let mode = time_box.mode;
            let body = div()
                .flex()
                .flex_col()
                .gap_3()
                .child(
                    DatePicker::new(&time_box.picker)
                        .placeholder(tr(keys::Q_MONEY_PICK_A_DATE_AND_TIME_2)),
                )
                .children(
                    time_box
                        .error
                        .clone()
                        .map(|error| div().text_sm().text_color(cx.theme().danger).child(error)),
                )
                .child(
                    div()
                        .flex()
                        .justify_between()
                        .gap_2()
                        .child(
                            Button::new("suggest-time-none")
                                .label(mode.time_none())
                                .ghost()
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.submit_suggest_time(true, window, cx);
                                    this.close_kit_dialog_if_done(
                                        DialogKind::SuggestTime,
                                        window,
                                        cx,
                                    );
                                })),
                        )
                        .child(
                            Button::new("suggest-time-submit")
                                .label(mode.time_submit())
                                .primary()
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.submit_suggest_time(false, window, cx);
                                    this.close_kit_dialog_if_done(
                                        DialogKind::SuggestTime,
                                        window,
                                        cx,
                                    );
                                })),
                        ),
                )
                .into_any_element();
            let body = Rc::new(RefCell::new(Some(body)));
            dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title(mode.time_title()))
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
    /// The time box of suggested posts; above the price box, which it
    /// returns to.
    SuggestTime => DialogSpec::new(
        3960,
        |app| app.suggested_posts.time.is_some(),
        QuillApp::build_suggest_time_dialog,
    ),
    /// "Suggest a Message" / "Suggest Changes".
    SuggestPrice => DialogSpec::new(
        3968,
        |app| app.suggested_posts.price.is_some(),
        QuillApp::build_suggest_price_dialog,
    )
    .closes(|app, _, cx| app.close_suggest_price(cx)),
}
