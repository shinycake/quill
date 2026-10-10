//! inline keyboards, invoices, payment rows.

use super::app::QuillApp;
use super::*;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::ids::{ChatId, MessageId};
use quill::state::HistoryMessage;
use quill::telegram::envelope::{
    InlineKeyboardButton, InlineKeyboardButtonStyle, InlineKeyboardButtonType, InvoiceContent,
    MessageContent, PaymentReceivedContent, PaymentSuccessContent, ReplyMarkup,
    format_payment_price,
};
/// Phase 3.2: render `replyMarkupInlineKeyboard` as a button grid under the
/// message it belongs to. Parsing is generic (bot chats, groups, channels
/// all inherit it). Buttons stretch to share each row's width, like Telegram
/// desktop; empty rows are skipped.
pub(super) fn inline_keyboard(
    message: &HistoryMessage,
    fast_buttons: bool,
    cx: &mut Context<QuillApp>,
) -> Option<AnyElement> {
    // M2: an ephemeral payload carries its own `reply_markup`, shown
    // instead of the message's own (bot-built flows, anniversary post).
    let markup = message
        .ephemeral
        .as_ref()
        .and_then(|ephemeral| ephemeral.reply_markup.as_ref())
        .or(message.reply_markup.as_ref())?;
    // B1: only `replyMarkupInlineKeyboard` renders as an inline keyboard;
    // other markups (custom keyboards, force-reply) are handled separately.
    let ReplyMarkup::InlineKeyboard(keyboard) = markup else {
        return None;
    };
    // B1: game buttons need the game's short name from the message's
    // `messageGame` content (schema 1.8.67, line 7743).
    let game_short_name = if let MessageContent::Game(game) = &message.content {
        Some(game.short_name.as_str())
    } else {
        None
    };
    let message_id = message.id.0 as u64;
    let mut grid = div()
        .id(("inline-keyboard", message_id))
        .flex()
        .flex_col()
        .gap_1()
        .mt_2();
    let mut any = false;
    let row_lens: Vec<usize> = keyboard.rows.iter().map(Vec::len).collect();
    for (row_index, row) in keyboard.rows.iter().enumerate() {
        if row.is_empty() {
            continue;
        }
        any = true;
        let mut line = div()
            .id(format!("inline-keyboard-row-{message_id}-{row_index}"))
            .flex()
            .gap_1();
        for (button_index, button) in row.iter().enumerate() {
            let element = inline_keyboard_button(
                message.chat_id,
                message.id,
                row_index,
                button_index,
                button,
                game_short_name,
                cx,
            );
            let number = fast_buttons
                .then(|| quill::fast_buttons::badge_number(&row_lens, row_index, button_index))
                .flatten();
            line = line.child(match number {
                Some(number) => fast_button_cell(message.id.0 as u64, number, element, cx),
                None => element.flex_1().into_any_element(),
            });
        }
        grid = grid.child(line);
    }
    any.then(|| grid.into_any_element())
}

/// A button with its fast-button digit in the top-left corner, like
/// tdesktop's numbered reply keyboard.
fn fast_button_cell(
    message_id: u64,
    number: u8,
    button: Button,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    div()
        .id(("fast-button-cell", message_id * 10 + u64::from(number)))
        .relative()
        .flex_1()
        .flex()
        .child(button.w_full())
        .child(
            div()
                .absolute()
                .top_0()
                .left_2()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(number.to_string()),
        )
        .into_any_element()
}

/// Phase 3.2 (extended in B1): one inline keyboard button. `Url` opens in
/// the OS browser (same gate as URLs in message text); `LoginUrl` resolves
/// via `getLoginUrlInfo` (TDLib 1.8.67, schema:12985), degrading to the raw
/// URL on error; `WebApp` opens the mini app window
/// (`docs/decisions/codex-miniapp-webview.md`); `Callback` sends `getCallbackQueryAnswer`;
/// `CallbackWithPassword` prompts for the 2-step password and sends
/// `callbackQueryPayloadDataWithPassword` (schema:7740); `CallbackGame`
/// sends `callbackQueryPayloadGame` with the message's `messageGame`
/// short name (schema:7743); `User` opens the private chat with the user;
/// `SwitchInline` inserts the query into the current chat's composer;
/// `CopyText` copies to the clipboard. `Buy` stays disabled — payments are
/// a later slice; unknown types never crash.
pub(super) fn inline_keyboard_button(
    chat_id: ChatId,
    message_id: MessageId,
    row_index: usize,
    button_index: usize,
    button: &InlineKeyboardButton,
    game_short_name: Option<&str>,
    cx: &mut Context<QuillApp>,
) -> Button {
    let label = if button.text.is_empty() {
        match &button.kind {
            InlineKeyboardButtonType::Unknown { type_name } if !type_name.is_empty() => {
                format!("({type_name})")
            }
            _ => "(button)".to_string(),
        }
    } else {
        button.text.clone()
    };
    let element = Button::new(format!(
        "inline-btn-{}-{row_index}-{button_index}",
        message_id.0
    ))
    .label(label)
    .tooltip(button_tooltip(button));
    let element = match button.style {
        InlineKeyboardButtonStyle::Primary => element.primary(),
        InlineKeyboardButtonStyle::Danger => element.danger(),
        InlineKeyboardButtonStyle::Success => element.success(),
        InlineKeyboardButtonStyle::Link => element.link(),
        InlineKeyboardButtonStyle::Default => element.ghost(),
    };
    if !button_is_pressable(&button.kind) {
        return element.disabled(true);
    }
    let button = button.clone();
    let game_short_name = game_short_name.map(str::to_string);
    element.on_click(cx.listener(move |this, _, window, cx| {
        this.activate_inline_button(
            chat_id,
            message_id,
            &button,
            game_short_name.as_deref(),
            window,
            cx,
        );
    }))
}

