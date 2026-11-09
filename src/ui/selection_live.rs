//! The message selection is a set of picked ids. What it counts and acts
//! on is those ids that were not deleted: a message deleted while it is
//! selected (a tombstone in its chat history, tdesktop `itemRemoved`) drops
//! out of the count, the forward bar and every bulk action without anyone
//! updating a copy. A message that only left the loaded window (trimmed, or a
//! jump replaced the window) stays picked: unloaded is not gone.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use quill::composer::ForwardDraft;

impl QuillApp {
    /// The selection or forward draft as it stands now, `None` when no
    /// picked message is left. Read this, not `share.pending_forward`, for
    /// anything shown or done with the selected messages.
    ///
    /// A selection (no forward bar waits for it) is the conversation's it
    /// was made in: while the view shows another topic, thread or Saved
    /// sublist of its chat it is not there, so nothing counts, shows,
    /// deletes or forwards it in the frame before `sync_chat_ui` drops it.
    pub(super) fn forward_draft(&self) -> Option<ForwardDraft> {
        let live = self.live_forward_draft()?;
        let elsewhere = self.share.forward_bar_dest.is_none()
            && live.from_topic != self.open_topic_id(live.from_chat_id);
        (!elsewhere).then_some(live)
    }

    /// The stored draft without the ids deleted since they were picked.
    fn live_forward_draft(&self) -> Option<ForwardDraft> {
        let draft = self.share.pending_forward.as_ref()?;
        let session = self.session();
        let live = draft.clone().retaining(|id| {
            session.is_none_or(|session| !session.message_deleted(draft.from_chat_id, id))
        });
        (!live.is_empty()).then_some(live)
    }

    /// Forget picked ids that no longer exist, before a change builds on
    /// the stored set (a toggle must not leave a ghost id behind).
    pub(super) fn drop_deleted_selection(&mut self) {
        let live = self
            .live_forward_draft()
            .map(|draft| draft.message_ids)
            .unwrap_or_default();
        if let Some(draft) = self.share.pending_forward.as_mut() {
            draft.message_ids = live;
        }
    }
}
