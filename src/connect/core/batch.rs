//! Poll batches: work that only needs to run once after a burst of
//! envelopes (the UI redraws once per batch too) waits for `end_batch`.
use super::*;
use crate::telegram::envelope::{
    ChatListPayload, ChatsPayload, EnvelopePayload, MediaPayload, UsersPayload,
};

/// What a poll batch postponed.
#[derive(Debug, Default)]
pub(crate) struct BatchState {
    /// Between `begin_batch` and `end_batch`.
    open: bool,
    /// An envelope was applied since the open chat's custom emoji were
    /// last looked at.
    emoji_dirty: bool,
    /// Display files of the open chat's custom emoji that the last emoji
    /// pass left undownloaded. One that stops without completing (a
    /// refused resume ends, a cancel settles, our request fails) needs the
    /// pass again, or nothing asks for it until an unrelated update.
    emoji_waiting_files: std::collections::HashSet<crate::ids::FileId>,
    /// The shown chats' messages changed: mark them viewed and fetch the
    /// messages they reply to.
    views_dirty: bool,
    /// The shown chats' media, or a file they wait on, changed: download
    /// their thumbnails and auto-download media.
    media_dirty: bool,
    /// How often the shown chats' media pass ran.
    #[cfg(test)]
    pub(crate) media_passes: usize,
}

impl<S: JsonSender> ConnectDriver<S> {
    /// Start a poll batch: ingesting envelopes then skips per-envelope
    /// work that `end_batch` does once (chat-list sorting, resolving the
    /// open chat's custom emoji).
    pub fn begin_batch(&mut self) {
        self.batch.open = true;
        self.session.begin_update_batch();
    }

    /// Ingest the sender's answers to requests the agent UI driver refused
    /// (`AgentScreen`, TDLib's error shape) like TDLib's own. Whether there
    /// were any.
    pub fn ingest_refused_answers(&mut self) -> bool {
        let answers = self.sender.take_refused_answers();
        for answer in &answers {
            if let Ok(envelope) = crate::telegram::envelope::parse_envelope(answer) {
                let _ = self.ingest(crate::telegram::client::OwnedEnvelope {
                    seq: crate::telegram::client::LOCAL_ANSWER_SEQ,
                    client_id: envelope.client_id,
                    envelope,
                });
            }
        }
        !answers.is_empty()
    }

    /// Finish the batch started by `begin_batch`.
    pub fn end_batch(&mut self) -> Result<(), ConnectSendError> {
        self.batch.open = false;
        self.session.end_update_batch();
        if std::mem::take(&mut self.batch.views_dirty) {
            self.run_shown_views()?;
        }
        if std::mem::take(&mut self.batch.media_dirty) {
            self.run_shown_media()?;
        }
        if std::mem::take(&mut self.batch.emoji_dirty) {
            return self.resolve_open_chat_custom_emoji();
        }
        Ok(())
    }

    /// `end_batch` for an account not on screen: nothing shows its chats,
    /// so the passes for them wait for the first batch once it is shown.
    pub fn end_offscreen_batch(&mut self) {
        self.batch.open = false;
        self.session.end_update_batch();
    }

    /// After an envelope that may show a new custom emoji: resolve and download the open chat's custom
    /// emoji now, or once at the end of the batch.
    pub(super) fn sync_open_chat_custom_emoji(&mut self) -> Result<(), ConnectSendError> {
        if self.batch.open {
            self.batch.emoji_dirty = true;
            return Ok(());
        }
        self.resolve_open_chat_custom_emoji()
    }

    /// After an envelope that changed the shown chats' messages: view them
    /// and fetch their reply targets now, or once at the end of the batch.
    pub(super) fn sync_shown_views(&mut self) -> Result<(), ConnectSendError> {
        if self.batch.open {
            self.batch.views_dirty = true;
            return Ok(());
        }
        self.run_shown_views()
    }

    /// After an envelope that may change what the shown chats need
    /// downloaded: ask for it now, or once at the end of the batch.
    pub(super) fn sync_shown_media(&mut self) -> Result<(), ConnectSendError> {
        if self.batch.open {
            self.batch.media_dirty = true;
            return Ok(());
        }
        self.run_shown_media()
    }

    fn run_shown_views(&mut self) -> Result<(), ConnectSendError> {
        self.maybe_view_open_messages()?;
        let _ = self.maybe_fetch_replied_messages();
        Ok(())
    }

    fn run_shown_media(&mut self) -> Result<(), ConnectSendError> {
        #[cfg(test)]
        {
            self.batch.media_passes += 1;
        }
        self.maybe_download_shown_thumbs()?;
        self.maybe_download_open_chat_media()?;
        Ok(())
    }

    fn resolve_open_chat_custom_emoji(&mut self) -> Result<(), ConnectSendError> {
        let ids = self.session.open_chat_custom_emoji_ids();
        self.maybe_resolve_message_custom_emoji(&ids)?;
        let stickers = &self.session.stickers.emoji.custom_emoji_stickers;
        self.batch.emoji_waiting_files = ids
            .iter()
            .filter_map(|id| stickers.lookup(*id)?.display_file_id())
            .filter(|file| {
                self.session
                    .file(*file)
                    .is_none_or(|parsed| parsed.usable_path().is_none())
            })
            .collect();
        if !stickers.is_empty() {
            let files = self.session.custom_emoji_files_for(&ids);
            self.ensure_media_files(&files)?;
        }
        Ok(())
    }

    /// `payload` reports a file the open chat's custom emoji wait on that
    /// stopped without completing.
    pub(super) fn stops_waiting_emoji_file(&self, payload: &EnvelopePayload) -> bool {
        matches!(
            payload,
            EnvelopePayload::Media(MediaPayload::UpdateFile(file) | MediaPayload::File(file))
                if file.local.is_idle_incomplete()
                    && self.batch.emoji_waiting_files.contains(&file.id)
        )
    }
}

/// Frequent payloads that never bring a custom emoji id into view, so the
/// open chat's custom emoji need no second look. Anything not listed still
/// counts: a missed kind costs time, not a missing emoji. A file update
/// still counts when `stops_waiting_emoji_file` says the emoji wait on it.
pub(super) fn adds_no_custom_emoji(payload: &EnvelopePayload) -> bool {
    matches!(
        payload,
        EnvelopePayload::Media(MediaPayload::UpdateFile(_) | MediaPayload::File(_))
            | EnvelopePayload::Users(UsersPayload::UpdateUserStatus { .. })
            | EnvelopePayload::Chats(
                ChatsPayload::UpdateChatAction { .. }
                    | ChatsPayload::UpdateChatReadInbox { .. }
                    | ChatsPayload::UpdateChatReadOutbox { .. }
                    | ChatsPayload::UpdateChatOnlineMemberCount { .. }
            )
            | EnvelopePayload::ChatList(
                ChatListPayload::UpdateUnreadMessageCount { .. }
                    | ChatListPayload::UpdateUnreadChatCount { .. }
            )
    )
}