/// Whether pressing a button of this kind does anything (`Disabled` and
/// unknown kinds stay inert).
pub(super) fn button_is_pressable(kind: &InlineKeyboardButtonType) -> bool {
    !matches!(
        kind,
        InlineKeyboardButtonType::Disabled | InlineKeyboardButtonType::Unknown { .. }
    )
}

impl QuillApp {
    /// Press one inline button, from a click or a fast-button key.
    pub(super) fn activate_inline_button(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        button: &InlineKeyboardButton,
        game_short_name: Option<&str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match &button.kind {
            InlineKeyboardButtonType::Url { url } => {
                self.open_message_url(url, cx);
            }
            // Mini apps open in their own window (`ui/web_app_ui.rs`).
            InlineKeyboardButtonType::WebApp { url } => {
                self.open_inline_web_app(chat_id, message_id, url, cx);
            }
            InlineKeyboardButtonType::LoginUrl { url, id } => {
                self.press_login_url(chat_id, message_id, *id, url, cx);
            }
            InlineKeyboardButtonType::Callback { data } => {
                self.press_inline_callback(chat_id, message_id, data.clone(), cx);
            }
            InlineKeyboardButtonType::CallbackWithPassword { data } => {
                self.open_callback_password_dialog(chat_id, message_id, data.clone(), window, cx);
            }
            InlineKeyboardButtonType::CallbackGame => {
                self.press_game_button(
                    chat_id,
                    message_id,
                    game_short_name.map(str::to_string),
                    cx,
                );
            }
            InlineKeyboardButtonType::User { user_id } => {
                self.open_user_chat(*user_id, window, cx);
            }
            InlineKeyboardButtonType::SwitchInline { query, .. } => {
                self.insert_switch_inline_query(query, window, cx);
            }
            InlineKeyboardButtonType::CopyText { text } => self.copy_inline_text(text, cx),
            // Slice P1: the Buy button fetches the `paymentForm` and opens
            // the checkout dialog (schema:15262). It is only ever attached
            // to a `messageInvoice` (schema:3798).
            InlineKeyboardButtonType::Buy => {
                self.press_buy_button(chat_id, message_id, window, cx);
            }
            InlineKeyboardButtonType::Disabled | InlineKeyboardButtonType::Unknown { .. } => {}
        }
    }
}

/// Short hint for unsupported / unknown inline buttons.
pub(super) fn button_tooltip(button: &InlineKeyboardButton) -> &'static str {
    match &button.kind {
        InlineKeyboardButtonType::Url { .. }
        | InlineKeyboardButtonType::LoginUrl { .. }
        | InlineKeyboardButtonType::WebApp { .. }
        | InlineKeyboardButtonType::Callback { .. }
        | InlineKeyboardButtonType::CallbackWithPassword { .. }
        | InlineKeyboardButtonType::CallbackGame
        | InlineKeyboardButtonType::SwitchInline { .. }
        | InlineKeyboardButtonType::CopyText { .. }
        | InlineKeyboardButtonType::User { .. } => "",
        InlineKeyboardButtonType::Buy => "",
        InlineKeyboardButtonType::Disabled => "This button is disabled",
        InlineKeyboardButtonType::Unknown { .. } => "Unsupported button",
    }
}

/// Slice P1: `messageInvoice` card — product title/description, total
/// price, TEST badge; paid invoices link to their receipt.
pub(super) fn invoice_body(
    chat_id: ChatId,
    message_id: MessageId,
    invoice: &InvoiceContent,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let mut card = div()
        .id(("invoice-card", message_id.0 as u64))
        .flex()
        .flex_col()
        .gap_1()
        .p_3()
        .rounded_md()
        .border_1()
        .border_color(accent())
        .bg(bg_canvas())
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(div().text_sm().font_semibold().child("🧾"))
                .child(div().text_sm().font_semibold().child(invoice.title.clone()))
                .child(if invoice.is_test {
                    div().text_xs().text_color(warning_text()).child("TEST")
                } else {
                    div()
                }),
        );
    if !invoice.description.is_empty() {
        card = card.child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(invoice.description.clone()),
        );
    }
    card = card.child(div().text_sm().font_semibold().child(format_payment_price(
        &invoice.currency,
        invoice.total_amount,
    )));
    if invoice.receipt_message_id != 0 {
        // `receipt_message_id` is the `messagePaymentSuccessful` message
        // (schema:5270); `getPaymentReceipt` takes that message's id
        // (schema:15280).
        let receipt_id = invoice.receipt_message_id;
        card = card.child(
            Button::new(format!("invoice-receipt-{receipt_id}"))
                .label("View receipt")
                .ghost()
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.open_payment_receipt(chat_id, MessageId(receipt_id), cx);
                })),
        );
    }
    card.into_any_element()
}

/// Slice P1: `messagePaymentSuccessful` / `messagePaymentSuccessfulBot`
/// compact receipt row.
pub(super) fn payment_success_row(
    success: &PaymentSuccessContent,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let mut label = format!(
        "✅ Payment successful — {}",
        format_payment_price(&success.currency, success.total_amount)
    );
    if success.is_recurring {
        label.push_str(" (recurring)");
    }
    div()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child(label)
        .into_any_element()
}

pub(super) fn payment_received_row(
    received: &PaymentReceivedContent,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let mut label = format!(
        "💸 Payment received — {}",
        format_payment_price(&received.currency, received.total_amount)
    );
    if received.is_recurring {
        label.push_str(" (recurring)");
    }
    div()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child(label)
        .into_any_element()
}
