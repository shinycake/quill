//! The account switch shortcuts (Alt+1..6, Ctrl+1..6 on macOS; rebindable
//! on the Shortcuts page): tdesktop `Command::ShowAccount1..6`, handled in
//! `SessionController` only while no box is shown and the window is not
//! locked. The Nth account is the Nth row of the Accounts list.

use super::app::QuillApp;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::ids::AccountKey;
use quill::settings::{AccountRecord, active_account, list_accounts, safe_app_root};

/// The account a switch shortcut for `index` goes to: the `index`th of
/// `accounts`, unless it is the one on screen or there is none.
pub(super) fn account_shortcut_target(
    accounts: &[AccountRecord],
    on_screen: Option<&AccountKey>,
    index: usize,
) -> Option<AccountRecord> {
    accounts
        .get(index)
        .filter(|record| Some(&record.key) != on_screen)
        .cloned()
}

impl QuillApp {
    /// Switch to the `index`th account. `false` (the key goes on to the
    /// focused field) when nothing switched: a box, viewer or the lock is up,
    /// there is no such account, or it is already on screen.
    pub(super) fn show_account_by_key(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.account.passcode.is_locked()
            || self.viewer.state.is_open()
            || self.stories.viewer.is_open()
            || window.has_active_dialog(cx)
        {
            return false;
        }
        let Some(root) = safe_app_root() else {
            return false;
        };
        let on_screen = self
            .session()
            .map(|session| session.account.clone())
            .unwrap_or_else(|| active_account(&root));
        let Some(target) = account_shortcut_target(&list_accounts(&root), Some(&on_screen), index)
        else {
            return false;
        };
        self.switch_account(&target.key, &target.display_name, window, cx);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::account_shortcut_target;
    use quill::ids::AccountKey;
    use quill::settings::AccountRecord;

    fn record(key: &str, name: &str) -> AccountRecord {
        AccountRecord {
            key: AccountKey::new(key).expect("key"),
            display_name: name.to_string(),
        }
    }

    #[test]
    fn the_nth_shortcut_goes_to_the_nth_listed_account() {
        let accounts = [
            record("primary", "Primary"),
            record("account-1", "Work"),
            record("account-2", "Family"),
        ];
        let primary = AccountKey::primary();
        let target = account_shortcut_target(&accounts, Some(&primary), 2);
        assert_eq!(target.map(|r| r.display_name), Some("Family".to_string()));
        let target = account_shortcut_target(&accounts, Some(&primary), 1);
        assert_eq!(target.map(|r| r.display_name), Some("Work".to_string()));
    }

    #[test]
    fn the_account_on_screen_and_missing_accounts_are_no_target() {
        let accounts = [record("primary", "Primary"), record("account-1", "Work")];
        let primary = AccountKey::primary();
        assert_eq!(account_shortcut_target(&accounts, Some(&primary), 0), None);
        assert_eq!(account_shortcut_target(&accounts, Some(&primary), 2), None);
        assert_eq!(account_shortcut_target(&accounts, Some(&primary), 5), None);
    }
}
