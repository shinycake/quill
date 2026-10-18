//! Connect driver: promoted chats (`chatPosition.source`). The announcement
//! badge text comes from the language pack; "Hide this announcement" is
//! `deleteChatHistory` with `remove_from_chat_list`, which TDLib turns into
//! `help.hidePromoData` for an announcement.
use super::*;
use crate::ids::{ChatId, RequestId};
use crate::promotion::Promotion;
use crate::state::{LanguagePurpose, RequestPurpose, SettingsPurpose};
use crate::telegram::requests::{delete_chat_history, get_language_pack_keys};

impl<S: JsonSender> ConnectDriver<S> {
    /// Ask for the badge text of announcement types that are on the list
    /// and not asked for yet. Called from the app's poll loop; a no-op
    /// otherwise.
    pub fn pump_psa_labels(&mut self) {
        if !self.chats_path_active() {
            return;
        }
        let list = &mut self.session.chat_list;
        let keys = list
            .psa_labels
            .take_wanted(list.promoted_chats.values().filter_map(Promotion::psa_kind));
        if keys.is_empty() {
            return;
        }
        let pack = self
            .session
            .settings
            .language
            .current
            .as_ref()
            .map_or_else(|| "en".to_string(), |info| info.id.clone());
        let purpose =
            RequestPurpose::Settings(SettingsPurpose::Language(LanguagePurpose::PsaLabels));
        let extra = self.session.request(purpose, None);
        // A failed send leaves the generic "PSA" badge, which is fine.
        let _ = self.send_json_request(extra, &get_language_pack_keys(extra, &pack, &keys));
    }

    /// "Hide this announcement": only for a promoted announcement with a
    /// type (tdesktop `addHidePromotion`).
    pub fn hide_promotion(
        &mut self,
        chat_id: ChatId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let purpose = RequestPurpose::RemoveChatFromList;
        if self.session.requests.has_purpose_for_chat(purpose, chat_id)
            || !self
                .session
                .chat_list
                .promoted_chats
                .get(&chat_id.0)
                .is_some_and(Promotion::can_hide)
        {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        self.send_json_request(extra, &delete_chat_history(extra, chat_id.0, true, false))
            .map(Some)
    }
}
