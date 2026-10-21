//! What people type for settings that Telegram names differently. Matched
//! against whole folded queries; each alias is searched like a query.

use super::fuzzy::{typo, words};

const SYNONYMS: &[(&str, &[&str])] = &[
    ("sign out", &["log out"]),
    ("signout", &["log out"]),
    ("logout", &["log out"]),
    ("dark mode", &["night mode"]),
    ("dark", &["night mode"]),
    ("light mode", &["night mode"]),
    ("2fa", &["two-step verification"]),
    ("two factor", &["two-step verification"]),
    ("password", &["two-step verification", "local passcode"]),
    ("ptt", &["push to talk"]),
    ("zoom", &["interface scale"]),
    ("vpn", &["proxy"]),
    ("cache", &["storage usage"]),
    ("disk", &["storage usage"]),
    ("autostart", &["launch at login"]),
    ("startup", &["launch at login"]),
    ("hotkeys", &["keyboard shortcuts"]),
    ("keybindings", &["keyboard shortcuts"]),
    ("sessions", &["devices"]),
    ("changelog", &["what's new"]),
    ("release notes", &["what's new"]),
    ("wallpaper", &["chat background"]),
    ("mic", &["microphone"]),
];

/// Aliases for `query`, when its folded words equal a known phrase or are
/// one typo per word away from it ("drk mode").
pub fn expand(query: &str) -> impl Iterator<Item = &'static str> {
    let wanted = words(query);
    SYNONYMS
        .iter()
        .filter(move |(phrase, _)| {
            let have = words(phrase);
            have == wanted || (have.len() == wanted.len() && typo(query, phrase).is_some())
        })
        .flat_map(|(_, aliases)| aliases.iter().copied())
}
