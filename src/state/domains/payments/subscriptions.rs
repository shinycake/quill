//! The Stars Subscriptions dialog's list (`getStarSubscriptions`) and the
//! one edit (`editStarSubscription` / `reuseStarSubscription`) it can have
//! in flight.
use crate::ids::RequestId;
use crate::telegram::envelope::StarSubscriptionsData;

/// Where the list's page stands. The data loaded stays under every stage.
///
/// `Loading` leaves on its own answer (`Idle`, or `Stale` when an edit
/// landed meanwhile) or its error (`Failed`, which is not asked again by
/// itself); `Stale` leaves when the refetch goes out; `Failed` on a retry.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum SubsLoad {
    #[default]
    Idle,
    /// An edit landed: the list refetches on the next pump (never
    /// optimistic).
    Stale,
    /// The page is out as `request`; only its answer or error lands (a
    /// stale refetch and an appended page never mix). `again`: an edit
    /// landed while it was out, so its answer is already old.
    Loading {
        request: RequestId,
        again: bool,
    },
    Failed(String),
}

/// The edit in flight: one at a time, the dialog disables its buttons.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum SubsEdit {
    #[default]
    Idle,
    Editing {
        request: RequestId,
    },
    /// The last edit failed or did not leave; the line is drawn in the
    /// dialog until the next edit.
    Failed(String),
}

/// The fetched `starSubscriptions` and where its page and edit stand.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StarSubscriptionsList {
    data: Option<StarSubscriptionsData>,
    pub load: SubsLoad,
    pub edit: SubsEdit,
}

impl StarSubscriptionsList {
    pub fn data(&self) -> Option<&StarSubscriptionsData> {
        self.data.as_ref()
    }

    pub fn data_mut(&mut self) -> Option<&mut StarSubscriptionsData> {
        self.data.as_mut()
    }

    /// Pagination offset of the next page (empty = no more pages).
    pub fn offset(&self) -> &str {
        self.data
            .as_ref()
            .map_or("", |data| data.next_offset.as_str())
    }

    pub fn is_loading(&self) -> bool {
        matches!(self.load, SubsLoad::Loading { .. })
    }

    pub fn is_stale(&self) -> bool {
        self.load == SubsLoad::Stale
    }

    pub fn is_editing(&self) -> bool {
        matches!(self.edit, SubsEdit::Editing { .. })
    }

    /// The line to draw: the page's failure, else the edit's.
    pub fn error(&self) -> Option<&str> {
        match (&self.load, &self.edit) {
            (SubsLoad::Failed(error), _) | (_, SubsEdit::Failed(error)) => Some(error),
            _ => None,
        }
    }

    /// A page went out as `request`.
    pub fn begin_load(&mut self, request: RequestId) {
        self.load = SubsLoad::Loading {
            request,
            again: false,
        };
    }

    /// The page just asked did not leave.
    pub fn load_not_sent(&mut self, reason: String) {
        self.load = SubsLoad::Failed(reason);
    }

    /// The refetch of a stale list could not go out: it waits no more.
    pub fn stale_dropped(&mut self) {
        if self.is_stale() {
            self.load = SubsLoad::Idle;
        }
    }

    /// `request` answered with `page`; a follow-up page appends, a fresh
    /// fetch replaces. Dropped unless it is the page in flight.
    pub fn page_answered(&mut self, request: RequestId, append: bool, page: StarSubscriptionsData) {
        let SubsLoad::Loading {
            request: asked,
            again,
        } = self.load
        else {
            return;
        };
        if asked != request {
            return;
        }
        self.load = if again {
            SubsLoad::Stale
        } else {
            SubsLoad::Idle
        };
        match self.data.as_mut() {
            Some(existing) if append => {
                existing.star_amount = page.star_amount;
                existing.required_star_count = page.required_star_count;
                existing.next_offset = page.next_offset;
                existing.subscriptions.extend(page.subscriptions);
            }
            _ => self.data = Some(page),
        }
        self.edit_failure_clears();
    }

    /// `request` failed: only the page in flight fails the list.
    pub fn page_failed(&mut self, request: RequestId, reason: String) {
        if matches!(self.load, SubsLoad::Loading { request: asked, .. } if asked == request) {
            self.load = SubsLoad::Failed(reason);
        }
    }

