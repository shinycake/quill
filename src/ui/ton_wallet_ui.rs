//! TON wallet surfaces in chats: the TON address menu, the transfer card
//! under a wallet-transfer service line, the "Review request" button and
//! the profile's wallet-address row. The data and wording live in
//! `quill::ton_wallet`. Quill shows and reads, it never acts: Send Money,
//! decrypting a comment and reviewing a TON Connect request open the shared
//! "not supported" notice (`wallet_notice.rs`) while the wallet is on for
//! the account. Telegram Desktop has a real wallet (`SourceFiles/wallet/`)
//! that Quill deliberately does not build; the layout follows the official
//! iOS app (`ChatControllerOpenTonAddressContextMenu`,
//! `ChatMessageTransferBubbleContentNode`). A card click opens the
//! transaction on the Money page (`ton_wallet_money.rs`).
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::entity_links::LinkPopup;
use super::message_media::{MediaCorners, spoiler_surface};
use super::message_text::MONO_FONT;
use super::wallet_notice::notice_button;
use super::*;
use crate::ui::status_note::StatusNote;
use gpui_kit::component::menu::{ContextMenuExt as _, PopupMenuItem};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::lang::{keys, tr, tr_with};
use quill::state::{HistoryMessage, Session};
use quill::telegram::envelope::{MessageContent, ServiceAction};
use quill::text::LinkTarget;
use quill::ton_wallet::{
    SERVICE_NOTIFICATIONS_CHAT_ID, TonConnectRequestState, TonWalletTransfer, WalletSurface,
    explorer_url, format_gram_amount_with, group_address, signed_gram_amount,
};

/// The card's width (iOS `cardSize`).
const CARD_WIDTH: f32 = 216.0;
/// Width of the hidden placeholder for an encrypted comment (iOS).
const ENCRYPTED_COMMENT_WIDTH: f32 = 120.0;
/// iOS ribbon colors: blue for a received transfer, green for a sent one.
const RECEIVED_RIBBON: u32 = 0x42b0ff;
const SENT_RIBBON: u32 = 0x00cf00;

impl QuillApp {
    /// A click on a TON address opens its menu (the iOS app's tap and long
    /// press both open it), and asks once for the app-config explorer.
    pub(super) fn open_ton_address_menu(&mut self, link: LinkTarget, window: &Window) {
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.fetch_ton_explorer_url();
        }
        self.message_ui.link_popup = Some(LinkPopup {
            position: window.mouse_position(),
            link,
        });
    }

    /// "View in Explorer": the address on the app-config blockchain
    /// explorer (Telegram Desktop's `TonAddressUrl`).
    pub(super) fn open_ton_explorer(&mut self, address: &str, cx: &mut Context<Self>) {
        let url = self.session().map_or_else(
            || explorer_url(None, address),
            |session| session.ton_wallet.explorer_url_for(address),
        );
        self.open_link(&url, cx);
    }

    /// "Copy Address" (iOS toast: "Address copied").
    pub(super) fn copy_ton_address(&mut self, address: String, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(address));
        self.connection.set_note(StatusNote::info(tr(
            keys::Q_MONEY_ADDRESS_COPIED_TO_CLIPBOARD,
        )));
        cx.notify();
    }
}

