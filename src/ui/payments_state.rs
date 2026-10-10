//! Payments and the gift marketplace.

use super::*;
use gpui_kit::component::input::TextareaState;
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
        }
    }
}
