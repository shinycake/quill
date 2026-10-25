//! "Gift Stars": a contact list, then the packs for the contact you picked
//! (tdesktop `boxes/gift_credits_box.cpp`, opened by "Send Stars" on the
//! Stars page). A pack opens the payment checkout
//! (`ui/invoice_checkout.rs`); nothing is paid until Pay there.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::link_info_boxes::finish;
use super::shell::{DialogKind, QuillShell};
use crate::ui::status_note::StatusNote;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::Input;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::lang::{keys, tr, tr_with};
use quill::premium_hub::group_digits;
use quill::stars_gift::can_gift_stars;
use quill::telegram::envelope::{ChatKind, format_payment_price};

fn title() -> &'static str {
    tr(keys::GIFT_STARS_SUBTITLE)
}
/// Rows drawn at once; the search narrows the rest.
const ROWS_MAX: usize = 100;

/// The "Gift Stars" button under the Stars dialog's packs.
pub(super) fn gift_stars_button(cx: &mut Context<QuillApp>) -> AnyElement {
    Button::new("stars-gift-open")
        .label(tr(keys::GIFT_STARS_SUBTITLE))
        .small()
        .ghost()
        .on_click(cx.listener(|this, _, window, cx| this.open_stars_gift_dialog(window, cx)))
        .into_any_element()
}

