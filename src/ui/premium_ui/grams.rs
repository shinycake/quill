//! Settings → My Grams: the Gram balance, the Fragment top-up link, the
//! withdrawal section and the history (tdesktop `Settings::Credits` with
//! `CreditsType::Ton`). The Stars screen's layout with the currency
//! switched: same history rows, tabs and receipt, amounts in Grams.
//!
//! Read-only: "Top-up via Fragment" opens Fragment in the browser through
//! the link policy; nothing here moves Grams.
#![deny(clippy::indexing_slicing)]

use super::sticker_tile;
use crate::ui::app::QuillApp;
use crate::ui::chat_theme::success;
use crate::ui::message_text::format_unix_date_time;
use crate::ui::shell::{DialogKind, QuillShell};
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::tab::{Tab, TabBar};
use gpui_kit::component::*;
use gpui_kit::*;
use quill::lang::{keys, tr, tr_with};
use quill::link_policy::{OpenDecision, open_decision};
use quill::premium_hub::TxFilter;
use quill::premium_hub::ton::{
    GramsHub, TOPUP_URL, TonTx, TonTxKind, gram_amount_2dp, gram_balance, gram_balance_short,
    signed_gram_amount, usd_line,
};
use quill::state::Session;
use quill::telegram::envelope::MessageSender;
use quill::text::LinkTarget;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

/// The Gram sign Quill draws next to amounts (the Money page uses it too).
const GRAM: &str = "\u{1F48E}";
/// Leading icon slot of a history row.
const ROW_ICON: f32 = 36.;

crate::ui::shell::register_dialogs! {
    /// Settings → My Grams.
    Grams => DialogSpec::new(
        3805,
        |app| app.session().is_some_and(|s| s.payments.hub.grams.open),
        QuillApp::build_grams_dialog,
    ),
}

fn tab_label(filter: TxFilter) -> &'static str {
    tr(match filter {
        TxFilter::All => keys::CREDITS_SUMMARY_HISTORY_TAB_FULL,
        TxFilter::Incoming => keys::CREDITS_SUMMARY_HISTORY_TAB_IN,
        TxFilter::Outgoing => keys::CREDITS_SUMMARY_HISTORY_TAB_OUT,
    })
}

/// The row headline: the type's title, or the peer's name.
fn row_title(session: &Session, tx: &TonTx) -> String {
    tx.title()
        .or_else(|| tx.peer().map(|peer| session.sender_label(peer)))
        .unwrap_or_else(|| tr(keys::Q_GRAMS_ENTRY_UNKNOWN).to_string())
}

fn muted_note(text: impl Into<SharedString>, cx: &App) -> Div {
    div()
        .text_sm()
        .text_center()
        .text_color(cx.theme().muted_foreground)
        .child(text.into())
}

fn fact_row(label: &str, value: String, cx: &App) -> Div {
    div()
        .flex()
        .justify_between()
        .gap_3()
        .text_sm()
        .child(
            div()
                .text_color(cx.theme().muted_foreground)
                .child(label.to_string()),
        )
        .child(div().font_medium().text_right().child(value))
}

impl QuillApp {
    fn grams_mut(&mut self) -> Option<&mut GramsHub> {
        if let Some(live) = self.live.as_mut() {
            Some(&mut live.driver.session.payments.hub.grams)
        } else {
            self.demo_session
                .as_mut()
                .map(|s| &mut s.payments.hub.grams)
        }
    }

    /// Settings → My Grams.
    pub(in crate::ui) fn open_grams(&mut self, cx: &mut Context<Self>) {
        let is_dark = cx.theme().is_dark();
        if let Some(live) = self.live.as_mut() {
            if live.driver.open_grams(is_dark).is_err() {
                self.connection.status_note = tr(keys::Q_GRAMS_COULD_NOT_LOAD).into();
            }
        } else if let Some(grams) = self.grams_mut() {
            grams.open = true;
        }
        cx.notify();
    }

    pub(in crate::ui) fn close_grams(&mut self, cx: &mut Context<Self>) {
        if let Some(grams) = self.grams_mut() {
            grams.open = false;
            grams.selected = None;
        }
        cx.notify();
    }

