//! The one way a change request that owns a `ChangeNote` is sent.
use super::*;
use crate::state::MaskChangeTarget;

/// Which note a change request belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::connect) enum NoteSlot {
    /// Privacy page block list.
    Block,
    /// Active sessions (terminate, toggles, device link).
    Sessions,
    /// Connected websites.
    Websites,
    /// A mask set change or reorder (also records the change itself).
    Masks(MaskChangeTarget),
}

impl<S: JsonSender> ConnectDriver<S> {
    /// Send a change request `extra` (already in the request table) and
    /// give its note the wait only after the send went out, so a request
    /// that never left is never waited for. A failed send drops the
    /// request and shows `refusal` on the note with no id kept.
    pub(in crate::connect) fn send_noted(
        &mut self,
        note: NoteSlot,
        extra: RequestId,
        refusal: &str,
        json: &str,
    ) -> Result<RequestId, ConnectSendError> {
        if let Err(err) = self.sender.send_json(json) {
            self.session.requests.take(extra);
            let settings = &mut self.session.settings;
            match note {
                NoteSlot::Block => settings.block_note.refused(None, refusal),
                NoteSlot::Sessions => settings.sessions_note.refused(None, refusal),
                NoteSlot::Websites => settings.websites_note.refused(None, refusal),
                NoteSlot::Masks(_) => self
                    .session
                    .stickers
                    .masks
                    .change_note
                    .refused(None, refusal),
            }
            return Err(err);
        }
        let settings = &mut self.session.settings;
        match note {
            NoteSlot::Block => settings.block_note.begin(extra),
            NoteSlot::Sessions => settings.sessions_note.begin(extra),
            NoteSlot::Websites => settings.websites_note.begin(extra),
            NoteSlot::Masks(target) => self.session.stickers.masks.begin_change(extra, target),
        }
        Ok(extra)
    }
}
