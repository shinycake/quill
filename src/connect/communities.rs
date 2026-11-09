//! Connect driver: the TDLib 1.8.70 community methods (joined list,
//! adding and removing chats, add-chat requests, collapsing,
//! notification settings, bans). The answers land in
//! `Session::communities`; finished requests queue a
//! `CommunityOutcome` for the UI.
use super::*;
use crate::ids::RequestId;
use crate::state::fetch::Fetch;
use crate::state::{CommunitiesPurpose, CommunityMember, ProfileChatsKind, RequestPurpose};
use crate::telegram::envelope::ChatNotificationSettings;
use crate::telegram::requests::{
    add_community_chat, get_community_add_chat_requests, get_community_member_chats,
    get_joined_communities, get_owned_bots, process_community_add_chat_request,
    process_community_add_chat_requests, remove_community_chat,
    set_community_notification_settings, toggle_community_are_chats_collapsed,
    toggle_community_member_is_banned,
};

/// tdesktop's `kPerPage` for the pending requests list.
pub const COMMUNITY_REQUESTS_PAGE: i32 = 100;

impl<S: JsonSender> ConnectDriver<S> {
    fn community_request_in_flight(
        &self,
        community_id: i64,
        matches: impl Fn(CommunitiesPurpose) -> bool,
    ) -> bool {
        self.session
            .community_request_in_flight(community_id, matches)
    }

    fn send_communities_request(
        &mut self,
        purpose: CommunitiesPurpose,
        community_id: i64,
        build: impl FnOnce(RequestId) -> String,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self.community_request_in_flight(community_id, |sent| sent == purpose) {
            return Ok(None);
        }
        let extra = self
            .session
            .request_for_community(RequestPurpose::Communities(purpose), community_id);
        self.send_json_request(extra, &build(extra)).map(Some)
    }

    /// The accessible community (an inaccessible one "can't be passed to
    /// any method").
    fn known_community(&self, community_id: i64) -> bool {
        self.session
            .groups
            .communities
            .get(&community_id)
            .is_some_and(|community| community.have_access)
    }

    /// `getJoinedCommunities`, asked again each time the picker opens
    /// (tdesktop `requestJoinedCommunities`); the last list stays while
    /// the new one loads.
    pub fn fetch_joined_communities(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self.session.communities.joined.is_loading() {
            return Ok(None);
        }
        let purpose = RequestPurpose::Communities(CommunitiesPurpose::GetJoined);
        let extra = self.session.request(purpose, None);
        let stale = self.session.communities.joined.take_data();
        self.session.communities.joined = Fetch::Loading { stale };
        match self.send_json_request(extra, &get_joined_communities(extra)) {
            Ok(extra) => Ok(Some(extra)),
            Err(err) => {
                let stale = self.session.communities.joined.take_data();
                self.session.communities.joined = stale.map_or(Fetch::Idle, Fetch::Ready);
                Err(err)
            }
        }
    }

    /// `createCommunity` from the identity box, with the picture chosen
    /// there: it goes up with `setCommunityPhoto` once the community
    /// exists (tdesktop uploads it in `create`'s done handler).
    pub fn create_community_with_photo(
        &mut self,
        chat_id: crate::ids::ChatId,
        name: &str,
        is_chat_hidden: bool,
        photo: Option<String>,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        let sent = self.create_community(chat_id, name, is_chat_hidden)?;
        if let Some(extra) = sent {
            self.session.communities.new_community_photo = photo.map(|path| (extra, path));
        }
        Ok(sent)
    }

    /// The community `communityId` names, when it answers our own
    /// `createCommunity` (`extra`).
    pub(super) fn created_community(
        &self,
        extra: Option<RequestId>,
        community_id: i64,
    ) -> Option<(i64, RequestId)> {
        let extra = extra?;
        (self.session.requests.purpose(extra) == Some(RequestPurpose::CreateCommunity))
            .then_some((community_id, extra))
    }

    /// A community we created exists: load its full info, and set the
    /// picture its identity box chose. `let _`: the community is already
    /// in the model, and a failed send must not fail the ingest.
    pub(super) fn community_created(&mut self, community_id: i64, extra: RequestId) {
        let _ = self.get_community_full_info(community_id);
        let photo = self
            .session
            .communities
            .new_community_photo
            .take_if(|(asked, _)| *asked == extra);
        if let Some((_, path)) = photo {
            let _ = self.set_community_photo(community_id, Some(&path));
        }
    }

