//! Connect driver: device login, active sessions, account TTL, auto-delete and connected websites.
use super::*;

impl<S: JsonSender> ConnectDriver<S> {
    /// Authorize only a validated scan after explicit user confirmation.
    pub fn confirm_device_login(
        &mut self,
        scanned_link: &str,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active()
            || self.session.sessions_mutating
            || !crate::auth::is_device_login_qr(scanned_link)
        {
            return Err(ConnectSendError::InvalidRequest);
        }
        let id = self
            .session
            .request(RequestPurpose::ConfirmDeviceLogin, None);
        self.session.device_login_result = None;
        self.session.sessions_mutating = true;
        self.session.sessions_error = None;
        let request = zeroize::Zeroizing::new(
            crate::telegram::requests::confirm_qr_code_authentication(id, scanned_link),
        );
        if let Err(error) = self.sender.send_json(&request) {
            self.session.requests.take(id);
            self.session.sessions_mutating = false;
            return Err(error);
        }
        Ok(id)
    }

    /// Slice A3: `getActiveSessions` (schema 1.8.67, line 15102) — once
    /// per session unless the list was marked stale by a terminate or an
    /// explicit refresh (guarded by the cache and the in-flight purpose).
    /// `Ok(None)` = no request needed.
    pub fn maybe_fetch_active_sessions(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if (self.session.sessions.is_some() && !self.session.sessions_stale)
            || self
                .session
                .requests
                .has_purpose(RequestPurpose::GetActiveSessions)
        {
            return Ok(None);
        }
        let extra = self
            .session
            .request(RequestPurpose::GetActiveSessions, None);
        self.session.sessions_loading = true;
        self.session.sessions_error = None;
        match self.sender.send_json(&get_active_sessions(extra)) {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.sessions_loading = false;
                Err(err)
            }
        }
    }

    /// Slice A3: refetch the sessions list after a terminate marked it
    /// stale — the reducer kept the old cache and marked it stale on the
    /// authoritative `ok` (the `refresh_notification_sounds_if_stale`
    /// pattern).
    pub fn refresh_active_sessions_if_stale(
        &mut self,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.session.sessions_stale {
            return Ok(None);
        }
        self.maybe_fetch_active_sessions()
    }

    /// Slice A3: send `terminateSession` (schema 1.8.67, line 15105).
    /// One mutation at a time; the list is refetched from the
    /// authoritative `ok` response — never optimistic. A doomed request
    /// (no such session in the cache) is rejected before it leaves;
    /// TDLib is the authority for the rest.
    pub fn terminate_session(&mut self, session_id: i64) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() || self.session.sessions_mutating {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self
            .session
            .sessions
            .as_ref()
            .is_some_and(|s| s.iter().any(|s| s.id == session_id))
        {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.sessions_error = None;
        let extra = self.session.request(
            RequestPurpose::Settings(SettingsPurpose::TerminateSession { session_id }),
            None,
        );
        self.session.sessions_mutating = true;
        match self.sender.send_json(&terminate_session(extra, session_id)) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.sessions_mutating = false;
                Err(err)
            }
        }
    }

    /// Slice A3: send `terminateAllOtherSessions` (schema 1.8.67, line
    /// 15108). One mutation at a time; the list is refetched from the
    /// authoritative `ok` response — never optimistic.
    pub fn terminate_all_other_sessions(&mut self) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() || self.session.sessions_mutating {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.sessions_error = None;
        let extra = self
            .session
            .request(RequestPurpose::TerminateAllOtherSessions, None);
        self.session.sessions_mutating = true;
        match self.sender.send_json(&terminate_all_other_sessions(extra)) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.sessions_mutating = false;
                Err(err)
            }
        }
    }

    /// Slice A7: send `deleteAccount` (schema 1.8.67, line 15675).
    /// Guarded on the authorized chats path; one mutation at a time.
    /// The password rides the request JSON only — never stored on the
    /// session or diagnostics (the A2 `password_op_send` rule).
    pub fn delete_account(
        &mut self,
        reason: &str,
        password: &str,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() || self.session.account_mutating {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.account_error = None;
        let extra = self.session.request(RequestPurpose::DeleteAccount, None);
        self.session.account_mutating = true;
        match self
            .sender
            .send_json(&delete_account(extra, reason, password))
        {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.account_mutating = false;
                Err(err)
            }
        }
    }

    /// `getDefaultMessageAutoDeleteTime` (schema 1.8.67, line 15682). The
    /// cached value is reused and an in-flight request never duplicated
    /// (`Ok(None)` = nothing sent).
    pub fn get_default_auto_delete(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self.session.default_auto_delete_secs.is_some() || self.session.default_auto_delete_busy
        {
            return Ok(None);
        }
        self.session.default_auto_delete_error = None;
        let extra = self
            .session
            .request(RequestPurpose::GetDefaultAutoDelete, None);
        self.session.default_auto_delete_busy = true;
        let json = crate::telegram::requests::get_default_message_auto_delete_time(extra);
        match self.sender.send_json(&json) {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.default_auto_delete_busy = false;
                Err(err)
            }
        }
    }

    /// `setDefaultMessageAutoDeleteTime` (schema 1.8.67, line 15679). Only
    /// 0 or whole days up to a year go out; the confirmed value lands from
    /// the authoritative `ok`.
    pub fn set_default_auto_delete(&mut self, seconds: i32) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active()
            || self.session.default_auto_delete_busy
            || !crate::auto_delete::is_valid_regular_ttl(seconds)
        {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.default_auto_delete_error = None;
        let extra = self.session.request(
            RequestPurpose::Settings(SettingsPurpose::SetDefaultAutoDelete { seconds }),
            None,
        );
        self.session.default_auto_delete_busy = true;
        let json = crate::telegram::requests::set_default_message_auto_delete_time(extra, seconds);
        match self.sender.send_json(&json) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.default_auto_delete_busy = false;
                Err(err)
            }
        }
    }

    /// Slice A7: send `getAccountTtl` (schema 1.8.67, line 15669). The
    /// cached value is reused and an in-flight fetch is never duplicated
    /// (`Ok(None)` = no request needed). The `password_op_send` fetch
    /// pattern, minus the password bookkeeping.
    pub fn get_account_ttl(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self.session.account_ttl_days.is_some() || self.session.account_ttl_loading {
            return Ok(None);
        }
        self.session.account_error = None;
        let extra = self.session.request(RequestPurpose::GetAccountTtl, None);
        self.session.account_ttl_loading = true;
        match self.sender.send_json(&get_account_ttl(extra)) {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.account_ttl_loading = false;
                Err(err)
            }
        }
    }

    /// Slice A7: send `setAccountTtl` (schema 1.8.67, line 15666). One
    /// mutation at a time; the confirmed days land from the
    /// authoritative `ok` (never an optimistic write).
    pub fn set_account_ttl(&mut self, days: i32) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() || self.session.account_mutating {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.account_error = None;
        let extra = self.session.request(
            RequestPurpose::Settings(SettingsPurpose::SetAccountTtl { days }),
            None,
        );
        self.session.account_mutating = true;
        match self.sender.send_json(&set_account_ttl(extra, days)) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.account_mutating = false;
                Err(err)
            }
        }
    }

    /// Slice A4: send `toggleSessionCanAcceptSecretChats` (schema 1.8.67,
    /// line 15117). The toggled value is the negation of the cached
    /// flag — one mutation at a time; the list is refetched from the
    /// authoritative `ok` response, never optimistically. A doomed
    /// request (no such session in the cache) is rejected before it
    /// leaves; TDLib is the authority for the rest.
    pub fn toggle_session_can_accept_secret_chats(
        &mut self,
        session_id: i64,
    ) -> Result<RequestId, ConnectSendError> {
        self.send_session_toggle(session_id, ToggleSessionKind::SecretChats)
    }

    /// Slice A4: send `toggleSessionCanAcceptCalls` (schema 1.8.67, line
    /// 15114) — the `toggleSessionCanAcceptSecretChats` twin.
    pub fn toggle_session_can_accept_calls(
        &mut self,
        session_id: i64,
    ) -> Result<RequestId, ConnectSendError> {
        self.send_session_toggle(session_id, ToggleSessionKind::Calls)
    }

    /// Slice A4: shared send path for the two per-session toggles.
    fn send_session_toggle(
        &mut self,
        session_id: i64,
        kind: ToggleSessionKind,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() || self.session.sessions_mutating {
            return Err(ConnectSendError::InvalidRequest);
        }
        let flags = self
            .session
            .sessions
            .as_ref()
            .and_then(|s| s.iter().find(|s| s.id == session_id))
            .map(|s| (s.can_accept_secret_chats, s.can_accept_calls));
        let Some((can_accept_secret_chats, can_accept_calls)) = flags else {
            return Err(ConnectSendError::InvalidRequest);
        };
        let (extra, json) = match kind {
            ToggleSessionKind::SecretChats => {
                let extra = self.session.request(
                    RequestPurpose::Settings(SettingsPurpose::ToggleSessionSecretChats {
                        session_id,
                    }),
                    None,
                );
                let value = !can_accept_secret_chats;
                (
                    extra,
                    toggle_session_can_accept_secret_chats(extra, session_id, value),
                )
            }
            ToggleSessionKind::Calls => {
                let extra = self.session.request(
                    RequestPurpose::Settings(SettingsPurpose::ToggleSessionCalls { session_id }),
                    None,
                );
                let value = !can_accept_calls;
                (
                    extra,
                    toggle_session_can_accept_calls(extra, session_id, value),
                )
            }
        };
        self.session.sessions_error = None;
        self.session.sessions_mutating = true;
        match self.sender.send_json(&json) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.sessions_mutating = false;
                Err(err)
            }
        }
    }

    /// Slice A4: `getConnectedWebsites` (schema 1.8.67, line 15124) —
    /// guarded-once like `maybe_fetch_active_sessions` (cached state
    /// reused, in-flight fetch deduped).
    pub fn maybe_fetch_connected_websites(
        &mut self,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if (self.session.connected_websites.is_some() && !self.session.websites_stale)
            || self
                .session
                .requests
                .has_purpose(RequestPurpose::GetConnectedWebsites)
        {
            return Ok(None);
        }
        let extra = self
            .session
            .request(RequestPurpose::GetConnectedWebsites, None);
        self.session.connected_websites_loading = true;
        self.session.websites_error = None;
        match self.sender.send_json(&get_connected_websites(extra)) {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.connected_websites_loading = false;
                Err(err)
            }
        }
    }

    /// Slice A4: refetch the websites list after a disconnect marked it
    /// stale — the reducer kept the old cache and marked it stale on the
    /// authoritative `ok` (the `refresh_active_sessions_if_stale`
    /// pattern).
    pub fn refresh_connected_websites_if_stale(
        &mut self,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.session.websites_stale {
            return Ok(None);
        }
        self.maybe_fetch_connected_websites()
    }

    /// Slice A4: send `disconnectWebsite` (schema 1.8.67, line 15127).
    /// One mutation at a time; the list is refetched from the
    /// authoritative `ok` response — never optimistic. A doomed request
    /// (no such website in the cache) is rejected before it leaves;
    /// TDLib is the authority for the rest.
    pub fn disconnect_website(&mut self, website_id: i64) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() || self.session.websites_mutating {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self
            .session
            .connected_websites
            .as_ref()
            .is_some_and(|s| s.iter().any(|s| s.id == website_id))
        {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.websites_error = None;
        let extra = self.session.request(
            RequestPurpose::Settings(SettingsPurpose::DisconnectWebsite { website_id }),
            None,
        );
        self.session.websites_mutating = true;
        match self
            .sender
            .send_json(&disconnect_website(extra, website_id))
        {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.websites_mutating = false;
                Err(err)
            }
        }
    }

    /// Slice A4: send `disconnectAllWebsites` (schema 1.8.67, line
    /// 15130). One mutation at a time; the list is refetched from the
    /// authoritative `ok` response — never optimistic.
    pub fn disconnect_all_websites(&mut self) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() || self.session.websites_mutating {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self
            .session
            .connected_websites
            .as_ref()
            .is_some_and(|s| !s.is_empty())
        {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.websites_error = None;
        let extra = self
            .session
            .request(RequestPurpose::DisconnectAllWebsites, None);
        self.session.websites_mutating = true;
        match self.sender.send_json(&disconnect_all_websites(extra)) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.websites_mutating = false;
                Err(err)
            }
        }
    }
}
