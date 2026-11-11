//! The members list of the "Choose members" page: every member of the
//! channel, a page at a time, and a server search for a typed name (tdesktop
//! `Giveaway::AwardMembersListController`, which pages `getSupergroupMembers`
//! and searches on the server).
//!
//! Each fresh list (a new search) gets a generation number carried by its
//! requests, so an answer for an older search is dropped.

/// Members asked for per page.
pub const MEMBERS_PAGE: i32 = 100;

/// Where the members list stands. One value, so a page cannot be loading and
/// failed at once.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum MembersStage {
    /// No list was started.
    #[default]
    Idle,
    /// A page is in flight.
    Loading,
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
    pub generation: u64,
}

impl AwardMembers {
    /// Start a new list for `chat_id` and `query`; returns its generation.
    pub fn restart(&mut self, chat_id: i64, query: &str) -> u64 {
        let generation = self.generation + 1;
        *self = Self {
            chat_id,
            query: query.to_string(),
            generation,
            stage: MembersStage::Loading,
            ..Self::default()
        };
        generation
    }

    /// A page is in flight.
    pub fn is_loading(&self) -> bool {
        self.stage == MembersStage::Loading
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

    /// Mark the next page as asked; returns its generation.
    pub fn begin_page(&mut self) -> u64 {
        self.stage = MembersStage::Loading;
        self.generation
    }

    /// Take a page: `ids` are the people kept from it, `received` is how
    /// many members the server sent (bots and yourself are left out of
    /// `ids` but still count towards the offset). A page of an older list
    /// is dropped.
    pub fn accept(&mut self, generation: u64, ids: Vec<i64>, received: i32, total: i32) -> bool {
        if generation != self.generation {
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

    /// A page failed; the loaded part stays.
    pub fn fail(&mut self, generation: u64, reason: String) {
        if generation == self.generation {
            self.stage = MembersStage::Failed(reason);
        }
    }
}
