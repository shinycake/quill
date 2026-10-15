//! Connect driver: the value of an upgraded gift
//! (`crate::premium_hub::gift_value`). Sends only the read
//! `getUpgradedGiftValueInfo`.
use super::*;
use crate::ids::RequestId;
use crate::premium_hub::gift_value::{GiftValueLookup, get_upgraded_gift_value_info};
use crate::state::RequestPurpose;

impl<S: JsonSender> ConnectDriver<S> {
    /// The Value row's "learn more" (tdesktop `MakeUniqueGiftValueValue`):
    /// asks for the value of `lookup.name`; the box opens with the answer.
    /// A gift whose value is already known reopens at once, and a second
    /// click while its request runs sends nothing (`Ok(None)` for both).
    /// Asking about another gift replaces the lookup, so the older answer
    /// is dropped when it lands.
    pub fn request_gift_value(
        &mut self,
        lookup: GiftValueLookup,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if let Some(current) = self.session.payments.hub.gift_value.as_mut()
            && current.name == lookup.name
        {
            if current.info.is_some() {
                current.open = true;
                return Ok(None);
            }
            if current.loading {
                return Ok(None);
            }
        }
        let extra = self.session.request(RequestPurpose::GetGiftValueInfo, None);
        let json = get_upgraded_gift_value_info(extra, &lookup.name);
        self.session.payments.hub.gift_value = Some(GiftValueLookup {
            request: extra.0,
            loading: true,
            ..lookup
        });
        if let Err(err) = self.send_json_request(extra, &json) {
            if let Some(current) = self.session.payments.hub.gift_value.as_mut() {
                current.loading = false;
                current.error = Some(crate::lang::tr_with(
                    crate::lang::keys::Q_GIFT_VALUE_COULDNT_LOAD,
                    &[("reason", &format!("{err:?}"))],
                ));
            }
            return Err(err);
        }
        Ok(Some(extra))
    }
}
