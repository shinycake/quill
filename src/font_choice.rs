//! Font family choice (tdesktop `ChooseFontBox`, `customFontFamily`).
//!
//! The pure parts: which installed families to offer, and whether a stored
//! family still exists. An empty stored family means the platform default.

/// Longest family name kept from a prefs file.
pub const MAX_FAMILY_LEN: usize = 120;

/// Trim a stored family and drop control characters; empty means default.
pub fn clean_family(stored: &str) -> String {
    let cleaned: String = stored.chars().filter(|c| !c.is_control()).collect();
    cleaned.trim().chars().take(MAX_FAMILY_LEN).collect()
}

/// Families to offer: sorted, case-insensitively de-duplicated, without the
/// hidden system faces (names starting with a dot) or empty names.
pub fn choices(installed: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut names: Vec<String> = installed
        .into_iter()
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty() && !name.starts_with('.'))
        .collect();
    names.sort_by_key(|name| name.to_lowercase());
    names.dedup_by_key(|name| name.to_lowercase());
    names
}

/// The installed spelling of `stored`, or `None` when it is empty or no
/// longer installed. The default font is used then; the prefs keep the name
/// so a reinstalled font comes back.
pub fn resolve<'a>(stored: &str, installed: &'a [String]) -> Option<&'a str> {
    let wanted = clean_family(stored).to_lowercase();
    if wanted.is_empty() {
        return None;
    }
    installed
        .iter()
        .find(|name| name.to_lowercase() == wanted)
        .map(String::as_str)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn choices_sort_dedupe_and_hide_system_faces() {
        let got = choices(names(&[
            "Zapfino",
            ".SF NS",
            "arial",
            "Arial",
            "  Helvetica ",
            "",
            "Avenir",
        ]));
        assert_eq!(got, names(&["arial", "Avenir", "Helvetica", "Zapfino"]));
    }

    #[test]
    fn resolve_finds_the_installed_spelling() {
        let installed = names(&["Avenir", "Helvetica Neue"]);
        assert_eq!(
            resolve("helvetica neue", &installed),
            Some("Helvetica Neue")
        );
        assert_eq!(resolve("  Avenir ", &installed), Some("Avenir"));
        assert_eq!(resolve("Comic Sans", &installed), None);
        assert_eq!(resolve("", &installed), None);
        assert_eq!(resolve("   ", &installed), None);
    }

    #[test]
    fn clean_family_strips_controls_and_caps_length() {
        assert_eq!(clean_family("  Fira\nCode\t "), "FiraCode");
        assert_eq!(clean_family(&"x".repeat(500)).len(), MAX_FAMILY_LEN);
    }
}
