//! Pure logic of the folder editor's chat sections and picker, after
//! tdesktop's `boxes/filters/edit_filter_box.cpp` ("Included chats" /
//! "Excluded chats" with an "Add Chats" button) and
//! `edit_filter_chats_list.cpp` (the picker box: search, "N / limit").

/// Which list the picker edits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickerMode {
    Include,
    Exclude,
}

impl PickerMode {
    /// Section title in the editor (`lng_filters_include` / `_exclude`).
    pub fn section_title(self) -> &'static str {
        match self {
            PickerMode::Include => "Included chats",
            PickerMode::Exclude => "Excluded chats",
        }
    }

    /// The line under the section title.
    pub fn about(self) -> &'static str {
        match self {
            PickerMode::Include => {
                "Choose chats or types of chats that will appear in this folder."
            }
            PickerMode::Exclude => {
                "Choose chats or types of chats that will not appear in this folder."
            }
        }
    }

    /// The button that opens the picker.
    pub fn add_label(self) -> &'static str {
        match self {
            PickerMode::Include => "Add Chats",
            PickerMode::Exclude => "Add Chats to Exclude",
        }
    }

    /// Picker box title (`lng_filters_include_title` / `_exclude_title`).
    pub fn picker_title(self) -> &'static str {
        match self {
            PickerMode::Include => "Include Chats",
            PickerMode::Exclude => "Exclude Chats",
        }
    }
}

/// "1 chat" / "N chats" (`lng_filters_chats_count`).
pub fn chats_count_text(count: usize) -> String {
    if count == 1 {
        "1 chat".to_string()
    } else {
        format!("{count} chats")
    }
}

/// The picker's counter next to its title: "3 / 100".
pub fn picker_counter(selected: usize, limit: i32) -> String {
    format!("{selected} / {limit}")
}

/// Whether a row may be ticked: unticking is always allowed, ticking only
/// below the limit (tdesktop opens the limit box otherwise).
pub fn may_tick(selected: usize, limit: i32, already_ticked: bool) -> bool {
    already_ticked || (selected as i64) < i64::from(limit)
}

/// The picker rows for a search query: case-insensitive match on the
/// title, every chat for a blank query. Order is kept.
pub fn filter_chats(chats: &[(i64, String)], query: &str) -> Vec<(i64, String)> {
    let needle = query.trim().to_lowercase();
    chats
        .iter()
        .filter(|(_, title)| needle.is_empty() || title.to_lowercase().contains(&needle))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{PickerMode, chats_count_text, filter_chats, may_tick, picker_counter};

    #[test]
    fn counts_read_like_telegram() {
        assert_eq!(chats_count_text(0), "0 chats");
        assert_eq!(chats_count_text(1), "1 chat");
        assert_eq!(chats_count_text(12), "12 chats");
        assert_eq!(picker_counter(3, 100), "3 / 100");
    }

    #[test]
    fn ticking_stops_at_the_limit_but_unticking_never_does() {
        assert!(may_tick(99, 100, false));
        assert!(!may_tick(100, 100, false));
        assert!(may_tick(100, 100, true));
        assert!(!may_tick(0, 0, false));
    }

    #[test]
    fn search_matches_titles_ignoring_case_and_padding() {
        let chats = vec![
            (1, "Maya Chen".to_string()),
            (2, "Design team".to_string()),
            (3, "Leo Park".to_string()),
        ];
        assert_eq!(filter_chats(&chats, "").len(), 3);
        assert_eq!(filter_chats(&chats, "   ").len(), 3);
        assert_eq!(filter_chats(&chats, " DESIGN "), vec![chats[1].clone()]);
        assert_eq!(filter_chats(&chats, "a").len(), 3);
        assert!(filter_chats(&chats, "zzz").is_empty());
    }

    #[test]
    fn mode_texts_differ() {
        assert_eq!(PickerMode::Include.add_label(), "Add Chats");
        assert_eq!(PickerMode::Exclude.add_label(), "Add Chats to Exclude");
        assert_eq!(PickerMode::Include.picker_title(), "Include Chats");
        assert_eq!(PickerMode::Exclude.section_title(), "Excluded chats");
        assert_ne!(PickerMode::Include.about(), PickerMode::Exclude.about());
    }
}
