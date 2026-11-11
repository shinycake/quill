//! Batch 4: presence and account notices — the TDLib `online` option, the
//! new-login alert review and the terms of service accept.
use super::*;
use crate::ids::RequestId;
use crate::state::RequestPurpose;
use crate::state::SettingsPurpose;
use crate::telegram::requests::{
    accept_terms_of_service, confirm_session, set_option_boolean, terminate_session,
};

impl<S: JsonSender> ConnectDriver<S> {
    /// `setOption("online", value)` (schema 1.8.67, :15662). TDLib's
    /// default is false and it keeps re-announcing a true value itself,
    /// so callers send only changes (`crate::presence::PresenceSync`).
    pub fn set_online(&mut self, online: bool) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self.session.request(RequestPurpose::SetOnline, None);
        match self
            .sender
            .send_json(&set_option_boolean(extra, "online", online))
        {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// The new-login alert's answer: "Yes, it's me" confirms every
    /// unconfirmed session (`confirmSession`), "No, it's not me!"
    /// terminates them (`terminateSession`) — tdesktop reviews them all
    /// at once. The session ids come from `getActiveSessions`
    /// (`is_unconfirmed`); the alert only offers the buttons once they
    /// are known.
    pub fn review_unconfirmed_sessions(&mut self, confirmed: bool) -> Result<(), ConnectSendError> {
        if !self.chats_path_active()
            || self.session.settings.notices.review.busy()
            || self.session.settings.notices.unconfirmed_entries.is_empty()
        {
            return Err(ConnectSendError::InvalidRequest);
        }
        let ids: Vec<i64> = self
            .session
            .settings
            .notices
            .unconfirmed_entries
            .iter()
            .map(|entry| entry.id)
            .collect();
        self.session.begin_login_review(confirmed, ids.len());
        for (at, id) in ids.iter().copied().enumerate() {
            let extra = self.session.request(
                RequestPurpose::Settings(SettingsPurpose::ReviewUnconfirmedSession {
                    confirmed,
                    session_id: id,
                }),
                None,
            );
            let json = if confirmed {
                confirm_session(extra, id)
            } else {
                terminate_session(extra, id)
            };
            if let Err(err) = self.sender.send_json(&json) {
                self.session.requests.take(extra);
                // This one and the ones not sent yet will not be answered.
                for id in ids.iter().skip(at).copied() {
                    self.session
                        .finish_login_review(id, Some("Could not review the new login.".into()));
                }
                return Err(err);
            }
        }
        Ok(())
    }

    /// `acceptTermsOfService` (schema 1.8.67, :16143) for the pending
    /// `updateTermsOfService`.
    pub fn accept_terms(&mut self) -> Result<RequestId, ConnectSendError> {
        let Some(terms) = self.session.settings.notices.terms.clone() else {
            return Err(ConnectSendError::InvalidRequest);
        };
        if self.session.settings.notices.terms_accept.is_loading() {
            return Err(ConnectSendError::InvalidRequest);
        }
        // The box cannot be closed: a press that cannot go out says why.
        let not_sent = "Could not send your answer. Please try again.";
        if !self.chats_path_active() {
            self.session.settings.notices.terms_accept.fail(not_sent);
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::AcceptTermsOfService, None);
        self.session.settings.notices.terms_accept.begin();
        match self
            .sender
            .send_json(&accept_terms_of_service(extra, &terms.id))
        {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.settings.notices.terms_accept.fail(not_sent);
                Err(err)
            }
        }
    }
}
