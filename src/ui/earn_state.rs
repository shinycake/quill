//! What the revenue and affiliate screens keep on the UI side (`ui/earn`):
//! the text fields, the open boxes that hold no TDLib state, the setup
//! sliders and the info-panel pages to go back to.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use gpui_kit::component::input::InputState;
use gpui_kit::component::slider::SliderState;
use gpui_kit::*;
use quill::earn::{EarnPeer, ProgramInfo, TonTx};
use quill::lang::{keys, tr};
use quill::premium_hub::StarTx;

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

/// The setup page's two sliders and the stops each moves over, for the
/// page of `bot_user_id` as `program` stood when it opened.
pub(crate) struct SetupSliders {
    pub(super) bot_user_id: i64,
    pub(super) program: Option<ProgramInfo>,
    pub(super) commission: (Entity<SliderState>, Vec<i32>),
    pub(super) duration: (Entity<SliderState>, Vec<i32>),
}

/// The history row box that is open; one at a time, and it leaves to `None`
/// when closed.
pub(crate) enum TxBox {
    None,
    Ton(TonTx),
    Star(StarTx),
}

impl TxBox {
    pub(super) fn ton(&self) -> Option<&TonTx> {
        match self {
            TxBox::Ton(tx) => Some(tx),
            TxBox::None | TxBox::Star(_) => None,
        }
    }

    pub(super) fn star(&self) -> Option<&StarTx> {
        match self {
            TxBox::Star(tx) => Some(tx),
            TxBox::None | TxBox::Ton(_) => None,
        }
    }
}

/// The "Learn more" box of the Monetization page and its TON easter egg
/// (tdesktop counts diamonds); closing the box drops the count.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct LearnBox {
    pub(super) diamonds: u32,
}

pub(crate) struct EarnUi {
    /// The Stars to withdraw (tdesktop `AddInputFieldForCredits`).
    pub(super) amount: Entity<InputState>,
    /// The 2FA password of a withdrawal; emptied the moment it is sent.
    pub(super) password: Entity<InputState>,
    /// The withdrawal amount last put in `amount`, per page peer, so a
    /// new balance refills it once.
    pub(super) prefilled: Option<(EarnPeer, i64)>,
    /// The layer's stack, covered panel and fade.
    pub(super) layer: super::earn::EarnLayer,
    /// The "Learn more" box of the Monetization page, while open.
    pub(super) learn: Option<LearnBox>,
    /// A history row's details box: a TON row's details, or a Stars row's
    /// receipt.
    pub(super) tx_box: TxBox,
    /// The boost box a locked switch opens.
    pub(super) ask_boost: Option<AskBoost>,
    /// The setup page's sliders, built for one bot's program and rebuilt
    /// when the page shows another.
    pub(super) sliders: Option<Box<SetupSliders>>,
    /// The earn page's scroll; its end loads the next programs page.
    pub(super) scroll: ScrollHandle,
    /// The programs box's list.
    pub(super) box_scroll: ScrollHandle,
}

/// The earn boxes (setup confirm, programs, join, link, revoke, the
/// withdrawal) keep their state in the session's `payments.earn`, which
/// goes to the background with the account: each closes here. The layer's
/// open flag is the session's info column too, while its back stack and
/// the column it covered live in `EarnUi::layer`: it closes, giving that
/// column back, or it would come back on switch-back with neither.
impl crate::ui::on_screen::Farewell for EarnUi {
    fn farewell(
        app: &mut QuillApp,
        window: &mut gpui_kit::Window,
        cx: &mut gpui_kit::Context<QuillApp>,
    ) {
        app.close_earn_boxes(window, cx);
        if app.earn_layer_open() {
            app.drop_earn_layer(cx);
        }
    }
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
            layer: super::earn::EarnLayer::Closed,
            learn: None,
            tx_box: TxBox::None,
            ask_boost: None,
            sliders: None,
            scroll: ScrollHandle::new(),
            box_scroll: ScrollHandle::new(),
        }
    }
}
