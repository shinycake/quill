use super::super::geo::{GeoOutcome, GeoPoint};
use super::super::map_view::{MIN_ZOOM, MapSize, START_ZOOM, TileKey};
use super::super::venues::{Venue, VenueList};
use super::{AddressLine, HereState, LocationPicker};
use crate::ids::{ChatId, FileId};

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

fn fresh() -> LocationPicker {
    LocationPicker::open(ChatId(7), None, point(31., 35.))
}

#[test]
fn without_a_fix_it_starts_at_the_country_and_waits() {
    let mut picker = fresh();
    assert_eq!(picker.view().zoom(), MIN_ZOOM);
    assert_eq!(picker.venues(), &VenueList::Loading);
    assert!(!picker.can_search());
    assert_eq!(picker.venue_query_to_send(), None);
    picker.geolocated(GeoOutcome::Denied, false);
    assert_eq!(picker.here(), HereState::Denied);
    assert_eq!(picker.venues(), &VenueList::WaitingForLocation);
    assert_eq!(picker.address_line(), AddressLine::Hidden);
}

#[test]
fn a_previous_fix_starts_there_with_venues_and_address() {
    let here = point(40.7, -74.);
    let mut picker = LocationPicker::open(ChatId(7), Some(here), point(31., 35.));
    assert_eq!(picker.view().zoom(), START_ZOOM);
    assert!(picker.can_search());
    let (generation, at, query) = picker.venue_query_to_send().expect("venues");
    assert!(at.same_place(&here));
    assert_eq!(query, "");
    assert_eq!(picker.take_address_job(), Some(here));
    picker.venue_query_sent(generation);
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
    assert_eq!(picker.take_address_job(), Some(here));
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
    let _ = picker.take_address_job();
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

#[test]
fn settling_elsewhere_offers_places_and_postpones_the_address() {
    let mut picker = fresh();
    picker.geolocated(GeoOutcome::Found(point(48.85, 2.35)), true);
    let _ = picker.take_address_job();
    let moved = picker.view().panned(120., 60.);
    picker.moved_to(moved);
    picker.move_ended();
    assert!(picker.places_in_area_shown());
    assert!(picker.address_postponed());
    assert_eq!(picker.take_address_job(), None);
    picker.resolve_postponed();
    let job = picker.take_address_job().expect("resolve after the pause");
    assert!(job.same_place(&moved.center()));
    // The answer for an older place is ignored.
    picker.address_resolved(point(0., 0.), Some("Elsewhere".into()));
    assert_eq!(picker.address_line(), AddressLine::Hidden);
    picker.search_places_in_area();
    assert!(!picker.places_in_area_shown());
    let (_, at, _) = picker.venue_query_to_send().expect("venues here");
    assert!(at.same_place(&moved.center()));
}

#[test]
fn stale_venue_answers_are_dropped_and_results_cached() {
    let mut picker = fresh();
    let here = point(48.85, 2.35);
    picker.geolocated(GeoOutcome::Found(here), true);
    let (first, ..) = picker.venue_query_to_send().expect("first");
    picker.venue_query_sent(first);
    picker.moved_to(picker.view().panned(300., 0.));
    picker.move_ended();
    picker.search_places_in_area();
    let (second, ..) = picker.venue_query_to_send().expect("second");
    picker.venues_answered(first, vec![venue("Old")]);
    assert_eq!(picker.venues(), &VenueList::Loading);
    picker.venues_answered(second, vec![venue("New")]);
    assert_eq!(picker.venues(), &VenueList::Found(vec![venue("New")]));
    picker.venues_failed(second);
    assert_eq!(
        picker.venues(),
        &VenueList::NothingFound {
            query: String::new()
        }
    );
}

#[test]
fn search_debounces_and_restores_the_nearby_list() {
    let mut picker = fresh();
    let here = point(48.85, 2.35);
    picker.geolocated(GeoOutcome::Found(here), true);
    let (nearby, ..) = picker.venue_query_to_send().expect("nearby");
    picker.venue_query_sent(nearby);
    picker.venues_answered(nearby, vec![venue("Near")]);
    picker.search_changed(Some("Pizza ".into()));
    assert!(picker.search_shown());
    assert!(picker.search_debounce_pending());
    assert_eq!(picker.venues(), &VenueList::Loading);
    assert_eq!(picker.venue_query_to_send(), None);
    picker.search_debounce_fired();
    let (search, _, query) = picker.venue_query_to_send().expect("search");
    assert_eq!(query, "pizza");
    picker.venues_answered(search, Vec::new());
    assert_eq!(
        picker.venues(),
        &VenueList::NothingFound {
            query: "Pizza ".into()
        }
    );
    picker.search_changed(None);
    assert!(!picker.search_shown());
    assert_eq!(picker.venues(), &VenueList::Found(vec![venue("Near")]));
    assert_eq!(picker.venue_query_to_send(), None);
    // The same search again comes from the cache, without a timer.
    picker.search_changed(Some("pizza".into()));
    assert!(!picker.search_debounce_pending());
}

#[test]
fn tiles_follow_the_resting_view() {
    let mut picker = fresh();
    assert_eq!(picker.tile_to_request(), None);
    picker.set_map_size(SIZE);
    let key = picker.tile_to_request().expect("tile");
    assert_eq!(key, TileKey::for_view(&picker.view(), SIZE));
    picker.tile_requested(key, 11);
    assert_eq!(picker.tile_to_request(), None);
    assert!(picker.tile_answered(11, Some(FileId(5))));
    assert!(!picker.tile_answered(12, Some(FileId(6))));
    assert_eq!(picker.tiles(), vec![(key, FileId(5))]);
    picker.moved_to(picker.view().panned(100., 0.));
    assert_eq!(picker.tile_to_request(), None, "no tile while moving");
    picker.move_ended();
    let next = picker.tile_to_request().expect("next tile");
    assert_ne!(next, key);
    picker.tile_requested(next, 12);
    picker.tile_answered(12, Some(FileId(6)));
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
        picker.tile_requested(key, n);
        picker.tile_answered(n, Some(FileId(n as i32)));
    }
    assert!(picker.tiles().len() <= 24);
}
