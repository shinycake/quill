//! Free-message codes in the current user's box (tdesktop
//! `SetupFreeCodes`): an account that charges for messages can swap its
//! code for one that lets the scanner message it for free, temporary
//! (a minute) or permanent.

use super::link::UserLinkData;
use crate::ids::RequestId;

/// A `getUserLink` call with `remove_messages_fee` set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FreeRequest {
    Temporary,
    Permanent,
    ResetTemporary,
    ResetPermanent,
}

impl FreeRequest {
    /// `return_permanent_link`.
    pub fn permanent(self) -> bool {
        matches!(self, FreeRequest::Permanent | FreeRequest::ResetPermanent)
    }

    /// `revoke_previous_links`.
    pub fn revoke(self) -> bool {
        matches!(
            self,
            FreeRequest::ResetTemporary | FreeRequest::ResetPermanent
        )
    }
}

/// A free-code `getUserLink` result: the code, or the server's reason.
pub type FreeResult = Result<FreeCode, String>;

/// A `getUserLink` answer for a free code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FreeCode {
    pub link: UserLinkData,
    /// `can_get_permanent_link`.
    pub can_permanent: bool,
}

/// What the code encodes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum FreeMode {
    /// The plain link; scanners pay the fee.
    #[default]
    Fee,
    Temporary {
        code: FreeCode,
    },
    Permanent {
        code: FreeCode,
    },
}

impl FreeMode {
    pub fn code(&self) -> Option<&FreeCode> {
        match self {
            FreeMode::Fee => None,
            FreeMode::Temporary { code } | FreeMode::Permanent { code } => Some(code),
        }
    }
}

/// The pill's menu (tdesktop `showMenu`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FreeMenuItem {
    Reset,
    /// "Disable timer" on a temporary code, "Enable timer" on a
    /// permanent one.
    ToggleTimer,
    RestoreFee,
}

/// What the free-code pill is doing: nothing, its menu open, or a request
/// out. A request closes the menu by construction; the answer (or the
/// request found gone from the session's table), a dropped request
/// (`FreeState::drop_request`) and the box closing each leave it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FreeUi {
    #[default]
    Idle,
    MenuOpen,
    /// The `getUserLink` out as `request` (of this `what`); only its own
    /// answer applies.
    Requesting {
        what: FreeRequest,
        request: RequestId,
    },
}

/// The free-code part of the box.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FreeState {
    pub mode: FreeMode,
    pub ui: FreeUi,
}

impl FreeState {
    /// The request out, if one is.
    pub fn requesting(&self) -> Option<FreeRequest> {
        match self.ui {
            FreeUi::Requesting { what, .. } => Some(what),
            _ => None,
        }
    }

    /// The session request the pill waits for, if one is out.
    pub fn waiting_for(&self) -> Option<RequestId> {
        match self.ui {
            FreeUi::Requesting { request, .. } => Some(request),
            _ => None,
        }
    }

    /// The request of this `what` went out as `request`: the stage is
    /// `Requesting` it, closing the menu.
    pub fn sent(&mut self, what: FreeRequest, request: RequestId) {
        self.ui = FreeUi::Requesting { what, request };
    }

    /// The answer of `request`: what it asked for (and the stage is `Idle`)
    /// only while it is the request out.
    pub fn settle_request(&mut self, request: RequestId) -> Option<FreeRequest> {
        match self.ui {
            FreeUi::Requesting { what, request: out } if out == request => {
                self.ui = FreeUi::Idle;
                Some(what)
            }
            _ => None,
        }
    }

    /// The request out, or the menu, is dropped (tdesktop `dropPending`).
    pub fn drop_request(&mut self) {
        self.ui = FreeUi::Idle;
    }

    /// The pill's menu is open.
    pub fn menu_open(&self) -> bool {
        self.ui == FreeUi::MenuOpen
    }

    /// Closes the menu; a request out stays.
    pub fn close_menu(&mut self) {
        if self.menu_open() {
            self.ui = FreeUi::Idle;
        }
    }

    /// Permanent codes are offered unless the server said they are not,
    /// or the account's own link is itself temporary.
    pub fn permanent_allowed(&self, temporary_only: bool) -> bool {
        !temporary_only && self.mode.code().is_none_or(|code| code.can_permanent)
    }

    /// The menu's items in order (tdesktop leaves the timer item out
    /// when only temporary codes exist).
    pub fn menu_items(&self, temporary_only: bool) -> Vec<FreeMenuItem> {
        let mut items = vec![FreeMenuItem::Reset];
        if self.permanent_allowed(temporary_only) {
            items.push(FreeMenuItem::ToggleTimer);
        }
        items.push(FreeMenuItem::RestoreFee);
        items
    }
}