    /// An edit went out as `request`.
    pub fn begin_edit(&mut self, request: RequestId) {
        self.edit = SubsEdit::Editing { request };
        if matches!(self.load, SubsLoad::Failed(_)) {
            self.load = SubsLoad::Idle;
        }
    }

    /// The edit just asked did not leave.
    pub fn edit_not_sent(&mut self) {
        if self.is_editing() {
            self.edit = SubsEdit::Idle;
        }
    }

    /// The edit answered `ok`: the server list is the truth now. The old
    /// cache stays visible until the refetch replaces it.
    /// Only the edit in flight settles: an answer of another request
    /// changes nothing.
    pub fn edit_done(&mut self, request: RequestId) {
        if !matches!(self.edit, SubsEdit::Editing { request: asked } if asked == request) {
            return;
        }
        self.edit = SubsEdit::Idle;
        self.load = match self.load {
            SubsLoad::Loading { request, .. } => SubsLoad::Loading {
                request,
                again: true,
            },
            _ => SubsLoad::Stale,
        };
    }

    /// The edit `request` failed with `reason`; only the edit in flight
    /// fails.
    pub fn edit_failed(&mut self, request: RequestId, reason: String) {
        if !matches!(self.edit, SubsEdit::Editing { request: asked } if asked == request) {
            return;
        }
        self.edit = SubsEdit::Failed(reason);
    }

    /// A page answer ends an old edit failure line.
    fn edit_failure_clears(&mut self) {
        if matches!(self.edit, SubsEdit::Failed(_)) {
            self.edit = SubsEdit::Idle;
        }
    }

    /// The Subscriptions data was replaced by hand (a demo, a test).
    pub fn set_data(&mut self, data: Option<StarSubscriptionsData>) {
        self.data = data;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(next: &str) -> StarSubscriptionsData {
        StarSubscriptionsData {
            star_amount: 5,
            subscriptions: Vec::new(),
            required_star_count: 0,
            next_offset: next.to_string(),
        }
    }

    #[test]
    fn an_edit_landing_while_a_page_is_out_makes_its_answer_stale() {
        let mut list = StarSubscriptionsList::default();
        list.begin_load(RequestId(1));
        list.begin_edit(RequestId(2));
        list.edit_done(RequestId(2));
        assert!(
            list.is_loading(),
            "the page in flight stays the one waited for"
        );
        list.page_answered(RequestId(1), false, page(""));
        assert!(list.is_stale(), "the refetch is still due");
        assert!(list.data().is_some());
    }

    #[test]
    fn an_edit_failing_does_not_drop_the_page_in_flight() {
        let mut list = StarSubscriptionsList::default();
        list.begin_load(RequestId(1));
        list.begin_edit(RequestId(2));
        list.edit_failed(RequestId(2), "no".into());
        assert!(!list.is_editing());
        assert_eq!(list.error(), Some("no"));
        list.page_answered(RequestId(1), false, page("more"));
        assert_eq!(list.offset(), "more");
        assert!(
            list.error().is_none(),
            "an answer ends the old failure line"
        );
    }

    #[test]
    fn only_the_page_in_flight_lands_or_fails() {
        let mut list = StarSubscriptionsList::default();
        list.begin_load(RequestId(2));
        list.page_answered(RequestId(1), false, page(""));
        list.page_failed(RequestId(1), "old".into());
        assert!(list.is_loading());
        assert!(list.data().is_none());
        list.page_failed(RequestId(2), "bad".into());
        assert_eq!(list.error(), Some("bad"));
        assert!(
            !list.is_stale(),
            "a failed list is not asked again by itself"
        );
    }

    #[test]
    fn only_the_edit_in_flight_settles() {
        let mut list = StarSubscriptionsList::default();
        list.begin_edit(RequestId(2));
        list.edit_done(RequestId(1));
        list.edit_failed(RequestId(1), "old".into());
        assert!(list.is_editing(), "another request's answer changes nothing");
        assert!(list.error().is_none());
        list.edit_failed(RequestId(2), "no".into());
        assert_eq!(list.error(), Some("no"));
    }
}
