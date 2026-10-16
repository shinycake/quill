//! The message selection is a set of picked ids. What it counts and acts
//! on is those ids that are still in the history: a message deleted while
//! it is selected drops out of the count, the forward bar and every bulk
//! action without anyone updating a copy.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use quill::composer::ForwardDraft;

impl QuillApp {
    /// The selection or forward draft as it stands now, `None` when no
    /// picked message is left. Read this, not `share.pending_forward`, for
    /// anything shown or done with the selected messages.
    pub(super) fn forward_draft(&self) -> Option<ForwardDraft> {
        let draft = self.share.pending_forward.as_ref()?;
        let session = self.session();
        let live = draft.clone().retaining(|id| {
            session.is_none_or(|session| session.message_still_there(draft.from_chat_id, id))
        });
        (!live.is_empty()).then_some(live)
    }

    /// Forget picked ids that no longer exist, before a change builds on
    /// the stored set (a toggle must not leave a ghost id behind).
    pub(super) fn drop_deleted_selection(&mut self) {
        let live = self
            .forward_draft()
            .map(|draft| draft.message_ids)
            .unwrap_or_default();
        if let Some(draft) = self.share.pending_forward.as_mut() {
            draft.message_ids = live;
        }
    }
}
