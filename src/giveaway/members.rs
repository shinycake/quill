//! The members list of the "Choose members" page: every member of the
//! channel, a page at a time, and a server search for a typed name (tdesktop
//! `Giveaway::AwardMembersListController`, which pages `getSupergroupMembers`
//! and searches on the server).
//!
//! Each fresh list (a new search) gets a generation number carried by its
//! requests, so an answer for an older search is dropped.

/// Members asked for per page.
pub const MEMBERS_PAGE: i32 = 100;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AwardMembers {
    pub chat_id: i64,
    /// The search the list answers; empty = recent members.
    pub query: String,
    pub user_ids: Vec<i64>,
    /// Members received so far, the next page's offset.
    pub offset: i32,
    pub total: i32,
    pub loading: bool,
    /// The last page came.
    pub done: bool,
    pub error: Option<String>,
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
            loading: true,
            ..Self::default()
        };
        generation
    }

    /// Another page can be asked for.
    pub fn can_load_more(&self) -> bool {
        self.generation > 0 && !self.loading && !self.done
    }

    /// Mark the next page as asked; returns its generation.
    pub fn begin_page(&mut self) -> u64 {
        self.loading = true;
        self.error = None;
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
        self.loading = false;
        self.error = None;
        let got = received.max(0);
        for id in ids {
            if !self.user_ids.contains(&id) {
                self.user_ids.push(id);
            }
        }
        self.offset = self.offset.saturating_add(got);
        self.total = total;
        self.done = got == 0 || (total > 0 && self.offset >= total);
        true
    }

    /// A page failed; the loaded part stays.
    pub fn fail(&mut self, generation: u64, reason: String) {
        if generation == self.generation {
            self.loading = false;
            self.error = Some(reason);
        }
    }
}
