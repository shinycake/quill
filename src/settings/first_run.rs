//! Whether the first-run onboarding (intro clip, notice, highlights) was
//! finished, app-wide in `first_run.json` under the app data root.

use super::{AccountPaths, active_account, write_json_atomic};
use std::path::Path;

const FILE_NAME: &str = "first_run.json";

/// How far the first run got. The stage is read by [`first_run_stage`]
/// (pure) and moved by [`first_run_pending`] and [`mark_first_run_done`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FirstRun {
    /// No file and no signed-in data: a fresh install.
    NotStarted,
    /// The onboarding was shown and not finished (or its file is broken).
    Started,
    /// The onboarding was finished.
    Done,
    /// No file, but an account with a TDLib database: an install from before
    /// the onboarding. It counts as finished once [`first_run_pending`]
    /// writes that down.
    Upgraded,
}

/// The stage `app_root` is in. Reads the file and the account's database;
/// changes nothing.
pub fn first_run_stage(app_root: &Path) -> FirstRun {
    match load(app_root) {
        Some(true) => FirstRun::Done,
        Some(false) => FirstRun::Started,
        None if AccountPaths::for_root(app_root, &active_account(app_root)).database_exists() => {
            FirstRun::Upgraded
        }
        None => FirstRun::NotStarted,
    }
}

/// Whether to show the onboarding at startup. A started run is written
/// down at once, so quitting before the notice was acknowledged shows it
/// again even though TDLib has made its database by then. Without that
/// file, an account with a TDLib database is an install from before the
/// onboarding (a signed-in user who upgraded): it is marked finished.
pub fn first_run_pending(app_root: &Path) -> bool {
    match first_run_stage(app_root) {
        FirstRun::Started => true,
        FirstRun::Done => false,
        FirstRun::Upgraded => {
            let _ = mark_first_run_done(app_root);
            false
        }
        FirstRun::NotStarted => {
            // Without this file the next start cannot tell a new install from
            // one that predates the onboarding; nothing else is durable, so
            // the failure is at least logged.
            if let Err(err) = save(app_root, false) {
                log::warn!("first run: could not record the started onboarding: {err}");
            }
            true
        }
    }
}

pub fn mark_first_run_done(app_root: &Path) -> std::io::Result<()> {
    save(app_root, true)
}

/// The file's one key, `completed`, is what older versions wrote.
fn save(app_root: &Path, done: bool) -> std::io::Result<()> {
    write_json_atomic(
        &app_root.join(FILE_NAME),
        &serde_json::json!({ "completed": done }),
    )
}

/// The saved file: `Some(finished)`; a broken one reads as a run that was
/// started.
fn load(app_root: &Path) -> Option<bool> {
    let bytes = std::fs::read(app_root.join(FILE_NAME)).ok()?;
    let finished = serde_json::from_slice::<serde_json::Value>(&bytes)
        .ok()
        .and_then(|value| value.get("completed").and_then(serde_json::Value::as_bool))
        .unwrap_or(false);
    Some(finished)
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
