//! inline keyboards, invoices, payment rows.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::*;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::ids::{ChatId, MessageId};
use quill::lang::{keys, tr, tr_with};
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
    session: Option<&quill::state::Session>,
    cx: &mut Context<QuillApp>,
) -> Option<AnyElement> {
    let markup = markup_keyboard(message, fast_buttons, session, cx);
    // A suggested post's Decline / Accept / Suggest Changes rows come after
    // the message's own rows (tdesktop `HistoryMessageReplyMarkup`
    // appends them).
    let Some(suggestion) = super::history::suggested_post_block::suggestion_buttons(message, cx)
    else {
        return markup;
    };
    // The message's own grid brings its top margin.
    let has_markup = markup.is_some();
    Some(
        div()
            .id(("inline-keyboard-with-suggestion", message.id.0 as u64))
            .flex()
            .flex_col()
            .gap_1()
            .when(!has_markup, |rows| rows.mt_2())
            .children(markup)
            .child(suggestion)
            .into_any_element(),
    )
}

/// The message's own `replyMarkupInlineKeyboard`.
fn markup_keyboard(
    message: &HistoryMessage,
    fast_buttons: bool,
    session: Option<&quill::state::Session>,
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
        .w_full()
        .min_w_0()
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
        // Buttons share the row and never widen it: a long label ends in
        // "…" (tdesktop `ReplyKeyboard` elides each button's text).
        let mut line = div()
            .id(format!("inline-keyboard-row-{message_id}-{row_index}"))
            .w_full()
            .min_w_0()
            .flex()
            .gap_1();
        for (button_index, button) in row.iter().enumerate() {
            let waiting = callback_key(&button.kind).is_some_and(|key| {
                session.is_some_and(|session| {
                    session.callback_button_waiting(message.chat_id, message.id, &key)
                })
            });
            let element = inline_keyboard_button(
                message.chat_id,
                message.id,
                row_index,
                button_index,
                button,
                game_short_name,
                waiting,
                cx,
            );
            let number = fast_buttons
                .then(|| quill::fast_buttons::badge_number(&row_lens, row_index, button_index))
                .flatten();
            let corner = corner_icon(&button.kind);
            line = line.child(match (number, corner) {
                (None, None) => element.flex_auto().min_w_0().into_any_element(),
                _ => button_cell(
                    (message_id, row_index, button_index),
                    element,
                    number,
                    corner.map(|icon| (icon, corner_icon_color(button.style, cx))),
                    cx,
                ),
            });
        }
        grid = grid.child(line);
    }
    any.then(|| grid.into_any_element())
}

/// The type icon a button draws in its top-right corner (tdesktop
/// `HistoryMessageMarkupButton::IconOfType`): links and logins, switch
/// to inline mode, buy, web app and copy.
fn corner_icon(kind: &InlineKeyboardButtonType) -> Option<gpui_kit::assets::IconName> {
    use gpui_kit::assets::IconName;
    match kind {
        InlineKeyboardButtonType::Url { .. } | InlineKeyboardButtonType::LoginUrl { .. } => {
            Some(IconName::ArrowUpRight)
        }
        InlineKeyboardButtonType::SwitchInline { .. } => Some(IconName::Forward),
        InlineKeyboardButtonType::Buy => Some(IconName::CreditCard),
        InlineKeyboardButtonType::WebApp { .. } => Some(IconName::AppWindow),
        InlineKeyboardButtonType::CopyText { .. } => Some(IconName::Copy),
        _ => None,
    }
}

/// The corner icon's color: the label's own, a step back on the plain
/// button (tdesktop `msgBotKbIconFg`).
fn corner_icon_color(style: InlineKeyboardButtonStyle, cx: &App) -> Hsla {
    let theme = cx.theme();
    match style {
        InlineKeyboardButtonStyle::Default | InlineKeyboardButtonStyle::Link => {
            theme.muted_foreground
        }
        InlineKeyboardButtonStyle::Primary => theme.primary_foreground,
        InlineKeyboardButtonStyle::Danger => theme.danger_foreground,
        InlineKeyboardButtonStyle::Success => theme.success_foreground,
    }
}

