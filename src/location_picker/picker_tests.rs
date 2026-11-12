use super::super::geo::{GeoOutcome, GeoPoint};
use super::super::map_view::{MIN_ZOOM, MapSize, START_ZOOM, TileKey};
use super::super::venues::{Venue, VenueView};
use super::{AddressLine, HereState, LocationPicker};
use crate::ids::{ChatId, FileId, RequestId};

fn point(lat: f64, lon: f64) -> GeoPoint {
    GeoPoint::new(lat, lon).expect("finite")
}

fn venue(title: &str) -> Venue {
    Venue {
        point: point(1., 1.),
        title: title.into(),
        address: "Street".into(),
        provider: "foursquare".into(),
        id: format!("id-{title}"),
        venue_type: "food/cafe".into(),
    }
}

const SIZE: MapSize = MapSize {
    width: 364,
    height: 220,
    scale: 2,
};

/// The UI's step: take the lookup the picker wants and start it.
fn take_job(picker: &mut LocationPicker) -> Option<GeoPoint> {
    let point = picker.address_job()?;
    picker.address_started(point, crate::task_hold::TaskHold::new(()));
    Some(point)
}

fn fresh() -> LocationPicker {
    LocationPicker::open(ChatId(7), None, point(31., 35.))
}

#[test]
fn without_a_fix_it_starts_at_the_country_and_waits() {
    let mut picker = fresh();
    assert_eq!(picker.view().zoom(), MIN_ZOOM);
    assert_eq!(picker.venues(), VenueView::Loading);
    assert!(!picker.can_search());
    assert_eq!(picker.venue_query_to_send(), None);
    picker.geolocated(GeoOutcome::Denied, false);
    assert_eq!(picker.here(), HereState::Denied);
    assert_eq!(picker.venues(), VenueView::WaitingForLocation);
    assert_eq!(picker.address_line(), AddressLine::Hidden);
}

#[test]
fn my_location_after_a_refusal_is_locating_until_it_answers() {
    let mut picker = fresh();
    picker.geolocated(GeoOutcome::Denied, false);
    assert_eq!(picker.here(), HereState::Denied);
    picker.locate_started();
    assert_eq!(picker.here(), HereState::Locating);
    picker.geolocated(GeoOutcome::Unavailable, false);
    assert_eq!(picker.here(), HereState::Unavailable);
}

#[test]
fn a_previous_fix_starts_there_with_venues_and_address() {
    let here = point(40.7, -74.);
    let mut picker = LocationPicker::open(ChatId(7), Some(here), point(31., 35.));
    assert_eq!(picker.view().zoom(), START_ZOOM);
    assert!(picker.can_search());
    let (at, query) = picker.venue_query_to_send().expect("venues");
    assert!(at.same_place(&here));
    assert_eq!(query, "");
    assert_eq!(take_job(&mut picker), Some(here));
    picker.venue_query_requested(RequestId(1));
    assert_eq!(picker.venue_query_to_send(), None);
}

#[test]
fn a_new_fix_flies_there_and_looks_around() {
    let mut picker = fresh();
    let here = point(48.85, 2.35);
    picker.geolocated(GeoOutcome::Found(here), true);
    assert!(picker.view().center().same_place(&here));
    assert_eq!(picker.view().zoom(), START_ZOOM);
    assert!(!picker.is_moving());
    assert!(!picker.places_in_area_shown());
    assert!(picker.venue_query_to_send().is_some());
    assert_eq!(take_job(&mut picker), Some(here));
    picker.address_resolved(here, Some("Rue de Rivoli, Paris, France".into()));
    assert_eq!(
        picker.address_line(),
        AddressLine::Text("Rue de Rivoli, Paris, France")
    );
}

