//! The Money page's detail views: one transaction, one collectible, and
//! "Your first Grams!" (iOS `WalletTransactionScreen`,
//! `WalletNftScreen`, `WalletFirstGramsScreen`). All read-only: no Send,
//! no Trade, no decrypting a comment.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::message_media::{MediaCorners, spoiler_surface};
use super::message_text::{MONO_FONT, format_unix_date_time};
use super::wallet_notice::notice_button;
use gpui_kit::component::button::*;
use gpui_kit::component::description_list::DescriptionList;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::lang::{keys, tr, tr_with};
use quill::local_path::sandboxed_display_path;
use quill::state::Session;
use quill::ton_wallet::{
    TonWalletState, WalletNft, WalletSurface, WalletTx, format_gram_amount_with, group_address,
    own, transaction_explorer_url,
};
use std::path::PathBuf;

/// One Gram in nanograms, for the hint's rate line.
const ONE_GRAM: i64 = 1_000_000_000;

/// Label and value pairs as a kit description list (one column).
pub(super) fn facts(items: impl IntoIterator<Item = (String, String)>) -> DescriptionList {
    items.into_iter().fold(
        DescriptionList::horizontal()
            .columns(1)
            .bordered(false)
            .small()
            .label_width(px(140.)),
        |list, (label, value)| list.item(SharedString::from(label), SharedString::from(value), 1),
    )
}

fn back_button(cx: &mut Context<QuillApp>) -> AnyElement {
    Button::new("money-back")
        .label(tr(keys::Q_MONEY_BACK_2))
        .ghost()
        .small()
        .on_click(cx.listener(|this, _, _, cx| this.money_back(cx)))
        .into_any_element()
}

/// An address in groups of four with Copy Address and View in Explorer.
pub(super) fn address_block(
    id: &'static str,
    label: &'static str,
    address: &str,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let copy = address.to_string();
    let open = address.to_string();
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(label),
        )
        .child(
            div()
                .text_sm()
                .font_family(MONO_FONT)
                .debug_selector(move || id.to_string())
                .child(group_address(address)),
        )
        .child(
            div()
                .flex()
                .gap_1()
                .child(
                    Button::new((id, 1u64))
                        .label(tr(keys::Q_MONEY_COPY_ADDRESS))
                        .ghost()
                        .small()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.copy_ton_address(copy.clone(), cx);
                        })),
                )
                .child(
                    Button::new((id, 2u64))
                        .label(tr(keys::Q_MONEY_VIEW_IN_EXPLORER))
                        .ghost()
                        .small()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.open_ton_explorer(&open, cx);
                        })),
                ),
        )
        .into_any_element()
}

/// The collectible's picture, or a gem while it downloads.
pub(super) fn nft_thumb(
    id: impl Into<ElementId>,
    session: &Session,
    nft: &WalletNft,
    roots: &[PathBuf],
    side: f32,
    cx: &App,
) -> AnyElement {
    let frame = div()
        .id(id)
        .size(px(side))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded_md()
        .overflow_hidden()
        .bg(cx.theme().secondary);
    let path = nft.thumbnail.as_ref().and_then(|thumb| {
        let file = session.media.files.get(&thumb.id.0).unwrap_or(thumb);
        file.usable_path()
            .and_then(|path| sandboxed_display_path(path, roots))
    });
    match path {
        Some(path) => frame
            .child(
                img(super::image_budget::sized_media(
                    &path,
                    (px(side), px(side)),
                    None,
                    super::image_budget::Fit::Cover,
                ))
                .size(px(side))
                .object_fit(ObjectFit::Cover),
            )
            .into_any_element(),
        None => frame
            .text_size(px(side * 0.45))
            .child("\u{1F48E}")
            .into_any_element(),
    }
}

/// While a transfer card's transaction loads, or why it didn't.
pub(super) fn tx_lookup_state(error: Option<&str>, cx: &mut Context<QuillApp>) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap_2()
        .child(back_button(cx))
        .child(
            div()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(error.unwrap_or(tr(keys::Q_MONEY_LOADING)).to_string()),
        )
        .into_any_element()
}

