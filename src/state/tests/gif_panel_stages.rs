//! The GIF panel's two requests each move through one stage: the saved list
//! and the search never read as loading and failed at once, and a search
//! that was left takes its failure with it.
use crate::ids::RequestId;
use crate::state::{GifList, GifPanel};

/// The saved list asked for and answered, as the driver would.
fn seed_saved(panel: &mut GifPanel) {
    panel.saved_asked(RequestId(900));
    panel.saved_answered(Some(RequestId(900)), Vec::new());
}

#[test]
fn a_new_search_starts_clean_after_a_failed_one() {
    let mut panel = GifPanel::default();
    panel.search_begun("cats".into());
    panel.search_failed_now();
    assert!(panel.search_failed() && !panel.search_loading());
    // Back to the saved GIFs: the failure goes with the search.
    panel.search_left();
    assert_eq!(panel.list, GifList::Saved);
    assert!(!panel.search_failed());
    // The next search is loading, not still failed.
    panel.search_begun("cats".into());
    assert!(panel.search_loading() && !panel.search_failed());
    panel.search_sent(RequestId(1));
    panel.search_answered(Some(RequestId(1)));
    assert!(!panel.search_loading() && !panel.search_failed());
}

#[test]
fn a_late_page_of_an_earlier_search_does_not_answer_the_new_one() {
    let mut panel = GifPanel::default();
    panel.search_begun("cats".into());
    panel.search_sent(RequestId(1));
    // The user searches again before the first page lands.
    panel.search_begun("dogs".into());
    panel.search_sent(RequestId(2));
    assert!(!panel.search_waits_for(Some(RequestId(1))));
    assert!(panel.search_waits_for(Some(RequestId(2))));
    // The old request failing does not fail the new search either.
    panel.search_request_failed(Some(RequestId(1)));
    assert!(panel.search_loading() && !panel.search_failed());
    panel.search_request_failed(Some(RequestId(2)));
    assert!(panel.search_failed());
    assert_eq!(panel.search_query(), "dogs");
}

#[test]
fn a_failed_save_while_the_list_loads_keeps_the_fetch() {
    let mut panel = GifPanel::default();
    panel.saved_asked(RequestId(1));
    panel.saved_mutation_failed();
    assert!(panel.loading() && !panel.failed());
}

#[test]
fn a_failed_search_asks_again_loading_not_failed() {
    let mut panel = GifPanel::default();
    panel.search_begun("cats".into());
    panel.search_failed_now();
    panel.search_asked();
    assert!(panel.search_loading() && !panel.search_failed());
}

#[test]
fn an_answer_to_a_left_search_changes_nothing() {
    let mut panel = GifPanel::default();
    assert!(!panel.search_answered(Some(RequestId(1))));
    panel.search_failed_now();
    assert_eq!(panel.list, GifList::Saved);
}

#[test]
fn the_saved_list_is_loaded_after_an_answer_or_a_failure_and_asked_again_after_a_retry() {
    let mut panel = GifPanel::default();
    assert!(!panel.loaded());
    panel.saved_asked(RequestId(1));
    assert!(panel.loading() && !panel.loaded());
    panel.saved_answered(Some(RequestId(1)), Vec::new());
    assert!(panel.loaded() && !panel.loading() && !panel.failed());
    // A refresh runs over the answer: still loaded, and loading.
    panel.saved_asked(RequestId(2));
    assert!(panel.loaded() && panel.loading());
    panel.saved_failed();
    assert!(panel.loaded() && panel.failed() && !panel.loading());
    // Opening the panel again after a failure asks from scratch.
    panel.saved_retry();
    assert!(!panel.loaded() && !panel.failed());
}

#[test]
fn a_failed_save_leaves_the_list_in_place() {
    let mut panel = GifPanel::default();
    seed_saved(&mut panel);
    panel.saved_mutation_failed();
    assert!(panel.loaded() && panel.failed());
    // The next request starts clean.
    panel.saved_failure_cleared();
    assert!(panel.loaded() && !panel.failed());
}

#[test]
fn clearing_a_failure_leaves_a_list_that_did_not_fail_alone() {
    let mut panel = GifPanel::default();
    seed_saved(&mut panel);
    panel.saved_failure_cleared();
    assert!(panel.loaded() && !panel.failed());
    let mut idle = GifPanel::default();
    idle.saved_failure_cleared();
    assert!(!idle.loaded());
    idle.saved_asked(RequestId(1));
    idle.saved_failure_cleared();
    assert!(idle.loading());
}

#[test]
fn a_new_search_replaces_what_the_last_one_found() {
    // Sweep r2-19 (06-032): results and offsets are the search's own.
    let mut panel = GifPanel::default();
    panel.search_begun("cats".into());
    panel.search_sent(RequestId(1));
    panel.search_page_asked(String::new());
    panel.search_sent(RequestId(2));
    panel.search_page_answered(Some(RequestId(2)), Vec::new(), "50".into(), true);
    assert_eq!(panel.search_next_offset(), "50");
    panel.search_begun("dogs".into());
    assert!(panel.search_results().is_empty());
    assert_eq!(panel.search_next_offset(), "");
    panel.search_left();
    assert_eq!(panel.search_next_offset(), "");
}

#[test]
fn a_saved_list_marked_out_of_date_drops_the_older_answer_and_is_asked_again() {
    // Sweep r2-19 (06-028): the mark is the fetch's own `Outdated`.
    let mut panel = GifPanel::default();
    seed_saved(&mut panel);
    assert!(panel.loaded() && !panel.saved_is_outdated());
    panel.saved_asked(RequestId(1));
    panel.saved_outdated();
    panel.saved_answered(Some(RequestId(1)), Vec::new());
    assert!(
        !panel.loaded() || panel.saved_is_outdated(),
        "the answer predates the change"
    );
    panel.saved_failed();
    assert!(
        !panel.loaded() || panel.saved_is_outdated(),
        "a failure keeps the mark"
    );
    panel.saved_asked(RequestId(2));
    assert!(panel.loaded() && !panel.saved_is_outdated());
}

/// A stray answer (no request waits for it) changes nothing.
#[test]
fn a_saved_answer_nobody_waits_for_is_dropped() {
    let mut panel = GifPanel::default();
    panel.saved_asked(RequestId(1));
    panel.saved_asked(RequestId(2));
    panel.saved_answered(Some(RequestId(1)), Vec::new());
    assert!(panel.loading() && !panel.loaded(), "the older ask is dropped");
    panel.saved_answered(Some(RequestId(2)), Vec::new());
    assert!(panel.loaded() && !panel.loading());
}
