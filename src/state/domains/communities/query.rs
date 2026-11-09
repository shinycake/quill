//! What the screens read about a community while its requests are in
//! flight: the asked collapse value, the newest sent notification
//! settings, which chats are being added or processed.
use crate::state::*;
use crate::telegram::envelope::ChatNotificationSettings;

impl Session {
    fn community_pending(
        &self,
        community_id: i64,
    ) -> impl Iterator<Item = (&PendingRequest, CommunitiesPurpose)> {
        self.requests.pending.values().filter_map(move |pending| {
            let RequestPurpose::Communities(purpose) = pending.purpose else {
                return None;
            };
            (pending.community_id == Some(community_id)).then_some((pending, purpose))
        })
    }

    /// Whether a request of this community matching `matches` is in
    /// flight.
    pub fn community_request_in_flight(
        &self,
        community_id: i64,
        matches: impl Fn(CommunitiesPurpose) -> bool,
    ) -> bool {
        self.community_pending(community_id)
            .any(|(_, purpose)| matches(purpose))
    }

    /// "Show as One Chat": the asked value while the toggle is in
    /// flight, else the community's flag.
    pub fn community_chats_collapsed(&self, community_id: i64) -> bool {
        let asked = self
            .community_pending(community_id)
            .filter_map(|(pending, purpose)| match purpose {
                CommunitiesPurpose::ToggleCollapsed { collapsed } => {
                    Some((pending.id.0, collapsed))
                }
                _ => None,
            })
            .max_by_key(|(id, _)| *id)
            .map(|(_, collapsed)| collapsed);
        asked.unwrap_or_else(|| {
            self.groups
                .communities
                .get(&community_id)
                .is_some_and(|community| community.are_chats_collapsed)
        })
    }

    /// The community's notification settings: the newest ones sent and
    /// not yet answered, else the full info's.
    pub fn community_notification_settings(
        &self,
        community_id: i64,
    ) -> Option<ChatNotificationSettings> {
        let sent = self
            .communities
            .notification_sends
            .iter()
            .filter(|(_, (id, _))| *id == community_id)
            .max_by_key(|(extra, _)| extra.0)
            .map(|(_, (_, settings))| settings.clone());
        sent.or_else(|| {
            self.groups
                .community_full_infos
                .get(&community_id)
                .map(|info| info.notification_settings.clone())
        })
    }

    /// Whether `addCommunityChat` for this chat is in flight.
    pub fn community_add_in_flight(&self, community_id: i64, chat_id: i64) -> bool {
        self.community_request_in_flight(community_id, |purpose| {
            matches!(purpose, CommunitiesPurpose::AddChat { chat_id: c, .. } if c == chat_id)
        })
    }

    /// Whether the pending requests list has an Add/Decline (one or all)
    /// in flight.
    pub fn community_requests_processing(&self, community_id: i64) -> bool {
        self.community_request_in_flight(community_id, |purpose| {
            matches!(
                purpose,
                CommunitiesPurpose::ProcessAddChatRequest { .. }
                    | CommunitiesPurpose::ProcessAddChatRequests { .. }
            )
        })
    }
}
