//! Connect driver: the account's login passkeys (`crate::passkeys`).
//! Lists them, removes one, and creates one: the creation options for the
//! ceremony (`crate::webauthn`) and its result to the server.
use super::*;
use crate::ids::RequestId;
use crate::passkeys::{
    PasskeyAdd, PasskeysPurpose, add_login_passkey, get_login_passkeys, get_passkey_parameters,
    remove_login_passkey,
};
use crate::state::{RequestPurpose, SettingsPurpose};
use crate::webauthn::Registration;
use std::time::Instant;

fn purpose(purpose: PasskeysPurpose) -> RequestPurpose {
    RequestPurpose::Settings(SettingsPurpose::Passkeys(purpose))
}

impl<S: JsonSender> ConnectDriver<S> {
    fn list_in_flight(&self) -> bool {
        self.session
            .requests
            .has_purpose(purpose(PasskeysPurpose::List(
                self.session.settings.passkeys.generation,
            )))
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
        if !state.wants_fetch(Instant::now()) || self.list_in_flight() {
            return Ok(None);
        }
        let generation = state.generation;
        let extra = self
            .session
            .request(purpose(PasskeysPurpose::List(generation)), None);
        let sent = self.send_json_request(extra, &get_login_passkeys(extra))?;
        // Only a look that went out counts: a failed send leaves the list
        // stale and the throttle open for the next try, and the answer
        // (not the send) clears the stale mark.
        let state = &mut self.session.settings.passkeys;
        state.requested_at = Some(Instant::now());
        state.error = None;
        Ok(Some(sent))
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

    /// "Create Passkey" (tdesktop `Passkeys::initRegistration`): the
    /// creation options, reused while still within their timeout, else
    /// `getPasskeyParameters`. Once they are known the state holds them
    /// in `add` for the dialog to start. `Ok(None)` when no request was
    /// needed.
    pub fn begin_register(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        let state = &self.session.settings.passkeys;
        if !self.chats_path_active()
            || !state.can_register()
            || matches!(state.add, PasskeyAdd::Sending(_))
        {
            return Err(ConnectSendError::InvalidRequest);
        }
        match &state.add {
            PasskeyAdd::FetchingOptions(_) => return Ok(None),
            PasskeyAdd::Starting(pending) | PasskeyAdd::Cached(pending)
                if pending.fresh(Instant::now()) =>
            {
                let pending = pending.clone();
                let state = &mut self.session.settings.passkeys;
                state.add = PasskeyAdd::Starting(pending);
                return Ok(None);
            }
            _ => {}
        }
        let extra = self
            .session
            .request(purpose(PasskeysPurpose::Parameters), None);
        let sent = self.send_json_request(extra, &get_passkey_parameters(extra))?;
        // Options left over (answered after the lock, expired) are
        // replaced: the fresh ones start the ceremony, not a second system
        // sheet after an expired one. The request id is what the answer is
        // matched to, so the answer of a box closed earlier is ignored.
        let state = &mut self.session.settings.passkeys;
        state.add = PasskeyAdd::FetchingOptions(sent);
        state.error = None;
        Ok(Some(sent))
    }

    /// Whether "Create Passkey" is waiting for the server's options.
    pub fn passkey_parameters_pending(&self) -> bool {
        matches!(
            self.session.settings.passkeys.add,
            PasskeyAdd::FetchingOptions(_)
        )
    }

    /// The ceremony box was closed before it finished: options still on
    /// their way are not started for it.
    pub fn cancel_register(&mut self) {
        self.session.settings.passkeys.cancel_register();
    }

    /// The ceremony finished: `addLoginPasskey` with its client data and
    /// attestation object (tdesktop `Passkeys::registerPasskey`).
    pub fn finish_register(
        &mut self,
        registration: &Registration,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active()
            || matches!(self.session.settings.passkeys.add, PasskeyAdd::Sending(_))
        {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self.session.request(purpose(PasskeysPurpose::Add), None);
        let sent = self.send_json_request(extra, &add_login_passkey(extra, registration))?;
        let state = &mut self.session.settings.passkeys;
        state.add = PasskeyAdd::Sending(sent);
        state.error = None;
        Ok(sent)
    }

    /// After each ingest: on the sign-in screens ask for the app config
    /// once (the passkey link), fetch the list again once a removal marked
    /// it stale, and ask for the software icons nobody has resolved yet.
    pub(crate) fn after_passkeys_ingest(&mut self) {
        self.maybe_request_login_config();
        if self.session.settings.passkeys.stale {
            let _ = self.request_passkeys();
        }
        let _ = self.maybe_resolve_passkey_icons();
    }

    /// `getCustomEmojiStickers` for the passkeys' software icons, once per
    /// id (shares the custom emoji cache with message text).
    fn maybe_resolve_passkey_icons(&mut self) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() || !self.custom_emoji_lookup_idle() {
            return Ok(());
        }
        let emoji = &self.session.stickers.emoji;
        let mut ids: Vec<i64> = self
            .session
            .settings
            .passkeys
            .icon_ids()
            .filter(|id| {
                !emoji.custom_lookups.attempted(*id)
                    && !emoji
                        .custom_emoji_stickers
                        .iter()
                        .any(|item| item.custom_emoji_id == Some(*id))
            })
            .collect();
        ids.sort_unstable();
        ids.dedup();
        self.lookup_custom_emoji(&ids).map(|_| ())
    }
}
