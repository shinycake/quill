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
use quill::state::InfoPanelTarget;

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

/// The earn box that is open. They hold no TDLib state, and one at a time:
/// opening another replaces it. Open one through
/// [`QuillApp::open_earn_box`], which also records the asking window.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum EarnBox {
    /// The "Learn more" box of the Monetization page (its TON easter egg
    /// counts in [`EarnUi::diamonds`], which outlives the box).
    Learn,
    /// A TON history row's details box.
    TonTx(TonTx),
    /// A Stars history row's receipt.
    StarTx(StarTx),
    /// The boost box a locked switch opens.
    AskBoost(AskBoost),
}

impl EarnBox {
    pub(crate) fn kind(&self) -> super::shell::DialogKind {
        use super::shell::DialogKind;
        match self {
            Self::Learn => DialogKind::EarnLearn,
            Self::TonTx(_) => DialogKind::EarnTonTx,
            Self::StarTx(_) => DialogKind::EarnStarTx,
            Self::AskBoost(_) => DialogKind::EarnAskBoost,
        }
    }
}

impl QuillApp {
    /// Show `shown` and open its dialog in the window that asked.
    pub(crate) fn open_earn_box(&mut self, shown: EarnBox) {
        let kind = shown.kind();
        self.earn.open_box = Some(shown);
        self.open_box(kind);
    }
}

/// The setup page's two sliders and the stops each moves over, for the
/// page of `bot_user_id` as `program` stood when it opened.
pub(crate) struct SetupSliders {
    pub(super) bot_user_id: i64,
    pub(super) program: Option<ProgramInfo>,
    pub(super) commission: (Entity<SliderState>, Vec<i32>),
    pub(super) duration: (Entity<SliderState>, Vec<i32>),
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
    /// The earn box that is open.
    pub(super) open_box: Option<EarnBox>,
    /// The TON easter egg of the learn box (tdesktop counts diamonds); kept
    /// between openings.
    pub(super) diamonds: u32,
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
/// the column it covered are this screen's: it closes, giving that column
/// back, or it would come back on switch-back with neither.
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
    pub(crate) fn learn_open(&self) -> bool {
        matches!(self.open_box, Some(EarnBox::Learn))
    }

    pub(crate) fn ton_tx(&self) -> Option<&TonTx> {
        match &self.open_box {
            Some(EarnBox::TonTx(tx)) => Some(tx),
            _ => None,
        }
    }

    pub(crate) fn star_tx(&self) -> Option<&StarTx> {
        match &self.open_box {
            Some(EarnBox::StarTx(tx)) => Some(tx),
            _ => None,
        }
    }

    pub(crate) fn ask_boost(&self) -> Option<AskBoost> {
        match self.open_box {
            Some(EarnBox::AskBoost(ask)) => Some(ask),
            _ => None,
        }
    }

    /// Close the open box when it is one `is` says yes to.
    pub(super) fn close_box_if(&mut self, is: impl FnOnce(&EarnBox) -> bool) {
        if self.open_box.as_ref().is_some_and(is) {
            self.open_box = None;
        }
    }

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
            open_box: None,
            diamonds: 0,
            sliders: None,
            scroll: ScrollHandle::new(),
            box_scroll: ScrollHandle::new(),
        }
    }
}
