//! The `#` popup's recent hashtags: one list for the account on screen,
//! shared by every window's composer and kept in that account's own
//! folder, as tdesktop keeps them per account (`Storage::Account`).

use super::app::QuillApp;
use quill::ids::{AccountKey, OwnedBy};
use quill::settings::{AccountPaths, safe_app_root};
use quill::suggest::RecentHashtags;

#[derive(Default)]
pub(crate) struct RecentHashtagsUi {
    /// The list, read from the account's folder at first use.
    list: OwnedBy<Option<RecentHashtags>>,
}

impl crate::ui::on_screen::Farewell for RecentHashtagsUi {}

impl RecentHashtagsUi {
    /// The list as last read, if it is `account`'s.
    pub(super) fn of(&self, account: &AccountKey) -> Option<&RecentHashtags> {
        if self.list.owner() == Some(account) {
            self.list.as_ref()
        } else {
            None
        }
    }
}

/// Where `account` keeps its recent hashtags.
fn account_paths(account: &AccountKey) -> Option<AccountPaths> {
    safe_app_root().map(|root| AccountPaths::for_root(&root, account))
}

impl QuillApp {
    /// The account on screen's recent hashtags, read from its folder the
    /// first time. `None` with no account on screen.
    pub(super) fn recent_hashtags(&mut self) -> Option<&RecentHashtags> {
        let account = self.session()?.account.clone();
        let list = self.screen.recent_hashtags.list.for_owner(&account);
        if list.is_none() {
            let loaded = account_paths(&account)
                .map(|paths| quill::settings::load_recent_hashtags(&paths))
                .unwrap_or_default();
            *list = Some(loaded);
        }
        list.as_ref()
    }

    /// Remember the hashtags of a message that is being sent, for every
    /// window's composer, and save them in the account's folder.
    pub(super) fn remember_sent_hashtags(&mut self, text: &str) {
        let Some(account) = self.session().map(|session| session.account.clone()) else {
            return;
        };
        if self.recent_hashtags().is_none() {
            return;
        }
        let list = self.screen.recent_hashtags.list.for_owner(&account);
        let Some(list) = list.as_mut() else {
            return;
        };
        if !list.record_message(text) {
            return;
        }
        if let Some(paths) = account_paths(&account) {
            quill::settings::save_recent_hashtags(&paths, list);
        }
    }
}
