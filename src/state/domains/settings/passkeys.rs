//! Login passkeys (`crate::passkeys`): the list answer, a removal's `ok`,
//! the creation options, the added passkey and failures. Nothing is
//! optimistic: a passkey leaves the list only after the server confirmed
//! the removal, and joins it only with the server's `passkey`.
use crate::passkeys::{PasskeysPayload, PasskeysPurpose};
use crate::state::*;
use std::time::Instant;

fn passkeys_purpose(purpose: Option<RequestPurpose>) -> Option<PasskeysPurpose> {
    match purpose {
        Some(RequestPurpose::Settings(SettingsPurpose::Passkeys(purpose))) => Some(purpose),
        _ => None,
    }
}

impl Session {
    pub(crate) fn apply_passkeys(
        &mut self,
        payload: PasskeysPayload,
        pending: Option<&PendingRequest>,
    ) {
        let purpose = passkeys_purpose(pending.map(|p| p.purpose));
        match payload {
            PasskeysPayload::List(list) => {
                // Only our own `getLoginPasskeys` answer replaces the list,
                // and not one asked before a removal.
                if let Some(PasskeysPurpose::List(generation)) = purpose {
                    self.settings.passkeys.accept_list(list, generation);
                }
            }
            PasskeysPayload::Added(passkey) => {
                if purpose == Some(PasskeysPurpose::Add) {
                    self.settings.passkeys.accept_added(passkey);
                }
            }
            PasskeysPayload::AddedUnreadable => {
                // No passkey to show: the registration ends here, not in
                // a stage nothing answers.
                if purpose == Some(PasskeysPurpose::Add) {
                    let passkeys = &mut self.settings.passkeys;
                    if passkeys.fail_register(pending.map(|p| p.id)) {
                        passkeys.error =
                            Some("Could not add the passkey: unreadable answer".into());
                    }
                }
            }
        }
    }

    /// `getPasskeyParameters` answered: the ceremony can start. Options
    /// that cannot be read end the attempt with an error line.
    pub(crate) fn apply_passkey_parameters(&mut self, id: Option<RequestId>, json: &str) {
        let passkeys = &mut self.settings.passkeys;
        if !passkeys.accept_parameters(id, json, Instant::now()) {
            passkeys.error = Some("Could not add the passkey: unreadable parameters".into());
        }
    }

    /// `getAuthenticationPasskeyParameters` answered: the sign-in ceremony
    /// can start. tdesktop ignores options it cannot read; Quill says so
    /// under the link.
    pub(crate) fn apply_passkey_login_parameters(&mut self, id: Option<RequestId>, json: &str) {
        let login = &mut self.settings.passkeys.login;
        if !login.accept_parameters(id, json, Instant::now()) {
            crate::webauthn::log("unreadable sign-in options");
            login.fail(crate::passkeys::login::UNREADABLE_OPTIONS.into());
        }
    }

    pub(crate) fn apply_passkeys_ok(&mut self, pending: Option<&PendingRequest>) {
        let id = pending.map(|p| p.id);
        match passkeys_purpose(pending.map(|p| p.purpose)) {
            Some(PasskeysPurpose::Remove) => {
                // Only the removal in flight settles it.
                let passkeys = &mut self.settings.passkeys;
                if let Some(removal) = passkeys.removing.take_if(|r| Some(r.request) == id) {
                    passkeys.finish_remove(&removal.passkey_id);
                }
            }
            // The authorization state moves on by itself (to Ready, or to
            // the password screen after SESSION_PASSWORD_NEEDED).
            Some(PasskeysPurpose::CheckLogin) => self.settings.passkeys.login.finish_check(id),
            _ => {}
        }
    }

    /// `true` when the failed request was a passkey request.
    pub(crate) fn apply_passkeys_error(
        &mut self,
        err: &TdError,
        asked: Option<(RequestId, RequestPurpose)>,
    ) -> bool {
        let passkeys = &mut self.settings.passkeys;
        let id = asked.map(|(id, _)| id);
        match passkeys_purpose(asked.map(|(_, purpose)| purpose)) {
            Some(PasskeysPurpose::List(generation)) => {
                // Retrying is the user's next look, not every ingest. A look
                // older than a removal says nothing about the list now.
                passkeys.list_failed(generation, sessions_error_line("load the passkeys", err));
                true
            }
            Some(PasskeysPurpose::Remove) => {
                // Only the removal in flight fails.
                if passkeys
                    .removing
                    .take_if(|r| Some(r.request) == id)
                    .is_some()
                {
                    passkeys.error = Some(sessions_error_line("delete the passkey", err));
                }
                true
            }
            Some(PasskeysPurpose::Parameters) => {
                if passkeys.fail_register(id) {
                    passkeys.error = Some(sessions_error_line("add the passkey", err));
                }
                true
            }
            Some(PasskeysPurpose::Add) => {
                // tdesktop drops the options after a failed registration.
                if passkeys.fail_register(id) {
                    passkeys.error = Some(sessions_error_line("add the passkey", err));
                }
                true
            }
            Some(PasskeysPurpose::LoginConfig) => {
                passkeys.login.config.fail(login_error_text(err));
                true
            }
            Some(PasskeysPurpose::LoginParameters | PasskeysPurpose::CheckLogin) => {
                // tdesktop `FinishPasskeyLogin`'s fail: forget the options
                // and show the server's error type. Only the request the
                // sign-in waits for fails it.
                crate::webauthn::log(&format!("sign-in step failed: {}", err.code));
                passkeys.login.fail_request(id, login_error_text(err));
                true
            }
            None => false,
        }
    }
}

/// What the QR screen shows for a failed sign-in step: the server's error
/// type, as tdesktop shows it, or the classified line when TDLib's message
/// is not one.
fn login_error_text(err: &TdError) -> String {
    err.server_type
        .clone()
        .unwrap_or_else(|| sessions_error_line("sign in with the passkey", err))
}