#[test]
fn moving_hides_the_address_and_returning_restores_it() {
    let mut picker = fresh();
    let here = point(48.85, 2.35);
    picker.geolocated(GeoOutcome::Found(here), true);
    let _ = take_job(&mut picker);
    picker.address_resolved(here, None);
    assert_eq!(
        picker.address_line(),
        AddressLine::Text("(48.850000, 2.350000)")
    );
    let start = picker.view();
    picker.moved_to(start.panned(40., 0.));
    assert_eq!(picker.address_line(), AddressLine::Loading);
    picker.moved_to(start);
    picker.move_ended();
    assert_eq!(
        picker.address_line(),
        AddressLine::Text("(48.850000, 2.350000)")
    );
    assert!(!picker.address_postponed());
    assert!(!picker.places_in_area_shown());
}

/// Sweep 03-086: an address kept for place A is not shown for place B when
/// the pin is moved again before B's address arrives.
#[test]
fn a_hidden_address_is_not_restored_for_another_place() {
    let mut picker = fresh();
    let a = point(48.85, 2.35);
    picker.geolocated(GeoOutcome::Found(a), true);
    let _ = take_job(&mut picker);
    picker.address_resolved(a, Some("A street".into()));
    let start = picker.view();
    // Drag to B and rest: B's lookup starts.
    let b_view = start.panned(400., 0.);
    picker.moved_to(b_view);
    picker.move_ended();
    picker.resolve_postponed();
    let b = take_job(&mut picker).expect("B is looked up");
    // Drag slightly again before it answers and release at B.
    picker.moved_to(b_view);
    picker.move_ended();
    assert_ne!(
        picker.address_line(),
        AddressLine::Text("A street"),
        "not A's address under B"
    );
    picker.address_resolved(b, Some("B street".into()));
    assert_eq!(picker.address_line(), AddressLine::Text("B street"));
}

/// An address that arrives while the pin is lifted is not drawn under it;
/// it comes back when the pin rests at the place it is for.
#[test]
fn an_address_answered_while_the_pin_is_lifted_waits_for_its_return() {
    let mut picker = fresh();
    let here = point(48.85, 2.35);
    picker.geolocated(GeoOutcome::Found(here), true);
    let _ = take_job(&mut picker);
    let start = picker.view();
    picker.moved_to(start.panned(40., 0.));
    picker.address_resolved(here, Some("Street".into()));
    assert_eq!(picker.address_line(), AddressLine::Loading);
    picker.moved_to(start);
    picker.move_ended();
    assert_eq!(picker.address_line(), AddressLine::Text("Street"));
}

#[test]
fn settling_elsewhere_offers_places_and_postpones_the_address() {
    let mut picker = fresh();
    picker.geolocated(GeoOutcome::Found(point(48.85, 2.35)), true);
    let _ = take_job(&mut picker);
    let moved = picker.view().panned(120., 60.);
    picker.moved_to(moved);
    picker.move_ended();
    assert!(picker.places_in_area_shown());
    assert!(picker.address_postponed());
    assert_eq!(take_job(&mut picker), None);
    picker.resolve_postponed();
    let job = take_job(&mut picker).expect("resolve after the pause");
    assert!(job.same_place(&moved.center()));
    picker.address_resolved(point(0., 0.), Some("Elsewhere".into()));
    assert_eq!(picker.address_line(), AddressLine::Hidden);
    picker.search_places_in_area();
    assert!(!picker.places_in_area_shown());
    let (at, _) = picker.venue_query_to_send().expect("venues here");
    assert!(at.same_place(&moved.center()));
}

#[test]
fn stale_venue_answers_are_dropped_and_results_cached() {
    let mut picker = fresh();
    let here = point(48.85, 2.35);
    picker.geolocated(GeoOutcome::Found(here), true);
    picker.venue_query_to_send().expect("first");
    let (first, second) = (RequestId(1), RequestId(2));
    picker.venue_query_requested(first);
    picker.moved_to(picker.view().panned(300., 0.));
    picker.move_ended();
    picker.search_places_in_area();
    picker.venue_query_to_send().expect("second");
    picker.venue_query_requested(second);
    picker.venues_answered(first, vec![venue("Old")]);
    assert_eq!(picker.venues(), VenueView::Loading);
    picker.venues_answered(second, vec![venue("New")]);
    assert_eq!(picker.venues(), VenueView::Found(&[venue("New")]));
    // An error of the older query, or of the answered one, changes nothing.
    picker.venues_failed(first);
    picker.venues_failed(second);
    assert_eq!(picker.venues(), VenueView::Found(&[venue("New")]));
}

