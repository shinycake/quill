//! Recent settings searches (`settings_search_recent.json`).

use super::{AccountPaths, load_json_prefs, save_json_prefs};
use crate::settings_catalog::RecentSearches;

/// Missing or corrupt files read as no recent searches.
pub fn load_settings_search_recent(paths: &AccountPaths) -> RecentSearches {
    load_json_prefs(paths, "settings_search_recent.json")
}

pub fn save_settings_search_recent(paths: &AccountPaths, recent: &RecentSearches) {
    save_json_prefs(paths, "settings_search_recent.json", recent)
}
