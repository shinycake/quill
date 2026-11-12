//! State reducer tests: how a history was opened at its first unread message.
use super::*;
use crate::state::UnreadOpen;

#[test]
fn the_bar_and_the_held_reads_leave_together_or_one_at_a_time() {
    let anchor = MessageId(30);
    let mut open = UnreadOpen::opened_at(Some(anchor));
    assert_eq!(open.anchor(), Some(anchor));
    // Deleting the bar's message drops the bar; reads stay held.
    open.drop_bar();
    assert_eq!(open, UnreadOpen::Opened { bar: None });
    assert_eq!(open.anchor(), None);
    // The jump-down button brings the bar back without ending the hold.
    open.show_bar(anchor);
    assert_eq!(open, UnreadOpen::Opened { bar: Some(anchor) });

    // After the open ended, the bar alone can be shown and dropped.
    let mut ended = UnreadOpen::Closed;
    ended.show_bar(anchor);
    assert_eq!(ended, UnreadOpen::Bar(anchor));
    ended.drop_bar();
    assert_eq!(ended, UnreadOpen::Closed);
    assert_eq!(UnreadOpen::opened_at(None), UnreadOpen::Closed);
}

#[test]
fn ending_the_open_clears_the_bar_and_the_hold_in_one_step() {
    let mut history = HistoryState {
        unread_open: UnreadOpen::opened_at(Some(MessageId(5))),
        ..Default::default()
    };
    assert!(history.opened_with_unread());
    history.end_unread_open();
    assert!(!history.opened_with_unread());
    assert_eq!(history.unread_anchor(), None);
}