/// Sweep r2-06b 03-C011: a failed lookup is drawn as a failure, and
/// Retry asks for the same place and text again.
#[test]
fn a_failed_lookup_is_a_failure_and_retry_asks_again() {
    let mut picker = fresh();
    picker.geolocated(GeoOutcome::Found(point(48.85, 2.35)), true);
    let first = RequestId(1);
    picker.venue_query_to_send().expect("nearby");
    picker.venue_query_requested(first);
    picker.venues_failed(first);
    assert_eq!(picker.venues(), VenueView::Failed);
    assert_eq!(picker.venue_query_to_send(), None, "not re-asked by itself");
    picker.retry_venues();
    assert_eq!(picker.venues(), VenueView::Loading);
    let (_, query) = picker.venue_query_to_send().expect("asked again");
    let again = RequestId(2);
    picker.venue_query_requested(again);
    assert_eq!(query, "");
    picker.venues_answered(first, vec![venue("Stale")]);
    assert_eq!(picker.venues(), VenueView::Loading, "the failed ask's id");
    picker.venues_answered(again, vec![venue("Near")]);
    assert_eq!(picker.venues(), VenueView::Found(&[venue("Near")]));
    picker.retry_venues();
    assert_eq!(picker.venue_query_to_send(), None, "nothing to retry");
}

#[test]
fn search_debounces_and_restores_the_nearby_list() {
    let mut picker = fresh();
    let here = point(48.85, 2.35);
    picker.geolocated(GeoOutcome::Found(here), true);
    picker.venue_query_to_send().expect("nearby");
    picker.venue_query_requested(RequestId(1));
    picker.venues_answered(RequestId(1), vec![venue("Near")]);
    picker.search_changed(Some("Pizza ".into()));
    assert!(picker.search_shown());
    assert!(picker.search_debounce_pending());
    assert_eq!(picker.venues(), VenueView::Loading);
    assert_eq!(picker.venue_query_to_send(), None);
    picker.search_debounce_fired();
    let (_, query) = picker.venue_query_to_send().expect("search");
    assert_eq!(query, "pizza");
    picker.venue_query_requested(RequestId(2));
    picker.venues_answered(RequestId(2), Vec::new());
    assert_eq!(picker.venues(), VenueView::NothingFound { query: "Pizza " });
    picker.search_changed(None);
    assert!(!picker.search_shown());
    assert_eq!(picker.venues(), VenueView::Found(&[venue("Near")]));
    assert_eq!(picker.venue_query_to_send(), None);
    picker.search_changed(Some("pizza".into()));
    assert!(!picker.search_debounce_pending());
}

/// Sweep 03-085: closing the search after a failed lookup asks again
/// instead of leaving the list on the spinner with nothing pending.
#[test]
fn closing_the_search_after_a_failed_lookup_asks_again() {
    let mut picker = fresh();
    picker.geolocated(GeoOutcome::Found(point(48.85, 2.35)), true);
    picker.venue_query_to_send().expect("nearby");
    picker.venue_query_requested(RequestId(1));
    picker.venues_failed(RequestId(1));
    picker.search_changed(Some("p".into()));
    assert_eq!(picker.venues(), VenueView::Loading);
    picker.search_changed(None);
    assert_eq!(picker.venues(), VenueView::Loading);
    let (_, query) = picker.venue_query_to_send().expect("asked again");
    assert_eq!(query, "");
    picker.venue_query_requested(RequestId(2));
    picker.venues_answered(RequestId(2), vec![venue("Near")]);
    assert_eq!(picker.venues(), VenueView::Found(&[venue("Near")]));
}

