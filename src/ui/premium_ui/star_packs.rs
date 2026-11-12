//! "Buy more Stars" in the Stars dialog: the packs `getStarPaymentOptions`
//! lists (tdesktop `settings_credits.cpp`, `Ui::AddCreditsOptions`).
//!
//! Telegram Desktop buys a pack with `inputInvoiceTelegram` and
//! `telegramPaymentPurposeStars`. A press on a pack opens the payment
//! checkout (`ui/invoice_checkout.rs`); nothing is paid until Pay there.

use super::*;
use quill::lang::{keys, tr, tr_with};
use quill::telegram::envelope::format_payment_price;

impl QuillApp {
    pub(super) fn toggle_star_packs(&mut self, cx: &mut Context<Self>) {
        let open = self.hub_mut().is_some_and(|hub| hub.stars.toggle_packs());
        if open && let Some(live) = self.live.as_mut() {
            let _ = live.driver.maybe_fetch_star_packs();
        }
        cx.notify();
    }
}

/// The "Buy more Stars" button and, when open, the pack list.
pub(super) fn star_packs_section(session: &Session, cx: &mut Context<QuillApp>) -> AnyElement {
    let packs = &session.payments.star_packs;
    let expanded = session.payments.hub.stars.packs_expanded();
    let muted = cx.theme().muted_foreground;
    let mut section = div().flex().flex_col().gap_2().child(
        Button::new("stars-buy-more")
            .label(tr(keys::Q_MONEY_BUY_MORE_STARS))
            .small()
            .when(expanded, |b| b.primary())
            .when(!expanded, |b| b.ghost())
            .on_click(cx.listener(|this, _, _, cx| this.toggle_star_packs(cx))),
    );
    if !expanded {
        return section.into_any_element();
    }
    if packs.options.is_loading() {
        section = section.child(
            div()
                .text_sm()
                .text_color(muted)
                .child(tr(keys::Q_MONEY_LOADING_OPTIONS)),
        );
    } else if let Some(error) = packs.options.error() {
        section = section.child(div().text_sm().child(error.to_string()));
    } else if packs.options.items().is_empty() && !packs.options.is_idle() {
        section = section.child(
            div()
                .text_sm()
                .text_color(muted)
                .child(tr(keys::Q_MONEY_NO_OPTIONS_AVAILABLE)),
        );
    }
    for (ix, pack) in packs.options.items().iter().enumerate() {
        section = section.child(
            div()
                .id(("stars-pack", ix as u64))
                .flex()
                .items_center()
                .justify_between()
                .gap_3()
                .px_2()
                .py_1()
                .rounded_md()
                .bg(fill_muted())
                .cursor_pointer()
                .tab_index(0)
                .role(gpui_kit::Role::Button)
                .aria_label(tr_with(
                    keys::Q_MONEY_BUY_STARS_FOR,
                    &[
                        ("star_count", &(group_digits(pack.star_count)).to_string()),
                        (
                            "amount",
                            &(format_payment_price(&pack.currency, pack.amount)).to_string(),
                        ),
                    ],
                ))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.buy_stars_pack(ix, window, cx);
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
    section.into_any_element()
}
