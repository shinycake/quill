//! Pure decisions behind the topic tab strip: which child of the scrolling
//! row is the selected tab, and which labels may be cut by the width cap.
//! The reveal-when-changed rule is the folder strip's
//! (`folder_tabs_fit::needs_reveal`).
#![deny(clippy::indexing_slicing)]

/// Widest a tab label grows before it is cut with an ellipsis; the tab's
/// tooltip carries the full name.
pub(super) const LABEL_MAX_WIDTH: f32 = 180.;
/// Characters of the `text_sm` semibold label that fit under the cap (7.5 px
/// on average). Longer names get the tooltip; the estimate only decides
/// whether the tooltip is worth showing, the cap itself is exact.
const LABEL_FIT_CHARS: usize = 22;

/// Index of the selected tab among the scrolling row's children: "All" is
/// child 0 and the topics follow in strip order. A selected topic that is
/// not in the strip (hidden, or not loaded yet) reveals nothing special: the
/// strip stays where it is, so `None`.
pub(super) fn selected_child(active: Option<i32>, topic_ids: &[i32]) -> Option<usize> {
    match active {
        None => Some(0),
        Some(id) => topic_ids
            .iter()
            .position(|topic| *topic == id)
            .map(|position| position.saturating_add(1)),
    }
}

/// Whether the label might be cut by [`LABEL_MAX_WIDTH`].
pub(super) fn label_may_be_cut(label: &str) -> bool {
    label.chars().count() > LABEL_FIT_CHARS
}

#[cfg(test)]
mod tests {
    use super::{label_may_be_cut, selected_child};

    #[test]
    fn all_is_the_first_child_and_topics_follow() {
        let ids = [10, 20, 30];
        assert_eq!(selected_child(None, &ids), Some(0));
        assert_eq!(selected_child(Some(10), &ids), Some(1));
        assert_eq!(selected_child(Some(30), &ids), Some(3));
    }

    #[test]
    fn a_topic_missing_from_the_strip_reveals_nothing() {
        assert_eq!(selected_child(Some(99), &[10, 20]), None);
        assert_eq!(selected_child(Some(1), &[]), None);
    }

    #[test]
    fn only_long_labels_get_a_tooltip() {
        assert!(!label_may_be_cut("General"));
        assert!(label_may_be_cut(
            "An unreasonably long topic name that overflows"
        ));
        // Counts characters, not bytes.
        assert!(!label_may_be_cut("abc abc abc abc abc"));
    }
}
