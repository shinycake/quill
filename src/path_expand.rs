//! Expanding a leading `~` in a path the user typed.
//!
//! `HOME` is not set on Windows, so the home directory comes from the
//! platform lookup (`USERPROFILE` / known folders via `directories`), with
//! `HOME` only as a fallback.
#![deny(clippy::indexing_slicing)]

use std::path::{Path, PathBuf};

/// The user's home directory: the platform's, else `$HOME`, else `USERPROFILE`.
pub fn home_dir() -> Option<PathBuf> {
    directories::UserDirs::new()
        .map(|dirs| dirs.home_dir().to_path_buf())
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
        .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
        .filter(|dir| !dir.as_os_str().is_empty())
}

/// `value` with a leading `~` replaced by the home directory. Only `~`
/// alone or `~` followed by a path separator expands (`~other` is left
/// as typed, since it names another user's home).
pub fn expand_tilde(value: &str) -> String {
    expand_tilde_with(value, home_dir().as_deref())
}

/// [`expand_tilde`] with the home directory supplied (testable).
pub fn expand_tilde_with(value: &str, home: Option<&Path>) -> String {
    let Some(rest) = value.strip_prefix('~') else {
        return value.to_string();
    };
    let Some(home) = home else {
        return value.to_string();
    };
    if rest.is_empty() {
        return home.display().to_string();
    }
    match rest.strip_prefix(['/', '\\']) {
        Some(tail) => home.join(tail).display().to_string(),
        None => value.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::expand_tilde_with;
    use std::path::Path;

    #[test]
    fn tilde_expands_to_the_given_home() {
        let home = Some(Path::new("/home/a"));
        assert_eq!(expand_tilde_with("~", home), "/home/a");
        assert_eq!(
            expand_tilde_with("~/Pictures/x.jpg", home),
            Path::new("/home/a")
                .join("Pictures/x.jpg")
                .display()
                .to_string()
        );
        assert_eq!(
            expand_tilde_with(r"~\Pictures", home),
            Path::new("/home/a").join("Pictures").display().to_string()
        );
    }

    #[test]
    fn other_values_are_left_alone() {
        let home = Some(Path::new("/home/a"));
        assert_eq!(expand_tilde_with("~bob/x", home), "~bob/x");
        assert_eq!(expand_tilde_with("/tmp/~x", home), "/tmp/~x");
        assert_eq!(expand_tilde_with("", home), "");
        // No home known: keep what was typed so the "file not found" note shows it.
        assert_eq!(expand_tilde_with("~/x", None), "~/x");
    }
}
