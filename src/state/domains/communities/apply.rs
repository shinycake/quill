//! Applies the answers to the TDLib 1.8.70 community methods.
use crate::state::fetch::Fetch;
use crate::state::*;
use crate::telegram::envelope::{CommunitiesPayload, CommunityAddChatRequests};

/// `addCommunityChat` sent a request to the admins instead of adding.
const REQUEST_CREATED: &str = "COMMUNITY_REQUEST_CREATED";
/// The community already has the most chats (or bots) it can hold.
const PEERS_TOO_MUCH: &str = "COMMUNITY_PEERS_TOO_MUCH";

fn communities_purpose(pending: Option<&PendingRequest>) -> Option<(CommunitiesPurpose, i64)> {
    let pending = pending?;
    let RequestPurpose::Communities(purpose) = pending.purpose else {
        return None;
    };
    Some((purpose, pending.community_id?))
}

fn community_error(err: &TdError) -> CommunityError {
    if err.server_type.as_deref() == Some(PEERS_TOO_MUCH) {
        CommunityError::PeersTooMuch
    } else {
        CommunityError::Other(error_reason(err))
    }
}

impl Session {
    /// Applies one communities payload; called by
    /// [`Session::apply_payload`]. Only answers to our own asks count.
    pub(crate) fn apply_communities_payload(
        &mut self,
        payload: CommunitiesPayload,
        pending: Option<&PendingRequest>,
        _extra: Option<RequestId>,
        _seq: u64,
    ) {
        match payload {
            CommunitiesPayload::Communities { community_ids } => {
                if pending.map(|p| p.purpose)
                    == Some(RequestPurpose::Communities(CommunitiesPurpose::GetJoined))
                {
                    self.communities.joined = Fetch::Ready(community_ids);
                }
            }
            CommunitiesPayload::AddChatRequests(page) => {
                if let Some((CommunitiesPurpose::GetAddChatRequests { first_page }, community_id)) =
                    communities_purpose(pending)
                {
                    self.apply_add_chat_requests_page(community_id, first_page, page);
                }
            }
            CommunitiesPayload::MemberChats(chats) => {
                if let Some((CommunitiesPurpose::GetMemberChats { member }, community_id)) =
                    communities_purpose(pending)
                {
                    self.communities
                        .member_chats
                        .insert((community_id, member), Fetch::Ready(chats));
                }
            }
        }
    }

    /// The `users` answer to our `getOwnedBots`; `true` when ours.
    pub(crate) fn apply_community_owned_bots(
        &mut self,
        user_ids: &[i64],
        pending: Option<&PendingRequest>,
    ) -> bool {
        if pending.map(|p| p.purpose)
            != Some(RequestPurpose::Communities(
                CommunitiesPurpose::GetOwnedBots,
            ))
        {
            return false;
        }
        self.communities.owned_bots = Fetch::Ready(user_ids.to_vec());
        true
    }

    fn apply_add_chat_requests_page(
        &mut self,
        community_id: i64,
        first_page: bool,
        page: CommunityAddChatRequests,
    ) {
        let fetch = self
            .communities
            .add_chat_requests
            .entry(community_id)
            .or_default();
        let mut list = if first_page {
            AddChatRequestList::default()
        } else {
            fetch.take_data().unwrap_or_default()
        };
        for request in page.requests {
            if !list.requests.iter().any(|r| r.chat_id == request.chat_id) {
                list.requests.push(request);
            }
        }
        list.total_count = page.total_count;
        list.next_offset = (!page.next_offset.is_empty()).then_some(page.next_offset);
        *fetch = Fetch::Ready(list);
    }

    /// `ok` for a community request.
    pub(crate) fn apply_communities_ok(&mut self, pending: Option<&PendingRequest>) {
        let Some((purpose, community_id)) = communities_purpose(pending) else {
            return;
        };
        let outcome = match purpose {
            CommunitiesPurpose::AddChat { chat_id, .. } => CommunityOutcome::ChatAdded {
                community_id,
                chat_id,
            },
            CommunitiesPurpose::RemoveChat { chat_id } => {
                if let Some(info) = self.groups.community_full_infos.get_mut(&community_id) {
                    info.chats.retain(|chat| chat.chat_id != chat_id);
                }
                CommunityOutcome::ChatRemoved {
                    community_id,
                    chat_id,
                }
            }
            CommunitiesPurpose::ProcessAddChatRequest { chat_id, approve } => {
                self.drop_add_chat_requests(community_id, Some(chat_id));
                CommunityOutcome::RequestProcessed {
                    community_id,
                    chat_id,
                    approve,
                }
            }
            CommunitiesPurpose::ProcessAddChatRequests { approve, count } => {
                self.drop_add_chat_requests(community_id, None);
                CommunityOutcome::AllRequestsProcessed {
                    community_id,
                    approve,
                    count,
                }
            }
            CommunitiesPurpose::SetNotificationSettings => {
                let sent = pending.and_then(|p| self.communities.notification_sends.remove(&p.id));
                if let Some((_, settings)) = sent
                    && let Some(info) = self.groups.community_full_infos.get_mut(&community_id)
                {
                    info.notification_settings = settings;
                }
                return;
            }
            CommunitiesPurpose::ToggleMemberBanned { member, is_banned } => {
                if !is_banned {
                    return;
                }
                CommunityOutcome::MemberBanned {
                    community_id,
                    member,
                }
            }
            CommunitiesPurpose::ToggleCollapsed { .. }
            | CommunitiesPurpose::GetJoined
            | CommunitiesPurpose::GetOwnedBots
            | CommunitiesPurpose::GetAddChatRequests { .. }
            | CommunitiesPurpose::GetMemberChats { .. } => return,
        };
        self.communities.outcomes.push_back(outcome);
    }