#[test]
fn tiles_follow_the_resting_view() {
    let mut picker = fresh();
    assert_eq!(picker.tile_to_request(), None);
    picker.set_map_size(SIZE);
    let key = picker.tile_to_request().expect("tile");
    assert_eq!(key, TileKey::for_view(&picker.view(), SIZE));
    picker.tile_requested(key, RequestId(11));
    assert_eq!(picker.tile_to_request(), None);
    assert!(picker.tile_answered(RequestId(11), Some(FileId(5))));
    assert!(!picker.tile_answered(RequestId(12), Some(FileId(6))));
    assert_eq!(picker.tiles(), vec![(key, FileId(5))]);
    picker.moved_to(picker.view().panned(100., 0.));
    assert_eq!(picker.tile_to_request(), None, "no tile while moving");
    picker.move_ended();
    let next = picker.tile_to_request().expect("next tile");
    assert_ne!(next, key);
    picker.tile_requested(next, RequestId(12));
    picker.tile_answered(RequestId(12), Some(FileId(6)));
    assert_eq!(picker.tiles().last(), Some(&(next, FileId(6))));
}

#[test]
fn tiles_are_bounded() {
    let mut picker = fresh();
    picker.set_map_size(SIZE);
    for n in 0..100u64 {
        picker.moved_to(picker.view().panned(400., 0.));
        picker.move_ended();
        let key = picker.tile_to_request().expect("tile");
        picker.tile_requested(key, RequestId(n));
        picker.tile_answered(RequestId(n), Some(FileId(n as i32)));
    }
    assert!(picker.tiles().len() <= 24);
}

/// Fails every ask for the current view's tile until the picker stops
/// asking, and returns the tile's key.
fn fail_the_tile_until_it_rests(picker: &mut LocationPicker) -> TileKey {
    let key = picker.tile_to_request().expect("tile");
    let mut asks = 0u64;
    while let Some(again) = picker.tile_to_request() {
        assert_eq!(again, key);
        asks += 1;
        assert!(asks <= 3, "a failing tile is not asked for without end");
        picker.tile_requested(key, RequestId(asks));
        assert_eq!(picker.tile_to_request(), None, "one ask at a time");
        assert!(picker.tile_answered(RequestId(asks), None));
    }
    assert_eq!(asks, 3, "a lost answer is asked again, a few times");
    key
}

#[test]
fn a_failed_tile_is_asked_a_few_times_then_again_once_the_map_rests() {
    let mut picker = fresh();
    picker.set_map_size(SIZE);
    let view = picker.view();
    let key = fail_the_tile_until_it_rests(&mut picker);
    // Away and back to the very same view: the same tile, asked again.
    picker.moved_to(view.panned(100., 0.));
    picker.moved_to(view);
    picker.move_ended();
    assert_eq!(picker.tile_to_request(), Some(key), "asked after a move");
    picker.tile_requested(key, RequestId(9));
    assert!(picker.tile_answered(RequestId(9), Some(FileId(5))));
    assert_eq!(picker.tile_to_request(), None);
    assert_eq!(picker.tiles(), vec![(key, FileId(5))]);
}

#[test]
fn a_failed_tile_is_asked_again_after_a_resize() {
    let mut picker = fresh();
    picker.set_map_size(SIZE);
    fail_the_tile_until_it_rests(&mut picker);
    picker.set_map_size(SIZE);
    assert_eq!(
        picker.tile_to_request(),
        None,
        "same size keeps the failure"
    );
    picker.set_map_size(MapSize {
        width: SIZE.width + 10,
        ..SIZE
    });
    assert!(picker.tile_to_request().is_some());
}

