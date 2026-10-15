//! Reducer for the value of an upgraded gift
//! (`crate::premium_hub::gift_value`). An answer or failure counts only
//! for the request in flight; a stale one (the user asked about another
//! gift meanwhile) is dropped.
use crate::premium_hub::gift_value::GiftValuePayload;
use crate::state::*;

impl Session {
    pub(crate) fn apply_gift_value(
        &mut self,
        payload: GiftValuePayload,
        pending: Option<&PendingRequest>,
    ) {
        let Some(pending) = pending.filter(|p| p.purpose == RequestPurpose::GetGiftValueInfo)
        else {
            return;
        };
        let GiftValuePayload::Info(info) = payload;
        if let Some(lookup) = self.payments.hub.gift_value.as_mut() {
            lookup.land(pending.id.0, info);
        }
    }

    pub(crate) fn apply_gift_value_error(
        &mut self,
        err: &TdError,
        pending: Option<&PendingRequest>,
    ) {
        let Some(pending) = pending else {
            return;
        };
        if let Some(lookup) = self.payments.hub.gift_value.as_mut() {
            lookup.fail(
                pending.id.0,
                crate::lang::tr_with(
                    crate::lang::keys::Q_GIFT_VALUE_COULDNT_LOAD,
                    &[("reason", &error_reason(err))],
                ),
            );
        }
    }
}
