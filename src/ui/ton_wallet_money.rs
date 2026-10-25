//! The Money page: the own TON wallet, read-only. Telegram Desktop has no
//! wallet, so this follows the official iOS app's `WalletScreen` (balance
//! with its fiat value, Transactions and Collectibles tabs, the Currency
//! choice) and its Settings row "Money", minus everything that acts: no
//! Send, no Receive QR flow, no Add Funds, no Keys & Backup actions. The
//! address, the backup status and a note saying where to send come in its
//! place. The details views are in `ton_wallet_details.rs`.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::chat_theme::success;
use super::message_text::format_unix_date_time;
use super::shell::{DialogKind, QuillShell};
use super::wallet_notice::notice_button;
use crate::ui::status_note::StatusNote;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::list::ListItem;
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::tab::{Tab, TabBar};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::lang::{keys, tr, tr_with};
use quill::state::Session;
use quill::ton_wallet::{
    MoneyTab, TonWalletState, WalletNft, WalletSurface, WalletTx, fiat, format_gram_amount_with,
    own,
};
use std::cell::RefCell;
use std::rc::Rc;

impl QuillApp {
    pub(super) fn ton_wallet_mut(&mut self) -> Option<&mut TonWalletState> {
        if let Some(live) = self.live.as_mut() {
            Some(&mut live.driver.session.ton_wallet)
        } else {
            self.demo_session.as_mut().map(|s| &mut s.ton_wallet)
        }
    }

    /// Settings → Money.
    pub(super) fn open_money(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            if live.driver.open_money_page().is_err() {
                self.connection
                    .set_note(StatusNote::error(tr(keys::Q_MONEY_COULD_NOT_OPEN_MONEY)));
            }
        } else if let Some(wallet) = self.ton_wallet_mut()
            && wallet.can_use
        {
            wallet.money.open = true;
        }
        cx.notify();
    }

    pub(super) fn close_money(&mut self, cx: &mut Context<Self>) {
        if self
            .ton_wallet_mut()
            .is_some_and(|wallet| wallet.money.hint_open)
        {
            self.dismiss_first_grams_hint(cx);
        }
        if let Some(live) = self.live.as_mut() {
            live.driver.close_money_page();
        } else if let Some(wallet) = self.ton_wallet_mut() {
            wallet.money = Default::default();
        }
        cx.notify();
    }

    pub(super) fn set_money_tab(&mut self, tab: MoneyTab, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.set_money_tab(tab);
        } else if let Some(wallet) = self.ton_wallet_mut() {
            wallet.money.tab = tab;
        }
        cx.notify();
    }

    fn load_more_money(&mut self, tab: MoneyTab, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            let _ = match tab {
                MoneyTab::Transactions => live.driver.fetch_wallet_history(true),
                MoneyTab::Collectibles => live.driver.fetch_wallet_nfts(true),
            };
        }
        cx.notify();
    }

    pub(super) fn select_wallet_tx(&mut self, tx: WalletTx, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            live.driver.session.show_wallet_transaction(tx);
        } else if let Some(session) = self.demo_session.as_mut() {
            session.show_wallet_transaction(tx);
        }
        cx.notify();
    }

    pub(super) fn select_wallet_nft(&mut self, address: Option<String>, cx: &mut Context<Self>) {
        if let Some(wallet) = self.ton_wallet_mut() {
            wallet.money.selected_nft = address;
            wallet.money.selected_tx = None;
        }
        cx.notify();
    }

    pub(super) fn money_back(&mut self, cx: &mut Context<Self>) {
        if let Some(wallet) = self.ton_wallet_mut() {
            wallet.money.selected_tx = None;
            wallet.money.selected_nft = None;
            wallet.money.tx_error = None;
            wallet.money.tx_loading = false;
        }
        cx.notify();
    }

    /// "Got it" on "Your first Grams!".
    pub(super) fn dismiss_first_grams_hint(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.dismiss_first_transfer_hint();
        } else if let Some(session) = self.demo_session.as_mut() {
            session.ton_wallet.money.hint_open = false;
            session
                .chat_list
                .suggestions
                .actions
                .remove(quill::ton_wallet::FIRST_TRANSFER_HINT);
        }
        cx.notify();
    }

    fn set_money_currency(&mut self, code: &'static str, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            if live.driver.set_wallet_currency(code).is_err() {
                self.connection.set_note(StatusNote::error(tr(
                    keys::Q_MONEY_COULD_NOT_SAVE_THE_CURRENCY,
                )));
            }
        } else if let Some(wallet) = self.ton_wallet_mut() {
            wallet.currency = Some(code.to_string());
        }
        cx.notify();
    }

    /// The Settings row: "Money" with the balance, only while the wallet is
    /// on (iOS `PeerInfoSettingsItems`, shown when `WalletConfiguration`
    /// is available; the balance shows above 0.01 Gram).
    pub(super) fn money_settings_row(
        &self,
        on_open: impl Fn(&mut Window, &mut App) + 'static,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let wallet = &self.session()?.ton_wallet;
        if !wallet.can_use {
            return None;
        }
        let balance = wallet
            .own
            .as_ref()
            .filter(|own| own.gram_amount > 10_000_000)
            .map(|own| settings_balance(own.gram_amount));
        Some(
            Button::new("settings-Money")
                .label(tr(keys::Q_MONEY_MONEY))
                .ghost()
                .when_some(balance, |button, balance| {
                    button.child(
                        div()
                            .ml_auto()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(balance),
                    )
                })
                .on_click(move |_, window, cx| on_open(window, cx))
                .into_any_element(),
        )
    }

    pub(super) fn build_money_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close = QuillShell::on_close_kind(app, shell, DialogKind::Money, |this, _, cx| {
            this.close_money(cx);
        });
        app.update(cx, |this, cx| {
            let dialog = dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title(tr(keys::Q_MONEY_MONEY)));
            let roots = this.media_display_roots();
            let Some(session) = this.session() else {
                return dialog.on_close(on_close);
            };
            let money = &session.ton_wallet.money;
            let body = if money.hint_open {
                super::ton_wallet_details::first_grams_hint(&session.ton_wallet, cx)
            } else if let Some(tx) = money.selected_tx.as_ref() {
                super::ton_wallet_details::tx_details(session, tx, &roots, cx)
            } else if let Some(nft) = money.selected_nft() {
                super::ton_wallet_details::nft_details(session, nft, &roots, cx)
            } else if money.tx_loading || money.tx_error.is_some() {
                super::ton_wallet_details::tx_lookup_state(money.tx_error.as_deref(), cx)
            } else {
                overview(session, &roots, cx)
            };
            dialog
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
                .on_close(on_close)
        })
    }
}