/// One transaction: the amount, the comment, who it was with, the fee and
/// the date, then View in Explorer (iOS `WalletTransactionScreen`).
pub(super) fn tx_details(
    session: &Session,
    tx: &WalletTx,
    roots: &[PathBuf],
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let wallet = &session.ton_wallet;
    let muted = cx.theme().muted_foreground;
    let mut col = div()
        .flex()
        .flex_col()
        .gap_2()
        .debug_selector(|| "money-tx-details".to_string())
        .child(back_button(cx));
    if let own::TxKind::NftTransfer { nft, .. } = &tx.kind {
        col = col.child(div().flex().justify_center().child(nft_thumb(
            "money-tx-nft",
            session,
            nft,
            roots,
            96.,
            cx,
        )));
        col = col.child(
            div()
                .text_center()
                .font_semibold()
                .child(nft.title().to_string()),
        );
    }
    let amount = tx.amount_text();
    let color = if tx.failed() {
        cx.theme().danger
    } else if tx.is_incoming() {
        super::chat_theme::success().into()
    } else {
        cx.theme().foreground
    };
    if !amount.is_empty() {
        col = col.child(
            div()
                .text_center()
                .text_size(px(26.))
                .font_semibold()
                .text_color(color)
                .child(amount),
        );
    }
    let gram = tx.gram_amount();
    if gram != 0 {
        col = col.child(
            div()
                .text_center()
                .text_sm()
                .text_color(muted)
                .child(wallet.fiat_text(gram.saturating_abs())),
        );
    }
    if tx.failed() {
        col = col.child(
            div()
                .text_center()
                .text_sm()
                .text_color(cx.theme().danger)
                .child(tr(keys::Q_MONEY_FAILED)),
        );
    }
    if let Some(comment) = tx.public_comment() {
        col = col
            .child(
                div()
                    .text_sm()
                    .p_2()
                    .rounded_md()
                    .bg(cx.theme().secondary)
                    .debug_selector(|| "money-tx-comment".to_string())
                    .child(comment.to_string()),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child(tr(keys::Q_MONEY_THE_COMMENT_IS_VISIBLE_TO)),
            );
    } else if tx.has_encrypted_comment() {
        col = col.child(
            div()
                .id("money-tx-encrypted-open")
                .cursor_pointer()
                .role(gpui_kit::Role::Button)
                .tab_index(0)
                .aria_label(tr(keys::Q_MONEY_ENCRYPTED_COMMENT))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.show_wallet_notice(WalletSurface::EncryptedComment, cx);
                }))
                .child(
                    spoiler_surface(
                        0x7702_0000_0000_0000,
                        None,
                        px(160.),
                        px(16.),
                        MediaCorners::small(),
                    )
                    .role(gpui_kit::Role::Image)
                    .aria_label(tr(keys::Q_MONEY_ENCRYPTED_COMMENT))
                    .debug_selector(|| "money-tx-encrypted".to_string()),
                ),
        );
    }
    let party = super::ton_wallet_money::tx_title(session, tx);
    let has_peer = tx.peer_user_id != 0 || !tx.peer_domain.is_empty();
    if has_peer && !matches!(tx.kind, own::TxKind::OnRampDeposit { .. }) {
        let label = if tx.is_incoming() {
            tr(keys::Q_MONEY_SENDER)
        } else {
            tr(keys::Q_MONEY_RECIPIENT)
        };
        col = col.child(facts([(label.to_string(), party)]));
    }
    if !tx.peer_address.is_empty() {
        col = col.child(address_block(
            "money-tx-address",
            tr(keys::CONTACT_DETAILS_ADDRESS),
            &tx.peer_address,
            cx,
        ));
    }
    let mut rows = Vec::new();
    if !tx.is_incoming() && tx.fee_amount != 0 {
        let gasless = matches!(
            tx.kind,
            own::TxKind::Transfer {
                is_gasless: true,
                ..
            }
        );
        let fee = if gasless {
            tr(keys::Q_MONEY_FREE_PAID_BY_TELEGRAM).to_string()
        } else {
            format!(
                "\u{1F48E} {}",
                format_gram_amount_with(tx.fee_amount.saturating_abs(), 5)
            )
        };
        rows.push((tr(keys::Q_MONEY_FEE).to_string(), fee));
    }
    rows.push((
        tr(keys::GIFT_RESALE_DATE).to_string(),
        format_unix_date_time(i64::from(tx.date)),
    ));
    col = col.child(facts(rows));
    let url = tx
        .tx_hash
        .as_deref()
        .and_then(|hash| transaction_explorer_url(wallet.explorer_url.as_deref(), hash));
    col.when_some(url, |col, url| {
        col.child(
            Button::new("money-tx-explorer")
                .label(tr(keys::Q_MONEY_VIEW_IN_EXPLORER))
                .outline()
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.open_link(&url, cx);
                })),
        )
    })
    .into_any_element()
}

