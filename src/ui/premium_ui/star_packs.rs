//! "Buy more Stars" in the Stars dialog: the packs `getStarPaymentOptions`
//! lists (tdesktop `settings_credits.cpp`, `Ui::AddCreditsOptions`).
//!
//! Telegram Desktop buys a pack with `inputInvoiceTelegram` and
//! `telegramPaymentPurposeStars`. Quill's checkout only handles invoices in
//! chat messages, so the packs are shown but not bought here.

use super::*;
use quill::star_packs::StarPacksState;
use quill::telegram::envelope::format_payment_price;

impl QuillApp {
    fn packs_mut(&mut self) -> Option<&mut StarPacksState> {
        if let Some(live) = self.live.as_mut() {
            Some(&mut live.driver.session.payments.star_packs)
        } else {
            self.demo_session
                .as_mut()
                .map(|s| &mut s.payments.star_packs)
        }
    }

    pub(super) fn toggle_star_packs(&mut self, cx: &mut Context<Self>) {
        let open = match self.packs_mut() {
            Some(packs) => {
                packs.open = !packs.open;
                packs.open
            }
            None => false,
        };
        if open && let Some(live) = self.live.as_mut() {
            let _ = live.driver.maybe_fetch_star_packs();
        }
        cx.notify();
    }
}

/// The "Buy more Stars" button and, when open, the pack list.
pub(super) fn star_packs_section(session: &Session, cx: &mut Context<QuillApp>) -> AnyElement {
    let packs = &session.payments.star_packs;
    let muted = cx.theme().muted_foreground;
    let mut section = div().flex().flex_col().gap_2().child(
        Button::new("stars-buy-more")
            .label("Buy more Stars")
            .small()
            .when(packs.open, |b| b.primary())
            .when(!packs.open, |b| b.ghost())
            .on_click(cx.listener(|this, _, _, cx| this.toggle_star_packs(cx))),
    );
    if !packs.open {
        return section.into_any_element();
    }
    if packs.loading {
        section = section.child(
            div()
                .text_sm()
                .text_color(muted)
                .child("Loading options\u{2026}"),
        );
    } else if let Some(error) = packs.error.as_ref() {
        section = section.child(div().text_sm().child(error.clone()));
    } else if packs.options.is_empty() && packs.loaded {
        section = section.child(
            div()
                .text_sm()
                .text_color(muted)
                .child("No options available."),
        );
    }
    for (ix, pack) in packs.options.iter().enumerate() {
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
