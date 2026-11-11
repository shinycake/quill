//! A Premium gift code opened from `t.me/giftcode/<code>`: what
//! `checkPremiumGiftCode` said and the progress of an explicit Apply.
use crate::state::*;
use crate::telegram::envelope::GiftCodeInfoData;

/// Where a gift code box stands. One value: each stage leaves by the
/// answer, the error, or the box closing (the lookup is dropped with it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GiftCodeStage {
    /// `checkPremiumGiftCode` is out as this request.
    Checking(RequestId),
    /// Checked; Apply is offered when the code is unused.
    Known(GiftCodeInfoData),
    /// `applyPremiumGiftCode` is out as `request`.
    Applying {
        request: RequestId,
        info: GiftCodeInfoData,
    },
    /// `applyPremiumGiftCode` answered `ok`.
    Applied(GiftCodeInfoData),
    /// The check failed: the link is expired or not valid.
    Failed(String),
    /// The apply failed; the checked code stays, with the reason under it.
    ApplyFailed {
        info: GiftCodeInfoData,
        error: String,
    },
}

/// The code a link carries and where its box stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GiftCodeLookup {
    pub code: String,
    /// The box waits only for the request its stage names: an answer to
    /// another box's request (one closed, then another link opened)
    /// settles nothing here.
    pub stage: GiftCodeStage,
}

impl GiftCodeLookup {
    /// A box whose check went out as `check`.
    pub(crate) fn checking(code: String, check: RequestId) -> Self {
        Self {
            code,
            stage: GiftCodeStage::Checking(check),
        }
    }

    /// The check is still out.
    pub fn loading(&self) -> bool {
        matches!(self.stage, GiftCodeStage::Checking(_))
    }

    pub fn applying(&self) -> bool {
        matches!(self.stage, GiftCodeStage::Applying { .. })
    }

    pub fn applied(&self) -> bool {
        matches!(self.stage, GiftCodeStage::Applied(_))
    }

    /// What the check said, once it did.
    pub fn info(&self) -> Option<&GiftCodeInfoData> {
        match &self.stage {
            GiftCodeStage::Known(info)
            | GiftCodeStage::Applying { info, .. }
            | GiftCodeStage::Applied(info)
            | GiftCodeStage::ApplyFailed { info, .. } => Some(info),
            GiftCodeStage::Checking(_) | GiftCodeStage::Failed(_) => None,
        }
    }

    /// Why the check or the apply failed.
    pub fn error(&self) -> Option<&str> {
        match &self.stage {
            GiftCodeStage::Failed(error) | GiftCodeStage::ApplyFailed { error, .. } => Some(error),
            _ => None,
        }
    }

    /// Apply is allowed: the code is known and unused, and no apply is
    /// out or done.
    pub fn can_apply(&self) -> bool {
        matches!(
            &self.stage,
            GiftCodeStage::Known(info) | GiftCodeStage::ApplyFailed { info, .. }
                if !info.is_used()
        )
    }
}

/// The box's own request: `pending` is `wanted`, and of `purpose`.
fn is_ours(pending: Option<&PendingRequest>, purpose: RequestPurpose, wanted: RequestId) -> bool {
    pending.is_some_and(|p| p.purpose == purpose && p.id == wanted)
}

impl Session {
    /// `premiumGiftCodeInfo` for our own `checkPremiumGiftCode`.
    pub(crate) fn apply_gift_code_info(
        &mut self,
        info: GiftCodeInfoData,
        pending: Option<&PendingRequest>,
    ) {
        if let Some(lookup) = self.payments.gift_code.as_mut()
            && let GiftCodeStage::Checking(check) = lookup.stage
            && is_ours(pending, RequestPurpose::CheckPremiumGiftCode, check)
        {
            lookup.stage = GiftCodeStage::Known(info);
        }
    }

    /// `ok` for `applyPremiumGiftCode`.
    pub(crate) fn apply_gift_code_ok(&mut self, pending: Option<&PendingRequest>) {
        if let Some(lookup) = self.payments.gift_code.as_mut()
            && let GiftCodeStage::Applying { request, info } = &lookup.stage
            && is_ours(pending, RequestPurpose::ApplyPremiumGiftCode, *request)
        {
            lookup.stage = GiftCodeStage::Applied(info.clone());
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
        match &lookup.stage {
            GiftCodeStage::Checking(check)
                if is_ours(pending, RequestPurpose::CheckPremiumGiftCode, *check) =>
            {
                lookup.stage = GiftCodeStage::Failed(
                    "This gift code link has expired or is not valid.".into(),
                );
            }
            GiftCodeStage::Applying { request, info }
                if is_ours(pending, RequestPurpose::ApplyPremiumGiftCode, *request) =>
            {
                lookup.stage = GiftCodeStage::ApplyFailed {
                    info: info.clone(),
                    error: format!("Couldn't use the gift code: {}", error_reason(err)),
                };
            }
            _ => {}
        }
    }
}