impl QuillApp {
    /// The wallet address on a user's profile, from
    /// `getUserTonWalletAddresses`: tap copies it, right-click offers Copy
    /// Address and View in Explorer. Nothing shows while the wallet option
    /// is off or the user has no wallet.
    pub(super) fn ton_wallet_profile_row(
        &self,
        user_id: i64,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let address = self
            .session()?
            .ton_wallet
            .profile_address(user_id)?
            .to_string();
        let muted = cx.theme().muted_foreground;
        let hover = cx.theme().secondary;
        let balance = self
            .session()
            .and_then(|session| session.user_full_info(user_id))
            .and_then(|info| info.extras.ton_wallet_card_balance);
        let copy = address.clone();
        let owner = cx.entity().downgrade();
        let menu_address = address.clone();
        let can_send = self
            .session()
            .is_some_and(|session| !session.is_bot_user(user_id));
        let send = can_send.then(|| {
            div().px_3().pb_2().child(notice_button(
                "info-ton-wallet-send",
                "info-ton-wallet-send".to_string(),
                tr(keys::WALLET_PROFILE_SEND_MONEY),
                WalletSurface::SendMoney,
                cx,
            ))
        });
        let row = div()
            .id("info-ton-wallet")
            .flex()
            .flex_col()
            .px_3()
            .py_2()
            .border_t_1()
            .border_color(cx.theme().border)
            .cursor_pointer()
            .role(gpui_kit::Role::Button)
            .aria_label(tr_with(
                keys::Q_MONEY_WALLET_ADDRESS,
                &[("address", &address.to_string())],
            ))
            .tab_index(0)
            .hover(move |style| style.bg(hover))
            .debug_selector(|| "info-ton-wallet".to_string())
            .child(
                div()
                    .text_sm()
                    .font_family(MONO_FONT)
                    .child(group_address(&address)),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child(tr(keys::Q_MONEY_WALLET_ADDRESS_2)),
            )
            .when_some(balance, |row, nanos| {
                row.child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .debug_selector(|| "info-ton-wallet-balance".to_string())
                        .child(tr_with(
                            keys::WALLET_SEND_BALANCE,
                            &[("amount", &format_gram_amount_with(nanos, 2))],
                        )),
                )
            })
            .on_click(cx.listener(move |this, _, _, cx| {
                this.copy_ton_address(copy.clone(), cx);
            }))
            .context_menu(move |menu, _, _| {
                let copy_owner = owner.clone();
                let open_owner = owner.clone();
                let copy = menu_address.clone();
                let open = menu_address.clone();
                menu.item(PopupMenuItem::new(tr(keys::Q_MONEY_COPY_ADDRESS)).on_click(
                    move |_, _, cx| {
                        let _ = copy_owner.update(cx, |this, cx| {
                            this.copy_ton_address(copy.clone(), cx);
                        });
                    },
                ))
                .item(
                    PopupMenuItem::new(tr(keys::Q_MONEY_VIEW_IN_EXPLORER)).on_click(
                        move |_, _, cx| {
                            let _ = open_owner.update(cx, |this, cx| {
                                this.open_ton_explorer(&open, cx);
                            });
                        },
                    ),
                )
            });
        Some(
            div()
                .flex()
                .flex_col()
                .child(row)
                .children(send)
                .into_any_element(),
        )
    }

    /// A click on a transfer card opens that transaction on the Money page
    /// (the iOS card opens the wallet's transaction screen).
    fn open_transfer_details(&mut self, transaction_id: &str, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.open_wallet_transaction(transaction_id);
        } else if let Some(session) = self.demo_session.as_mut()
            && session.ton_wallet.can_use
        {
            session.ton_wallet.money.open = true;
            let known = session
                .ton_wallet
                .money
                .history
                .items
                .iter()
                .find(|tx| tx.id == transaction_id)
                .cloned();
            if let Some(tx) = known {
                session.show_wallet_transaction(tx);
            }
        }
        cx.notify();
    }
}

/// The "View in Explorer" row of the TON address menu.
pub(super) fn explorer_menu_row(
    id: &'static str,
    address: String,
    on_open: impl Fn(&mut QuillApp) + 'static,
    key_target: Option<(&super::context_menu_keys::ContextMenuItems, u32)>,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let row_hover = cx.theme().accent;
    let row = div()
        .id(id)
        .flex()
        .items_center()
        .gap_3()
        .px_3()
        .py_1p5()
        .rounded_md()
        .cursor_pointer()
        .text_sm()
        .text_color(text_menu())
        .hover(|style| style.bg(row_hover))
        .role(gpui_kit::Role::MenuItem)
        .aria_label(tr(keys::Q_MONEY_VIEW_IN_EXPLORER))
        .debug_selector(move || id.to_string())
        .child(Icon::new(gpui_kit::assets::IconName::Search).size(px(16.)))
        .child(tr(keys::Q_MONEY_VIEW_IN_EXPLORER))
        .on_click(cx.listener(move |this, _, _, cx| {
            on_open(this);
            this.open_ton_explorer(&address, cx);
        }));
    match key_target {
        Some((items, key)) => {
            use super::context_menu_keys::MenuKeyTarget;
            row.menu_key_target(key, items, cx)
        }
        None => row,
    }
    .into_any_element()
}

