//! Collapsed communities in the chat list ("Show as One Chat"): the rows
//! that stand in for their chats, built from the cached full infos.
use crate::community_row::{CommunityMemberChat, CommunityRowSummary};
use crate::state::*;
use crate::telegram::envelope::ChatNotificationSettings;

impl Session {
    /// One row per collapsed community with at least one chat in the
    /// main list. A community whose full info has not arrived yet has no
    /// row, and its chats stay where they are. Chats outside the main
    /// list (archived or not joined) are the row's "and N more"
    /// (`notInCount`).
    pub fn collapsed_community_rows(&self) -> Vec<CommunityRowSummary> {
        let mut rows: Vec<CommunityRowSummary> = self
            .groups
            .communities
            .values()
            .filter(|community| {
                community.have_access && self.community_chats_collapsed(community.id)
            })
            .filter_map(|community| {
                let info = self.groups.community_full_infos.get(&community.id)?;
                let mut joined = Vec::new();
                let mut not_joined = 0;
                for chat in &info.chats {
                    match self
                        .chats
                        .get(&chat.chat_id)
                        .filter(|summary| summary.in_main_list)
                    {
                        Some(summary) => joined.push(CommunityMemberChat {
                            chat_id: summary.id.0,
                            title: summary.title.clone(),
                            date: summary.last_message.as_ref().map_or(0, |m| m.date),
                            unread: summary.is_unread(),
                            muted: self.effective_muted(summary),
                            mentioned: summary.unread_mention_count > 0,
                        }),
                        None => not_joined += 1,
                    }
                }
                CommunityRowSummary::build(community.id, &community.name, &joined, not_joined)
            })
            .collect();
        rows.sort_by_key(|row| row.community_id);
        rows
    }

    /// The notification settings of the community `chat_id` belongs to,
    /// which the chat falls back to before the scope defaults
    /// (`chatNotificationSettings.use_default_*`, TDLib 1.8.70).
    pub(crate) fn chat_community_notification_settings(
        &self,
        chat_id: i64,
    ) -> Option<&ChatNotificationSettings> {
        self.groups
            .community_full_infos
            .values()
            .find(|info| info.chats.iter().any(|chat| chat.chat_id == chat_id))
            .map(|info| &info.notification_settings)
    }
}
