//! Connect driver: the account's login passkeys (`crate::passkeys`).
//! Lists them and removes one; creating one waits for WebAuthn.
use super::*;
use crate::ids::RequestId;
use crate::passkeys::{PasskeysPurpose, get_login_passkeys, remove_login_passkey};
use crate::state::{RequestPurpose, SettingsPurpose};
use std::time::Instant;

fn purpose(purpose: PasskeysPurpose) -> RequestPurpose {
    RequestPurpose::Settings(SettingsPurpose::Passkeys(purpose))
}

impl<S: JsonSender> ConnectDriver<S> {
    fn passkeys_in_flight(&self, passkeys: PasskeysPurpose) -> bool {
        self.session.requests.has_purpose(purpose(passkeys))
    }

    /// A look at the passkeys (the Privacy and Security screen or the
    /// Passkeys list): `getLoginPasskeys` unless the list was asked for in
    /// the last 5 seconds or is on its way (tdesktop
    /// `Passkeys::requestList`). Also asks for the application config once,
    /// for `settings_display_passkeys`.
    pub fn request_passkeys(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let _ = self.ensure_earn_app_config();
        let state = &self.session.settings.passkeys;
        if !state.wants_fetch(Instant::now()) || self.passkeys_in_flight(PasskeysPurpose::List) {
            return Ok(None);
        }
        let extra = self.session.request(purpose(PasskeysPurpose::List), None);
        let state = &mut self.session.settings.passkeys;
        state.requested_at = Some(Instant::now());
        state.stale = false;
        state.error = None;
        self.send_json_request(extra, &get_login_passkeys(extra))
            .map(Some)
    }

    /// `removeLoginPasskey` for a passkey in the list, one at a time. The
    /// passkey leaves the list on the server's `ok`, and the list is asked
    /// for again.
    pub fn remove_passkey(&mut self, passkey_id: &str) -> Result<RequestId, ConnectSendError> {
        let state = &self.session.settings.passkeys;
        if !self.chats_path_active() || state.removing.is_some() || state.find(passkey_id).is_none()
        {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self.session.request(purpose(PasskeysPurpose::Remove), None);
        let state = &mut self.session.settings.passkeys;
        state.removing = Some(passkey_id.to_string());
        state.error = None;
        let sent = self.send_json_request(extra, &remove_login_passkey(extra, passkey_id));
        if sent.is_err() {
            self.session.settings.passkeys.removing = None;
        }
        sent
    }

    /// After each ingest: fetch the list again once a removal marked it
    /// stale, and ask for the software icons nobody has resolved yet.
    pub(crate) fn after_passkeys_ingest(&mut self) {
        if self.session.settings.passkeys.stale {
            let _ = self.request_passkeys();
        }
        let _ = self.maybe_resolve_passkey_icons();
    }

    /// `getCustomEmojiStickers` for the passkeys' software icons, once per
    /// id (shares the custom emoji cache with message text).
    fn maybe_resolve_passkey_icons(&mut self) -> Result<(), ConnectSendError> {
        if !self.chats_path_active()
            || self
                .session
                .requests
                .has_purpose(RequestPurpose::GetCustomEmojiStickers)
        {
            return Ok(());
        }
        let emoji = &self.session.stickers.emoji;
        let mut ids: Vec<i64> = self
            .session
            .settings
            .passkeys
            .icon_ids()
            .filter(|id| {
                !emoji.status_resolution_attempted.contains(id)
                    && !emoji
                        .custom_emoji_stickers
                        .iter()
                        .any(|item| item.custom_emoji_id == Some(*id))
            })
            .collect();
        ids.sort_unstable();
        ids.dedup();
        if ids.is_empty() {
            return Ok(());
        }
        self.session
            .stickers
            .emoji
            .status_resolution_attempted
            .extend(ids.iter().copied());
        let extra = self
            .session
            .request(RequestPurpose::GetCustomEmojiStickers, None);
        let json = crate::telegram::requests_emoji::get_custom_emoji_stickers(extra, &ids);
        self.send_json_request(extra, &json).map(|_| ())
    }
}