/// The Settings row's balance: two decimals at least, as iOS pads it.
fn settings_balance(nanos: i64) -> String {
    let mut text = format_gram_amount_with(nanos, 2);
    match text.split_once('.') {
        Some((_, fraction)) if fraction.len() < 2 => text.push('0'),
        Some(_) => {}
        None => text.push_str(".00"),
    }
    format!("\u{1F48E} {text}")
}

/// Balance, fiat value, address, backup status, the note, then the tabs.
fn overview(
    session: &Session,
    roots: &[std::path::PathBuf],
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let wallet = &session.ton_wallet;
    let money = &wallet.money;
    let muted = cx.theme().muted_foreground;
    let mut body = div().flex().flex_col().gap_3();
    let own = wallet.own.as_ref();
    let balance = own.map(|own| own.gram_amount);
    body = body.child(
        div()
            .id("money-balance")
            .flex()
            .flex_col()
            .items_center()
            .gap_1()
            .py_2()
            .role(gpui_kit::Role::Group)
            .aria_label(tr_with(
                keys::Q_MONEY_BALANCE_GRAMS,
                &[
                    (
                        "unwrap_or",
                        &(format_gram_amount_with(balance.unwrap_or(0), 2)).to_string(),
                    ),
                    (
                        "unwrap_or_2",
                        &(wallet.fiat_text(balance.unwrap_or(0))).to_string(),
                    ),
                ],
            ))
            .debug_selector(|| "money-balance".to_string())
            .child(div().text_size(px(30.)).font_semibold().child(format!(
                "\u{1F48E} {}",
                balance.map_or("\u{2013}".to_string(), |b| format_gram_amount_with(b, 2))
            )))
            .child(
                div()
                    .text_sm()
                    .text_color(muted)
                    .debug_selector(|| "money-fiat".to_string())
                    .child(match balance {
                        Some(nanos) => wallet.fiat_text(nanos),
                        None => "\u{2014}".to_string(),
                    }),
            ),
    );
    let mut actions: Vec<(&'static str, &'static str, WalletSurface)> = Vec::new();
    match own {
        Some(own) if own.exists() => {
            actions.extend([
                (
                    "money-send",
                    tr(keys::WALLET_FIRST_SEND_TITLE),
                    WalletSurface::Send,
                ),
                (
                    "money-receive",
                    tr(keys::Q_MONEY_RECEIVE),
                    WalletSurface::Receive,
                ),
                (
                    "money-add-funds",
                    tr(keys::WALLET_ADD_FUNDS),
                    WalletSurface::AddFunds,
                ),
                (
                    "money-backup",
                    tr(keys::WALLET_KEYS_TITLE),
                    WalletSurface::Backup,
                ),
            ]);
            body = body.child(super::ton_wallet_details::address_block(
                "money-address",
                tr(keys::Q_MONEY_YOUR_ADDRESS),
                &own.address,
                cx,
            ));
            body = body.child(super::ton_wallet_details::facts([(
                tr(keys::Q_MONEY_ENCRYPTED_BACKUP).to_string(),
                if own.is_backup_enabled {
                    tr(keys::Q_MONEY_ON)
                } else {
                    tr(keys::Q_MONEY_OFF)
                }
                .to_string(),
            )]));
        }
        Some(own) if own.is_being_created => {
            body = body.child(
                div()
                    .text_sm()
                    .text_color(muted)
                    .child(tr(keys::Q_MONEY_YOUR_WALLET_IS_BEING_CREATED)),
            );
        }
        Some(_) => {
            actions.push((
                "money-create",
                tr(keys::WALLET_REPLACE_CREATE),
                WalletSurface::CreateWallet,
            ));
            body = body.child(
                div()
                    .text_sm()
                    .text_color(muted)
                    .child(tr(keys::Q_MONEY_YOU_DONT_HAVE_A_WALLET)),
            );
        }
        None => {
            body = body.child(
                div()
                    .text_sm()
                    .text_color(muted)
                    .child(tr(keys::Q_MONEY_LOADING)),
            );
        }
    }
    body = body.child(currency_row(wallet, cx));
    if !actions.is_empty() {
        body = body.child(
            div().flex().flex_wrap().gap_2().children(
                actions.into_iter().map(|(id, label, surface)| {
                    notice_button(id, id.to_string(), label, surface, cx)
                }),
            ),
        );
    }
    let tabs = [MoneyTab::Transactions, MoneyTab::Collectibles];
    let owner = cx.entity().downgrade();
    let tabs = TabBar::new("money-tabs")
        .underline()
        .children(tabs.iter().map(|tab| {
            Tab::new().label(match tab {
                MoneyTab::Transactions => tr(keys::Q_MONEY_TRANSACTIONS),
                MoneyTab::Collectibles => tr(keys::Q_MONEY_COLLECTIBLES),
            })
        }))
        .selected_index(tabs.iter().position(|tab| *tab == money.tab).unwrap_or(0))
        .on_click(move |ix, _, cx| {
            if let Some(tab) = tabs.get(*ix).copied() {
                let _ = owner.update(cx, |this, cx| this.set_money_tab(tab, cx));
            }
        });
    body = body.child(tabs);
    let (loading, loaded, error, empty, more) = match money.tab {
        MoneyTab::Transactions => (
            money.history.loading,
            money.history.loaded,
            money.history.error.clone(),
            money.history.items.is_empty(),
            money.history.has_more(),
        ),
        MoneyTab::Collectibles => (
            money.nfts.loading,
            money.nfts.loaded,
            money.nfts.error.clone(),
            money.nfts.items.is_empty(),
            money.nfts.has_more(),
        ),
    };
    if let Some(error) = error {
        body = body.child(div().text_sm().child(error));
    }
    if loading && empty {
        body = body.child(
            div()
                .text_sm()
                .text_color(muted)
                .child(tr(keys::Q_MONEY_LOADING)),
        );
    } else if loaded && empty {
        body = body.child(div().text_sm().text_color(muted).child(match money.tab {
            MoneyTab::Transactions => tr(keys::Q_MONEY_NO_TRANSACTIONS_YET),
            MoneyTab::Collectibles => tr(keys::Q_MONEY_NO_COLLECTIBLES_YET),
        }));
    }
    match money.tab {
        MoneyTab::Transactions => {
            for (ix, tx) in money.history.items.iter().enumerate() {
                body = body.child(tx_row(session, ix, tx, cx));
            }
        }
        MoneyTab::Collectibles => {
            for (ix, nft) in money.nfts.items.iter().enumerate() {
                body = body.child(nft_row(session, ix, nft, roots, cx));
            }
        }
    }
    if more {
        let tab = money.tab;
        body = body.child(
            Button::new("money-load-more")
                .label(if loading {
                    tr(keys::Q_MONEY_LOADING)
                } else {
                    tr(keys::Q_MONEY_LOAD_MORE)
                })
                .ghost()
                .disabled(loading)
                .on_click(cx.listener(move |this, _, _, cx| this.load_more_money(tab, cx))),
        );
    }
    body.into_any_element()
}