    /// `getOwnedBots` for the "Add a Chat to Community" picker, asked
    /// again each time it opens; the last list stays while the new one
    /// loads.
    pub fn fetch_owned_bots(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self.session.communities.owned_bots.is_loading() {
            return Ok(None);
        }
        let purpose = RequestPurpose::Communities(CommunitiesPurpose::GetOwnedBots);
        let extra = self.session.request(purpose, None);
        let stale = self.session.communities.owned_bots.take_data();
        self.session.communities.owned_bots = Fetch::Loading { stale };
        match self.send_json_request(extra, &get_owned_bots(extra)) {
            Ok(extra) => Ok(Some(extra)),
            Err(err) => {
                let stale = self.session.communities.owned_bots.take_data();
                self.session.communities.owned_bots = stale.map_or(Fetch::Idle, Fetch::Ready);
                Err(err)
            }
        }
    }

    /// `getSuitableCommunityChats` for the "Add a Chat to Community"
    /// picker; always asked again.
    pub fn fetch_suitable_community_chats(
        &mut self,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        self.session
            .users_state
            .profile_chat_lists
            .remove(&(ProfileChatsKind::SuitableCommunityChats, 0));
        self.fetch_profile_chats(ProfileChatsKind::SuitableCommunityChats, 0)
    }

    /// `addCommunityChat`; one attempt per community and chat at a time.
    pub fn add_community_chat(
        &mut self,
        community_id: i64,
        chat_id: i64,
        is_hidden: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.known_community(community_id) {
            return Ok(None);
        }
        if self.community_request_in_flight(
            community_id,
            |sent| matches!(sent, CommunitiesPurpose::AddChat { chat_id: c, .. } if c == chat_id),
        ) {
            return Ok(None);
        }
        self.send_communities_request(
            CommunitiesPurpose::AddChat { chat_id, is_hidden },
            community_id,
            |extra| add_community_chat(extra, community_id, chat_id, is_hidden),
        )
    }

    /// `removeCommunityChat`.
    pub fn remove_community_chat(
        &mut self,
        community_id: i64,
        chat_id: i64,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.known_community(community_id) {
            return Ok(None);
        }
        self.send_communities_request(
            CommunitiesPurpose::RemoveChat { chat_id },
            community_id,
            |extra| remove_community_chat(extra, community_id, chat_id),
        )
    }

    /// One page of `getCommunityAddChatRequests`: the first page, or the
    /// next one after the loaded list. Nothing when a page is in flight
    /// or the list is complete.
    pub fn fetch_community_add_chat_requests(
        &mut self,
        community_id: i64,
        first_page: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.known_community(community_id) {
            return Ok(None);
        }
        let fetch = self.session.communities.add_chat_requests(community_id);
        if fetch.is_loading() {
            return Ok(None);
        }
        let offset = if first_page {
            String::new()
        } else {
            match fetch.data().and_then(|list| list.next_offset.clone()) {
                Some(offset) => offset,
                None => return Ok(None),
            }
        };
        let sent = self.send_communities_request(
            CommunitiesPurpose::GetAddChatRequests { first_page },
            community_id,
            |extra| {
                get_community_add_chat_requests(
                    extra,
                    community_id,
                    &offset,
                    COMMUNITY_REQUESTS_PAGE,
                )
            },
        )?;
        if sent.is_some() {
            let fetch = self
                .session
                .communities
                .add_chat_requests
                .entry(community_id)
                .or_default();
            let stale = fetch.take_data();
            *fetch = Fetch::Loading { stale };
        }
        Ok(sent)
    }

    /// `processCommunityAddChatRequest` for one suggested chat.
    pub fn process_community_add_chat_request(
        &mut self,
        community_id: i64,
        chat_id: i64,
        approve: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.known_community(community_id) {
            return Ok(None);
        }
        if self.community_request_in_flight(community_id, |sent| {
            matches!(sent, CommunitiesPurpose::ProcessAddChatRequest { chat_id: c, .. } if c == chat_id)
        }) {
            return Ok(None);
        }
        self.send_communities_request(
            CommunitiesPurpose::ProcessAddChatRequest { chat_id, approve },
            community_id,
            |extra| process_community_add_chat_request(extra, community_id, chat_id, approve),
        )
    }

