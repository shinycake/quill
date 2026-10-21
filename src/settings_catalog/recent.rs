//! Settings opened from search, newest first (tdesktop keeps the recent
//! searches and shows them for an empty query). Ids, so they survive a
//! language change; ids no longer in the catalogue are dropped on read.

use serde::{Deserialize, Serialize};

/// How many picks are kept.
pub const MAX_RECENT: usize = 8;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecentSearches {
    #[serde(default)]
    ids: Vec<String>,
}

impl RecentSearches {
    /// Newest first, only ids the catalogue still knows.
    pub fn ids(&self) -> impl Iterator<Item = &str> {
        self.ids
            .iter()
            .map(String::as_str)
            .filter(|id| super::target(id).is_some())
    }

    /// Remember `id` as the newest pick.
    pub fn record(&mut self, id: &str) {
        self.ids.retain(|known| known != id);
        self.ids.insert(0, id.to_string());
        self.ids.truncate(MAX_RECENT);
    }

    pub fn remove(&mut self, id: &str) {
        self.ids.retain(|known| known != id);
    }

    /// The ids as search ranking wants them.
    pub fn as_slice(&self) -> &[String] {
        &self.ids
    }
}