/// A button with its corner marks: the fast-button digit top-left, like
/// tdesktop's numbered reply keyboard, and the type icon top-right. The
/// marks draw over the button and never take its clicks.
fn button_cell(
    (message_id, row, column): (u64, usize, usize),
    button: Button,
    number: Option<u8>,
    corner: Option<(gpui_kit::assets::IconName, Hsla)>,
    cx: &App,
) -> AnyElement {
    let has_corner = corner.is_some();
    div()
        .id(format!("inline-button-cell-{message_id}-{row}-{column}"))
        .relative()
        .flex_auto()
        .min_w_0()
        .flex()
        // The label keeps clear of the corner icon when it is cut short.
        .child(button.flex_auto().min_w_0().when(has_corner, |b| b.px_5()))
        .children(number.map(|number| {
            div()
                .absolute()
                .top_0()
                .left_2()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(number.to_string())
        }))
        .children(corner.map(|(icon, color)| {
            div()
                .absolute()
                .top(px(CORNER_ICON_PADDING))
                .right(px(CORNER_ICON_PADDING))
                .child(Icon::new(icon).size(px(CORNER_ICON_SIZE)).text_color(color))
        }))
        .into_any_element()
}

/// tdesktop `msgBotKbIconPadding` and the 10px `inline_button_*` icons.
const CORNER_ICON_PADDING: f32 = 4.;
const CORNER_ICON_SIZE: f32 = 10.;

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
/// a later slice; unknown types never crash. `waiting`: a callback press
/// of this button waits for the bot, so it spins and takes no presses
/// (tdesktop draws the pressed button loading until the answer).
#[allow(clippy::too_many_arguments)]
pub(super) fn inline_keyboard_button(
    chat_id: ChatId,
    message_id: MessageId,
    row_index: usize,
    button_index: usize,
    button: &InlineKeyboardButton,
    game_short_name: Option<&str>,
    waiting: bool,
    cx: &mut Context<QuillApp>,
) -> Button {
    let label = if button.text.is_empty() {
        match &button.kind {
            InlineKeyboardButtonType::Unknown { type_name } if !type_name.is_empty() => {
                format!("({type_name})")
            }
            _ => tr(keys::Q_MONEY_BUTTON).to_string(),
        }
    } else {
        button.text.clone()
    };
    let element = Button::new(format!(
        "inline-btn-{}-{row_index}-{button_index}",
        message_id.0
    ))
    .label(label.clone())
    // A label cut short by the row width reads in full on hover.
    .tooltip(match button_tooltip(button) {
        "" => label,
        hint => hint.to_string(),
    })
    // The spinner takes the icon slot; a plain label has none of its own.
    .when(waiting, |element| {
        element.icon(gpui_kit::assets::IconName::LoaderCircle)
    })
    .loading(waiting);
    let element = inline_keyboard_style(element, button.style);
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

/// The press key of a button whose press asks the bot (`getCallbackQueryAnswer`).
fn callback_key(kind: &InlineKeyboardButtonType) -> Option<quill::state::CallbackButtonKey> {
    match kind {
        InlineKeyboardButtonType::Callback { data }
        | InlineKeyboardButtonType::CallbackWithPassword { data } => {
            Some(quill::state::CallbackButtonKey::Data(data.clone()))
        }
        InlineKeyboardButtonType::CallbackGame => Some(quill::state::CallbackButtonKey::Game),
        _ => None,
    }
}

/// The look of an inline keyboard button for its `style`; also used by
/// the rows Quill adds itself (a suggested post's decisions).
pub(super) fn inline_keyboard_style(element: Button, style: InlineKeyboardButtonStyle) -> Button {
    match style {
        InlineKeyboardButtonStyle::Primary => element.primary(),
        InlineKeyboardButtonStyle::Danger => element.danger(),
        InlineKeyboardButtonStyle::Success => element.success(),
        InlineKeyboardButtonStyle::Link => element.link(),
        InlineKeyboardButtonStyle::Default => element,
    }
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
                self.open_link(url, cx);
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
                // Back to the chat an inline results button left, if any.
                if !self.press_switch_inline_with_return(chat_id, query, window, cx) {
                    self.insert_switch_inline_query(query, window, cx);
                }
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
        InlineKeyboardButtonType::Disabled => tr(keys::Q_MONEY_THIS_BUTTON_IS_DISABLED),
        InlineKeyboardButtonType::Unknown { .. } => tr(keys::Q_MONEY_UNSUPPORTED_BUTTON),
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
                .label(tr(keys::Q_MONEY_VIEW_RECEIPT))
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
    let mut label = tr_with(
        keys::Q_MONEY_PAYMENT_SUCCESSFUL,
        &[(
            "total_amount",
            &(format_payment_price(&success.currency, success.total_amount)).to_string(),
        )],
    );
    if success.is_recurring {
        label.push_str(tr(keys::Q_MONEY_RECURRING));
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
    let mut label = tr_with(
        keys::Q_MONEY_PAYMENT_RECEIVED,
        &[(
            "total_amount",
            &(format_payment_price(&received.currency, received.total_amount)).to_string(),
        )],
    );
    if received.is_recurring {
        label.push_str(tr(keys::Q_MONEY_RECURRING));
    }
    div()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child(label)
        .into_any_element()
}