/// One collectible: picture, name, number, description and attributes.
pub(super) fn nft_details(
    session: &Session,
    nft: &WalletNft,
    roots: &[PathBuf],
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let mut col = div()
        .flex()
        .flex_col()
        .gap_2()
        .debug_selector(|| "money-nft-details".to_string())
        .child(back_button(cx))
        .child(div().flex().justify_center().child(nft_thumb(
            "money-nft-big",
            session,
            nft,
            roots,
            160.,
            cx,
        )))
        .child(
            div()
                .text_center()
                .text_lg()
                .font_semibold()
                .child(nft.title().to_string()),
        );
    if !nft.index.is_empty() {
        col = col.child(
            div()
                .text_center()
                .text_sm()
                .text_color(muted)
                .child(format!("#{}", nft.index)),
        );
    }
    if !nft.description.is_empty() {
        col = col.child(div().text_sm().child(nft.description.clone()));
    }
    if !nft.attributes.is_empty() {
        col = col.child(facts(nft.attributes.iter().cloned()));
    }
    if !nft.address.is_empty() {
        col = col.child(address_block(
            "money-nft-address",
            tr(keys::CONTACT_DETAILS_ADDRESS),
            &nft.address,
            cx,
        ));
    }
    col.child(notice_button(
        "money-nft-send",
        "money-nft-send".to_string(),
        tr_with(keys::WALLET_COLLECTIBLE_SEND, &[("name", nft.title())]),
        WalletSurface::SendNft,
        cx,
    ))
    .into_any_element()
}

/// "Your first Grams!": shown once, when the first incoming transfer's
/// details open (iOS `WalletFirstGramsScreen`). Send and Trade are left
/// out: Quill can't send, and it doesn't point people at exchanges.
pub(super) fn first_grams_hint(wallet: &TonWalletState, cx: &mut Context<QuillApp>) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let rate = wallet.units_per_gram().map(|_| wallet.fiat_text(ONE_GRAM));
    div()
        .flex()
        .flex_col()
        .items_center()
        .gap_3()
        .py_2()
        .debug_selector(|| "money-first-grams".to_string())
        .child(div().text_size(px(48.)).child("\u{1F48E}"))
        .child(
            div()
                .text_xl()
                .font_semibold()
                .child(tr(keys::Q_MONEY_YOUR_FIRST_GRAMS)),
        )
        .when_some(rate, |col, rate| {
            col.child(div().text_sm().text_center().child(tr_with(
                keys::Q_MONEY_1_GRAM_CURRENTLY_EQUALS,
                &[("rate", &rate.to_string())],
            )))
        })
        .child(
            div()
                .text_sm()
                .text_center()
                .text_color(muted)
                .child(tr(keys::Q_MONEY_THEYRE_IN_YOUR_WALLET_YOU)),
        )
        .child(
            div()
                .flex()
                .gap_2()
                .child(notice_button(
                    "money-hint-send",
                    "money-hint-send".to_string(),
                    tr(keys::WALLET_FIRST_SEND_TITLE),
                    WalletSurface::Send,
                    cx,
                ))
                .child(
                    div()
                        .debug_selector(|| "money-hint-got-it".to_string())
                        .child(
                            Button::new("money-hint-got-it")
                                .label(tr(keys::CHANNEL_EARN_LEARN_CLOSE))
                                .primary()
                                .on_click(
                                    cx.listener(|this, _, _, cx| this.dismiss_first_grams_hint(cx)),
                                ),
                        ),
                ),
        )
        .into_any_element()
}
