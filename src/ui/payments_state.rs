//! Payments and the gift marketplace.
#![deny(clippy::indexing_slicing)]

use super::*;
use gpui_kit::component::input::{InputEvent, InputState, TextareaState};
use gpui_kit::*;
use quill::lang::{keys, tr};

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
    /// Pay a Premium gift with Stars instead of money.
    pub(super) gift_premium_by_stars: bool,
    /// The search field of the lists you pick from: contacts to gift
    /// Stars, members to award, countries (`stars_gift_ui`,
    /// `giveaway_pick_ui`).
    pub(super) pick_search: Entity<InputState>,
    /// "Gift Premium or Stars", the gift recipient list, is open
    /// (`gift_recipients_ui`).
    pub(super) gift_recipients_open: bool,
    /// Its search field.
    pub(super) gift_recipient_search: Entity<InputState>,
    /// The scroll region of the giveaway dialog, which pages the members
    /// list near its end (`scroll_load`).
    pub(super) pick_scroll: ScrollHandle,
    /// The same for the subscriptions dialog.
    pub(super) subs_scroll: ScrollHandle,
    /// The same for the Stars, received-gifts and Gram dialogs
    /// (`premium_ui/paging.rs`).
    pub(super) stars_scroll: ScrollHandle,
    pub(super) gifts_scroll: ScrollHandle,
    pub(super) grams_scroll: ScrollHandle,
    /// The list length each paged dialog last asked at on its own, so a
    /// page that never arrives cannot start a request loop.
    pub(super) giveaway_asked: super::scroll_load::Asked,
    pub(super) subs_asked: super::scroll_load::Asked,
    pub(super) stars_asked: super::scroll_load::Asked,
    pub(super) gifts_asked: super::scroll_load::Asked,
    pub(super) grams_asked: super::scroll_load::Asked,
    /// The 2-step verification password of "Send via Blockchain"
    /// (`gift_transfer_ui`); emptied when it is sent and when the box
    /// closes.
    pub(super) gift_export_password: Entity<InputState>,
    /// The password box has taken the keyboard since it last opened.
    pub(super) gift_export_password_focused: bool,
    /// The 2-step verification password of the Gram withdrawal
    /// (`premium_ui::grams_withdraw`); emptied when it is sent and when the
    /// box closes.
    pub(super) grams_withdraw_password: Entity<InputState>,
    pub(super) grams_withdraw_password_focused: bool,
    /// The transfer picker's people for the current search
    /// (`gift_transfer_view`).
    pub(super) transfer_people:
        std::cell::RefCell<Option<super::gift_transfer_view::CandidateCache>>,
    /// Where card tokenization is sent instead of the provider (tests).
    pub(super) card_transport:
        Option<std::sync::Arc<dyn quill::payment_card::transport::Transport>>,
}

impl PaymentUi {
    pub(super) fn new(window: &mut Window, cx: &mut Context<QuillApp>) -> Self {
        let pick_search = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(tr(keys::Q_MONEY_SEARCH))
                .submit_on_enter(false)
        });
        cx.subscribe_in(
            &pick_search,
            window,
            |this, _, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Change) {
                    this.pick_search_changed(cx);
                }
            },
        )
        .detach();
        let gift_recipient_search = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(tr(keys::GIFT_RECIPIENT_SEARCH))
                .submit_on_enter(false)
        });
        cx.subscribe_in(
            &gift_recipient_search,
            window,
            |_, _, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            },
        )
        .detach();
        let gift_export_password = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(tr(keys::Q_MONEY_ENTER_CURRENT_PASSWORD))
                .masked(true)
                .submit_on_enter(false)
        });
        // Enter submits the password box, like its Submit button.
        cx.subscribe_in(
            &gift_export_password,
            window,
            |this, _, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    this.submit_gift_export_password(window, cx);
                }
            },
        )
        .detach();
        let grams_withdraw_password = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(tr(keys::Q_MONEY_ENTER_CURRENT_PASSWORD))
                .masked(true)
                .submit_on_enter(false)
        });
        cx.subscribe_in(
            &grams_withdraw_password,
            window,
            |this, _, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    this.submit_grams_withdraw_password(window, cx);
                }
            },
        )
        .detach();
        Self {
            dialog: None,
            marketplace_open: false,
            marketplace_name_input: cx.new(|cx| {
                TextareaState::new(window, cx)
                    .placeholder(tr(keys::Q_MONEY_COLLECTIBLE_GIFT_NAME_E_G))
            }),
            marketplace_comment_input: cx.new(|cx| {
                TextareaState::new(window, cx)
                    .placeholder(tr(keys::Q_MONEY_PERSONAL_COMMENT))
                    .submit_on_enter(false)
            }),
            marketplace_private: true,
            marketplace_error: None,
            gift_price_input: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder(tr(keys::Q_MONEY_PRICE_IN_STARS))
                    .submit_on_enter(false)
            }),
            gift_collection_input: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder(tr(keys::Q_MONEY_COLLECTION_NAME))
                    .submit_on_enter(false)
            }),
            gift_message_input: cx.new(|cx| {
                TextareaState::new(window, cx)
                    .placeholder(tr(keys::Q_MONEY_ADD_A_MESSAGE_OPTIONAL))
                    .submit_on_enter(false)
            }),
            gift_hide_name: false,
            gift_pay_upgrade: false,
            gift_premium_by_stars: false,
            pick_search,
            gift_recipients_open: false,
            gift_recipient_search,
            pick_scroll: ScrollHandle::new(),
            subs_scroll: ScrollHandle::new(),
            stars_scroll: ScrollHandle::new(),
            gifts_scroll: ScrollHandle::new(),
            grams_scroll: ScrollHandle::new(),
            giveaway_asked: Default::default(),
            subs_asked: Default::default(),
            stars_asked: Default::default(),
            gifts_asked: Default::default(),
            grams_asked: Default::default(),
            gift_export_password,
            gift_export_password_focused: false,
            grams_withdraw_password,
            grams_withdraw_password_focused: false,
            transfer_people: std::cell::RefCell::new(None),
            card_transport: None,
        }
    }
}
