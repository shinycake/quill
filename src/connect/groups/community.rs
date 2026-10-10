//! Connect driver: communities (create, info, name, photo, permissions, delete).
use super::*;

impl<S: JsonSender> ConnectDriver<S> {
    /// Slice (communities backend core): `createCommunity` (schema 1.8.67,
    /// line 11806) — creates a community from an owned chat (owned basic
    /// group / supergroup / channel, or a chat with an owned bot; basic
    /// groups are auto-upgraded to supergroups). Empty names are refused
    /// client-side (`Err(InvalidRequest)`); the chat must exist or the
    /// call is refused with `Ok(None)`. The response is `communityId`
    /// (the update `updateCommunity` is guaranteed to arrive first); the
    /// ingest chain resolves the id into `getCommunityFullInfo`.
    pub fn create_community(
        &mut self,
        chat_id: ChatId,
        name: &str,
        is_chat_hidden: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if name.trim().is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self.session.chats.contains_key(&chat_id.0) {
            return Ok(None);
        }
        let purpose = RequestPurpose::CreateCommunity;
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        if let Err(err) =
            self.sender
                .send_json(&create_community(extra, name, chat_id.0, is_chat_hidden))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// `getCommunityFullInfo` (TDLib 1.8.68; replaced
    /// `loadCommunityFullInfo`). No-op when the pack is already cached or
    /// a fetch is in flight (deduped per community id); the response is
    /// the `communityFullInfo` pack itself, and later changes arrive as
    /// `updateCommunityFullInfo`.
    pub fn get_community_full_info(
        &mut self,
        community_id: i64,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let purpose = RequestPurpose::GetCommunityFullInfo;
        if self
            .session
            .community_full_infos
            .contains_key(&community_id)
            || self
                .session
                .requests
                .has_purpose_for_community(purpose, community_id)
        {
            return Ok(None);
        }
        let extra = self.session.request_for_community(purpose, community_id);
        if let Err(err) = self
            .sender
            .send_json(&get_community_full_info(extra, community_id))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Slice (communities backend core): `setCommunityName` (schema 1.8.67,
    /// line 11811). Empty names are refused client-side
    /// (`Err(InvalidRequest)`). Not optimistic — the new name arrives via
    /// `updateCommunity`; on `ok` the state drops the cached full-info
    /// pack and the ingest refetches it (the welcome-message-mutation
    /// pattern). In-flight dedupe is per community id.
    pub fn set_community_name(
        &mut self,
        community_id: i64,
        name: &str,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if name.trim().is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let purpose = RequestPurpose::SetCommunityName;
        if self
            .session
            .requests
            .has_purpose_for_community(purpose, community_id)
        {
            return Ok(None);
        }
        let extra = self.session.request_for_community(purpose, community_id);
        if let Err(err) = self
            .sender
            .send_json(&set_community_name(extra, community_id, name))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Sends a per-community request unless one with the same purpose is
    /// already in flight (`Ok(None)`); the shared tail of the TDLib
    /// 1.8.68 community-management drivers.
    fn send_community_request(
        &mut self,
        purpose: RequestPurpose,
        community_id: i64,
        build: impl FnOnce(RequestId) -> String,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if self
            .session
            .requests
            .has_purpose_for_community(purpose, community_id)
        {
            return Ok(None);
        }
        let extra = self.session.request_for_community(purpose, community_id);
        if let Err(err) = self.sender.send_json(&build(extra)) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// The cached community, when it is accessible (`have_access`; the
    /// schema says an inaccessible community's id "can't be passed to any
    /// method").
    fn accessible_community(&self, community_id: i64) -> Option<&ParsedCommunity> {
        self.session
            .communities
            .get(&community_id)
            .filter(|community| community.have_access)
    }

    /// `setCommunityPhoto` (TDLib 1.8.68). `photo_path` is a local image
    /// (`inputChatPhotoStatic`); `None` deletes the photo. Refused with
    /// `Ok(None)` without the `can_change_info` right (TDLib would answer
    /// "Have not enough rights"). Not optimistic — the photo arrives via
    /// `updateCommunity`.
    pub fn set_community_photo(
        &mut self,
        community_id: i64,
        photo_path: Option<&str>,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self
            .accessible_community(community_id)
            .is_some_and(|community| community.can_change_info)
        {
            return Ok(None);
        }
        let photo = match photo_path {
            Some(path) => serde_json::json!({
                "@type": "inputChatPhotoStatic",
                "photo": { "@type": "inputFileLocal", "path": path },
            }),
            None => serde_json::Value::Null,
        };
        self.send_community_request(RequestPurpose::SetCommunityPhoto, community_id, |extra| {
            set_community_photo(extra, community_id, photo)
        })
    }

    /// `setCommunityPermissions` (TDLib 1.8.68) — whether regular members
    /// may change the community's chat list. Refused with `Ok(None)`
    /// without the `can_ban_members` right. Not optimistic — the new
    /// permissions arrive via `updateCommunity`.
    pub fn set_community_permissions(
        &mut self,
        community_id: i64,
        members_can_edit_chat_list: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self
            .accessible_community(community_id)
            .is_some_and(|community| community.can_ban_members)
        {
            return Ok(None);
        }
        self.send_community_request(
            RequestPurpose::SetCommunityPermissions,
            community_id,
            |extra| set_community_permissions(extra, community_id, members_can_edit_chat_list),
        )
    }

    /// `deleteCommunity` (TDLib 1.8.68) — owner only (`Ok(None)`
    /// otherwise). On `ok` the state drops the community and its pack.
    pub fn delete_community(
        &mut self,
        community_id: i64,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self
            .accessible_community(community_id)
            .is_some_and(|community| community.is_owner)
        {
            return Ok(None);
        }
        self.send_community_request(RequestPurpose::DeleteCommunity, community_id, |extra| {
            delete_community(extra, community_id)
        })
    }
}
