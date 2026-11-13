use super::{GeoOutcome, GeoPoint, LocationSharing, PickerRequest, Venue, VenueView};
use crate::ids::{ChatId, FileId, RequestId};

fn point(lat: f64, lon: f64) -> GeoPoint {
    GeoPoint::new(lat, lon).expect("finite")
}

fn cafe() -> Venue {
    Venue {
        point: point(1., 1.),
        title: "Cafe".into(),
        address: "Street".into(),
        provider: "foursquare".into(),
        id: "v1".into(),
        venue_type: "food/cafe".into(),
    }
}

/// The answer of a lookup that is not the one waited for (an older option
/// value, another request) changes nothing.
#[test]
fn a_venue_bot_answer_of_another_request_is_dropped() {
    let resolve = || PickerRequest::ResolveVenueBot {
        username: "foursquare".into(),
    };
    let mut sharing = LocationSharing::default();
    sharing.set_venue_bot_username("foursquare".into());
    sharing.open(ChatId(3), None);
    sharing.geolocated(GeoOutcome::Found(point(10., 20.)));
    assert_eq!(sharing.next_requests(), vec![resolve()]);
    sharing.request_sent(&resolve(), RequestId(7));
    sharing.venue_bot_resolved(RequestId(8), Some(99));
    sharing.venue_bot_failed(RequestId(8));
    assert!(sharing.next_requests().is_empty(), "still waiting for 7");
    sharing.venue_bot_resolved(RequestId(7), Some(99));
    assert!(matches!(
        sharing.next_requests().first(),
        Some(PickerRequest::Venues {
            bot_user_id: 99,
            ..
        })
    ));
}

#[test]
fn venues_resolve_the_bot_first_then_ask_it() {
    let mut sharing = LocationSharing::default();
    sharing.set_venue_bot_username("foursquare".into());
    sharing.open(ChatId(3), None);
    sharing.geolocated(GeoOutcome::Found(point(10., 20.)));
    let requests = sharing.next_requests();
    assert_eq!(
        requests,
        vec![PickerRequest::ResolveVenueBot {
            username: "foursquare".into()
        }]
    );
    sharing.request_sent(&requests[0], RequestId(7));
    assert!(sharing.next_requests().is_empty(), "resolving once");
    sharing.venue_bot_resolved(RequestId(7), Some(99));
    let requests = sharing.next_requests();
    let Some(PickerRequest::Venues {
        bot_user_id,
        chat_id,
        query,
        ..
    }) = requests.first().cloned()
    else {
        panic!("venue query: {requests:?}");
    };
    assert_eq!((bot_user_id, chat_id, query.as_str()), (99, ChatId(3), ""));
    sharing.request_sent(&requests[0], RequestId(9));
    sharing.venues_answered(RequestId(9), vec![cafe()]);
    let icons = sharing.next_requests();
    assert_eq!(
        icons,
        vec![PickerRequest::VenueIcon {
            venue_type: "food/cafe".into(),
            url: "https://ss3.4sqi.net/img/categories_v2/food/cafe_64.png".into(),
        }]
    );
    sharing.request_sent(&icons[0], RequestId(41));
    sharing.file_answered(RequestId(41), Some(FileId(8)));
    assert_eq!(sharing.icon_file("food/cafe"), Some(FileId(8)));
    assert_eq!(sharing.files_to_download(), vec![FileId(8)]);
}

#[test]
fn no_bot_means_nothing_found_not_a_spinner() {
    let mut sharing = LocationSharing::default();
    sharing.open(ChatId(3), None);
    sharing.geolocated(GeoOutcome::Found(point(10., 20.)));
    assert!(sharing.next_requests().is_empty());
    assert_eq!(
        sharing.picker().map(|p| p.venues()),
        Some(VenueView::NothingFound { query: "" })
    );
    sharing.set_venue_bot_username("bot".into());
    assert!(sharing.next_requests().is_empty());
}

/// Round 8, 2j: a failed lookup is not a missing bot. This picker shows
/// no venues; the next one asks for the bot again.
#[test]
fn a_failed_bot_lookup_is_asked_again_by_the_next_picker() {
    let resolve = || PickerRequest::ResolveVenueBot {
        username: "foursquare".into(),
    };
    let mut sharing = LocationSharing::default();
    sharing.set_venue_bot_username("foursquare".into());
    sharing.open(ChatId(3), None);
    sharing.geolocated(GeoOutcome::Found(point(10., 20.)));
    assert_eq!(sharing.next_requests(), vec![resolve()]);
    sharing.request_sent(&resolve(), RequestId(7));
    sharing.venue_bot_failed(RequestId(7));
    assert!(
        sharing.next_requests().is_empty(),
        "not again in this picker"
    );
    assert_eq!(
        sharing.picker().map(|p| p.venues()),
        Some(VenueView::Failed)
    );
    sharing.retry_venues();
    assert_eq!(
        sharing.next_requests(),
        vec![resolve()],
        "Retry looks again"
    );
    sharing.close();
    sharing.open(ChatId(3), None);
    sharing.geolocated(GeoOutcome::Found(point(10., 21.)));
    assert_eq!(sharing.next_requests(), vec![resolve()], "the next picker");
}

