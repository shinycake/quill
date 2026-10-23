use super::{GeoOutcome, GeoPoint, LocationSharing, PickerRequest, Venue, VenueList};
use crate::ids::{ChatId, FileId};

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
    assert!(sharing.next_requests().is_empty(), "resolving once");
    sharing.venue_bot_resolved(Some(99));
    let requests = sharing.next_requests();
    let Some(PickerRequest::Venues {
        generation,
        bot_user_id,
        chat_id,
        query,
        ..
    }) = requests.first().cloned()
    else {
        panic!("venue query: {requests:?}");
    };
    assert_eq!((bot_user_id, chat_id, query.as_str()), (99, ChatId(3), ""));
    sharing.venues_answered(generation, vec![cafe()]);
    let icons = sharing.next_requests();
    assert_eq!(
        icons,
        vec![PickerRequest::VenueIcon {
            venue_type: "food/cafe".into(),
            url: "https://ss3.4sqi.net/img/categories_v2/food/cafe_64.png".into(),
        }]
    );
    sharing.request_sent(&icons[0], 41);
    sharing.file_answered(41, Some(FileId(8)));
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
        sharing.picker().map(|p| p.venues().clone()),
        Some(VenueList::NothingFound {
            query: String::new()
        })
    );
    sharing.set_venue_bot_username("bot".into());
    let _ = sharing.next_requests();
    sharing.venue_bot_resolved(None);
    assert!(sharing.next_requests().is_empty());
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
    assert!(matches!(
        sharing.picker().map(|p| p.venues().clone()),
        Some(VenueList::NothingFound { .. })
    ));
    if let Some(picker) = sharing.picker_mut() {
        picker.set_map_size(super::MapSize::new(364., 220., 2.));
    }
    let tile = sharing.next_requests();
    assert!(matches!(tile.as_slice(), [PickerRequest::MapTile { .. }]));
    sharing.request_unsent(&tile[0]);
    assert_eq!(sharing.next_requests(), tile, "asked again");
}