#[test]
fn a_tile_whose_ask_cannot_be_sent_is_not_asked_on_every_pass() {
    let mut picker = fresh();
    picker.set_map_size(SIZE);
    let key = picker.tile_to_request().expect("tile");
    for _ in 0..3 {
        assert_eq!(picker.tile_to_request(), Some(key));
        picker.tile_unsent(key);
    }
    assert_eq!(picker.tile_to_request(), None, "it waits for a move");
    // A refused send between two failed answers counts with them.
    let mut picker = fresh();
    picker.set_map_size(SIZE);
    picker.tile_requested(key, RequestId(1));
    assert!(picker.tile_answered(RequestId(1), None));
    picker.tile_unsent(key);
    picker.tile_requested(key, RequestId(2));
    assert!(picker.tile_answered(RequestId(2), None));
    assert_eq!(picker.tile_to_request(), None);
}

/// Sweep r3-02 (05-005): a venue query or a tile whose request the session
/// dropped fails like an error would, instead of waiting for good.
#[test]
fn requests_the_session_dropped_fail_the_list_and_the_tile() {
    use std::collections::HashSet;
    let mut picker = fresh();
    picker.set_map_size(SIZE);
    picker.geolocated(GeoOutcome::Found(point(48.85, 2.35)), true);
    picker.venue_query_to_send().expect("nearby");
    picker.venue_query_requested(RequestId(1));
    let key = picker.tile_to_request().expect("tile");
    picker.tile_requested(key, RequestId(2));
    // Both still pending: nothing changes.
    picker.forget_dropped(&HashSet::from([RequestId(1), RequestId(2)]));
    assert_eq!(picker.venues(), VenueView::Loading);
    assert_eq!(picker.tile_to_request(), None);
    // Gone from the table without an answer: the list says it failed and
    // the tile is asked for again.
    picker.forget_dropped(&HashSet::new());
    assert_eq!(picker.venues(), VenueView::Failed);
    assert_eq!(picker.tile_to_request(), Some(key));
    // Retry asks for the venues again.
    picker.retry_venues();
    assert!(picker.venue_query_to_send().is_some());
}

#[test]
fn typing_before_any_place_is_known_does_not_sit_on_loading() {
    let mut picker = fresh();
    assert_eq!(picker.venues(), VenueView::Loading, "still locating");
    picker.geolocated(GeoOutcome::Denied, true);
    assert!(!picker.can_search());
    picker.search_changed(Some("pizza".into()));
    assert!(!picker.search_debounce_pending(), "nothing to send");
    assert_eq!(picker.venues(), VenueView::WaitingForLocation);
}

/// Sweep r4-04 05-002: a query typed before any place is known is sent around
/// the first fix, and closing the search brings the nearby list back.
#[test]
fn a_query_typed_before_the_first_fix_is_sent_when_the_fix_comes() {
    let mut picker = fresh();
    picker.search_changed(Some("Pizza".into()));
    assert_eq!(picker.venue_query_to_send(), None, "nowhere to search yet");
    let here = point(48.85, 2.35);
    picker.geolocated(GeoOutcome::Found(here), true);
    let (at, query) = picker
        .venue_query_to_send()
        .expect("the typed query goes out");
    assert_eq!((at, query.as_str()), (here, "pizza"));
    assert_eq!(picker.venues(), VenueView::Loading);
    picker.venue_query_requested(RequestId(1));
    picker.venues_answered(RequestId(1), vec![venue("Pizzeria")]);
    assert_eq!(picker.venues(), VenueView::Found(&[venue("Pizzeria")]));
    picker.search_changed(None);
    let (_, nearby) = picker
        .venue_query_to_send()
        .expect("the nearby list is asked");
    assert_eq!(nearby, "");
}