    fn set_grams_filter(&mut self, filter: TxFilter, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.set_grams_filter(filter);
        } else if let Some(grams) = self.grams_mut() {
            grams.filter = filter;
        }
        cx.notify();
    }

    pub(in crate::ui) fn select_grams_tx(&mut self, id: Option<String>, cx: &mut Context<Self>) {
        if let Some(grams) = self.grams_mut() {
            grams.selected = id;
        }
        cx.notify();
    }

    /// "Top-up via Fragment": the Fragment page in the browser, through the
    /// link policy (tdesktop `UrlClickHandler::Open`).
    fn grams_topup(&mut self, cx: &mut Context<Self>) {
        let link = LinkTarget::Url {
            url: TOPUP_URL.to_string(),
            label: None,
        };
        if let OpenDecision::Open = open_decision(&link) {
            self.open_message_url(TOPUP_URL, cx);
        }
    }

    /// The "My Grams" row under Stars in Settings: asks for the balance the
    /// first time Settings shows (tdesktop `tonLoad`) and appears while the
    /// balance is not zero, with the balance on the right.
    pub(in crate::ui) fn grams_settings_row(
        &mut self,
        on_open: impl Fn(&mut Window, &mut App) + 'static,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.probe_grams_balance();
        }
        let grams = &self.session()?.payments.hub.grams;
        if !grams.shows_settings_row() {
            return None;
        }
        let balance = gram_balance_short(grams.balance_nano.unwrap_or(0));
        Some(
            Button::new("settings-Grams")
                .label(tr(keys::SETTINGS_CURRENCY))
                .ghost()
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(format!("{GRAM} {balance}")),
                )
                .on_click(move |_, window, cx| on_open(window, cx))
                .into_any_element(),
        )
    }

    pub(in crate::ui) fn build_grams_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close = QuillShell::on_close_kind(app, shell, DialogKind::Grams, |this, _, cx| {
            this.close_grams(cx);
        });
        app.update(cx, |this, cx| {
            let dialog = dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title(tr(
                    keys::CREDITS_CURRENCY_SUMMARY_TITLE,
                )));
            let roots = this.media_display_roots();
            let Some(session) = this.session() else {
                return dialog.on_close(on_close);
            };
            let grams = &session.payments.hub.grams;
            let body = match grams.selected_tx() {
                Some(tx) => receipt(session, tx, &roots, cx),
                None => overview(session, grams, &roots, cx),
            };
            let body = Rc::new(RefCell::new(Some(body)));
            dialog
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

/// About, top-up, balance, withdrawal, then the history.
fn overview(
    session: &Session,
    grams: &GramsHub,
    roots: &[PathBuf],
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let usd_per_gram = session.ton_wallet.ton_usd_rate;
    let balance = grams.balance_nano.unwrap_or(0);
    let mut body = div().flex().flex_col().gap_3();
    body = body
        .child(muted_note(tr(keys::CREDITS_CURRENCY_SUMMARY_ABOUT), cx))
        .child(
            Button::new("grams-topup")
                .label(tr(keys::CREDITS_CURRENCY_SUMMARY_IN_BUTTON))
                .primary()
                .w_full()
                .on_click(cx.listener(|this, _, _, cx| this.grams_topup(cx))),
        )
        .child(
            div()
                .id("grams-balance")
                .flex()
                .flex_col()
                .items_center()
                .gap_0p5()
                .debug_selector(|| "grams-balance".to_string())
                .child(
                    div().text_lg().child(tr_with(
                        keys::CREDITS_BALANCE_ME_COUNT,
                        &[
                            ("emoji", GRAM),
                            (
                                "amount",
                                &grams
                                    .balance_nano
                                    .map_or("\u{2013}".to_string(), gram_balance),
                            ),
                        ],
                    )),
                )
                .children(usd_line(balance, usd_per_gram, 3).map(|usd| {
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(usd)
                })),
        )
        .child(muted_note(
            tr(keys::CREDITS_CURRENCY_SUMMARY_IN_SUBTITLE),
            cx,
        ));
    if let Some(revenue) = grams.withdrawable() {
        body = body.child(
            div()
                .id("grams-withdrawal")
                .debug_selector(|| "grams-withdrawal".to_string())
                .flex()
                .flex_col()
                .items_center()
                .gap_1()
                .pt_3()
                .border_t_1()
                .border_color(cx.theme().border)
                .child(div().text_size(px(28.)).font_semibold().child(format!(
                    "{GRAM} {}",
                    gram_amount_2dp(revenue.available_nano)
                )))
                .children(
                    usd_line(revenue.available_nano, revenue.usd_per_gram(), 2).map(|usd| {
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(usd)
                    }),
                )
                .child(muted_note(tr(keys::CREDITS_CURRENCY_SUMMARY_SUBTITLE), cx)),
        );
    }
    body = body.child(history(session, grams, roots, cx));
    body.into_any_element()
}

/// The tabs (or the "All Transactions" title), the rows and "Load more".
fn history(
    session: &Session,
    grams: &GramsHub,
    roots: &[PathBuf],
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let mut col = div().flex().flex_col().gap_1().pt_3();
    if let Some(error) = grams.error.as_ref() {
        col = col.child(div().text_sm().child(error.clone()));
    }
    // tdesktop shows no history section until it has rows.
    if grams.entries.is_empty() && !grams.shows_tabs() {
        if grams.loading {
            col = col.child(
                div()
                    .text_sm()
                    .text_color(muted)
                    .child(tr(keys::Q_MONEY_LOADING_TRANSACTIONS)),
            );
        }
        return col.into_any_element();
    }
    if grams.shows_tabs() {
        let owner = cx.entity().downgrade();
        let selected = TxFilter::ALL
            .iter()
            .position(|filter| *filter == grams.filter)
            .unwrap_or(0);
        col = col.child(
            TabBar::new("grams-tabs")
                .underline()
                .children(
                    TxFilter::ALL
                        .iter()
                        .map(|f| Tab::new().label(tab_label(*f))),
                )
                .selected_index(selected)
                .on_click(move |ix, _, cx| {
                    if let Some(filter) = TxFilter::ALL.get(*ix).copied() {
                        let _ = owner.update(cx, |this, cx| this.set_grams_filter(filter, cx));
                    }
                }),
        );
    } else {
        col = col.child(
            div()
                .text_sm()
                .font_semibold()
                .text_color(muted)
                .child(tab_label(TxFilter::All)),
        );
    }
    if grams.loading && grams.entries.is_empty() {
        col = col.child(
            div()
                .text_sm()
                .text_color(muted)
                .child(tr(keys::Q_MONEY_LOADING_TRANSACTIONS)),
        );
    }
    for tx in &grams.entries {
        col = col.child(tx_row(session, tx, roots, cx));
    }
    if grams.has_more() {
        col = col.child(
            Button::new("grams-load-more")
                .label(if grams.loading {
                    tr(keys::Q_MONEY_LOADING)
                } else {
                    tr(keys::Q_MONEY_LOAD_MORE)
                })
                .ghost()
                .disabled(grams.loading)
                .on_click(cx.listener(|this, _, _, cx| {
                    if let Some(live) = this.live.as_mut() {
                        let _ = live.driver.fetch_more_grams();
                    }
                    cx.notify();
                })),
        );
    }
    col.into_any_element()
}

/// The row's picture: the peer's photo, the sticker, or a glyph.
fn row_icon(session: &Session, tx: &TonTx, roots: &[PathBuf], side: f32) -> AnyElement {
    if let Some(peer) = tx.peer() {
        let photo = match peer {
            MessageSender::User { user_id } => session.user_photo_path(user_id),
            MessageSender::Chat { chat_id } => session.chat_photo_path(quill::ids::ChatId(chat_id)),
        }
        .and_then(|path| quill::local_path::sandboxed_display_path(path, roots));
        return crate::ui::chat_row::chat_avatar(
            &session.sender_label(peer),
            photo.as_deref(),
            side,
        )
        .into_any_element();
    }
    let fallback = match tx.kind {
        TonTxKind::GiftPurchaseOffer { .. }
        | TonTxKind::UpgradedGiftPurchase { .. }
        | TonTxKind::UpgradedGiftSale { .. } => "\u{1F381}",
        TonTxKind::StakeDiceStake | TonTxKind::StakeDicePayout => "\u{1F3B2}",
        _ => GRAM,
    };
    sticker_tile(
        ("grams-row-icon", tx_key(tx)),
        tx.sticker(),
        &session.media.files,
        roots,
        side,
        fallback,
    )
}

/// A stable element key from the transaction id.
fn tx_key(tx: &TonTx) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    tx.id.hash(&mut hasher);
    hasher.finish()
}

