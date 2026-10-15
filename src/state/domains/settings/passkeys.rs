//! Login passkeys (`crate::passkeys`): the list answer, a removal's `ok`
//! and failures. Nothing is optimistic: a passkey leaves the list only
//! after the server confirmed the removal, and the list is fetched again.
use crate::passkeys::{PasskeysPayload, PasskeysPurpose};
use crate::state::*;

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
        match payload {
            PasskeysPayload::List(list) => {
                // Only our own `getLoginPasskeys` answer replaces the list.
                if passkeys_purpose(pending.map(|p| p.purpose)) == Some(PasskeysPurpose::List) {
                    self.settings.passkeys.accept_list(list);
                    self.settings.passkeys.error = None;
                }
            }
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
            None => false,
        }
    }
}
