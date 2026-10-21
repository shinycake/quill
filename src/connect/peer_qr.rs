//! Connect driver: the QR box's requests (`getProfileShareThemeBackdrops`
//! and `getUserLink`).
use super::*;
use crate::peer_qr::free::{FreeRequest, FreeTicket};
use crate::peer_qr::link::{OwnLinkState, UserLinkFetch};
use crate::peer_qr::session::BackdropsFetch;
use crate::state::{RequestPurpose, UsersPurpose};
use serde_json::json;

impl<S: JsonSender> ConnectDriver<S> {
    /// The colors panel's backdrops, once per session (and again after a
    /// failure).
    pub fn fetch_share_theme_backdrops(&mut self) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let backdrops = &mut self.session.users_state.peer_qr.backdrops;
        if matches!(
            backdrops,
            BackdropsFetch::Loading | BackdropsFetch::Ready(_)
        ) {
            return Ok(());
        }
        *backdrops = BackdropsFetch::Loading;
        let extra = self
            .session
            .request(RequestPurpose::GetProfileShareThemeBackdrops, None);
        let request = json!({
            "@type": "getProfileShareThemeBackdrops",
            "@extra": extra.as_extra(),
        });
        if let Err(err) = self.sender.send_json(&request.to_string()) {
            self.session.requests.take(extra);
            self.session.users_state.peer_qr.backdrops = BackdropsFetch::Failed;
            return Err(err);
        }
        Ok(())
    }

    /// Your own box with no username and a hidden number shows a
    /// temporary contact link: ask for one once the privacy rules say it
    /// is needed and none is there or on its way. Cheap to call on every
    /// step while the box is open.
    pub fn ensure_own_contact_link(&mut self, now: i64) -> Result<(), ConnectSendError> {
        match self.session.own_qr_link("", now) {
            Some(OwnLinkState::NeedsContactLink) => self.fetch_own_user_link(None),
            _ => Ok(()),
        }
    }

    /// `getUserLink`: the current user's contact link (`free` is `None`)
    /// or a free-message code. A contact link already on its way is not
    /// asked for twice.
    pub fn fetch_own_user_link(
        &mut self,
        free: Option<FreeTicket>,
    ) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if free.is_none() {
            let contact = &mut self.session.users_state.peer_qr.contact_link;
            if *contact == UserLinkFetch::Loading {
                return Ok(());
            }
            *contact = UserLinkFetch::Loading;
        }
        let extra = self.session.request(
            RequestPurpose::Users(UsersPurpose::GetUserLink { free }),
            None,
        );
        let what = free.map(|ticket| ticket.request);
        let request = json!({
            "@type": "getUserLink",
            "@extra": extra.as_extra(),
            "remove_messages_fee": free.is_some(),
            "return_permanent_link": what.is_some_and(FreeRequest::permanent),
            "revoke_previous_links": what.is_some_and(FreeRequest::revoke),
        });
        if let Err(err) = self.sender.send_json(&request.to_string()) {
            self.session.requests.take(extra);
            self.session
                .users_state
                .peer_qr
                .fail_link(free, String::new());
            return Err(err);
        }
        Ok(())
    }
}
