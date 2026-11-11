//! The members list of the "Choose members" page: every member of the
//! channel, a page at a time, and a server search for a typed name (tdesktop
//! `Giveaway::AwardMembersListController`, which pages `getSupergroupMembers`
//! and searches on the server).
//!
//! A page in flight holds the id of its request in the stage; a new search
//! leaves the stage, so an answer for an older search or page is dropped.

use crate::ids::RequestId;

/// Members asked for per page.
pub const MEMBERS_PAGE: i32 = 100;

/// Where the members list stands. One value, so a page cannot be loading and
/// failed at once.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum MembersStage {
    /// No list was started.
    #[default]
    Idle,
    /// A page is in flight as `request`.
    Loading { request: RequestId },
    /// The last page came and the server may have more.
    Ready,
    /// The last page failed; the loaded part stays and a scroll asks again.
    Failed(String),
    /// The last page came: nothing more to ask for.
    Done,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AwardMembers {
    pub chat_id: i64,
    /// The search the list answers; empty = recent members.
    pub query: String,
    pub user_ids: Vec<i64>,
    /// Members received so far, the next page's offset.
    pub offset: i32,
    pub total: i32,
    pub stage: MembersStage,
}

impl AwardMembers {
    /// Start a new list for `chat_id` and `query`. Nothing is asked yet
    /// ([`Self::begin_page`] does that); a page still out for the list it
    /// replaces is dropped when it answers.
    pub fn restart(&mut self, chat_id: i64, query: &str) {
        *self = Self {
            chat_id,
            query: query.to_string(),
            ..Self::default()
        };
    }

    /// A page is in flight.
    pub fn is_loading(&self) -> bool {
        matches!(self.stage, MembersStage::Loading { .. })
    }

    /// The last page failed.
    pub fn error(&self) -> Option<&str> {
        match &self.stage {
            MembersStage::Failed(reason) => Some(reason),
            _ => None,
        }
    }

    /// The last page came: nothing more to ask for.
    pub fn is_done(&self) -> bool {
        self.stage == MembersStage::Done
    }

    /// Another page can be asked for.
    pub fn can_load_more(&self) -> bool {
        matches!(self.stage, MembersStage::Ready | MembersStage::Failed(_))
    }

    /// The next page went out as `request`.
    pub fn begin_page(&mut self, request: RequestId) {
        self.stage = MembersStage::Loading { request };
    }

    /// Take the page answering `request`: `ids` are the people kept from
    /// it, `received` is how many members the server sent (bots and
    /// yourself are left out of `ids` but still count towards the offset).
    /// A page of an older list or request is dropped.
    pub fn accept(&mut self, request: RequestId, ids: Vec<i64>, received: i32, total: i32) -> bool {
        if self.stage != (MembersStage::Loading { request }) {
            return false;
        }
        let got = received.max(0);
        for id in ids {
            if !self.user_ids.contains(&id) {
                self.user_ids.push(id);
            }
        }
        self.offset = self.offset.saturating_add(got);
        self.total = total;
        let done = got == 0 || (total > 0 && self.offset >= total);
        self.stage = if done {
            MembersStage::Done
        } else {
            MembersStage::Ready
        };
        true
    }

    /// `request` failed; the loaded part stays. An older request's error
    /// changes nothing.
    pub fn fail(&mut self, request: RequestId, reason: String) {
        if self.stage == (MembersStage::Loading { request }) {
            self.stage = MembersStage::Failed(reason);
        }
    }

    /// The page could not be asked for at all (no request exists).
    pub fn fail_unsent(&mut self, reason: String) {
        self.stage = MembersStage::Failed(reason);
    }
}