/// The "Send Money" row of a TON address menu; it opens the shared notice.
pub(super) fn send_money_menu_row(
    id: &'static str,
    on_open: impl Fn(&mut QuillApp) + 'static,
    key_target: Option<(&super::context_menu_keys::ContextMenuItems, u32)>,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let row_hover = cx.theme().accent;
    let label = tr(keys::WALLET_PROFILE_SEND_MONEY);
    let row = div()
        .id(id)
        .flex()
        .items_center()
        .gap_3()
        .px_3()
        .py_1p5()
        .rounded_md()
        .cursor_pointer()
        .text_sm()
        .text_color(text_menu())
        .hover(|style| style.bg(row_hover))
        .role(gpui_kit::Role::MenuItem)
        .aria_label(label)
        .debug_selector(move || id.to_string())
        .child(Icon::new(gpui_kit::assets::IconName::ArrowUp).size(px(16.)))
        .child(label)
        .on_click(cx.listener(move |this, _, _, cx| {
            on_open(this);
            this.show_wallet_notice(WalletSurface::SendMoney, cx);
        }));
    match key_target {
        Some((items, key)) => {
            use super::context_menu_keys::MenuKeyTarget;
            row.menu_key_target(key, items, cx)
        }
        None => row,
    }
    .into_any_element()
}

/// The transfer of a wallet-transfer service message, if it is one.
pub(super) fn transfer_of(content: &MessageContent) -> Option<&TonWalletTransfer> {
    match content {
        MessageContent::Action(action) => match action.as_ref() {
            ServiceAction::TonWalletTransfer(transfer) => Some(transfer.as_ref()),
            _ => None,
        },
        _ => None,
    }
}

/// "Review request" under a pending TON Connect request (tdesktop's "Click to
/// review"), shown while the wallet is on; it opens the shared notice.
pub(super) fn connect_review_button(
    row_id: u64,
    content: &MessageContent,
    session: Option<&Session>,
    cx: &mut Context<QuillApp>,
) -> Option<AnyElement> {
    if !session.is_some_and(|session| session.ton_wallet.can_use) {
        return None;
    }
    let MessageContent::Action(action) = content else {
        return None;
    };
    let ServiceAction::TonConnectRequest(request) = action.as_ref() else {
        return None;
    };
    if !matches!(request.state, TonConnectRequestState::Pending { .. }) {
        return None;
    }
    Some(notice_button(
        ("ton-connect-review", row_id),
        format!("ton-connect-review-{row_id}"),
        tr(keys::Q_MONEY_REVIEW_REQUEST),
        WalletSurface::ConnectRequest,
        cx,
    ))
}

