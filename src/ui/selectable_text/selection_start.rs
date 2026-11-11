//! Where a text selection starts, so Quote & Reply can point at the
//! occurrence the user selected when the same phrase stands more than once
//! in a message.
#![deny(clippy::indexing_slicing)]

use super::with_selected;
use gpui_kit::*;

/// The text before the selection's start in the first painted text it
/// touches, with that text's document order.
#[derive(Default)]
pub(super) struct SelectionStart(Option<(u64, String)>);

impl SelectionStart {
    /// A painted text reports the text before its selected start, or
    /// `None` when none of it is selected. The text first in document order
    /// wins; a text that no longer holds the selection lets go.
    pub(super) fn note(&mut self, order: u64, before: Option<String>) {
        let earlier = self.0.as_ref().is_some_and(|(held, _)| *held < order);
        match before {
            Some(before) if !earlier => self.0 = Some((order, before)),
            None if self.0.as_ref().is_some_and(|(held, _)| *held == order) => self.0 = None,
            _ => {}
        }
    }

    /// How many whole copies of `selected` stand before the start.
    fn occurrence(&self, selected: &str) -> usize {
        let Some((_, before)) = self.0.as_ref() else {
            return 0;
        };
        if selected.is_empty() {
            return 0;
        }
        before
            .char_indices()
            .filter(|(at, _)| {
                before
                    .get(*at..)
                    .is_some_and(|rest| rest.starts_with(selected))
            })
            .count()
    }
}

/// The source text before the first selected byte of a painted text.
pub(super) fn text_before(
    text: &str,
    source: Option<&quill::display_text::SourceMap>,
    ranges: &[Option<std::ops::Range<usize>>],
) -> Option<String> {
    let start = ranges
        .iter()
        .flatten()
        .find(|range| !range.is_empty())
        .map(|range| range.start)?;
    let shown = text.get(..start).unwrap_or_default();
    Some(match source {
        Some(map) => map
            .source_of(text, 0..start)
            .unwrap_or_else(|| shown.to_string()),
        None => shown.to_string(),
    })
}

/// Which occurrence of `selected` in its message the window's selection
/// starts at (counted from 0), for a quote of text that repeats.
pub(in crate::ui) fn selected_occurrence(
    window: &mut Window,
    cx: &mut App,
    selected: &str,
) -> usize {
    with_selected(window, cx, |held| held.start.occurrence(selected))
}

#[cfg(test)]
mod tests {
    use super::SelectionStart;

    #[test]
    fn the_occurrence_is_how_many_copies_come_before_the_selection() {
        let mut start = SelectionStart::default();
        assert_eq!(start.occurrence("ok."), 0);
        start.note(1, Some("ok. fine. ok. ".into()));
        assert_eq!(start.occurrence("ok."), 2);
        assert_eq!(start.occurrence(""), 0);
    }

    #[test]
    fn the_first_text_in_document_order_holds_the_start() {
        let mut start = SelectionStart::default();
        start.note(5, Some("later ".into()));
        start.note(2, Some("earlier ".into()));
        start.note(5, Some("later still ".into()));
        assert_eq!(start.0.as_ref().map(|(order, _)| *order), Some(2));
        start.note(2, None);
        assert!(start.0.is_none());
    }
}
