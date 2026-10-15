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
                // Only our own `getLoginPasskeys` answer replaces the list.
                if purpose == Some(PasskeysPurpose::List) {
                    self.settings.passkeys.accept_list(list);
                    self.settings.passkeys.error = None;
                }
            }
            PasskeysPayload::Added(passkey) => {
                if purpose == Some(PasskeysPurpose::Add) {
                    self.settings.passkeys.accept_added(passkey);
                }
            }
        }
    }

    /// `getPasskeyParameters` answered: the ceremony can start. Options
    /// that cannot be read end the attempt with an error line.
    pub(crate) fn apply_passkey_parameters(&mut self, json: &str) {
        let passkeys = &mut self.settings.passkeys;
        if !passkeys.accept_parameters(json, Instant::now()) {
            passkeys.error = Some("Could not add the passkey: unreadable parameters".into());
        }
    }

    pub(crate) fn apply_passkeys_ok(&mut self, pending: Option<&PendingRequest>) {
        if passkeys_purpose(pending.map(|p| p.purpose)) != Some(PasskeysPurpose::Remove) {
            return;
        }
        let passkeys = &mut self.settings.passkeys;
        if let Some(id) = passkeys.removing.take() {
            passkeys.finish_remove(&id);
        }
    }

    /// `true` when the failed request was a passkey request.
    pub(crate) fn apply_passkeys_error(
        &mut self,
        err: &TdError,
        purpose: Option<RequestPurpose>,
    ) -> bool {
        let passkeys = &mut self.settings.passkeys;
        match passkeys_purpose(purpose) {
            Some(PasskeysPurpose::List) => {
                // Retrying is the user's next look, not every ingest.
                passkeys.stale = false;
                passkeys.error = Some(sessions_error_line("load the passkeys", err));
                true
            }
            Some(PasskeysPurpose::Remove) => {
                passkeys.removing = None;
                passkeys.error = Some(sessions_error_line("delete the passkey", err));
                true
            }
            Some(PasskeysPurpose::Parameters) => {
                passkeys.error = Some(sessions_error_line("add the passkey", err));
                true
            }
            Some(PasskeysPurpose::Add) => {
                // tdesktop drops the options after a failed registration.
                passkeys.adding = false;
                passkeys.registration = None;
                passkeys.error = Some(sessions_error_line("add the passkey", err));
                true
            }
            None => false,
        }
    }
}
