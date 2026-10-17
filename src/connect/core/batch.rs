//! Poll batches: work that only needs to run once after a burst of
//! envelopes (the UI redraws once per batch too) waits for `end_batch`.
use super::*;

/// What a poll batch postponed.
#[derive(Debug, Default)]
pub(crate) struct BatchState {
    /// Between `begin_batch` and `end_batch`.
    open: bool,
    /// An envelope was applied since the open chat's custom emoji were
    /// last looked at.
    emoji_dirty: bool,
}

impl<S: JsonSender> ConnectDriver<S> {
    /// Start a poll batch: ingesting envelopes then skips per-envelope
    /// work that `end_batch` does once (chat-list sorting, resolving the
    /// open chat's custom emoji).
    pub fn begin_batch(&mut self) {
        self.batch.open = true;
        self.session.begin_update_batch();
    }

    /// Finish the batch started by `begin_batch`.
    pub fn end_batch(&mut self) -> Result<(), ConnectSendError> {
        self.batch.open = false;
        self.session.end_update_batch();
        if std::mem::take(&mut self.batch.emoji_dirty) {
            return self.resolve_open_chat_custom_emoji();
        }
        Ok(())
    }

    /// After an envelope: resolve and download the open chat's custom
    /// emoji now, or once at the end of the batch.
    pub(super) fn sync_open_chat_custom_emoji(&mut self) -> Result<(), ConnectSendError> {
        if self.batch.open {
            self.batch.emoji_dirty = true;
            return Ok(());
        }
        self.resolve_open_chat_custom_emoji()
    }

    fn resolve_open_chat_custom_emoji(&mut self) -> Result<(), ConnectSendError> {
        let ids = self.session.open_chat_custom_emoji_ids();
        self.maybe_resolve_message_custom_emoji(&ids)?;
        if !self.session.stickers.emoji.custom_emoji_stickers.is_empty() {
            let files = self.session.custom_emoji_files_for(&ids);
            self.ensure_media_files(&files)?;
        }
        Ok(())
    }
}
