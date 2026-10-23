//! Places from the venue search bot (tdesktop `Data::InputVenue`,
//! `PickerVenueState`, `ParseVenues`).

use super::geo::GeoPoint;
use serde_json::Value;

/// One place the bot found: what `inputMessageVenue` sends.
#[derive(Debug, Clone, PartialEq)]
pub struct Venue {
    pub point: GeoPoint,
    pub title: String,
    pub address: String,
    pub provider: String,
    /// The provider's venue id.
    pub id: String,
    /// The provider's category, e.g. `food/default` (also names the icon).
    pub venue_type: String,
}

impl Venue {
    /// The Foursquare category icon tdesktop draws in the row
    /// (`Data::Session::venueIconDocument`), when the venue has a type.
    pub fn icon_url(&self) -> Option<String> {
        venue_icon_url(&self.venue_type)
    }
}

/// `https://ss3.4sqi.net/img/categories_v2/<type>_64.png`, for a type made
/// of the characters Foursquare uses in category paths.
pub fn venue_icon_url(venue_type: &str) -> Option<String> {
    let ok = !venue_type.is_empty()
        && venue_type
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '_' | '-'));
    ok.then(|| format!("https://ss3.4sqi.net/img/categories_v2/{venue_type}_64.png"))
}

/// The venue inside one `inlineQueryResultVenue`; `None` for any other
/// result or a venue without a usable location.
pub fn parse_venue_result(value: &Value) -> Option<Venue> {
    if value.get("@type").and_then(Value::as_str) != Some("inlineQueryResultVenue") {
        return None;
    }
    let venue = value.get("venue")?;
    let location = venue.get("location")?;
    let point = GeoPoint::new(
        location.get("latitude")?.as_f64()?,
        location.get("longitude")?.as_f64()?,
    )?;
    let text = |key: &str| {
        venue
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    Some(Venue {
        point,
        title: text("title"),
        address: text("address"),
        provider: text("provider"),
        id: text("id"),
        venue_type: text("type"),
    })
}

/// What the "Or choose a venue" list shows (tdesktop `PickerVenueState`).
#[derive(Debug, Clone, PartialEq)]
pub enum VenueList {
    Loading,
    /// The bot found nothing; `query` is the search text, empty for
    /// places around the pin.
    NothingFound {
        query: String,
    },
    /// No place to search around yet ("Choose location to see places nearby.").
    WaitingForLocation,
    Found(Vec<Venue>),
}

/// tdesktop `NormalizeVenuesQuery`: trimmed and lower-cased.
pub fn normalize_query(query: &str) -> String {
    query.trim().to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_a_venue_result() {
        let venue = parse_venue_result(&json!({
            "@type": "inlineQueryResultVenue",
            "id": "1",
            "venue": {
                "@type": "venue",
                "location": {"@type": "location", "latitude": 40.7, "longitude": -74.0},
                "title": "Cafe",
                "address": "1 Main St",
                "provider": "foursquare",
                "id": "4b0",
                "type": "food/cafe"
            }
        }))
        .expect("venue");
        assert_eq!(venue.title, "Cafe");
        assert_eq!(venue.venue_type, "food/cafe");
        assert_eq!(
            venue.icon_url().as_deref(),
            Some("https://ss3.4sqi.net/img/categories_v2/food/cafe_64.png")
        );
        assert!(parse_venue_result(&json!({"@type": "inlineQueryResultArticle"})).is_none());
        assert!(
            parse_venue_result(&json!({"@type": "inlineQueryResultVenue", "venue": {}})).is_none()
        );
    }

    #[test]
    fn icon_url_rejects_odd_types() {
        assert!(venue_icon_url("").is_none());
        assert!(venue_icon_url("food/../x?y").is_none());
    }

    #[test]
    fn queries_normalize() {
        assert_eq!(normalize_query("  Pizza Place "), "pizza place");
    }
}
