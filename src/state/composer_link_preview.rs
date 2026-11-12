//! The composers' `getLinkPreview` prefetches, one per URL asked.
use super::*;
use crate::telegram::envelope::MessagesPayload;

/// Where one URL's `getLinkPreview` stands (TGX `LinkPreview.isLoading`,
/// `isNotFound`).
#[derive(Debug, Clone)]
pub enum LinkPreviewFetch {
    /// The request is in flight under this id: "Getting link info". Only
    /// its answer, its error or its sweep (a lost request settles as
    /// [`Self::Failed`]) leaves the stage.
    Asked(RequestId),
    /// The card.
    Loaded(Box<LinkPreview>),
    /// TDLib has no preview for the URL (404, or an answer that is no
    /// preview): "No preview for this link". Kept for the session.
    NotFound,
    /// The request failed for another reason (FLOOD_WAIT, a timeout, a
    /// 5xx): it says nothing about the URL, so a composer that types the
    /// URL again asks again.
    Failed,
}

/// MED4b: composer `getLinkPreview` prefetch state (TGX `LinkPreview`).
#[derive(Debug, Clone)]
pub struct ComposerLinkPreview {
    /// URL the prefetch was requested for.
    pub url: String,
    pub fetch: LinkPreviewFetch,
}

impl ComposerLinkPreview {
    /// The card, once loaded.
    pub fn preview(&self) -> Option<&LinkPreview> {
        match &self.fetch {
            LinkPreviewFetch::Loaded(preview) => Some(preview),
            _ => None,
        }
    }
}

/// The composers' link previews, one per URL asked (each window's
/// composer has its own URL, and its chip reads its own), the last
/// [`Self::KEPT`] asked.
#[derive(Debug, Clone, Default)]
pub struct ComposerLinkPreviews(Vec<ComposerLinkPreview>);

impl ComposerLinkPreviews {
    /// Enough for every open window's composer.
    pub const KEPT: usize = 8;

    /// The preview asked for `url`.
    pub fn get(&self, url: &str) -> Option<&ComposerLinkPreview> {
        self.0.iter().find(|preview| preview.url == url)
    }

    /// A request for `url` went out as `request`: its chip shows "Getting
    /// link info" until that request's answer. An earlier request for the
    /// same URL is forgotten with its slot, so its late answer lands nowhere.
    pub fn asked(&mut self, url: &str, request: RequestId) {
        self.put(url, LinkPreviewFetch::Asked(request));
    }

    fn put(&mut self, url: &str, fetch: LinkPreviewFetch) {
        self.0.retain(|preview| preview.url != url);
        self.0.push(ComposerLinkPreview {
            url: url.to_string(),
            fetch,
        });
        // Oldest answered entries go first: an entry still `Asked` waits
        // for its answer and is never evicted (the in-flight requests
        // bound how many stay beyond `KEPT`).
        while self.0.len() > Self::KEPT {
            let Some(at) = self
                .0
                .iter()
                .position(|preview| !matches!(preview.fetch, LinkPreviewFetch::Asked(_)))
            else {
                break;
            };
            self.0.remove(at);
        }
    }

    /// The request for `url` could not be sent: kept as failed, so the
    /// composers do not ask again on every frame.
    pub fn send_failed(&mut self, url: &str) {
        self.put(url, LinkPreviewFetch::Failed);
    }

    /// The URL was typed into a field again: a failed request is
    /// forgotten so it is asked once more. Any other state stays.
    pub fn forget_failed(&mut self, url: &str) {
        self.0.retain(|preview| {
            preview.url != url || !matches!(preview.fetch, LinkPreviewFetch::Failed)
        });
    }

    /// The end of `request`: taken only by the slot that waits for it. An
    /// answer for a request whose URL was asked again or evicted is dropped.
    pub fn answered(&mut self, request: RequestId, fetch: LinkPreviewFetch) {
        if let Some(slot) = self
            .0
            .iter_mut()
            .find(|slot| matches!(slot.fetch, LinkPreviewFetch::Asked(asked) if asked == request))
        {
            slot.fetch = fetch;
        }
    }
}

impl Session {
    /// The end of a `getLinkPreview` request, whatever it is: a card, "no
    /// link info" (404 / 400 or an answer that is no preview), or a failure
    /// asked again when the URL is typed again. A swept request arrives
    /// here as the 408 error of every lost request.
    pub(crate) fn settle_composer_link_preview(
        &mut self,
        pending: Option<&PendingRequest>,
        payload: &EnvelopePayload,
    ) {
        let Some(pending) = pending.filter(|p| p.purpose == RequestPurpose::GetLinkPreview) else {
            return;
        };
        let fetch = match payload {
            EnvelopePayload::Messages(MessagesPayload::LinkPreview { preview }) => {
                match preview.clone() {
                    Some(preview) => LinkPreviewFetch::Loaded(Box::new(preview)),
                    None => LinkPreviewFetch::NotFound,
                }
            }
            EnvelopePayload::Error(err) if err.code == 404 || err.code == 400 => {
                LinkPreviewFetch::NotFound
            }
            EnvelopePayload::Error(_) => LinkPreviewFetch::Failed,
            _ => LinkPreviewFetch::NotFound,
        };
        self.messages.composer_previews.answered(pending.id, fetch);
    }
}

#[cfg(test)]
mod tests {
    use super::{ComposerLinkPreviews, LinkPreviewFetch};
    use crate::ids::RequestId;

    /// Sweep 05-056: more links than `KEPT` never evict one still waiting
    /// for its answer, so that answer lands.
    #[test]
    fn an_asked_link_is_not_evicted_before_its_answer() {
        let mut previews = ComposerLinkPreviews::default();
        for n in 0..ComposerLinkPreviews::KEPT + 4 {
            previews.asked(&format!("https://example.com/{n}"), RequestId(n as u64));
        }
        assert!(
            previews.get("https://example.com/0").is_some(),
            "the oldest, still asked"
        );
        previews.answered(RequestId(0), LinkPreviewFetch::NotFound);
        assert!(matches!(
            previews.get("https://example.com/0").map(|p| &p.fetch),
            Some(LinkPreviewFetch::NotFound)
        ));
        // Once answered it is the first to go.
        previews.asked("https://example.com/new", RequestId(99));
        assert!(previews.get("https://example.com/0").is_none());
    }

    /// Sweep r2-17 (05-011): an answer is taken by the slot that waits for
    /// that request: an older request for the same URL, or one whose slot was
    /// evicted, changes nothing.
    #[test]
    fn an_answer_is_matched_to_its_request() {
        let mut previews = ComposerLinkPreviews::default();
        let url = "https://example.com/a";
        previews.asked(url, RequestId(1));
        previews.asked(url, RequestId(2));
        previews.answered(RequestId(1), LinkPreviewFetch::NotFound);
        assert!(matches!(
            previews.get(url).map(|p| &p.fetch),
            Some(LinkPreviewFetch::Asked(RequestId(2)))
        ));
        previews.answered(RequestId(2), LinkPreviewFetch::Failed);
        assert!(matches!(
            previews.get(url).map(|p| &p.fetch),
            Some(LinkPreviewFetch::Failed)
        ));
        // A settled slot takes no second answer.
        previews.answered(RequestId(2), LinkPreviewFetch::NotFound);
        assert!(matches!(
            previews.get(url).map(|p| &p.fetch),
            Some(LinkPreviewFetch::Failed)
        ));
    }
}
