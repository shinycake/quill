//! The GIF panel's two requests each move through one stage: the saved list
//! and the search never read as loading and failed at once, and a search
//! that was left takes its failure with it.
use crate::state::{GifList, GifPanel};

#[test]
fn a_new_search_starts_clean_after_a_failed_one() {
    let mut panel = GifPanel::default();
    panel.search_begun();
    panel.search_failed_now();
    assert!(panel.search_failed() && !panel.search_loading());
    // Back to the saved GIFs: the failure goes with the search.
    panel.search_left();
    assert_eq!(panel.list, GifList::Saved);
    assert!(!panel.search_failed());
    // The next search is loading, not still failed.
    panel.search_begun();
    assert!(panel.search_loading() && !panel.search_failed());
    panel.search_answered();
    assert!(!panel.search_loading() && !panel.search_failed());
}

#[test]
fn a_failed_search_asks_again_loading_not_failed() {
    let mut panel = GifPanel::default();
    panel.search_begun();
    panel.search_failed_now();
    panel.search_asked();
    assert!(panel.search_loading() && !panel.search_failed());
}

#[test]
fn an_answer_to_a_left_search_changes_nothing() {
    let mut panel = GifPanel::default();
    panel.search_answered();
    panel.search_failed_now();
    assert_eq!(panel.list, GifList::Saved);
}

#[test]
fn the_saved_list_is_loaded_after_an_answer_or_a_failure_and_asked_again_after_a_retry() {
    let mut panel = GifPanel::default();
    assert!(!panel.loaded());
    panel.saved_asked();
    assert!(panel.loading() && !panel.loaded());
    panel.saved_answered(Vec::new());
    assert!(panel.loaded() && !panel.loading() && !panel.failed());
    // A refresh runs over the answer: still loaded, and loading.
    panel.saved_asked();
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
    panel.saved_answered(Vec::new());
    panel.saved_mutation_failed();
    assert!(panel.loaded() && panel.failed());
    // The next request starts clean.
    panel.saved_failure_cleared();
    assert!(panel.loaded() && !panel.failed());
}

#[test]
fn clearing_a_failure_leaves_a_list_that_did_not_fail_alone() {
    let mut panel = GifPanel::default();
    panel.saved_answered(Vec::new());
    panel.saved_failure_cleared();
    assert!(panel.loaded() && !panel.failed());
    let mut idle = GifPanel::default();
    idle.saved_failure_cleared();
    assert!(!idle.loaded());
    idle.saved_asked();
    idle.saved_failure_cleared();
    assert!(idle.loading());
}
