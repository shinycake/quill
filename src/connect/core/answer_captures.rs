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
                        .then_some((id, pending.window))
                })
                .and_then(|(id, window)| {
                    let messages = &mut self.session.messages;
                    messages.instant_view_agent_requests.remove(&id);
                    messages
                        .instant_view_urls
                        .remove(&id)
                        .map(|url| (window, url, rich.clone()))
                }),
            _ => None,
        }
    }

    /// MED4b: the `getLinkPreview` answer for a URL still kept.
    pub(super) fn take_link_preview_answer(
        &mut self,
        envelope: &Envelope,
    ) -> Option<ComposerLinkPreview> {
        match &envelope.payload {
            EnvelopePayload::Messages(MessagesPayload::LinkPreview { preview }) => envelope
                .extra
                .and_then(|id| {
                    (self.session.requests.purpose(id) == Some(RequestPurpose::GetLinkPreview))
                        .then_some(id)
                })
                .and_then(|id| {
                    self.session
                        .messages
                        .composer_preview_urls
                        .remove(&id)
                        .map(|url| ComposerLinkPreview {
                            url,
                            fetch: match preview.clone() {
                                Some(preview) => LinkPreviewFetch::Loaded(Box::new(preview)),
                                None => LinkPreviewFetch::NotFound,
                            },
                        })
                }),
            _ => None,
        }
    }
}
