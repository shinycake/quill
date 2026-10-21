//! Connect driver: the QR box's requests (`getProfileShareThemeBackdrops`,
//! `getUserLink`, the peer's full info for its share theme, the owned
//! upgraded gifts and `setProfileShareTheme`).
use super::*;
use crate::ids::FileId;
use crate::peer_qr::free::{FreeRequest, FreeTicket};
use crate::peer_qr::link::{OwnLinkState, UserLinkFetch};
use crate::peer_qr::requests::{GiftsOwner, get_share_theme_gifts, set_profile_share_theme};
use crate::peer_qr::state::{QrPeer, SaveTicket};
use crate::peer_qr::theme::{ThemeChoice, ThemeOwner, can_save};
use crate::state::{RequestPurpose, UsersPurpose};
use serde_json::json;

impl<S: JsonSender> ConnectDriver<S> {
    /// The colors panel's backdrops: asked for when the box first opens,
    /// again an hour after the last request, and again after a failure
    /// (tdesktop `ShareThemes::backdropsValue`). The old list stays while
    /// a refresh is on its way.
    pub fn fetch_share_theme_backdrops(&mut self, now: i64) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let backdrops = &mut self.session.users_state.peer_qr.backdrops;
        if !backdrops.due(now) {
            return Ok(());
        }
        backdrops.start(now);
        let extra = self
            .session
            .request(RequestPurpose::GetProfileShareThemeBackdrops, None);
        let request = json!({
            "@type": "getProfileShareThemeBackdrops",
            "@extra": extra.as_extra(),
        });
        if let Err(err) = self.sender.send_json(&request.to_string()) {
            self.session.requests.take(extra);
            self.session.users_state.peer_qr.backdrops.fail();
            return Err(err);
        }
        Ok(())
    }

    /// `peer`'s full info, which carries its saved theme, unless it is
    /// already known (tdesktop `requestFullPeer` when nothing is kept).
    pub fn ensure_share_theme(&mut self, peer: QrPeer) -> Result<(), ConnectSendError> {
        match self.session.qr_theme_owner(peer) {
            Some(ThemeOwner::User(user_id)) => self.fetch_user_full_info(user_id).map(|_| ()),
            Some(ThemeOwner::Supergroup(supergroup_id)) => {
                self.fetch_supergroup_full_info(supergroup_id).map(|_| ())
            }
            None => Ok(()),
        }
    }

    /// The gifts panel's list for `peer`: the first page again (each time
    /// the panel opens, dropping a page on its way), or the next page
    /// when there is one and none is on its way (tdesktop
    /// `CreateQrGiftList`'s `refresh` and `loadMore`).
    pub fn fetch_share_theme_gifts(
        &mut self,
        peer: QrPeer,
        more: bool,
    ) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() || !can_save(self.session.qr_save_rights(peer)) {
            return Err(ConnectSendError::InvalidRequest);
        }
        let owner = self
            .session
            .qr_theme_owner(peer)
            .ok_or(ConnectSendError::InvalidRequest)?;
        let gifts_owner = match peer {
            QrPeer::User(user_id) => GiftsOwner::User(user_id),
            QrPeer::Chat(chat_id) => GiftsOwner::Chat(chat_id),
        };
        let gifts = &mut self.session.users_state.peer_qr.gifts;
        if gifts.owner != Some(owner) {
            *gifts = Default::default();
            gifts.owner = Some(owner);
        }
        if more && (!gifts.loaded || gifts.complete || gifts.loading) {
            return Ok(());
        }
        let offset = if more {
            gifts.offset.clone()
        } else {
            String::new()
        };
        gifts.serial += 1;
        gifts.loading = true;
        let serial = gifts.serial;
        let extra = self.session.request(
            RequestPurpose::Users(UsersPurpose::GetShareThemeGifts {
                owner,
                serial,
                first: !more,
            }),
            None,
        );
        let json = get_share_theme_gifts(extra, gifts_owner, &offset);
        if let Err(err) = self.send_json_request(extra, &json) {
            self.session
                .users_state
                .peer_qr
                .gifts
                .accept(Vec::new(), String::new(), !more);
            return Err(err);
        }
        Ok(())
    }

    /// `setProfileShareTheme` for `peer`. Its answer reaches the box as a
    /// save answer for `ticket`.
    pub fn save_share_theme(
        &mut self,
        ticket: SaveTicket,
        peer: QrPeer,
        choice: ThemeChoice,
    ) -> Result<(), ConnectSendError> {
        let rights = self.session.qr_save_rights(peer);
        let owner = self.session.qr_theme_owner(peer);
        let (true, Some(owner)) = (self.chats_path_active() && can_save(rights), owner) else {
            self.session
                .users_state
                .peer_qr
                .fail_save(ticket, String::new());
            return Err(ConnectSendError::InvalidRequest);
        };
        let chat_id = match peer {
            QrPeer::User(user_id) | QrPeer::Chat(user_id) => user_id,
        };
        let extra = self.session.request(
            RequestPurpose::Users(UsersPurpose::SetProfileShareTheme { ticket }),
            None,
        );
        let Some(json) = set_profile_share_theme(extra, chat_id, &choice) else {
            self.session.requests.take(extra);
            self.session
                .users_state
                .peer_qr
                .fail_save(ticket, String::new());
            return Err(ConnectSendError::InvalidRequest);
        };
        self.session
            .users_state
            .peer_qr
            .start_save(ticket, owner, choice);
        if let Err(err) = self.send_json_request(extra, &json) {
            self.session
                .users_state
                .peer_qr
                .fail_save(ticket, String::new());
            return Err(err);
        }
        Ok(())
    }

    /// Download the gift stickers a share theme draws that are not local
    /// yet.
    pub fn download_share_theme_files(&mut self, files: &[FileId]) {
        for file in files {
            let local = self
                .session
                .media
                .files
                .get(&file.0)
                .is_some_and(|known| known.usable_path().is_some());
            if file.0 != 0 && !local {
                let _ = self.download_file(*file, THUMB_DOWNLOAD_PRIORITY);
            }
        }
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
