//! Connect driver: promoted chats (`chatPosition.source`). The announcement
//! badge text comes from the language pack; "Hide this announcement" is
//! `deleteChatHistory` with `remove_from_chat_list`, which TDLib turns into
//! `help.hidePromoData` for an announcement.
use super::*;
use crate::ids::{ChatId, RequestId};
use crate::promotion::top_promotion;
use crate::state::{LanguagePurpose, RequestPurpose, SettingsPurpose};
use crate::telegram::requests::{delete_chat_history, get_language_pack_keys};

impl<S: JsonSender> ConnectDriver<S> {
    /// The language pack the interface uses, or is switching to.
    fn badge_language(&self) -> String {
        let language = &self.session.settings.language;
        let info = language
            .switching
            .as_ref()
            .map(|switch| &switch.info)
            .or(language.current.as_ref());
        match info {
            Some(info) if !info.id.is_empty() => info.id.clone(),
            _ => "en".to_string(),
        }
    }

    /// Ask for the badge text of the announcement types on the list in the
    /// current language, once per type and language. Called from the app's
    /// poll loop; a no-op otherwise.
    pub fn pump_psa_labels(&mut self) {
        self.sync_psa_about();
        if !self.chats_path_active() {
            return;
        }
        let pack = self.badge_language();
        let session = &self.session;
        let kinds: Vec<String> = session
            .chat_list
            .promoted_chats
            .keys()
            .filter_map(|id| session.chats.get(id))
            .filter_map(|chat| top_promotion(session, chat))
            .filter_map(|promotion| promotion.psa_kind().map(str::to_string))
            .collect();
        let (epoch, keys) = self
            .session
            .chat_list
            .psa_labels
            .take_wanted(&pack, kinds.iter().map(String::as_str));
        if keys.is_empty() {
            return;
        }
        let purpose =
            RequestPurpose::Settings(SettingsPurpose::Language(LanguagePurpose::PsaLabels {
                epoch,
            }));
        let extra = self.session.request(purpose, None);
        // A failed send leaves the generic "PSA" badge, which is fine.
        let _ = self.send_json_request(extra, &get_language_pack_keys(extra, &pack, &keys));
    }

    /// Read the saved "box was shown" flags (in the background, once),
    /// show the box of a chat that waited for them, and save new flags.
    pub(crate) fn sync_psa_about(&mut self) {
        self.session.chat_list.psa_about.start_load();
        self.session.poll_psa_about(std::time::Instant::now());
        self.session.chat_list.psa_about.save_unsaved();
    }

    /// "Hide this announcement": only where tdesktop offers it, a promoted
    /// announcement with a type in a channel the account is not in
    /// (`addHidePromotion`).
    pub fn hide_promotion(
        &mut self,
        chat_id: ChatId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let purpose = RequestPurpose::HidePromotion;
        let session = &self.session;
        let hideable = session
            .chats
            .get(&chat_id.0)
            .and_then(|chat| top_promotion(session, chat))
            .is_some_and(|promotion| promotion.can_hide());
        if !hideable || session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        self.send_json_request(extra, &delete_chat_history(extra, chat_id.0, true, false))
            .map(Some)
    }
}