/// A failed ask is drawn as a failure until another ask replaces it, even
/// when a cached answer is what replaces it.
#[test]
fn a_cached_answer_after_a_failure_replaces_the_failure() {
    let mut picker = fresh();
    let here = point(48.85, 2.35);
    picker.geolocated(GeoOutcome::Found(here), true);
    picker.venue_query_to_send().expect("nearby");
    picker.venue_query_requested(RequestId(1));
    picker.venues_answered(RequestId(1), vec![venue("Near")]);
    picker.search_changed(Some("pizza".into()));
    picker.search_debounce_fired();
    picker.venue_query_to_send().expect("search");
    picker.venue_query_requested(RequestId(2));
    picker.venues_failed(RequestId(2));
    assert_eq!(picker.venues(), VenueView::Failed);
    picker.search_changed(None);
    assert_eq!(
        picker.venues(),
        VenueView::Found(&[venue("Near")]),
        "the cached nearby list"
    );
    picker.search_changed(Some("pizza".into()));
    picker.retry_venues();
    assert_ne!(picker.venues(), VenueView::Failed);
}

#[test]
fn a_new_place_stops_the_lookup_of_the_old_one() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    struct Ended(Arc<AtomicBool>);
    impl Drop for Ended {
        fn drop(&mut self) {
            self.0.store(true, Ordering::Relaxed);
        }
    }
    let mut picker = fresh();
    let a = point(48.85, 2.35);
    picker.geolocated(GeoOutcome::Found(a), true);
    let ended = Arc::new(AtomicBool::new(false));
    let job = picker.address_job().expect("A is wanted");
    picker.address_started(job, crate::task_hold::TaskHold::new(Ended(ended.clone())));
    assert_eq!(picker.address_job(), None, "the lookup is out");
    assert!(!ended.load(Ordering::Relaxed));
    let b_view = picker.view().panned(400., 0.);
    picker.moved_to(b_view);
    picker.move_ended();
    assert!(
        ended.load(Ordering::Relaxed),
        "A's lookup ended with its stage"
    );
}

/// Sweep r4-04 fix 3: a pan while locating, then a Denied fix, leaves the list
/// waiting for a place, not spinning.
#[test]
fn a_pan_while_locating_then_a_refusal_does_not_spin() {
    let mut picker = fresh();
    picker.moved_to(crate::location_picker::MapView::new(point(10., 10.), 5));
    picker.move_ended();
    assert_eq!(picker.venues(), VenueView::Loading, "still locating");
    picker.geolocated(GeoOutcome::Denied, false);
    assert_eq!(picker.venues(), VenueView::WaitingForLocation);
}

/// Sweep r4-04 fix 3: a query typed with no place, a pan, then the fix: the
/// query is asked around the fix.
#[test]
fn a_query_typed_then_a_pan_then_the_fix_is_still_sent() {
    let mut picker = fresh();
    picker.search_changed(Some("pizza".into()));
    picker.moved_to(crate::location_picker::MapView::new(point(10., 10.), 5));
    picker.move_ended();
    let (_, query) = picker
        .venue_query_to_send()
        .expect("the pan gives the query a place");
    assert_eq!(query, "pizza");
    let here = point(48.85, 2.35);
    picker.geolocated(GeoOutcome::Found(here), true);
    let (at, query) = picker.venue_query_to_send().expect("still asking");
    assert_eq!(query, "pizza");
    assert!(at.same_place(&here) || at.same_place(&point(10., 10.)));
}

/// Sweep r4-04 fix 3: refused, a query typed, then a pan: the query is sent.
#[test]
fn a_refusal_then_a_query_then_a_pan_sends_the_query() {
    let mut picker = fresh();
    picker.geolocated(GeoOutcome::Denied, false);
    picker.search_changed(Some("pizza".into()));
    assert_eq!(picker.venue_query_to_send(), None);
    let there = point(10., 10.);
    picker.moved_to(crate::location_picker::MapView::new(there, 5));
    picker.move_ended();
    let (at, query) = picker.venue_query_to_send().expect("sent after the pan");
    assert_eq!((at, query.as_str()), (there, "pizza"));
    assert!(!picker.places_in_area_shown());
}
