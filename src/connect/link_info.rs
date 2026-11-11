//! Connect driver: the boxes behind `giftcode` and `setlanguage` links.
use super::*;
use crate::ids::RequestId;
use crate::state::{GiftCodeLookup, GiftCodeStage, LanguageLinkLookup, RequestPurpose};
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
        // An apply in flight owns the box until it answers: a new check
        // would drop its request, and Premium applied would never show.
        if self
            .session
            .payments
            .gift_code
            .as_ref()
            .is_some_and(|lookup| matches!(lookup.stage, GiftCodeStage::Applying { .. }))
        {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::CheckPremiumGiftCode, None);
        let previous = self
            .session
            .payments
            .gift_code
            .replace(GiftCodeLookup::checking(code.to_string(), extra));
        let result = self.send_json_request(extra, &check_premium_gift_code(extra, code));
        if result.is_err() {
            // The new check never left: the box keeps what it showed.
            self.session.payments.gift_code = previous;
        }
        result
    }

    /// `applyPremiumGiftCode` for the open box. Only the Apply button calls
    /// this, and only once the code is known to be unused.
    pub fn apply_gift_code(&mut self) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(lookup) = self.session.payments.gift_code.as_ref() else {
            return Err(ConnectSendError::InvalidRequest);
        };
        if !lookup.can_apply() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(info) = lookup.info().cloned() else {
            return Err(ConnectSendError::InvalidRequest);
        };
        let before = lookup.stage.clone();
        let code = lookup.code.clone();
        let extra = self
            .session
            .request(RequestPurpose::ApplyPremiumGiftCode, None);
        if let Some(lookup) = self.session.payments.gift_code.as_mut() {
            lookup.stage = GiftCodeStage::Applying {
                request: extra,
                info,
            };
        }
        let result = self.send_json_request(extra, &apply_premium_gift_code(extra, &code));
        if result.is_err()
            && let Some(lookup) = self.session.payments.gift_code.as_mut()
        {
            // The apply never left: back to the stage it was asked from.
            lookup.stage = before;
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
        self.session.settings.language_link =
            Some(LanguageLinkLookup::looking_up(id.to_string(), extra));
        let result = self.send_json_request(extra, &get_language_pack_info(extra, id));
        if result.is_err() {
            self.session.settings.language_link = None;
        }
        result
    }
}