    /// A handled request leaves the list (`chat_id`), or every request
    /// does (`None`). The pending count in the full info is TDLib's
    /// alone: it reloads the full info before answering `ok`
    /// (`CommunityManager::on_change_community_dialogs`), so the new count
    /// has already come with `updateCommunityFullInfo`.
    fn drop_add_chat_requests(&mut self, community_id: i64, chat_id: Option<i64>) {
        let Some(list) = self
            .communities
            .add_chat_requests
            .get_mut(&community_id)
            .and_then(Fetch::data_mut)
        else {
            return;
        };
        match chat_id {
            Some(chat_id) => {
                let before = list.requests.len();
                list.requests.retain(|r| r.chat_id != chat_id);
                if list.requests.len() < before {
                    list.total_count = list.total_count.saturating_sub(1).max(0);
                }
            }
            None => *list = AddChatRequestList::default(),
        }
    }

    /// A failed `getCommunityFullInfo`: not asked again in the background
    /// until the community changes (`CommunitiesState::full_info_failed`).
    pub(crate) fn note_community_full_info_failed(&mut self, pending: Option<&PendingRequest>) {
        if let Some(pending) = pending
            && pending.purpose == RequestPurpose::GetCommunityFullInfo
            && let Some(community_id) = pending.community_id
        {
            self.communities.full_info_failed.insert(community_id);
        }
    }

    /// A refused community request.
    pub(crate) fn apply_communities_error(
        &mut self,
        err: &TdError,
        pending: Option<&PendingRequest>,
        _extra: Option<RequestId>,
        _seq: u64,
    ) {
        let Some((purpose, community_id)) = communities_purpose(pending) else {
            let fetch = match pending.map(|p| p.purpose) {
                Some(RequestPurpose::Communities(CommunitiesPurpose::GetJoined)) => {
                    &mut self.communities.joined
                }
                Some(RequestPurpose::Communities(CommunitiesPurpose::GetOwnedBots)) => {
                    &mut self.communities.owned_bots
                }
                _ => return,
            };
            let stale = fetch.take_data();
            *fetch = Fetch::Failed {
                error: error_reason(err),
                stale,
            };
            return;
        };
        let outcome = match purpose {
            CommunitiesPurpose::AddChat { chat_id, .. } => {
                if err.server_type.as_deref() == Some(REQUEST_CREATED) {
                    CommunityOutcome::AddRequestSent {
                        community_id,
                        chat_id,
                    }
                } else {
                    CommunityOutcome::AddFailed {
                        community_id,
                        chat_id,
                        error: community_error(err),
                    }
                }
            }
            CommunitiesPurpose::RemoveChat { chat_id } => CommunityOutcome::RemoveFailed {
                community_id,
                chat_id,
                reason: error_reason(err),
            },
            CommunitiesPurpose::ProcessAddChatRequest { .. }
            | CommunitiesPurpose::ProcessAddChatRequests { .. } => {
                CommunityOutcome::ProcessFailed {
                    community_id,
                    error: community_error(err),
                }
            }
            CommunitiesPurpose::ToggleCollapsed { .. } => CommunityOutcome::CollapseFailed {
                community_id,
                reason: error_reason(err),
            },
            CommunitiesPurpose::SetNotificationSettings => {
                if let Some(pending) = pending {
                    self.communities.notification_sends.remove(&pending.id);
                }
                CommunityOutcome::NotificationsFailed {
                    community_id,
                    reason: error_reason(err),
                }
            }
            CommunitiesPurpose::ToggleMemberBanned { .. } => CommunityOutcome::BanFailed {
                community_id,
                reason: error_reason(err),
            },
            CommunitiesPurpose::GetAddChatRequests { .. } => {
                let fetch = self
                    .communities
                    .add_chat_requests
                    .entry(community_id)
                    .or_default();
                let stale = fetch.take_data();
                *fetch = Fetch::Failed {
                    error: error_reason(err),
                    stale,
                };
                return;
            }
            CommunitiesPurpose::GetMemberChats { member } => {
                self.communities.member_chats.insert(
                    (community_id, member),
                    Fetch::Failed {
                        error: error_reason(err),
                        stale: None,
                    },
                );
                return;
            }
            CommunitiesPurpose::GetJoined | CommunitiesPurpose::GetOwnedBots => return,
        };
        self.communities.outcomes.push_back(outcome);
    }
}
