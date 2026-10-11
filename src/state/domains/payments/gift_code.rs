//! A Premium gift code opened from `t.me/giftcode/<code>`: what
//! `checkPremiumGiftCode` said and the progress of an explicit Apply.
use crate::state::*;
use crate::telegram::envelope::GiftCodeInfoData;

/// The code a link carries and where its box stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GiftCodeLookup {
    pub code: String,
    /// `None` while loading or after a failed check.
    pub info: Option<GiftCodeInfoData>,
    pub loading: bool,
    /// Why the check or the apply failed.
    pub error: Option<String>,
    /// `applyPremiumGiftCode` is in flight.
    pub applying: bool,
    /// `applyPremiumGiftCode` answered `ok`.
    pub applied: bool,
}

impl GiftCodeLookup {
    pub fn new(code: String) -> Self {
        Self {
            code,
            info: None,
            loading: true,
            error: None,
            applying: false,
            applied: false,
        }
    }
}

impl Session {
    /// `premiumGiftCodeInfo` for our own `checkPremiumGiftCode`.
    pub(crate) fn apply_gift_code_info(
        &mut self,
        info: GiftCodeInfoData,
        pending: Option<&PendingRequest>,
    ) {
        if pending.map(|p| p.purpose) == Some(RequestPurpose::CheckPremiumGiftCode)
            && let Some(lookup) = self.payments.gift_code.as_mut()
        {
            lookup.loading = false;
            lookup.error = None;
            lookup.info = Some(info);
        }
    }

    /// `ok` for `applyPremiumGiftCode`.
    pub(crate) fn apply_gift_code_ok(&mut self, pending: Option<&PendingRequest>) {
        if pending.map(|p| p.purpose) == Some(RequestPurpose::ApplyPremiumGiftCode)
            && let Some(lookup) = self.payments.gift_code.as_mut()
        {
            lookup.applying = false;
            lookup.applied = true;
            lookup.error = None;
        }
    }

    /// A failed check or apply keeps the box open with the reason.
    pub(crate) fn apply_gift_code_error(
        &mut self,
        err: &TdError,
        pending: Option<&PendingRequest>,
    ) {
        let purpose = pending.map(|p| p.purpose);
        let Some(lookup) = self.payments.gift_code.as_mut() else {
            return;
        };
        match purpose {
            Some(RequestPurpose::CheckPremiumGiftCode) => {
                lookup.loading = false;
                lookup.info = None;
                lookup.error = Some("This gift code link has expired or is not valid.".into());
            }
            Some(RequestPurpose::ApplyPremiumGiftCode) => {
                lookup.applying = false;
                lookup.error = Some(format!("Couldn't use the gift code: {}", error_reason(err)));
            }
            _ => {}
        }
    }
}
