//! Connect driver: the boxes behind `giftcode` and `setlanguage` links.
use super::*;
use crate::ids::RequestId;
use crate::state::{GiftCodeLookup, LanguageLinkLookup, RequestPurpose};
use crate::telegram::requests::{
    apply_premium_gift_code, check_premium_gift_code, get_language_pack_info,
};

impl<S: JsonSender> ConnectDriver<S> {
    /// `checkPremiumGiftCode` for a gift code link. The answer lands in
    /// `payments.gift_code`; nothing is applied.
    pub fn request_gift_code_info(&mut self, code: &str) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() || code.is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::CheckPremiumGiftCode, None);
        self.session.payments.gift_code = Some(GiftCodeLookup::new(code.to_string()));
        let result = self.send_json_request(extra, &check_premium_gift_code(extra, code));
        if result.is_err() {
            self.session.payments.gift_code = None;
        }
        result
    }

    /// `applyPremiumGiftCode` for the open box. Only the Apply button calls
    /// this, and only once the code is known to be unused.
    pub fn apply_gift_code(&mut self) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(lookup) = self.session.payments.gift_code.as_mut() else {
            return Err(ConnectSendError::InvalidRequest);
        };
        if lookup.applying || lookup.applied || lookup.info.as_ref().is_none_or(|i| i.is_used()) {
            return Err(ConnectSendError::InvalidRequest);
        }
        lookup.applying = true;
        lookup.error = None;
        let code = lookup.code.clone();
        let extra = self
            .session
            .request(RequestPurpose::ApplyPremiumGiftCode, None);
        let result = self.send_json_request(extra, &apply_premium_gift_code(extra, &code));
        if result.is_err()
            && let Some(lookup) = self.session.payments.gift_code.as_mut()
        {
            lookup.applying = false;
        }
        result
    }

    /// `getLanguagePackInfo` for a `setlanguage` link. Only informs; no
    /// language is installed or selected.
    pub fn request_language_pack_info(&mut self, id: &str) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() || id.is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::GetLanguagePackInfo, None);
        self.session.settings.language_link = Some(LanguageLinkLookup::new(id.to_string()));
        let result = self.send_json_request(extra, &get_language_pack_info(extra, id));
        if result.is_err() {
            self.session.settings.language_link = None;
        }
        result
    }
}
