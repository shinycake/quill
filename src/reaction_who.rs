//! "Who reacted" on a reaction chip (tdesktop `Api::WhoReacted`,
//! `ShowWhoReactedMenu`): the hover text and whether the chip can open the
//! reactor list. Kept pure so it is unit tested.

/// `lng_context_seen_reactions_count`: "1 Reaction" / "N Reactions".
pub fn count_label(total: i32) -> String {
    if total == 1 {
        "1 Reaction".to_string()
    } else {
        format!("{total} Reactions")
    }
}

/// Hover text of one reaction chip. `names` are the recent reactors TDLib
/// sent, `total` how many people picked this reaction.
///
/// - every reactor known: their names ("Ann, Ben");
/// - some known: the names and the rest ("Ann, Ben and 4 more");
/// - none known (channels, large groups): the count.
pub fn tooltip_text(names: &[String], total: i32) -> String {
    let known: Vec<&str> = names
        .iter()
        .map(|name| name.trim())
        .filter(|name| !name.is_empty())
        .collect();
    if total <= 0 {
        return String::new();
    }
    if known.is_empty() {
        return count_label(total);
    }
    let joined = known.join(", ");
    let rest = total - known.len() as i32;
    if rest > 0 {
        format!("{joined} and {rest} more")
    } else {
        joined
    }
}

/// Whether a chip can open the reactor list: TDLib allows listing the
/// message's reactors and the chip is not a Saved Messages tag.
pub fn chip_opens_list(can_get_added_reactions: bool, are_tags: bool) -> bool {
    can_get_added_reactions && !are_tags
}

#[cfg(test)]
mod tests {
    use super::{chip_opens_list, count_label, tooltip_text};

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|name| name.to_string()).collect()
    }

    #[test]
    fn known_reactors_are_listed() {
        assert_eq!(tooltip_text(&names(&["Ann", "Ben"]), 2), "Ann, Ben");
    }

    #[test]
    fn the_rest_is_counted() {
        assert_eq!(
            tooltip_text(&names(&["Ann", "Ben"]), 6),
            "Ann, Ben and 4 more"
        );
    }

    #[test]
    fn unknown_reactors_fall_back_to_the_count() {
        assert_eq!(tooltip_text(&[], 1), "1 Reaction");
        assert_eq!(tooltip_text(&names(&["", " "]), 12), "12 Reactions");
        assert_eq!(tooltip_text(&[], 0), "");
        assert_eq!(count_label(3), "3 Reactions");
    }

    #[test]
    fn tags_and_unlisted_messages_do_not_open_the_list() {
        assert!(chip_opens_list(true, false));
        assert!(!chip_opens_list(false, false));
        assert!(!chip_opens_list(true, true));
    }
}
