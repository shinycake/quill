//! Answers `ingest` captures before `apply` takes their pending request.
use super::*;
use crate::ids::ChatId;
use crate::telegram::envelope::{Envelope, RichMessageContent};

impl<S: JsonSender> ConnectDriver<S> {
    /// MED4: the `getWebPageInstantView` answer, with the window that asked
    /// and the URL it was for.
    pub(super) fn take_instant_view_answer(
        &mut self,
        envelope: &Envelope,
    ) -> Option<(Option<ChatId>, String, RichMessageContent)> {
        match &envelope.payload {
            EnvelopePayload::Messages(MessagesPayload::WebPageInstantView { rich }) => envelope
                .extra
                .and_then(|id| {
                    let pending = self.session.requests.get(id)?;
                    (pending.purpose == RequestPurpose::GetWebPageInstantView)
                        .then_some((id, pending.asked_in.chat()))
                })
                .and_then(|(id, window)| {
                    let messages = &mut self.session.messages;
                    messages
                        .instant_view_requests
                        .remove(&id)
                        .map(|ask| (window, ask.url, rich.clone()))
                }),
            _ => None,
        }
    }
}
