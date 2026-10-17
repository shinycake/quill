//! Connect driver: opening a chat at its saved scroll position.
use super::*;

impl<S: JsonSender> ConnectDriver<S> {
    /// The first window of a chat opened at its saved position: the page
    /// around `anchor`, like a jump's but with no highlight
    /// (`HistoryState::saved_open` marks it). A failed send falls back to
    /// the default window.
    pub(crate) fn fetch_history_around_saved(
        &mut self,
        chat_id: ChatId,
        anchor: MessageId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        let extra = self.session.request_history_around(chat_id, anchor);
        if self
            .sender
            .send_json(&get_chat_history(
                extra,
                chat_id,
                anchor,
                HISTORY_AROUND_OFFSET,
                HISTORY_AROUND_LIMIT,
                false,
            ))
            .is_err()
        {
            self.session.requests.take(extra);
            if let Some(history) = self.session.histories.get_mut(&chat_id.0) {
                history.saved_open = None;
            }
            // The default window answers for the open: a failed send of
            // the around request is not a failed open.
            let fallback = self.fetch_history_for(chat_id);
            if fallback.is_err() {
                self.mark_history_load_failed(chat_id);
            }
            return fallback;
        }
        Ok(Some(extra))
    }

    /// The saved position's message was missing from its window, or that
    /// load failed: load the open chat's default window instead. Returns
    /// whether a page was requested.
    pub(crate) fn load_default_after_saved_open(&mut self) -> bool {
        let Some(chat_id) = self.session.take_saved_open_fallback() else {
            return false;
        };
        match self.fetch_history_for(chat_id) {
            Ok(requested) => requested.is_some(),
            Err(_) => {
                self.mark_history_load_failed(chat_id);
                false
            }
        }
    }

    /// The emptied window has nothing left to load it: show the usual
    /// Retry row instead of the skeleton.
    fn mark_history_load_failed(&mut self, chat_id: ChatId) {
        if let Some(history) = self.session.histories.get_mut(&chat_id.0) {
            history.load_failed = true;
        }
    }
}
