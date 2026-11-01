//! The badge memos recount when `ChatListState::unread_revision` moves.
//! Most updates a signed-in account receives (statuses, typing, file
//! progress) cannot change what the badges count, so they leave it alone.
use super::*;
use crate::telegram::envelope::{ChatsPayload, MediaPayload, SettingsPayload, UsersPayload};

impl Session {
    pub(crate) fn note_unread_inputs(&mut self, payload: &EnvelopePayload) {
        if feeds_unread(payload) {
            self.chat_list.unread_revision = self.chat_list.unread_revision.wrapping_add(1);
        }
    }

    /// A local change to what the unread badges count (read or marked
    /// unread before TDLib confirms it).
    pub fn touch_unread(&mut self) {
        self.revision = self.revision.wrapping_add(1);
        self.chat_list.unread_revision = self.chat_list.unread_revision.wrapping_add(1);
    }
}

/// False only for updates that touch nothing a badge reads: chat unread
/// counts and marks, list placement, mute settings, forum topics, the
/// totals.
fn feeds_unread(payload: &EnvelopePayload) -> bool {
    use EnvelopePayload as P;
    !matches!(
        payload,
        P::Unknown(_)
            | P::Settings(SettingsPayload::WebProxyCarrierData { .. })
            | P::Users(UsersPayload::UpdateUserStatus { .. })
            | P::Chats(ChatsPayload::UpdateChatAction { .. })
            | P::Chats(ChatsPayload::UpdateChatReadOutbox { .. })
            | P::Chats(ChatsPayload::UpdateChatOnlineMemberCount { .. })
            | P::Media(MediaPayload::UpdateFile(_))
            | P::Media(MediaPayload::UpdateFileDownload { .. })
    )
}