    /// `processCommunityAddChatRequests`; `count` is what the
    /// confirmation named, for the toast.
    pub fn process_community_add_chat_requests(
        &mut self,
        community_id: i64,
        approve: bool,
        count: i32,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.known_community(community_id) {
            return Ok(None);
        }
        if self.community_request_in_flight(community_id, |sent| {
            matches!(sent, CommunitiesPurpose::ProcessAddChatRequests { .. })
        }) {
            return Ok(None);
        }
        self.send_communities_request(
            CommunitiesPurpose::ProcessAddChatRequests { approve, count },
            community_id,
            |extra| process_community_add_chat_requests(extra, community_id, approve),
        )
    }

    /// `toggleCommunityAreChatsCollapsed`. While it is in flight the
    /// switch shows the asked value
    /// (`Session::community_chats_collapsed`).
    pub fn toggle_community_chats_collapsed(
        &mut self,
        community_id: i64,
        collapsed: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.known_community(community_id) {
            return Ok(None);
        }
        if self.community_request_in_flight(community_id, |sent| {
            matches!(sent, CommunitiesPurpose::ToggleCollapsed { .. })
        }) {
            return Ok(None);
        }
        self.send_communities_request(
            CommunitiesPurpose::ToggleCollapsed { collapsed },
            community_id,
            |extra| toggle_community_are_chats_collapsed(extra, community_id, collapsed),
        )
    }

    /// `setCommunityNotificationSettings`; the settings show at once and
    /// stay when TDLib answers `ok`.
    pub fn set_community_notification_settings(
        &mut self,
        community_id: i64,
        settings: ChatNotificationSettings,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self.known_community(community_id) {
            return Ok(None);
        }
        let purpose = RequestPurpose::Communities(CommunitiesPurpose::SetNotificationSettings);
        let extra = self.session.request_for_community(purpose, community_id);
        let json = set_community_notification_settings(extra, community_id, &settings);
        self.session
            .communities
            .notification_sends
            .insert(extra, (community_id, settings));
        match self.send_json_request(extra, &json) {
            Ok(extra) => Ok(Some(extra)),
            Err(err) => {
                self.session.communities.notification_sends.remove(&extra);
                Err(err)
            }
        }
    }

    /// `getCommunityMemberChats`, asked before a ban for its warning.
    pub fn fetch_community_member_chats(
        &mut self,
        community_id: i64,
        member: CommunityMember,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.known_community(community_id) {
            return Ok(None);
        }
        let sent = self.send_communities_request(
            CommunitiesPurpose::GetMemberChats { member },
            community_id,
            |extra| get_community_member_chats(extra, community_id, member),
        )?;
        if sent.is_some() {
            self.session
                .communities
                .member_chats
                .insert((community_id, member), Fetch::Loading { stale: None });
        }
        Ok(sent)
    }

    /// `toggleCommunityMemberIsBanned` with `is_banned: true`.
    pub fn ban_community_member(
        &mut self,
        community_id: i64,
        member: CommunityMember,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.known_community(community_id) {
            return Ok(None);
        }
        self.send_communities_request(
            CommunitiesPurpose::ToggleMemberBanned {
                member,
                is_banned: true,
            },
            community_id,
            |extra| toggle_community_member_is_banned(extra, community_id, member, true),
        )
    }

    /// The full info of every community the user can see: it holds the
    /// community's notification settings, which its chats fall back to
    /// (`use_default_mute_for`), and the chat list a collapsed row is built
    /// from. Asked once per community, and after a failure only once the
    /// community changes. Runs in every account's poll (`poll_ticks`), so a
    /// background account's toasts and badge follow a community's mute too.
    /// True when it asked for something.
    pub fn sync_community_full_infos(&mut self) -> Result<bool, ConnectSendError> {
        if !self.chats_path_active() {
            return Ok(false);
        }
        let missing: Vec<i64> = self
            .session
            .groups
            .communities
            .values()
            .filter(|community| community.have_access)
            .map(|community| community.id)
            .filter(|id| !self.session.groups.community_full_infos.contains_key(id))
            .collect();
        let mut asked = false;
        for community_id in missing {
            asked |= self.want_community_full_info(community_id)?.is_some();
        }
        Ok(asked)
    }
}
