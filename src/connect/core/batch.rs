//! Poll batches: work that only needs to run once after a burst of
//! envelopes (the UI redraws once per batch too) waits for `end_batch`.
use super::*;
use crate::telegram::envelope::{
    ChatListPayload, ChatsPayload, EnvelopePayload, MediaPayload, UsersPayload,
};

/// The passes a batch postponed.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PassSet {
    /// An envelope was applied since the open chat's custom emoji were
    /// last looked at.
    emoji: bool,
    /// The shown chats' messages changed: mark them viewed and fetch the
    /// messages they reply to.
    views: bool,
    /// The shown chats' media, or a file they wait on, changed: download
    /// their thumbnails and auto-download media.
    media: bool,
}

/// Where the poll batch stands.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Batch {
    /// No batch is open. `owed` are passes that a failed `end_batch` or an
    /// offscreen batch left for the next one.
    Closed { owed: PassSet },
    /// Between `begin_batch` and `end_batch`; `dirty` are the passes
    /// waiting for the end.
    Open { dirty: PassSet },
}

impl Default for Batch {
    fn default() -> Self {
        Self::Closed {
            owed: PassSet::default(),
        }
    }
}

impl Batch {
    fn open(&mut self) {
        if let Self::Closed { owed } = *self {
            *self = Self::Open { dirty: owed };
        }
    }

    /// The batch ends: the passes it holds, now owed by nobody.
    fn close(&mut self) -> PassSet {
        let held = match *self {
            Self::Open { dirty } => dirty,
            Self::Closed { owed } => owed,
        };
        *self = Self::default();
        held
    }

    /// Passes that did not run stay for the next batch.
    fn owe(&mut self, owed: PassSet) {
        if let Self::Closed { owed: held } = self {
            held.emoji |= owed.emoji;
            held.views |= owed.views;
            held.media |= owed.media;
        }
    }

    /// A pass is wanted: held for the end of an open batch. `false` when no
    /// batch is open and the pass runs now.
    fn hold(&mut self, mark: impl FnOnce(&mut PassSet)) -> bool {
        match self {
            Self::Open { dirty } => {
                mark(dirty);
                true
            }
            Self::Closed { .. } => false,
        }
    }
}

/// What a poll batch postponed.
#[derive(Debug, Default)]
pub(crate) struct BatchState {
    stage: Batch,
    /// Display files of the open chat's custom emoji that the last emoji
    /// pass left undownloaded. One that stops without completing (a
    /// refused resume ends, a cancel settles, our request fails) needs the
    /// pass again, or nothing asks for it until an unrelated update.
    emoji_waiting_files: std::collections::HashSet<crate::ids::FileId>,
    /// How often the shown chats' media pass ran.
    #[cfg(test)]
    pub(crate) media_passes: usize,
}

/// Whether the envelope may give the open chat a wallpaper file to fetch;
/// nothing else asks for it.
pub(super) fn changes_chat_look(payload: &EnvelopePayload) -> bool {
    matches!(
        payload,
        EnvelopePayload::Chats(
            ChatsPayload::UpdateChatBackground { .. }
                | ChatsPayload::UpdateChatTheme { .. }
                | ChatsPayload::UpdateEmojiChatThemes(_)
        )
    )
}

impl<S: JsonSender> ConnectDriver<S> {
    /// Start a poll batch: ingesting envelopes then skips per-envelope
    /// work that `end_batch` does once (chat-list sorting, resolving the
    /// open chat's custom emoji).
    pub fn begin_batch(&mut self) {
        self.batch.stage.open();
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
        let mut owed = self.batch.stage.close();
        self.session.end_update_batch();
        // A pass leaves the set only once it ran: one that failed (and the
        // ones after it) stays owed to the next batch.
        if owed.views {
            if let Err(err) = self.run_shown_views() {
                self.batch.stage.owe(owed);
                return Err(err);
            }
            owed.views = false;
        }
        if owed.media {
            if let Err(err) = self.run_shown_media() {
                self.batch.stage.owe(owed);
                return Err(err);
            }
            owed.media = false;
        }
        if owed.emoji {
            let result = self.resolve_open_chat_custom_emoji();
            if result.is_err() {
                self.batch.stage.owe(owed);
            }
            return result;
        }
        Ok(())
    }

    /// `end_batch` for an account not on screen: nothing shows its chats,
    /// so the passes for them wait for the first batch once it is shown.
    pub fn end_offscreen_batch(&mut self) {
        let held = self.batch.stage.close();
        self.batch.stage.owe(held);
        self.session.end_update_batch();
    }

    /// After an envelope that may show a new custom emoji: resolve and download the open chat's custom
    /// emoji now, or once at the end of the batch.
    pub(super) fn sync_open_chat_custom_emoji(&mut self) -> Result<(), ConnectSendError> {
        if self.batch.stage.hold(|set| set.emoji = true) {
            return Ok(());
        }
        self.resolve_open_chat_custom_emoji()
    }

    /// After an envelope that changed the shown chats' messages: view them
    /// and fetch their reply targets now, or once at the end of the batch.
    pub(super) fn sync_shown_views(&mut self) -> Result<(), ConnectSendError> {
        if self.batch.stage.hold(|set| set.views = true) {
            return Ok(());
        }
        self.run_shown_views()
    }

    /// After an envelope that may change what the shown chats need
    /// downloaded: ask for it now, or once at the end of the batch.
    pub(super) fn sync_shown_media(&mut self) -> Result<(), ConnectSendError> {
        if self.batch.stage.hold(|set| set.media = true) {
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
        self.maybe_download_shown_chat_media()?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pass_is_held_only_while_a_batch_is_open() {
        let mut batch = Batch::default();
        assert!(!batch.hold(|set| set.views = true), "runs at once");
        batch.open();
        assert!(batch.hold(|set| set.media = true));
        let held = batch.close();
        assert!(held.media && !held.views);
        assert_eq!(batch, Batch::default());
    }

    #[test]
    fn a_pass_that_did_not_run_is_owed_to_the_next_batch() {
        let mut batch = Batch::default();
        batch.open();
        batch.hold(|set| set.emoji = true);
        let held = batch.close();
        // The pass failed at the end: it stays owed.
        batch.owe(held);
        batch.open();
        assert_eq!(
            batch,
            Batch::Open {
                dirty: PassSet {
                    emoji: true,
                    ..PassSet::default()
                }
            }
        );
        assert!(batch.close().emoji);
        assert_eq!(batch.close(), PassSet::default(), "and then nothing is");
    }
}