impl QuillApp {
    /// Open the Gift Stars dialog.
    pub(super) fn open_stars_gift_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.payments
            .pick_search
            .update(cx, |input, cx| input.set_value("", window, cx));
        let opened = self.live.as_mut().map(|live| live.driver.open_stars_gift());
        if !matches!(opened, Some(Ok(()))) {
            self.connection.set_note(StatusNote::error(tr(
                keys::Q_MONEY_COULD_NOT_OPEN_GIFT_STARS,
            )));
        }
        cx.notify();
    }

    fn close_stars_gift_dialog(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            live.driver.close_stars_gift();
        }
        cx.notify();
    }

    fn pick_stars_gift_user(
        &mut self,
        user_id: i64,
        name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.payments
            .pick_search
            .update(cx, |input, cx| input.set_value("", window, cx));
        let sent = self
            .live
            .as_mut()
            .map(|live| live.driver.choose_stars_gift_user(user_id, &name));
        if !matches!(sent, Some(Ok(_))) {
            self.connection
                .set_note(StatusNote::error(tr(keys::Q_MONEY_COULD_NOT_LOAD_THE_STAR)));
        }
        cx.notify();
    }

    fn stars_gift_back(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            live.driver.session.payments.stars_gift.clear_target();
        }
        cx.notify();
    }

    /// A pack for the picked contact opens the checkout.
    fn buy_gifted_stars(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let invoice = self.session().and_then(|s| {
            let gift = &s.payments.stars_gift;
            let pack = gift.options.get(index)?;
            quill::payment_invoice::gifted_stars_invoice(pack, gift.target.as_ref()?.user_id)
        });
        match invoice {
            Some(invoice) => self.start_invoice_checkout(invoice, window, cx),
            None => self.set_status_note(
                StatusNote::silent(tr(keys::Q_MONEY_THAT_STARS_OPTION_IS_NO)),
                cx,
            ),
        }
    }

    pub(super) fn build_stars_gift_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::StarsGift, |this, _, cx| {
                this.close_stars_gift_dialog(cx);
            });
        app.update(cx, |this, cx| {
            let muted = cx.theme().muted_foreground;
            let Some(session) = this.session() else {
                return dialog.overlay(true).on_close(on_close);
            };
            let gift = &session.payments.stars_gift;
            let mut body = div().flex().flex_col().gap_2();
            if let Some(target) = gift.target.as_ref() {
                body = body
                    .child(
                        Button::new("stars-gift-back")
                            .label(tr(keys::Q_MONEY_CONTACTS))
                            .ghost()
                            .small()
                            .on_click(cx.listener(|this, _, _, cx| this.stars_gift_back(cx))),
                    )
                    .child(div().text_sm().font_semibold().child(tr_with(
                        keys::Q_MONEY_CHOOSE_HOW_MANY_STARS_GETS,
                        &[("name", &target.name.to_string())],
                    )));
                if gift.loading {
                    body = body.child(
                        div()
                            .text_sm()
                            .text_color(muted)
                            .child(tr(keys::Q_MONEY_LOADING_OPTIONS)),
                    );
                } else if let Some(error) = gift.error.as_ref() {
                    body = body.child(div().text_sm().child(error.clone()));
                } else if gift.options.is_empty() {
                    body = body.child(
                        div()
                            .text_sm()
                            .text_color(muted)
                            .child(tr(keys::Q_MONEY_NO_OPTIONS_AVAILABLE)),
                    );
                }
                for (ix, pack) in gift.options.iter().enumerate() {
                    body = body.child(
                        div()
                            .id(("stars-gift-pack", ix as u64))
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap_3()
                            .px_2()
                            .py_1()
                            .rounded_md()
                            .bg(super::chat_theme::fill_muted())
                            .cursor_pointer()
                            .tab_index(0)
                            .role(gpui_kit::Role::Button)
                            .aria_label(tr_with(
                                keys::Q_LEFTOVERS_GIFT_STARS_FOR,
                                &[
                                    ("count", &(group_digits(pack.star_count)).to_string()),
                                    (
                                        "price",
                                        &(format_payment_price(&pack.currency, pack.amount))
                                            .to_string(),
                                    ),
                                ],
                            ))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.buy_gifted_stars(ix, window, cx);
                            }))
                            .child(
                                div()
                                    .text_sm()
                                    .font_medium()
                                    .child(format!("\u{2B50} {}", group_digits(pack.star_count))),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(muted)
                                    .child(format_payment_price(&pack.currency, pack.amount)),
                            ),
                    );
                }
                body = body.child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child(tr(keys::Q_MONEY_YOU_PAY_IN_THE_CHECKOUT)),
                );
            } else {
                body = body.child(
                    Input::new(&this.payments.pick_search).aria_label(tr(keys::Q_MONEY_SEARCH)),
                );
                let query = this
                    .payments
                    .pick_search
                    .read(cx)
                    .value()
                    .trim()
                    .to_lowercase();
                let eligible = |id: i64| {
                    session
                        .users
                        .get(&id)
                        .filter(|u| can_gift_stars(u.id, session.my_user_id, u.is_bot))
                        .map(|u| (u.id, u.display_name()))
                };
                // tdesktop lists the frequent contacts first and leaves
                // them out of the contacts below, until a search starts.
                let frequent: Vec<(i64, String)> =
                    if query.is_empty() && !session.search.search.top_chats_disabled {
                        session
                            .search
                            .search
                            .top_chats
                            .iter()
                            .filter_map(|chat| match session.chats.get(&chat.0)?.kind {
                                ChatKind::Private { user_id } => eligible(user_id.0),
                                _ => None,
                            })
                            .collect()
                    } else {
                        Vec::new()
                    };
                let mut people: Vec<(i64, String)> = session
                    .users_state
                    .contacts
                    .iter()
                    .flatten()
                    .filter_map(|id| eligible(*id))
                    .filter(|(id, name)| {
                        !frequent.iter().any(|(f, _)| f == id)
                            && (query.is_empty() || name.to_lowercase().contains(&query))
                    })
                    .collect();
                people.sort_by_key(|(_, name)| name.to_lowercase());
                if !frequent.is_empty() {
                    body = body.child(
                        div()
                            .text_xs()
                            .font_semibold()
                            .text_color(muted)
                            .child(tr(keys::Q_MONEY_FREQUENT_CONTACTS)),
                    );
                    for (id, name) in frequent {
                        let label = name.clone();
                        body = body.child(
                            Button::new(("stars-gift-frequent", id.unsigned_abs()))
                                .label(label)
                                .ghost()
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.pick_stars_gift_user(id, name.clone(), window, cx);
                                })),
                        );
                    }
                    body = body.child(
                        div()
                            .text_xs()
                            .font_semibold()
                            .text_color(muted)
                            .child(tr(keys::MENU_CONTACTS)),
                    );
                }
                for (id, name) in people.into_iter().take(ROWS_MAX) {
                    let label = name.clone();
                    body = body.child(
                        Button::new(("stars-gift-user", id.unsigned_abs()))
                            .label(label)
                            .ghost()
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.pick_stars_gift_user(id, name.clone(), window, cx);
                            })),
                    );
                }
            }
            let footer = div().flex().justify_end().child(
                Button::new("stars-gift-close")
                    .label(tr(keys::CLOSE))
                    .ghost()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.close_stars_gift_dialog(cx);
                        this.close_kit_dialog_if_done(DialogKind::StarsGift, window, cx);
                    })),
            );
            finish(
                dialog,
                title(),
                body.into_any_element(),
                footer.into_any_element(),
                on_close,
            )
        })
    }
}

crate::ui::shell::register_dialogs! {
    /// "Gift Stars": pick a contact, then a pack for them.
    StarsGift => DialogSpec::new(
        3750,
        |app| app.session().is_some_and(|s| s.payments.stars_gift.open),
        QuillApp::build_stars_gift_dialog,
    )
    .closes(|app, _, cx| app.close_stars_gift_dialog(cx)),
}
