//! The app's own preferences folder, `<app root>/app`.
//!
//! Appearance, chat, spellcheck, translation and settings-search prefs and
//! the custom themes are the app's, not an account's (tdesktop keeps them
//! in its app-wide `settings` file and `tdata`). They lived in
//! `accounts/primary/`, the first account's folder, so removing that
//! account deleted them. They now live outside `accounts/`, which account
//! removal never touches, and the first use moves them across.

use super::*;

/// The folder name under the app root.
pub const APP_PREFS_DIR: &str = "app";

/// Written into the app folder once every item has been looked at.
const MIGRATED_MARKER: &str = ".migrated-from-primary";

/// The files moved out of `accounts/primary/`.
const MOVED_FILES: &[&str] = &[
    "appearance_prefs.json",
    "chat_prefs.json",
    "spellcheck_words.json",
    "spellcheck_languages.json",
    "translate_prefs.json",
    "recent_hashtags.json",
    "settings_search_recent.json",
];

/// The custom themes folder (`app_theme::store::themes_dir`), moved whole.
const MOVED_DIRS: &[&str] = &["themes"];

/// `<app root>/app`.
pub fn app_prefs_dir(app_root: &Path) -> PathBuf {
    app_root.join(APP_PREFS_DIR)
}

/// Paths for the app's own prefs: `root` is `<app root>/app`, which the
/// prefs loaders and savers join their file names to. The other fields
/// are unused. The first call for a root in this process moves the files
/// out of `accounts/primary/` ([`migrate_app_prefs`]), so nothing reads
/// the new folder before the move.
pub fn app_prefs_paths(app_root: &Path) -> AccountPaths {
    static MIGRATED: std::sync::Mutex<Vec<PathBuf>> = std::sync::Mutex::new(Vec::new());
    {
        let mut done = crate::locks::lock_or_recover(&MIGRATED);
        if !done.iter().any(|root| root == app_root) {
            migrate_app_prefs(app_root);
            done.push(app_root.to_path_buf());
        }
    }
    AccountPaths::in_folder(app_prefs_dir(app_root))
}

/// Move the app's prefs from `accounts/primary/` into `<app root>/app`.
///
/// Each item moves on its own, by rename (copy then remove when a rename
/// fails, as across drives), so an interrupted run leaves every item at
/// the old place or the new one. An item already in the app folder wins
/// and the old copy is left alone. The marker is written last; once it is
/// there the run is skipped. Nothing else in the account folder is
/// touched, and IO errors are logged, never fatal: a file that did not
/// move is tried again on the next start.
pub fn migrate_app_prefs(app_root: &Path) {
    let to = app_prefs_dir(app_root);
    if to.join(MIGRATED_MARKER).exists() {
        return;
    }
    let from = AccountPaths::for_root(app_root, &AccountKey::primary()).root;
    if let Err(err) = std::fs::create_dir_all(&to) {
        log::warn!("app prefs: could not create {}: {err}", to.display());
        return;
    }
    let mut all_moved = true;
    for name in MOVED_FILES.iter().chain(MOVED_DIRS) {
        if let Err(err) = move_item(&from.join(name), &to.join(name), &to) {
            log::warn!("app prefs: could not move {name}: {err}");
            all_moved = false;
        }
    }
    if all_moved && let Err(err) = std::fs::write(to.join(MIGRATED_MARKER), b"") {
        log::warn!("app prefs: could not write the migration marker: {err}");
    }
}

/// Move `old` to `new` unless `new` exists or `old` does not.
fn move_item(old: &Path, new: &Path, folder: &Path) -> std::io::Result<()> {
    if new.symlink_metadata().is_ok() {
        return Ok(());
    }
    let Ok(meta) = old.symlink_metadata() else {
        return Ok(());
    };
    if std::fs::rename(old, new).is_ok() {
        sync_dir(folder);
        return Ok(());
    }
    copy_into_place(old, new, folder, meta.is_dir())
}

