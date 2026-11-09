//! The composers' `getLinkPreview` prefetches, one per URL asked.
use super::*;

/// Where one URL's `getLinkPreview` stands (TGX `LinkPreview.isLoading`,
/// `isNotFound`).
#[derive(Debug, Clone, Default)]
pub enum LinkPreviewFetch {
    /// The request is in flight: "Getting link info".
    #[default]
    Asked,
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
#[derive(Debug, Clone, Default)]
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

    /// A request for `url` went out: its chip shows "Getting link info"
    /// until the answer.
    pub fn asked(&mut self, url: &str) {
        self.0.retain(|preview| preview.url != url);
        self.0.push(ComposerLinkPreview {
            url: url.to_string(),
            fetch: LinkPreviewFetch::Asked,
        });
        let over = self.0.len().saturating_sub(Self::KEPT);
        self.0.drain(..over);
    }

    /// The request for `url` could not be sent: kept as failed, so the
    /// composers do not ask again on every frame.
    pub fn send_failed(&mut self, url: &str) {
        self.asked(url);
        self.answered(url, LinkPreviewFetch::Failed);
    }

    /// The URL was typed into a field again: a failed request is
    /// forgotten so it is asked once more. Any other state stays.
    pub fn forget_failed(&mut self, url: &str) {
        self.0.retain(|preview| {
            preview.url != url || !matches!(preview.fetch, LinkPreviewFetch::Failed)
        });
    }

    /// The answer for `url`. An answer for a URL no longer kept is
    /// dropped, and a failure never replaces a preview already loaded.
    pub fn answered(&mut self, url: &str, fetch: LinkPreviewFetch) {
        let Some(slot) = self.0.iter_mut().find(|slot| slot.url == url) else {
            return;
        };
        let loaded = matches!(slot.fetch, LinkPreviewFetch::Loaded(_));
        if matches!(fetch, LinkPreviewFetch::Loaded(_)) || !loaded {
            slot.fetch = fetch;
        }
    }
}