fn amount_color(tx: &TonTx, cx: &App) -> Hsla {
    if tx.is_incoming() {
        success().into()
    } else {
        cx.theme().foreground
    }
}

fn tx_row(
    session: &Session,
    tx: &TonTx,
    roots: &[PathBuf],
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let title = row_title(session, tx);
    let mut sub = Vec::new();
    let detail = tx.detail();
    if !detail.is_empty() {
        sub.push(detail);
    }
    sub.push(format_unix_date_time(i64::from(tx.date)));
    let amount = signed_gram_amount(tx.gram_nano);
    let id = tx.id.clone();
    div()
        .id(SharedString::from(format!("grams-tx-{}", tx.id)))
        .debug_selector({
            let id = tx.id.clone();
            move || format!("grams-tx-{id}")
        })
        .flex()
        .items_center()
        .gap_3()
        .py_1p5()
        .cursor_pointer()
        .role(gpui_kit::Role::Button)
        .aria_label(format!("{title}, {amount}"))
        .tab_index(0)
        .on_click(cx.listener(move |this, _, _, cx| this.select_grams_tx(Some(id.clone()), cx)))
        .child(row_icon(session, tx, roots, ROW_ICON))
        .child(
            div()
                .flex()
                .flex_col()
                .gap_0p5()
                .flex_1()
                .min_w_0()
                .child(div().text_sm().font_semibold().truncate().child(title))
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .truncate()
                        .child(sub.join(" \u{00B7} ")),
                ),
        )
        .child(
            div()
                .flex_none()
                .font_semibold()
                .text_color(amount_color(tx, cx))
                .child(format!("{amount} {GRAM}")),
        )
        .into_any_element()
}