#[test]
fn the_last_fix_seeds_the_next_picker() {
    let mut sharing = LocationSharing::default();
    sharing.open(ChatId(3), Some(point(31., 35.)));
    sharing.geolocated(GeoOutcome::Found(point(10., 20.)));
    sharing.close();
    assert!(sharing.picker().is_none());
    sharing.open(ChatId(4), Some(point(31., 35.)));
    let picker = sharing.picker().expect("open");
    assert!(picker.view().center().same_place(&point(10., 20.)));
    assert!(picker.can_search());
}

#[test]
fn unsent_requests_roll_back() {
    let mut sharing = LocationSharing::default();
    sharing.set_venue_bot_username("bot".into());
    sharing.open(ChatId(3), None);
    sharing.geolocated(GeoOutcome::Found(point(10., 20.)));
    let resolve = sharing.next_requests();
    sharing.request_unsent(&resolve[0]);
    assert_eq!(
        sharing.picker().map(|p| p.venues()),
        Some(VenueView::Failed)
    );
    if let Some(picker) = sharing.picker_mut() {
        picker.set_map_size(super::MapSize::new(364., 220., 2.));
    }
    let tile = sharing.next_requests();
    assert!(matches!(tile.as_slice(), [PickerRequest::MapTile { .. }]));
    sharing.request_unsent(&tile[0]);
    assert_eq!(sharing.next_requests(), tile, "asked again");
}

/// Sweep 03-073: a request id is never reused by a later picker, so a
/// late answer of a closed picker cannot land in the next one.
#[test]
fn a_closed_pickers_late_answer_is_dropped_by_the_next_picker() {
    let mut sharing = LocationSharing::default();
    sharing.set_venue_bot_username("foursquare".into());
    sharing.open(ChatId(3), None);
    sharing.geolocated(GeoOutcome::Found(point(10., 20.)));
    let resolve = sharing.next_requests();
    sharing.request_sent(&resolve[0], RequestId(7));
    sharing.venue_bot_resolved(RequestId(7), Some(99));
    let venues = |requests: Vec<PickerRequest>| {
        requests
            .into_iter()
            .find(|request| matches!(request, PickerRequest::Venues { .. }))
    };
    let old = venues(sharing.next_requests()).expect("old query");
    sharing.request_sent(&old, RequestId(20));
    sharing.close();
    sharing.open(ChatId(4), None);
    sharing.geolocated(GeoOutcome::Found(point(50., 60.)));
    let new = venues(sharing.next_requests()).expect("new query");
    sharing.request_sent(&new, RequestId(21));
    sharing.venues_answered(RequestId(20), vec![cafe()]);
    assert_eq!(
        sharing.picker().map(|p| p.venues()),
        Some(VenueView::Loading),
        "the old picker's venues are not shown"
    );
    sharing.venues_answered(RequestId(21), vec![cafe()]);
    assert!(matches!(
        sharing.picker().map(|p| p.venues()),
        Some(VenueView::Found(_))
    ));
}

/// Sweep 03-074: a bot lookup that never answered, or answered before the
/// bot was cached, does not decide every later picker.
#[test]
fn a_new_picker_asks_again_after_a_lost_or_missing_bot_lookup() {
    let resolve = || PickerRequest::ResolveVenueBot {
        username: "foursquare".into(),
    };
    for missing in [false, true] {
        let mut sharing = LocationSharing::default();
        sharing.set_venue_bot_username("foursquare".into());
        sharing.open(ChatId(3), None);
        sharing.geolocated(GeoOutcome::Found(point(10., 20.)));
        assert_eq!(sharing.next_requests(), vec![resolve()]);
        sharing.request_sent(&resolve(), RequestId(7));
        if missing {
            sharing.venue_bot_resolved(RequestId(7), None);
        }
        sharing.close();
        sharing.open(ChatId(3), None);
        sharing.geolocated(GeoOutcome::Found(point(10., 21.)));
        assert_eq!(
            sharing.next_requests(),
            vec![resolve()],
            "missing={missing}"
        );
    }
}

