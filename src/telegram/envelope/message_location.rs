use super::*;
use serde_json::Value;

/// `location` (TDLib 1.8.67, `schema/td_api.tl:646`): `latitude` /
/// `longitude` in degrees, `horizontal_accuracy` in meters (0 = unknown).
/// Phase 4.3: coordinates are stored as integer **microdegrees**
/// (`lat_e6` / `lon_e6`, 10⁻⁶ degrees ≈ 11 cm) so the parsed model keeps
/// the `Eq` derive used across the envelope types — more than enough for
/// display and map-link generation. Accuracy is rounded to whole meters
/// (`accuracy_m`; 0 = unknown, per schema).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GeoLocation {
    pub lat_e6: i64,
    pub lon_e6: i64,
    pub accuracy_m: i32,
}

impl GeoLocation {
    /// Latitude in degrees.
    pub fn latitude(&self) -> f64 {
        self.lat_e6 as f64 / 1e6
    }

    /// Longitude in degrees.
    pub fn longitude(&self) -> f64 {
        self.lon_e6 as f64 / 1e6
    }

    /// User-facing coordinate line, e.g. `37.7749, -122.4194`.
    pub fn coords_label(&self) -> String {
        format!("{:.4}, {:.4}", self.latitude(), self.longitude())
    }

    /// OpenStreetMap deep link for the "Open map" row action. The
    /// coordinates come from the parsed message, so they contain no
    /// whitespace or control characters and the `https://` scheme passes
    /// `platform::open_external_url`'s scheme gate.
    pub fn open_street_map_url(&self) -> String {
        format!(
            "https://www.openstreetmap.org/?mlat={:.6}&mlon={:.6}",
            self.latitude(),
            self.longitude()
        )
    }
}

/// Safe rule for coordinate parsing (Phase 4.3, documented per
/// `messageLocation` / `messageVenue`): `latitude` and `longitude` must be
/// finite numbers with `|lat| <= 90` and `|lon| <= 180`. Anything else —
/// NaN, infinities, or out-of-range degrees — means corrupt data, and the
/// location is dropped entirely (the message renders as `Unsupported`)
/// rather than pinned to a clamped pole or fed to a map link.
pub(crate) fn geo_location(value: Option<&Value>) -> Option<GeoLocation> {
    let value = value?;
    let latitude = value.get("latitude").and_then(Value::as_f64)?;
    let longitude = value.get("longitude").and_then(Value::as_f64)?;
    if !latitude.is_finite() || !longitude.is_finite() {
        return None;
    }
    if latitude.abs() > 90.0 || longitude.abs() > 180.0 {
        return None;
    }
    let accuracy = value
        .get("horizontal_accuracy")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let accuracy_m = if accuracy.is_finite() && accuracy > 0.0 {
        accuracy.round().clamp(0.0, i32::MAX as f64) as i32
    } else {
        0
    };
    Some(GeoLocation {
        lat_e6: (latitude * 1e6).round() as i64,
        lon_e6: (longitude * 1e6).round() as i64,
        accuracy_m,
    })
}

/// `liveLocation` (TDLib 1.8.67, `schema/td_api.tl:653`): live-period state
/// attached to a location. `live_period` is relative to the message send
/// date in seconds (`0x7FFFFFFF` = updates forever); `heading` is 1–360
/// degrees (0 = unknown); `proximity_alert_radius` is 0–100000 meters
/// (0 = disabled).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LiveLocationState {
    pub live_period: i32,
    pub expires_in: i32,
    pub heading: i32,
    pub proximity_alert_radius: i32,
}

impl LiveLocationState {
    /// Static status line; live re-rendering is out of this slice, so the
    /// remaining time is a snapshot from `expires_in` at parse time.
    pub fn status_label(&self) -> String {
        if self.expires_in <= 0 {
            return "Live location ended".into();
        }
        let mut parts = vec![format!(
            "Live · expires in {}",
            duration_label(self.expires_in)
        )];
        if self.heading > 0 {
            parts.push(format!("heading {}°", self.heading));
        }
        if self.proximity_alert_radius > 0 {
            parts.push(format!(
                "proximity alert ≤ {}",
                meters_label(self.proximity_alert_radius)
            ));
        }
        parts.join(" · ")
    }
}

/// `mm:ss` or `Xh Ym` for `expires_in`-style second counts.
pub(crate) fn duration_label(seconds: i32) -> String {
    let seconds = seconds.max(0) as i64;
    if seconds >= 3600 {
        format!("{}h {:02}m", seconds / 3600, (seconds % 3600) / 60)
    } else {
        format!("{}:{:02}", seconds / 60, seconds % 60)
    }
}

