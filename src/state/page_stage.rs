//! Where a paged list stands: nothing asked, a page in flight, answered, or
//! failed. One value for what a `loading` bool, a `loaded` bool and an
//! `error` kept side by side said, so they cannot disagree.
//!
//! The request is inside the stage, so only its own answer or failure acts
//! on the list: a tab switched or a filter changed while a page is out
//! drops the older page's answer.

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum PageStage {
    /// No page was asked for the current list.
    #[default]
    Idle,
    /// The request `request` (0 until it is sent) is in flight; `more` is a
    /// later page, so earlier pages count as loaded.
    Loading { request: u64, more: bool },
    /// The last page arrived.
    Ready,
    /// The last request failed, with the line the list shows. Pages that
    /// arrived before stay.
    Failed(String),
}

impl PageStage {
    /// A request is in flight.
    pub fn is_loading(&self) -> bool {
        matches!(self, Self::Loading { .. })
    }

    /// Whether any page arrived for the current list (a failure also ends
    /// the first wait).
    pub fn loaded(&self) -> bool {
        match self {
            Self::Idle => false,
            Self::Loading { more, .. } => *more,
            Self::Ready | Self::Failed(_) => true,
        }
    }

    pub fn error(&self) -> Option<&str> {
        match self {
            Self::Failed(reason) => Some(reason),
            _ => None,
        }
    }

    /// A page is about to be asked for.
    pub fn begin(&mut self, more: bool) {
        *self = Self::Loading { request: 0, more };
    }

    /// The page asked for goes out as `request`.
    pub fn sent(&mut self, request: u64) {
        if let Self::Loading { more, .. } = *self {
            *self = Self::Loading { request, more };
        }
    }

    /// Whether the list waits for the answer to `request`.
    pub fn waits_for(&self, request: u64) -> bool {
        request != 0 && matches!(self, Self::Loading { request: r, .. } if *r == request)
    }

    /// The request `request` failed. Only the request the list waits for
    /// fails it; a newer one keeps loading.
    pub fn fail(&mut self, request: u64, note: String) {
        if self.waits_for(request) {
            *self = Self::Failed(note);
        }
    }

    /// The page asked for never left.
    pub fn send_failed(&mut self, note: String) {
        *self = Self::Failed(note);
    }

    /// The page arrived.
    pub fn ready(&mut self) {
        *self = Self::Ready;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_request_the_list_waits_for_answers_it() {
        let mut stage = PageStage::default();
        assert!(!stage.loaded() && !stage.is_loading());
        stage.begin(false);
        stage.sent(5);
        assert!(stage.is_loading() && !stage.loaded());
        stage.fail(4, "old".into());
        assert!(stage.is_loading(), "an older request's failure");
        stage.fail(5, "boom".into());
        assert_eq!(stage.error(), Some("boom"));
        assert!(stage.loaded(), "a failure ends the first wait");
        stage.begin(true);
        assert!(stage.loaded() && stage.is_loading());
        stage.ready();
        assert_eq!(stage, PageStage::Ready);
    }
}