/// "Currency" with the chosen code; the menu lists the currencies the
/// rates cover (iOS `Wallet.Currency`).
fn currency_row(wallet: &TonWalletState, cx: &mut Context<QuillApp>) -> AnyElement {
    let current = wallet.currency().to_string();
    let codes = fiat::available_currencies(&wallet.rates);
    let owner = cx.entity().downgrade();
    let label = tr_with(keys::Q_MONEY_CURRENCY, &[("current", &current.to_string())]);
    let button = Button::new("money-currency")
        .label(label)
        .ghost()
        .small()
        .disabled(codes.is_empty());
    let button = if codes.is_empty() {
        button.into_any_element()
    } else {
        button
            .dropdown_menu(move |mut menu, _, _| {
                menu = menu.scrollable(true).max_h(px(320.));
                for code in &codes {
                    let code = *code;
                    let owner = owner.clone();
                    menu = menu.item(PopupMenuItem::new(code).checked(code == current).on_click(
                        move |_, _, cx| {
                            let _ = owner.update(cx, |this, cx| this.set_money_currency(code, cx));
                        },
                    ));
                }
                menu
            })
            .into_any_element()
    };
    div().flex().child(button).into_any_element()
}

/// The title of a transaction row: the user's name, the domain, the
/// shortened address, or the top-up kind (iOS `WalletTransactionItemComponent`).
pub(super) fn tx_title(session: &Session, tx: &WalletTx) -> String {
    if let own::TxKind::OnRampDeposit { provider, .. } = &tx.kind {
        return if provider == "MoonPay" {
            tr(keys::Q_MONEY_CARD_TOP_UP).into()
        } else {
            tr(keys::Q_MONEY_CRYPTO_TOP_UP).into()
        };
    }
    if tx.peer_user_id != 0
        && let Some(user) = session.user(tx.peer_user_id)
    {
        let name = format!("{} {}", user.first_name, user.last_name);
        let name = name.trim();
        if !name.is_empty() {
            return name.to_string();
        }
    }
    own::counterparty(&tx.peer_address, &tx.peer_domain)
}