/// The move when a rename fails, as across drives: copy under a temporary
/// name, put the copy in place with a rename, and only then remove the
/// old one.
fn copy_into_place(old: &Path, new: &Path, folder: &Path, is_dir: bool) -> std::io::Result<()> {
    let name = new
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("item");
    let staging = folder.join(format!(".{name}.migrating"));
    remove_any(&staging)?;
    if is_dir {
        copy_dir(old, &staging)?;
    } else {
        std::fs::copy(old, &staging)?;
    }
    std::fs::rename(&staging, new)?;
    sync_dir(folder);
    remove_any(old)
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

/// Remove a file or folder; a missing one is fine.
fn remove_any(path: &Path) -> std::io::Result<()> {
    let result = match path.symlink_metadata() {
        Ok(meta) if meta.is_dir() => std::fs::remove_dir_all(path),
        Ok(_) => std::fs::remove_file(path),
        Err(_) => return Ok(()),
    };
    match result {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tmp_root(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "quill-app-prefs-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn primary(root: &Path) -> PathBuf {
        AccountPaths::for_root(root, &AccountKey::primary()).root
    }

    /// The first account's folder with prefs, a theme and its own data.
    fn old_layout(root: &Path) -> PathBuf {
        let old = primary(root);
        fs::create_dir_all(old.join("themes")).unwrap();
        fs::create_dir_all(old.join("tdlib")).unwrap();
        fs::write(old.join("appearance_prefs.json"), b"{\"theme\":\"dark\"}").unwrap();
        fs::write(old.join("chat_prefs.json"), b"{}").unwrap();
        fs::write(old.join("settings_search_recent.json"), b"[]").unwrap();
        fs::write(old.join("themes/Night.tdesktop-theme"), b"theme").unwrap();
        fs::write(old.join("tdlib/td.binlog"), b"db").unwrap();
        fs::write(old.join("media_prefs.json"), b"{}").unwrap();
        old
    }

    #[test]
    fn prefs_and_themes_move_out_of_the_first_account() {
        let root = tmp_root("move");
        let old = old_layout(&root);
        migrate_app_prefs(&root);
        let app = app_prefs_dir(&root);
        assert_eq!(
            fs::read(app.join("appearance_prefs.json")).unwrap(),
            b"{\"theme\":\"dark\"}"
        );
        assert!(app.join("chat_prefs.json").exists());
        assert!(app.join("settings_search_recent.json").exists());
        assert_eq!(
            fs::read(app.join("themes/Night.tdesktop-theme")).unwrap(),
            b"theme"
        );
        assert!(app.join(MIGRATED_MARKER).exists());
        assert!(!old.join("appearance_prefs.json").exists());
        assert!(!old.join("themes").exists());
        // The account's own files stay.
        assert!(old.join("tdlib/td.binlog").exists());
        assert!(old.join("media_prefs.json").exists());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn nothing_to_move_gives_defaults_and_no_error() {
        let root = tmp_root("none");
        migrate_app_prefs(&root);
        let paths = AccountPaths::in_folder(app_prefs_dir(&root));
        assert_eq!(
            load_appearance_prefs(&paths).theme,
            AppearancePrefs::default().theme
        );
        assert!(app_prefs_dir(&root).join(MIGRATED_MARKER).exists());
        assert!(!primary(&root).exists(), "no account folder is created");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_second_run_changes_nothing() {
        let root = tmp_root("twice");
        old_layout(&root);
        migrate_app_prefs(&root);
        let app = app_prefs_dir(&root);
        fs::write(app.join("appearance_prefs.json"), b"{\"theme\":\"light\"}").unwrap();
        // A file the old layout gets again (an older build ran) stays put.
        fs::write(primary(&root).join("chat_prefs.json"), b"{\"old\":1}").unwrap();
        migrate_app_prefs(&root);
        assert_eq!(
            fs::read(app.join("appearance_prefs.json")).unwrap(),
            b"{\"theme\":\"light\"}"
        );
        assert!(primary(&root).join("chat_prefs.json").exists());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn an_interrupted_run_finishes_the_rest() {
        let root = tmp_root("partial");
        let old = old_layout(&root);
        let app = app_prefs_dir(&root);
        // The earlier run moved one file and stopped before the marker.
        fs::create_dir_all(&app).unwrap();
        fs::rename(
            old.join("appearance_prefs.json"),
            app.join("appearance_prefs.json"),
        )
        .unwrap();
        migrate_app_prefs(&root);
        assert!(app.join("appearance_prefs.json").exists());
        assert!(app.join("chat_prefs.json").exists());
        assert!(app.join("themes/Night.tdesktop-theme").exists());
        assert!(!old.join("chat_prefs.json").exists());
        assert!(app.join(MIGRATED_MARKER).exists());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn the_app_folder_wins_and_the_old_copy_is_left_alone() {
        let root = tmp_root("both");
        let old = old_layout(&root);
        let app = app_prefs_dir(&root);
        fs::create_dir_all(app.join("themes")).unwrap();
        fs::write(app.join("appearance_prefs.json"), b"new").unwrap();
        fs::write(app.join("themes/Day.tdesktop-theme"), b"day").unwrap();
        migrate_app_prefs(&root);
        assert_eq!(fs::read(app.join("appearance_prefs.json")).unwrap(), b"new");
        assert_eq!(
            fs::read(old.join("appearance_prefs.json")).unwrap(),
            b"{\"theme\":\"dark\"}"
        );
        assert!(!app.join("themes/Night.tdesktop-theme").exists());
        assert!(old.join("themes/Night.tdesktop-theme").exists());
        // Files only the old folder had still move.
        assert!(app.join("chat_prefs.json").exists());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_copy_across_drives_lands_whole_and_replaces_a_half_copy() {
        let root = tmp_root("copy");
        let old = old_layout(&root);
        let app = app_prefs_dir(&root);
        // An earlier copy stopped half way.
        fs::create_dir_all(app.join(".themes.migrating")).unwrap();
        fs::write(app.join(".themes.migrating/half"), b"x").unwrap();
        copy_into_place(&old.join("themes"), &app.join("themes"), &app, true).unwrap();
        copy_into_place(
            &old.join("chat_prefs.json"),
            &app.join("chat_prefs.json"),
            &app,
            false,
        )
        .unwrap();
        assert_eq!(
            fs::read(app.join("themes/Night.tdesktop-theme")).unwrap(),
            b"theme"
        );
        assert!(!app.join("themes/half").exists());
        assert!(!app.join(".themes.migrating").exists());
        assert!(app.join("chat_prefs.json").exists());
        assert!(!old.join("themes").exists());
        assert!(!old.join("chat_prefs.json").exists());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn removing_the_first_account_keeps_the_app_prefs_and_themes() {
        let root = tmp_root("remove");
        old_layout(&root);
        let second = add_account(&root, "Work").unwrap();
        let paths = app_prefs_paths(&root);
        save_json_prefs(&paths, "translate_prefs.json", &serde_json::json!({}));
        flush_deferred_writes();
        set_active_account(&root, &second).unwrap();
        remove_account(&root, &AccountKey::primary()).unwrap();
        assert!(!primary(&root).exists(), "the account folder went");
        let app = app_prefs_dir(&root);
        assert_eq!(load_appearance_prefs(&paths).theme, ThemeChoice::Dark);
        assert!(app.join("translate_prefs.json").exists());
        assert!(app.join("themes/Night.tdesktop-theme").exists());
        let _ = fs::remove_dir_all(&root);
    }
}