/// Sweep 03-075: a venue icon that failed, or whose answer never came, is
/// asked again by the next picker.
#[test]
fn a_new_picker_asks_again_for_icons_that_failed_or_never_answered() {
    let mut sharing = LocationSharing::default();
    sharing.set_venue_bot_username("foursquare".into());
    sharing.open(ChatId(3), None);
    sharing.geolocated(GeoOutcome::Found(point(10., 20.)));
    let resolve = sharing.next_requests();
    sharing.request_sent(&resolve[0], RequestId(7));
    sharing.venue_bot_resolved(RequestId(7), Some(99));
    let query = sharing
        .next_requests()
        .into_iter()
        .find(|request| matches!(request, PickerRequest::Venues { .. }))
        .expect("venue query");
    sharing.request_sent(&query, RequestId(30));
    sharing.venues_answered(RequestId(30), vec![cafe()]);
    let icons = sharing.next_requests();
    sharing.request_sent(&icons[0], RequestId(41));
    sharing.request_unsent(&icons[0]);
    assert!(sharing.next_requests().is_empty(), "failed stays failed");

    sharing.open(ChatId(3), None);
    sharing.geolocated(GeoOutcome::Found(point(10., 20.)));
    let query = sharing
        .next_requests()
        .into_iter()
        .find(|request| matches!(request, PickerRequest::Venues { .. }))
        .expect("venue query again");
    sharing.request_sent(&query, RequestId(31));
    sharing.venues_answered(RequestId(31), vec![cafe()]);
    assert!(
        sharing
            .next_requests()
            .iter()
            .any(|request| matches!(request, PickerRequest::VenueIcon { .. })),
        "the icon is asked again"
    );
}

/// An answer to a lookup that a new picker dropped changes nothing, and an
/// answer for another request does not settle the live one.
#[test]
fn a_bot_answer_is_matched_to_the_lookup_that_asked() {
    let resolve = || PickerRequest::ResolveVenueBot {
        username: "foursquare".into(),
    };
    let mut sharing = LocationSharing::default();
    sharing.set_venue_bot_username("foursquare".into());
    sharing.open(ChatId(3), None);
    sharing.geolocated(GeoOutcome::Found(point(10., 20.)));
    assert_eq!(sharing.next_requests(), vec![resolve()]);
    sharing.request_sent(&resolve(), RequestId(7));
    sharing.close();
    sharing.open(ChatId(3), None);
    sharing.geolocated(GeoOutcome::Found(point(10., 21.)));
    assert_eq!(sharing.next_requests(), vec![resolve()]);
    sharing.request_sent(&resolve(), RequestId(8));
    // The first picker's lookup answers late: not the live lookup.
    sharing.venue_bot_resolved(RequestId(7), Some(99));
    sharing.venue_bot_failed(RequestId(7));
    assert!(sharing.next_requests().is_empty(), "still resolving");
    sharing.venue_bot_resolved(RequestId(8), Some(99));
    assert!(matches!(
        sharing.next_requests().as_slice(),
        [PickerRequest::Venues {
            bot_user_id: 99,
            ..
        }]
    ));
}

/// Sweep r3-02 (05-005): requests the session dropped without an answer
/// (a reconnect, a client restart) fail the bot lookup, the venue query,
/// the tile and the icon instead of leaving the picker waiting.
#[test]
fn dropped_requests_fail_instead_of_waiting_for_good() {
    use std::collections::HashSet;
    let mut sharing = LocationSharing::default();
    sharing.set_venue_bot_username("foursquare".into());
    sharing.open(ChatId(3), None);
    sharing.geolocated(GeoOutcome::Found(point(10., 20.)));
    let resolve = sharing.next_requests();
    sharing.request_sent(&resolve[0], RequestId(7));
    // Still pending: still resolving.
    sharing.forget_dropped(&HashSet::from([RequestId(7)]));
    assert!(sharing.next_requests().is_empty());
    // The lookup was dropped: the list fails (Retry looks the bot up again).
    sharing.forget_dropped(&HashSet::new());
    assert_eq!(
        sharing.picker().map(|p| p.venues()),
        Some(VenueView::Failed)
    );
    sharing.retry_venues();
    let resolve = sharing.next_requests();
    assert!(matches!(
        resolve.as_slice(),
        [PickerRequest::ResolveVenueBot { .. }]
    ));
    sharing.request_sent(&resolve[0], RequestId(8));
    sharing.venue_bot_resolved(RequestId(8), Some(99));
    let query = sharing.next_requests().remove(0);
    sharing.request_sent(&query, RequestId(9));
    sharing.forget_dropped(&HashSet::from([RequestId(9)]));
    assert_eq!(
        sharing.picker().map(|p| p.venues()),
        Some(VenueView::Loading)
    );
    sharing.forget_dropped(&HashSet::new());
    assert_eq!(
        sharing.picker().map(|p| p.venues()),
        Some(VenueView::Failed)
    );
    // A dropped icon request is asked again by the next picker.
    sharing.retry_venues();
    let query = sharing.next_requests().remove(0);
    sharing.request_sent(&query, RequestId(10));
    sharing.venues_answered(RequestId(10), vec![cafe()]);
    let icons = sharing.next_requests();
    sharing.request_sent(&icons[0], RequestId(11));
    sharing.forget_dropped(&HashSet::new());
    assert!(
        sharing
            .next_requests()
            .iter()
            .any(|request| matches!(request, PickerRequest::VenueIcon { .. })),
        "the icon is asked for again"
    );
}
