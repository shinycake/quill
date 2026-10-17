//! Whether the first-run onboarding (intro clip, notice, highlights) was
//! finished, app-wide in `first_run.json` under the app data root.

use super::{AccountPaths, active_account, write_json_atomic};
use serde::{Deserialize, Serialize};
use std::path::Path;

const FILE_NAME: &str = "first_run.json";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
struct FirstRunPrefs {
    completed: bool,
}

/// Whether to show the onboarding at startup. A started run is written
/// down at once, so quitting before the notice was acknowledged shows it
/// again even though TDLib has made its database by then. Without that
/// file, an account with a TDLib database is an install from before the
/// onboarding (a signed-in user who upgraded): it is marked finished.
pub fn first_run_pending(app_root: &Path) -> bool {
    match load(app_root) {
        Some(prefs) => !prefs.completed,
        None if AccountPaths::for_root(app_root, &active_account(app_root)).database_exists() => {
            let _ = mark_first_run_done(app_root);
            false
        }
        None => {
            let _ = save(app_root, false);
            true
        }
    }
}

pub fn mark_first_run_done(app_root: &Path) -> std::io::Result<()> {
    save(app_root, true)
}

fn save(app_root: &Path, completed: bool) -> std::io::Result<()> {
    write_json_atomic(&app_root.join(FILE_NAME), &FirstRunPrefs { completed })
}

/// The saved file; a broken one reads as a run that was started.
fn load(app_root: &Path) -> Option<FirstRunPrefs> {
    let bytes = std::fs::read(app_root.join(FILE_NAME)).ok()?;
    Some(serde_json::from_slice(&bytes).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::{first_run_pending, mark_first_run_done};
    use crate::ids::AccountKey;
    use crate::settings::AccountPaths;

    fn temp_root(name: &str) -> std::path::PathBuf {
        let root =
            std::env::temp_dir().join(format!("quill-first-run-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        root
    }

    #[test]
    fn a_fresh_install_is_pending_until_marked_done() {
        let root = temp_root("fresh");
        assert!(first_run_pending(&root));
        mark_first_run_done(&root).expect("write first_run.json");
        assert!(!first_run_pending(&root));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn an_install_with_a_database_skips_it_and_remembers() {
        let root = temp_root("existing");
        let paths = AccountPaths::for_root(&root, &AccountKey::primary());
        std::fs::create_dir_all(&paths.tdlib_database).expect("database dir");
        std::fs::write(paths.tdlib_database.join("td.binlog"), b"x").expect("binlog");
        assert!(!first_run_pending(&root));
        std::fs::remove_dir_all(&paths.tdlib_database).expect("drop database");
        assert!(!first_run_pending(&root), "remembered as finished");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_run_quit_before_the_end_shows_again_once_signed_in_data_exists() {
        let root = temp_root("quit");
        assert!(first_run_pending(&root));
        let paths = AccountPaths::for_root(&root, &AccountKey::primary());
        std::fs::create_dir_all(&paths.tdlib_database).expect("database dir");
        std::fs::write(paths.tdlib_database.join("td.binlog"), b"x").expect("binlog");
        assert!(first_run_pending(&root), "started, not finished");
        mark_first_run_done(&root).expect("write first_run.json");
        assert!(!first_run_pending(&root));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_broken_file_counts_as_not_finished() {
        let root = temp_root("broken");
        std::fs::create_dir_all(&root).expect("root");
        std::fs::write(root.join("first_run.json"), b"not json").expect("write");
        assert!(first_run_pending(&root));
        let _ = std::fs::remove_dir_all(&root);
    }
}
