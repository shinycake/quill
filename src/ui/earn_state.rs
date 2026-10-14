//! What the revenue and affiliate screens keep on the UI side (`ui/earn`):
//! the text fields, the open boxes that hold no TDLib state, the setup
//! sliders and the info-panel pages to go back to.

use super::app::QuillApp;
use gpui_kit::component::input::InputState;
use gpui_kit::component::slider::SliderState;
use gpui_kit::*;
use quill::earn::{EarnPeer, TonTx};
use quill::premium_hub::StarTx;
use quill::state::InfoPanelTarget;
use std::collections::HashSet;

pub(crate) struct EarnUi {
    /// The Stars to withdraw (tdesktop `AddInputFieldForCredits`).
    pub(super) amount: Entity<InputState>,
    /// The 2FA password of a withdrawal; emptied the moment it is sent.
    pub(super) password: Entity<InputState>,
    /// The withdrawal amount last put in `amount`, per page peer, so a
    /// new balance refills it once.
    pub(super) prefilled: Option<(EarnPeer, i64)>,
    /// The pages an earn page was opened from (tdesktop's back arrow).
    pub(super) back: Vec<InfoPanelTarget>,
    /// The "Learn more" box of the Monetization page.
    pub(super) learn: bool,
    /// The learn box's TON easter egg (tdesktop counts diamonds).
    pub(super) diamonds: u32,
    /// A TON history row's details box.
    pub(super) ton_tx: Option<TonTx>,
    /// A Stars history row's receipt.
    pub(super) star_tx: Option<StarTx>,
    /// The boost box a locked "Switch Off Ads" opens: the channel and the
    /// level it needs.
    pub(super) ask_boost: Option<(i64, i32)>,
    /// The setup page's sliders and the stops they move over.
    pub(super) commission: Option<(Entity<SliderState>, Vec<i32>)>,
    pub(super) duration: Option<(Entity<SliderState>, Vec<i32>)>,
    /// Bots whose photo was asked for by a program list.
    pub(super) photos_asked: HashSet<i64>,
    /// The earn page's scroll; its end loads the next programs page.
    pub(super) scroll: ScrollHandle,
    /// The programs box's list.
    pub(super) box_scroll: ScrollHandle,
}

impl EarnUi {
    pub(super) fn new(window: &mut Window, cx: &mut Context<QuillApp>) -> Self {
        Self {
            amount: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("Enter amount to withdraw")
                    .submit_on_enter(false)
            }),
            password: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("Password")
                    .masked(true)
                    .submit_on_enter(true)
            }),
            prefilled: None,
            back: Vec::new(),
            learn: false,
            diamonds: 0,
            ton_tx: None,
            star_tx: None,
            ask_boost: None,
            commission: None,
            duration: None,
            photos_asked: HashSet::new(),
            scroll: ScrollHandle::new(),
            box_scroll: ScrollHandle::new(),
        }
    }
}