/// The card under the "Dana sent you 5 Grams" line: the direction ribbon,
/// the signed amount, the other side's name and its address in groups of
/// four, then the public comment, or a hidden placeholder when the comment
/// is encrypted (Quill never holds the keys to read it).
pub(super) fn transfer_card(
    row_id: u64,
    message: &HistoryMessage,
    transfer: &TonWalletTransfer,
    session: Option<&Session>,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let received = transfer.is_received();
    let amount = signed_gram_amount(transfer.amount, received);
    let peer = if message.chat_id.0 == SERVICE_NOTIFICATIONS_CHAT_ID {
        tr(keys::Q_MONEY_UNKNOWN_USER).to_string()
    } else {
        session
            .and_then(|session| session.chats.get(&message.chat_id.0))
            .map(|chat| chat.title.to_uppercase())
            .unwrap_or_default()
    };
    let address = group_address(&transfer.peer_address);
    let (ribbon, ribbon_color) = if received {
        ("received", RECEIVED_RIBBON)
    } else {
        ("sent", SENT_RIBBON)
    };
    let muted = cx.theme().muted_foreground;
    let wallet_on = session.is_some_and(|session| session.ton_wallet.can_use);
    let transaction_id = transfer.transaction_id.clone();
    let label = format!(
        "{} {}{}",
        if received {
            tr(keys::Q_MONEY_RECEIVED)
        } else {
            tr(keys::Q_MONEY_SENT)
        },
        amount,
        if peer.is_empty() {
            String::new()
        } else {
            format!(", {peer}")
        }
    );
    let card = div()
        .id(("ton-transfer-card", row_id))
        .relative()
        .w(px(CARD_WIDTH))
        .rounded_xl()
        .bg(cx.theme().secondary.opacity(0.85))
        .text_color(cx.theme().foreground)
        .px_3()
        .pt_6()
        .pb_3()
        .flex()
        .flex_col()
        .items_center()
        .gap_1()
        .role(gpui_kit::Role::Group)
        .aria_label(label)
        .debug_selector(move || format!("ton-transfer-card-{row_id}"))
        .when(wallet_on && !transaction_id.is_empty(), |card| {
            card.cursor_pointer()
                .role(gpui_kit::Role::Button)
                .tab_index(0)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.open_transfer_details(&transaction_id, cx);
                }))
        })
        .child(
            div()
                .absolute()
                .top_2()
                .right_2()
                .px_1p5()
                .rounded_md()
                .bg(rgb(ribbon_color))
                .text_color(gpui_kit::white())
                .text_xs()
                .font_semibold()
                .child(ribbon),
        )
        .child(div().text_lg().font_bold().text_center().child(amount))
        .when(!peer.is_empty(), |card| {
            card.child(
                div()
                    .w_full()
                    .text_xs()
                    .font_family(MONO_FONT)
                    .font_semibold()
                    .text_center()
                    .text_color(accent())
                    .truncate()
                    .child(peer),
            )
        })
        .when(!address.is_empty(), |card| {
            card.child(
                div()
                    .w_full()
                    // iOS: 10 pt monospace, six groups of four a line.
                    .text_size(px(10.))
                    .font_family(MONO_FONT)
                    .text_center()
                    .text_color(muted)
                    .line_clamp(2)
                    .child(address),
            )
        });
    let comment = if let Some(comment) = transfer.public_comment() {
        Some(
            div()
                .max_w(px(CARD_WIDTH))
                .text_sm()
                .text_center()
                .debug_selector(move || format!("ton-transfer-comment-{row_id}"))
                .child(comment.to_string())
                .into_any_element(),
        )
    } else if transfer.has_encrypted_comment() {
        Some(
            div()
                .id(("ton-transfer-encrypted-open", row_id))
                .aria_label(tr(keys::Q_MONEY_ENCRYPTED_COMMENT))
                .when(wallet_on, |placeholder| {
                    placeholder
                        .cursor_pointer()
                        .role(gpui_kit::Role::Button)
                        .tab_index(0)
                        .debug_selector(move || format!("ton-transfer-encrypted-open-{row_id}"))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.show_wallet_notice(WalletSurface::EncryptedComment, cx);
                        }))
                })
                .child(
                    spoiler_surface(
                        row_id ^ 0x7701_0000_0000_0000,
                        None,
                        px(ENCRYPTED_COMMENT_WIDTH),
                        px(16.),
                        MediaCorners::small(),
                    )
                    .role(gpui_kit::Role::Image)
                    .aria_label(tr(keys::Q_MONEY_ENCRYPTED_COMMENT))
                    .debug_selector(move || format!("ton-transfer-encrypted-{row_id}")),
                )
                .into_any_element(),
        )
    } else {
        None
    };
    div()
        .flex()
        .flex_col()
        .items_center()
        .gap_1p5()
        .child(card)
        .when_some(comment, |column, comment| column.child(comment))
        .into_any_element()
}