/// The receipt: picture, amount, title and the facts (tdesktop
/// `ReceiptCreditsBox` for a Gram entry).
fn receipt(
    session: &Session,
    tx: &TonTx,
    roots: &[PathBuf],
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let mut col = div()
        .flex()
        .flex_col()
        .gap_2()
        .debug_selector(|| "grams-receipt".to_string())
        .child(
            Button::new("grams-back")
                .label(tr(keys::Q_MONEY_BACK_2))
                .ghost()
                .small()
                .on_click(cx.listener(|this, _, _, cx| this.select_grams_tx(None, cx))),
        );
    col = col.child(
        div()
            .flex()
            .flex_col()
            .items_center()
            .gap_1()
            .py_2()
            .child(match tx.gift() {
                // A gift deal shows the gift, as tdesktop's receipt does.
                Some(gift) => sticker_tile(
                    ("grams-receipt-gift", tx_key(tx)),
                    gift.sticker.as_ref(),
                    &session.media.files,
                    roots,
                    96.,
                    "\u{1F381}",
                ),
                None => row_icon(session, tx, roots, 72.),
            })
            .child(
                div()
                    .text_size(px(28.))
                    .font_semibold()
                    .text_color(amount_color(tx, cx))
                    .child(format!("{} {GRAM}", signed_gram_amount(tx.gram_nano))),
            )
            .child(div().font_semibold().child(row_title(session, tx))),
    );
    if let Some(peer) = tx.peer() {
        // The buyer of a sold gift, the seller of a bought one, the
        // channel of a suggested post.
        let label = match tx.kind {
            TonTxKind::UpgradedGiftSale { .. } => keys::CREDITS_BOX_HISTORY_ENTRY_GIFT_SOLD_TO,
            TonTxKind::UpgradedGiftPurchase { .. } => {
                keys::CREDITS_BOX_HISTORY_ENTRY_GIFT_BOUGHT_FROM
            }
            _ if tx.is_incoming() => keys::CREDITS_BOX_HISTORY_ENTRY_PEER_IN,
            _ => keys::CREDITS_BOX_HISTORY_ENTRY_PEER,
        };
        col = col.child(fact_row(tr(label), session.sender_label(peer), cx));
    }
    if matches!(
        tx.kind,
        TonTxKind::FragmentDeposit { .. } | TonTxKind::FragmentWithdrawal { .. }
    ) {
        col = col.child(fact_row(
            tr(keys::CREDITS_BOX_HISTORY_ENTRY_VIA),
            tr(keys::CREDITS_BOX_HISTORY_ENTRY_FRAGMENT).to_string(),
            cx,
        ));
    }
    if let TonTxKind::UpgradedGiftSale {
        commission_nano, ..
    } = tx.kind
        && commission_nano > 0
    {
        col = col.child(muted_note(
            tr_with(
                keys::CREDITS_COMMISSION,
                &[(
                    "amount",
                    &format!("{GRAM} {}", gram_amount_2dp(commission_nano)),
                )],
            ),
            cx,
        ));
    }
    let status = tx.detail();
    if matches!(tx.kind, TonTxKind::FragmentWithdrawal { .. }) && !status.is_empty() || tx.is_refund
    {
        col = col.child(fact_row(tr(keys::Q_MONEY_STATUS), status, cx));
    }
    col = col.child(fact_row(
        tr(keys::GIFT_RESALE_DATE),
        format_unix_date_time(i64::from(tx.date)),
        cx,
    ));
    let id = tx.id.clone();
    col.child(fact_row(
        tr(keys::CREDITS_BOX_HISTORY_ENTRY_ID),
        id.clone(),
        cx,
    ))
    .child(
        Button::new("grams-copy-id")
            .label(tr(keys::Q_MONEY_COPY_TRANSACTION_ID))
            .ghost()
            .small()
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.write_to_clipboard(ClipboardItem::new_string(id.clone()));
                this.connection.status_note = tr(keys::CREDITS_BOX_HISTORY_ENTRY_ID_COPIED).into();
                cx.notify();
            })),
    )
    .into_any_element()
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "grams_tests.rs"]
mod tests;
