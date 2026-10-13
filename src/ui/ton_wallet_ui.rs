//! TON wallet, phase 1 (read-only): the TON address menu and the transfer
//! card under a wallet-transfer service line. The data and wording live in
//! `quill::ton_wallet`; Telegram Desktop has no wallet, so this follows the
//! official iOS app (`ChatControllerOpenTonAddressContextMenu`,
//! `ChatMessageTransferBubbleContentNode`) minus everything that acts on
//! the wallet: no Send Money, no transaction screen, no decrypting.

use super::app::QuillApp;
use super::entity_links::LinkPopup;
use super::message_media::{MediaCorners, spoiler_surface};
use super::message_text::MONO_FONT;
use super::*;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::state::{HistoryMessage, Session};
use quill::telegram::envelope::{MessageContent, ServiceAction};
use quill::text::LinkTarget;
use quill::ton_wallet::{
    SERVICE_NOTIFICATIONS_CHAT_ID, TonWalletTransfer, explorer_url, group_address,
    signed_gram_amount,
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
        self.open_message_url(&url, cx);
    }

    /// "Copy Address" (iOS toast: "Address copied").
    pub(super) fn copy_ton_address(&mut self, address: String, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(address));
        self.connection.status_note = "address copied to clipboard".into();
        cx.notify();
    }
}

/// The "View in Explorer" row of the TON address menu.
pub(super) fn explorer_menu_row(
    id: &'static str,
    address: String,
    on_open: impl Fn(&mut QuillApp) + 'static,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let row_hover = cx.theme().accent;
    div()
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
        .aria_label("View in Explorer")
        .debug_selector(move || id.to_string())
        .child(Icon::new(gpui_kit::assets::IconName::Search).size(px(16.)))
        .child("View in Explorer")
        .on_click(cx.listener(move |this, _, _, cx| {
            on_open(this);
            this.open_ton_explorer(&address, cx);
        }))
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
        "UNKNOWN USER".to_string()
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
    let label = format!(
        "{} {}{}",
        if received { "Received" } else { "Sent" },
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
            spoiler_surface(
                row_id ^ 0x7701_0000_0000_0000,
                None,
                px(ENCRYPTED_COMMENT_WIDTH),
                px(16.),
                MediaCorners::small(),
            )
            .role(gpui_kit::Role::Image)
            .aria_label("Encrypted comment")
            .debug_selector(move || format!("ton-transfer-encrypted-{row_id}"))
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