fn tx_row(session: &Session, ix: usize, tx: &WalletTx, cx: &mut Context<QuillApp>) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let title = tx_title(session, tx);
    let subtitle = tx.subtitle();
    let amount = tx.amount_text();
    let color = if tx.failed() {
        cx.theme().danger
    } else if tx.is_incoming() {
        success().into()
    } else {
        cx.theme().foreground
    };
    let mut sub = Vec::new();
    if !subtitle.is_empty() {
        sub.push(subtitle);
    }
    sub.push(format_unix_date_time(i64::from(tx.date)));
    let selected = tx.clone();
    ListItem::new(SharedString::from(format!("money-tx-{}", tx.id)))
        .rounded_md()
        .px_1()
        .accessibility_label(format!("{title}, {amount}"))
        .debug_selector(move || format!("money-tx-{ix}"))
        .on_click(cx.listener(move |this, _, _, cx| {
            this.select_wallet_tx(selected.clone(), cx);
        }))
        .child(
            v_flex()
                .gap_0p5()
                .min_w_0()
                .child(div().text_sm().font_semibold().truncate().child(title))
                .child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child(sub.join(" \u{00B7} ")),
                ),
        )
        .suffix(move |_, _| {
            div()
                .flex_none()
                .font_semibold()
                .text_color(color)
                .child(amount.clone())
        })
        .into_any_element()
}

fn nft_row(
    session: &Session,
    ix: usize,
    nft: &WalletNft,
    roots: &[std::path::PathBuf],
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let address = nft.address.clone();
    let title = nft.title().to_string();
    ListItem::new(SharedString::from(format!("money-nft-{}", nft.address)))
        .rounded_md()
        .px_1()
        .accessibility_label(title.clone())
        .debug_selector(move || format!("money-nft-{ix}"))
        .on_click(cx.listener(move |this, _, _, cx| {
            this.select_wallet_nft(Some(address.clone()), cx);
        }))
        .child(
            h_flex()
                .gap_3()
                .min_w_0()
                .child(super::ton_wallet_details::nft_thumb(
                    SharedString::from(format!("money-nft-thumb-{}", nft.address)),
                    session,
                    nft,
                    roots,
                    40.,
                    cx,
                ))
                .child(
                    v_flex()
                        .gap_0p5()
                        .min_w_0()
                        .child(div().text_sm().font_semibold().truncate().child(title))
                        .when(!nft.index.is_empty(), |col| {
                            col.child(
                                div()
                                    .text_xs()
                                    .text_color(muted)
                                    .child(format!("#{}", nft.index)),
                            )
                        }),
                ),
        )
        .into_any_element()
}

crate::ui::shell::register_dialogs! {
    /// Settings → Money: the own TON wallet, read-only.
    Money => DialogSpec::new(
        3810,
        |app| app.session().is_some_and(|s| s.ton_wallet.can_use && s.ton_wallet.money.open),
        QuillApp::build_money_dialog,
    )
    .closes(|app, _, cx| app.close_money(cx)),
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "ton_wallet_money_tests.rs"]
mod tests;
