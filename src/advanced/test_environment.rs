//! The Telegram test environment, as tdesktop's `testmode` code exposes it.
//!
//! tdesktop (`settings_codes.cpp`, `Domain::addActivated`): only when there
//! is exactly one account and it is signed out, the code adds a new account
//! in the other environment and activates it, with a toast. The sign-in
//! screen of such an account shows a "Test Mode" label
//! (`Intro::Widget::_testModeLabel`).
//!
//! Quill keeps the environment next to the account: an account directory
//! holding the marker file below connects with `use_test_dc` in
//! `setTdlibParameters`. The marker lives with the TDLib database it
//! belongs to, so the two environments never share a database.

use crate::ids::AccountKey;
use crate::settings::{AccountPaths, add_account};
use std::path::Path;

/// File in the account directory that makes the account a test account.
pub const MARKER_FILE: &str = "test-environment";
/// Name of the account that `testmode` adds in the test environment.
pub const TEST_ACCOUNT_NAME: &str = "Test environment";
/// Name of the account that `testmode` adds back in production.
pub const PRODUCTION_ACCOUNT_NAME: &str = "Production";

pub const SWITCHED_TO_TEST: &str = "Switched to the test environment.";
pub const SWITCHED_TO_PRODUCTION: &str = "Switched to the production environment.";

/// Whether the account connects to the test environment.
pub fn is_test_environment(paths: &AccountPaths) -> bool {
    paths.root.join(MARKER_FILE).is_file()
}

/// Whether the account with this key connects to the test environment.
pub fn account_is_test(app_root: &Path, key: &AccountKey) -> bool {
    is_test_environment(&AccountPaths::for_root(app_root, key))
}

static ACTIVE_IS_TEST: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Remember whether the account on screen is a test account, for code that
/// has no app state (the schedule menu's test-only repeat periods).
pub fn set_active_test(on: bool) {
    ACTIVE_IS_TEST.store(on, std::sync::atomic::Ordering::Relaxed);
}

/// Whether the account on screen connects to the test environment.
pub fn active_is_test() -> bool {
    ACTIVE_IS_TEST.load(std::sync::atomic::Ordering::Relaxed)
}

/// Make the account a test account. Created before its first connect.
pub fn mark_test_environment(paths: &AccountPaths) -> std::io::Result<()> {
    std::fs::create_dir_all(&paths.root)?;
    std::fs::write(paths.root.join(MARKER_FILE), b"1")
}

/// What the `testmode` code does for the current accounts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestModeAction {
    /// Add a signed-out account in the test environment and switch to it.
    AddTest,
    /// Add a signed-out account in production and switch to it.
    AddProduction,
}

/// tdesktop runs the code only with a single account that has no session.
pub fn plan_test_mode(
    account_count: usize,
    active_is_signed_out: bool,
    active_is_test: bool,
) -> Option<TestModeAction> {
    if account_count != 1 || !active_is_signed_out {
        return None;
    }
    Some(if active_is_test {
        TestModeAction::AddProduction
    } else {
        TestModeAction::AddTest
    })
}

/// Add the account in the other environment; returns its key and the toast.
pub fn add_environment_account(
    app_root: &Path,
    action: TestModeAction,
) -> std::io::Result<(AccountKey, &'static str)> {
    let (name, test, toast) = match action {
        TestModeAction::AddTest => (TEST_ACCOUNT_NAME, true, SWITCHED_TO_TEST),
        TestModeAction::AddProduction => (PRODUCTION_ACCOUNT_NAME, false, SWITCHED_TO_PRODUCTION),
    };
    let key = add_account(app_root, name)?;
    if test {
        mark_test_environment(&AccountPaths::for_root(app_root, &key))?;
    }
    Ok((key, toast))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{list_accounts, remove_account};

    fn temp_root(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("quill-testenv-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_code_needs_one_signed_out_account() {
        assert_eq!(
            plan_test_mode(1, true, false),
            Some(TestModeAction::AddTest)
        );
        assert_eq!(
            plan_test_mode(1, true, true),
            Some(TestModeAction::AddProduction)
        );
        assert_eq!(plan_test_mode(1, false, false), None, "signed in");
        assert_eq!(plan_test_mode(2, true, false), None, "several accounts");
        assert_eq!(plan_test_mode(0, true, false), None);
    }

    #[test]
    fn a_test_account_is_marked_and_a_production_account_is_not() {
        let root = temp_root("mark");
        let (test_key, toast) = add_environment_account(&root, TestModeAction::AddTest).unwrap();
        assert_eq!(toast, SWITCHED_TO_TEST);
        assert!(account_is_test(&root, &test_key));
        let (prod_key, toast) =
            add_environment_account(&root, TestModeAction::AddProduction).unwrap();
        assert_eq!(toast, SWITCHED_TO_PRODUCTION);
        assert!(!account_is_test(&root, &prod_key));
        assert!(!account_is_test(&root, &AccountKey::primary()));
        let accounts = list_accounts(&root);
        assert_eq!(accounts.len(), 3);
        assert!(
            accounts
                .iter()
                .any(|a| a.key == test_key && a.display_name == TEST_ACCOUNT_NAME)
        );
        remove_account(&root, &test_key).unwrap();
        assert!(
            !account_is_test(&root, &test_key),
            "the marker goes with it"
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