pub(crate) fn meters_label(meters: i32) -> String {
    if meters >= 1000 {
        format!("{:.1} km", meters as f64 / 1000.0)
    } else {
        format!("{meters} m")
    }
}

/// `messageLocation` (`schema/td_api.tl:5214`) / `messageLiveLocation`
/// (`schema/td_api.tl:5211`, `expires_in` = seconds left for updates,
/// 0 = can't be updated anymore). For `messageLocation`, `live` is
/// `None`; for `messageLiveLocation`, it carries the live state. Note the
/// schema split: `messageLocation` itself carries **no** live fields — the
/// task's `live_period` / `heading` / `proximity_alert_radius` live on
/// `liveLocation` (`schema/td_api.tl:653`), which is why both constructors
/// are parsed here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocationContent {
    pub location: GeoLocation,
    pub live: Option<LiveLocationState>,
}

/// `venue` (TDLib 1.8.67, `schema/td_api.tl:663`). `id` and `type` are
/// provider-database identifiers and are not kept — Quill renders the
/// human-readable `title` + `address` and the map link from `location`.
/// `provider` (e.g. "foursquare", "gplaces") is kept for the subtitle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VenueContent {
    pub location: GeoLocation,
    pub title: String,
    pub address: String,
    pub provider: String,
}

/// `liveLocation` (TDLib 1.8.67, `schema/td_api.tl:653`). `None` when the
/// wrapper object itself is missing or null.
pub(crate) fn parse_live_location_state(value: Option<&Value>) -> Option<LiveLocationState> {
    let value = value?;
    if value.is_null() {
        return None;
    }
    Some(LiveLocationState {
        live_period: value
            .get("live_period")
            .and_then(Value::as_i64)
            .unwrap_or(0)
            .sat_i32(),
        expires_in: 0,
        heading: value
            .get("heading")
            .and_then(Value::as_i64)
            .unwrap_or(0)
            .sat_i32(),
        proximity_alert_radius: value
            .get("proximity_alert_radius")
            .and_then(Value::as_i64)
            .unwrap_or(0)
            .sat_i32(),
    })
}

/// `messageLocation` (TDLib 1.8.67, `schema/td_api.tl:5214`). An invalid
/// `location` (missing or failing the coordinate rule) yields
/// `Unsupported` so corrupt data never reaches a map link.
pub(crate) fn parse_message_location(value: &Value) -> (MessageContent, Vec<ParsedFile>) {
    match geo_location(value.get("location")) {
        Some(location) => (
            MessageContent::Location(LocationContent {
                location,
                live: None,
            }),
            Vec::new(),
        ),
        None => (
            MessageContent::Unsupported {
                type_name: "messageLocation".into(),
            },
            Vec::new(),
        ),
    }
}

/// `messageLiveLocation` (TDLib 1.8.67, `schema/td_api.tl:5211`).
/// `expires_in` rides on the message wrapper, `live_period` / `heading` /
/// `proximity_alert_radius` on the inner `liveLocation`.
pub(crate) fn parse_message_live_location(value: &Value) -> (MessageContent, Vec<ParsedFile>) {
    let live_location = value.get("location");
    match (
        geo_location(live_location.and_then(|v| v.get("location"))),
        parse_live_location_state(live_location),
    ) {
        (Some(location), Some(mut live)) => {
            live.expires_in = value
                .get("expires_in")
                .and_then(Value::as_i64)
                .unwrap_or(0)
                .sat_i32();
            (
                MessageContent::Location(LocationContent {
                    location,
                    live: Some(live),
                }),
                Vec::new(),
            )
        }
        _ => (
            MessageContent::Unsupported {
                type_name: "messageLiveLocation".into(),
            },
            Vec::new(),
        ),
    }
}

/// `messageVenue` (TDLib 1.8.67, `schema/td_api.tl:5217`). Provider `id`
/// and `type` are dropped (see `VenueContent` docs).
pub(crate) fn parse_message_venue(value: &Value) -> (MessageContent, Vec<ParsedFile>) {
    let venue = value.get("venue");
    match geo_location(venue.and_then(|v| v.get("location"))) {
        Some(location) => (
            MessageContent::Venue(VenueContent {
                location,
                title: venue
                    .and_then(|v| v.get("title"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                address: venue
                    .and_then(|v| v.get("address"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                provider: venue
                    .and_then(|v| v.get("provider"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            }),
            Vec::new(),
        ),
        None => (
            MessageContent::Unsupported {
                type_name: "messageVenue".into(),
            },
            Vec::new(),
        ),
    }
}
