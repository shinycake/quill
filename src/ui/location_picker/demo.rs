//! `ready-location-picker`: Send Location over the ready chat list
//! (injected, no live Telegram). Every map image is the fixture
//! `docs/screenshots/fixtures/demo-map-tile.png`; the venue bot answers with
//! a fixed list around the pin, filtered by the search text.
//! `QUILL_DEMO_LOCATION` picks what the platform says: `found` (default),
//! `denied`, `unavailable` or `locating` (no answer yet).
//! `QUILL_DEMO_LOCATION_SEARCH` opens the search with that text.
#![deny(clippy::indexing_slicing)]

use super::super::app::QuillApp;
use super::super::demo::{demo_file_json, demo_media_allowlist};
use super::super::screenshot_demo::{DemoSpec, register_demos};
use gpui_kit::*;
use quill::ids::FileId;
use quill::location_picker::{GeoOutcome, GeoPoint, PickerRequest, Venue};
use quill::telegram::client::OwnedEnvelope;
use quill::telegram::envelope::parse_envelope;
use std::sync::atomic::{AtomicU64, Ordering};

register_demos![
    DemoSpec::chats(
        "ready-location-picker",
        "screenshot demo — Send Location's map picker (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_location_picker),
];

const DEMO_BOT: i64 = 7_000_001;
const DEMO_CHAT: quill::ids::ChatId = quill::ids::ChatId(11);
const FIRST_TILE_FILE: i32 = 7_400_000;
static NEXT_EXTRA: AtomicU64 = AtomicU64::new(1 << 40);

fn demo_here() -> Option<GeoPoint> {
    GeoPoint::new(48.860_611, 2.337_644)
}

/// What the demo's platform answers to a bot's location request; `None`
/// never answers (`QUILL_DEMO_LOCATION=locating`).
pub(super) fn locate_outcome() -> Option<GeoOutcome> {
    demo_outcome()
}

fn demo_outcome() -> Option<GeoOutcome> {
    match std::env::var("QUILL_DEMO_LOCATION").as_deref() {
        Ok("denied") => Some(GeoOutcome::Denied),
        Ok("unavailable") => Some(GeoOutcome::Unavailable),
        Ok("locating") => None,
        _ => demo_here().map(GeoOutcome::Found),
    }
}

fn demo_venues(at: GeoPoint, query: &str) -> Vec<Venue> {
    const PLACES: [(&str, &str, &str, f64, f64); 6] = [
        (
            "Café Marly",
            "93 Rue de Rivoli",
            "food/cafe",
            0.0004,
            -0.0006,
        ),
        (
            "Jardin des Tuileries",
            "Place de la Concorde",
            "parks_outdoors/park",
            -0.0012,
            -0.0041,
        ),
        (
            "Pont des Arts",
            "Quai Malaquais",
            "travel/bridge",
            -0.0021,
            0.0002,
        ),
        (
            "Comédie-Française",
            "1 Place Colette",
            "arts_entertainment/theater",
            0.0019,
            0.0006,
        ),
        (
            "Palais Royal",
            "8 Rue de Montpensier",
            "building/historic",
            0.0032,
            0.0011,
        ),
        (
            "Shakespeare and Company",
            "37 Rue de la Bûcherie",
            "shops/bookstore",
            -0.0081,
            0.0129,
        ),
    ];
    let query = query.to_lowercase();
    PLACES
        .iter()
        .enumerate()
        .filter(|(_, (title, ..))| query.is_empty() || title.to_lowercase().contains(&query))
        .filter_map(|(index, (title, address, venue_type, dlat, dlon))| {
            Some(Venue {
                point: GeoPoint::new(at.latitude() + dlat, at.longitude() + dlon)?,
                title: (*title).into(),
                address: (*address).into(),
                provider: "foursquare".into(),
                id: format!("demo-{index}"),
                venue_type: (*venue_type).into(),
            })
        })
        .collect()
}

fn apply_json(app: &mut QuillApp, json: &str) {
    let (Some(session), Ok(envelope)) = (app.demo_session.as_mut(), parse_envelope(json)) else {
        return;
    };
    let seq = session.last_seq + 1;
    session.apply(OwnedEnvelope {
        seq,
        client_id: None,
        envelope,
    });
}

pub(super) fn start(app: &mut QuillApp) {
    let Some(session) = app.demo_session.as_mut() else {
        return;
    };
    session.location.set_venue_bot_username("foursquare".into());
    if let Some(outcome) = demo_outcome() {
        session.location.geolocated(outcome);
    }
}

pub(super) fn locate_again(app: &mut QuillApp) {
    if let (Some(session), Some(outcome)) = (app.demo_session.as_mut(), demo_outcome()) {
        session.location.geolocated(outcome);
    }
}

pub(super) fn pump(app: &mut QuillApp) {
    let tile = demo_media_allowlist()
        .join("demo-map-tile.png")
        .to_string_lossy()
        .into_owned();
    for _ in 0..4 {
        let requests = match app.demo_session.as_mut() {
            Some(session) => session.location.next_requests(),
            None => return,
        };
        if requests.is_empty() {
            return;
        }
        for request in requests {
            let extra = NEXT_EXTRA.fetch_add(1, Ordering::Relaxed);
            let file = FIRST_TILE_FILE.saturating_add(i32::try_from(extra % 100_000).unwrap_or(0));
            if matches!(request, PickerRequest::MapTile { .. }) {
                apply_json(
                    app,
                    &format!(
                        r#"{{"@type":"updateFile","file":{}}}"#,
                        demo_file_json(file, &tile, true)
                    ),
                );
            }
            let Some(session) = app.demo_session.as_mut() else {
                return;
            };
            let sharing = &mut session.location;
            sharing.request_sent(&request, extra);
            match request {
                PickerRequest::ResolveVenueBot { .. } => sharing.venue_bot_resolved(Some(DEMO_BOT)),
                PickerRequest::Venues {
                    generation,
                    point,
                    query,
                    ..
                } => sharing.venues_answered(generation, demo_venues(point, &query)),
                PickerRequest::MapTile { .. } => sharing.file_answered(extra, Some(FileId(file))),
                PickerRequest::VenueIcon { .. } => sharing.file_answered(extra, None),
            }
        }
    }
}

impl QuillApp {
    fn demo_location_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            session.open_chat(DEMO_CHAT);
        }
        self.open_location_picker(cx);
        if let Ok(text) = std::env::var("QUILL_DEMO_LOCATION_SEARCH") {
            self.location_search_open(window, cx);
            if let Some(input) = self.location_ui.search.clone() {
                input.update(cx, |input, cx| input.set_value(text.clone(), window, cx));
            }
            self.location_search_changed(Some(text), cx);
        }
    }
}
