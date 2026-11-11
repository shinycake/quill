//! Free-message codes in the current user's box (tdesktop
//! `SetupFreeCodes`): an account that charges for messages can swap its
//! code for one that lets the scanner message it for free, temporary
//! (a minute) or permanent.

use super::link::UserLinkData;

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

/// A sent free-code request: which box sent it and which of its requests
/// it was, so an answer meant for an earlier box or a dropped request is
/// ignored (tdesktop `generation`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FreeTicket {
    pub request: FreeRequest,
    pub box_id: u64,
    pub serial: u64,
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
/// out. A request closes the menu by construction; the answer, a dropped
/// request (`FreeState::drop_pending`) and the box closing each leave it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FreeUi {
    #[default]
    Idle,
    MenuOpen,
    /// The `getUserLink` out as ticket `serial`; only its answer applies.
    Requesting {
        request: FreeRequest,
        serial: u64,
    },
}

/// The free-code part of the box.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FreeState {
    pub mode: FreeMode,
    pub ui: FreeUi,
    /// The serial of the last request sent; every request takes the next.
    pub serial: u64,
}

impl FreeState {
    /// The request out, if one is.
    pub fn requesting(&self) -> Option<FreeRequest> {
        match self.ui {
            FreeUi::Requesting { request, .. } => Some(request),
            _ => None,
        }
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
