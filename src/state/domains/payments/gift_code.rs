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
    /// The `checkPremiumGiftCode` and `applyPremiumGiftCode` this box
    /// waits for: an answer to another box's request (one closed, then
    /// another link opened) settles nothing here.
    pub(crate) check: Option<RequestId>,
    pub(crate) apply: Option<RequestId>,
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
            check: None,
            apply: None,
        }
    }

    /// The box waits for `check` to answer.
    pub(crate) fn checking(mut self, check: RequestId) -> Self {
        self.check = Some(check);
        self
    }
}

/// The box's own request: `pending` is `wanted`, and of `purpose`.
fn is_ours(
    pending: Option<&PendingRequest>,
    purpose: RequestPurpose,
    wanted: Option<RequestId>,
) -> bool {
    pending.is_some_and(|p| p.purpose == purpose && Some(p.id) == wanted)
}

impl Session {
    /// `premiumGiftCodeInfo` for our own `checkPremiumGiftCode`.
    pub(crate) fn apply_gift_code_info(
        &mut self,
        info: GiftCodeInfoData,
        pending: Option<&PendingRequest>,
    ) {
        if let Some(lookup) = self.payments.gift_code.as_mut()
            && is_ours(pending, RequestPurpose::CheckPremiumGiftCode, lookup.check)
        {
            lookup.check = None;
            lookup.loading = false;
            lookup.error = None;
            lookup.info = Some(info);
        }
    }

    /// `ok` for `applyPremiumGiftCode`.
    pub(crate) fn apply_gift_code_ok(&mut self, pending: Option<&PendingRequest>) {
        if let Some(lookup) = self.payments.gift_code.as_mut()
            && is_ours(pending, RequestPurpose::ApplyPremiumGiftCode, lookup.apply)
        {
            lookup.apply = None;
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
        let Some(lookup) = self.payments.gift_code.as_mut() else {
            return;
        };
        let purpose = pending
            .filter(|p| Some(p.id) == lookup.check || Some(p.id) == lookup.apply)
            .map(|p| p.purpose);
        match purpose {
            Some(RequestPurpose::CheckPremiumGiftCode) => {
                lookup.check = None;
                lookup.loading = false;
                lookup.info = None;
                lookup.error = Some("This gift code link has expired or is not valid.".into());
            }
            Some(RequestPurpose::ApplyPremiumGiftCode) => {
                lookup.apply = None;
                lookup.applying = false;
                lookup.error = Some(format!("Couldn't use the gift code: {}", error_reason(err)));
            }
            _ => {}
        }
    }
}
