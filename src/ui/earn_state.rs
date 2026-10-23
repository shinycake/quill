//! What the revenue and affiliate screens keep on the UI side (`ui/earn`):
//! the text fields, the open boxes that hold no TDLib state, the setup
//! sliders and the info-panel pages to go back to.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use gpui_kit::component::input::InputState;
use gpui_kit::component::slider::SliderState;
use gpui_kit::*;
use quill::earn::{EarnPeer, TonTx};
use quill::lang::{keys, tr};
use quill::premium_hub::StarTx;
use quill::state::InfoPanelTarget;
use std::collections::HashSet;

/// What a locked switch needs boosts for (tdesktop `AskBoostReason`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AskBoostReason {
    /// "Switch Off Ads" (`AskBoostCpm`).
    Cpm,
    /// "Auto-translate messages" (`AskBoostAutotranslate`).
    Autotranslate,
}

/// The boost box of a locked switch: the channel and the level it needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AskBoost {
    pub(crate) chat_id: i64,
    pub(crate) level: i32,
    pub(crate) reason: AskBoostReason,
}

pub(crate) struct EarnUi {
    /// The Stars to withdraw (tdesktop `AddInputFieldForCredits`).
    pub(super) amount: Entity<InputState>,
    /// The 2FA password of a withdrawal; emptied the moment it is sent.
    pub(super) password: Entity<InputState>,
    /// The withdrawal amount last put in `amount`, per page peer, so a
    /// new balance refills it once.
    pub(super) prefilled: Option<(EarnPeer, i64)>,
    /// The earn pages the open one was opened from, inside the layer
    /// (tdesktop's back arrow).
    pub(super) back: Vec<InfoPanelTarget>,
    /// The info panel the layer covered; it comes back when the layer
    /// closes.
    pub(super) restore: Option<InfoPanelTarget>,
    /// The layer's fade in and out.
    pub(super) fade: Option<super::earn::LayerFade>,
    /// The "Learn more" box of the Monetization page.
    pub(super) learn: bool,
    /// The learn box's TON easter egg (tdesktop counts diamonds).
    pub(super) diamonds: u32,
    /// A TON history row's details box.
    pub(super) ton_tx: Option<TonTx>,
    /// A Stars history row's receipt.
    pub(super) star_tx: Option<StarTx>,
    /// The boost box a locked switch opens.
    pub(super) ask_boost: Option<AskBoost>,
    /// The setup page's sliders and the stops they move over.
    pub(super) commission: Option<(Entity<SliderState>, Vec<i32>)>,
    pub(super) duration: Option<(Entity<SliderState>, Vec<i32>)>,
    /// Bots whose photo was asked for by a program list.
    pub(super) photos_asked: HashSet<i64>,
    /// The earn page's scroll; its end loads the next programs page.
    pub(super) scroll: ScrollHandle,
    /// The programs box's list.
    pub(super) box_scroll: ScrollHandle,
    /// When the referral link box appeared: its link icon plays once
    /// (tdesktop's `starref_link` animation).
    pub(super) link_shown: Option<std::time::Instant>,
}

impl EarnUi {
    pub(super) fn new(window: &mut Window, cx: &mut Context<QuillApp>) -> Self {
        Self {
            amount: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder(tr(keys::BOT_EARN_OUT_PH))
                    .submit_on_enter(false)
            }),
            password: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder(tr(keys::Q_MONEY_ENTER_CURRENT_PASSWORD))
                    .masked(true)
                    .submit_on_enter(true)
            }),
            prefilled: None,
            back: Vec::new(),
            restore: None,
            fade: None,
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
            link_shown: None,
        }
    }
}
