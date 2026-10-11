//! Payments and the gift marketplace.

use super::*;
use gpui_kit::component::input::{InputState, TextareaState};
use gpui_kit::*;

pub(crate) struct PaymentUi {
    /// Slice P1: the payment checkout dialog's text inputs. The dialog
    /// renders from the session's `payment_form`; this holds the live
    /// text fields.
    pub(super) dialog: Option<PaymentDialog>,
    pub(super) marketplace_open: bool,
    pub(super) marketplace_name_input: Entity<TextareaState>,
    pub(super) marketplace_comment_input: Entity<TextareaState>,
    pub(super) marketplace_private: bool,
    pub(super) marketplace_error: Option<String>,
    /// Resale price of a collectible in the gift view (`gift_manage_ui`).
    pub(super) gift_price_input: Entity<InputState>,
    /// Name of a new or renamed gift collection.
    pub(super) gift_collection_input: Entity<InputState>,
    /// The comment sent with a gift (`gift_send_ui`).
    pub(super) gift_message_input: Entity<TextareaState>,
    /// "Hide my name and message" on the send-gift dialog.
    pub(super) gift_hide_name: bool,
    /// "Also pay for the upgrade" on the send-gift dialog.
    pub(super) gift_pay_upgrade: bool,
}

impl PaymentUi {
    pub(super) fn new(window: &mut Window, cx: &mut Context<QuillApp>) -> Self {
        Self {
            dialog: None,
            marketplace_open: false,
            marketplace_name_input: cx.new(|cx| {
                TextareaState::new(window, cx)
                    .placeholder("Collectible gift name, e.g. PlushPepe-123")
            }),
            marketplace_comment_input: cx.new(|cx| {
                TextareaState::new(window, cx)
                    .placeholder("Personal comment")
                    .submit_on_enter(false)
            }),
            marketplace_private: true,
            marketplace_error: None,
            gift_price_input: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("Price in Stars")
                    .submit_on_enter(false)
            }),
            gift_collection_input: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("Collection name")
                    .submit_on_enter(false)
            }),
            gift_message_input: cx.new(|cx| {
                TextareaState::new(window, cx)
                    .placeholder("Add a message (optional)")
                    .submit_on_enter(false)
            }),
            gift_hide_name: false,
            gift_pay_upgrade: false,
        }
    }
}
