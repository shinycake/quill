//! Static map thumbnails of location and venue messages
//! (`getMapThumbnailFile`, schema 1.8.67 line 15998; tdesktop draws the
//! same tile with `Data::LocationThumbnail`).
use super::*;
use crate::telegram::envelope::GeoLocation;
use std::collections::HashMap;

/// Map zoom of a message thumbnail (tdesktop `kZoom`).
pub const MAP_THUMB_ZOOM: i32 = 15;
/// Logical size of the tile in a bubble; the request asks for 2x pixels.
pub const MAP_THUMB_WIDTH: i32 = 320;
pub const MAP_THUMB_HEIGHT: i32 = 240;
/// `getMapThumbnailFile.scale` (1..=3): 2 keeps the tile sharp on Retina.
pub const MAP_THUMB_SCALE: i32 = 2;

/// Most places remembered. A chat's window holds at most
/// `HISTORY_WINDOW_CAP` messages, so it can never show more places than
/// fit; past the cap one answered or failed place is forgotten per new
/// one (and asked again if it is shown), never the whole map.
pub const MAP_THUMB_CAP: usize = 2 * crate::state::HISTORY_WINDOW_CAP;

/// Where a thumbnail is for: the rounded coordinates (about 11 cm).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MapKey {
    lat_e6: i64,
    lon_e6: i64,
}

impl MapKey {
    pub fn of(location: &GeoLocation) -> Self {
        Self {
            lat_e6: location.lat_e6,
            lon_e6: location.lon_e6,
        }
    }

    pub fn latitude(&self) -> f64 {
        self.lat_e6 as f64 / 1e6
    }

    pub fn longitude(&self) -> f64 {
        self.lon_e6 as f64 / 1e6
    }
}

/// Where one place's tile stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tile {
    /// Asked as this request; only its answer settles it.
    Asking(RequestId),
    Ready(i32),
    /// The request came back with an error (FLOOD_WAIT, a timeout). Not
    /// asked again while the chat it failed in stays open; opening another
    /// chat and coming back asks again ([`MapThumbs::retry_failed_outside`]).
    Failed {
        in_chat: Option<ChatId>,
    },
}

/// Thumbnails requested so far, one stage per place: which are awaited,
/// which file shows which place, and which requests failed.
#[derive(Debug, Default)]
pub struct MapThumbs {
    tiles: HashMap<MapKey, Tile>,
}

impl MapThumbs {
    /// The file holding the tile for `location`, once TDLib answered.
    pub fn file_id(&self, location: &GeoLocation) -> Option<i32> {
        match self.tiles.get(&MapKey::of(location)) {
            Some(Tile::Ready(file_id)) => Some(*file_id),
            _ => None,
        }
    }

    fn key_asked_as(&self, extra: RequestId) -> Option<MapKey> {
        self.tiles
            .iter()
            .find_map(|(key, tile)| (*tile == Tile::Asking(extra)).then_some(*key))
    }

    /// Remember that request `extra` asks for `key`'s tile.
    pub fn expect(&mut self, extra: RequestId, key: MapKey) {
        if self.tiles.len() >= MAP_THUMB_CAP && !self.tiles.contains_key(&key) {
            let evict = self
                .tiles
                .iter()
                .find_map(|(old, tile)| (!matches!(tile, Tile::Asking(_))).then_some(*old))
                .or_else(|| self.tiles.keys().next().copied());
            if let Some(old) = evict {
                self.tiles.remove(&old);
            }
        }
        self.tiles.insert(key, Tile::Asking(extra));
    }

    /// The answer to `extra` named `file_id`.
    pub fn answered(&mut self, extra: RequestId, file_id: i32) {
        if let Some(key) = self.key_asked_as(extra) {
            self.tiles.insert(key, Tile::Ready(file_id));
        }
    }

    /// The request `extra` failed while `open_chat` was open: the place
    /// keeps the coordinate card, and is asked again when its chat is
    /// opened afresh.
    pub fn failed(&mut self, extra: RequestId, open_chat: Option<ChatId>) {
        if let Some(key) = self.key_asked_as(extra) {
            self.tiles.insert(key, Tile::Failed { in_chat: open_chat });
        }
    }

    /// The request `extra` never left: the place may be asked again.
    pub fn unsent(&mut self, extra: RequestId) {
        if let Some(key) = self.key_asked_as(extra) {
            self.tiles.remove(&key);
        }
    }

    /// `chat` is the open chat: places that failed in another chat are
    /// asked for again.
    pub fn retry_failed_outside(&mut self, chat: ChatId) {
        self.tiles
            .retain(|_, tile| !matches!(tile, Tile::Failed { in_chat } if *in_chat != Some(chat)));
    }

    pub fn has_asked(&self, key: &MapKey) -> bool {
        self.tiles.contains_key(key)
    }
}

/// The places a message shows a map tile for: its location or venue, or the
/// places attached to a poll's options.
fn message_locations(content: &MessageContent) -> Vec<GeoLocation> {
    match content {
        MessageContent::Location(location) => vec![location.location],
        MessageContent::Venue(venue) => vec![venue.location],
        MessageContent::Poll(poll) => poll
            .poll
            .options
            .iter()
            .filter_map(|option| option.media.as_ref()?.place())
            .collect(),
        _ => Vec::new(),
    }
}

impl Session {
    /// Places in the open chat whose tile was not asked for yet, with the
    /// chat (TDLib wants it for rate limiting).
    pub fn map_thumbs_to_request(&self) -> Vec<(MapKey, ChatId)> {
        let Some(chat_id) = self.open_chat else {
            return Vec::new();
        };
        let Some(history) = self.histories.get(&chat_id.0) else {
            return Vec::new();
        };
        let mut wanted: Vec<MapKey> = Vec::new();
        for message in history.messages.values() {
            for location in message_locations(&message.content) {
                let key = MapKey::of(&location);
                if !self.media.map_thumbs.has_asked(&key) && !wanted.contains(&key) {
                    wanted.push(key);
                }
            }
        }
        wanted.into_iter().map(|key| (key, chat_id)).collect()
    }

    /// Tile files of the open chat that still need downloading.
    pub(crate) fn map_thumb_file_ids_to_download(&self) -> Vec<FileId> {
        let Some(history) = self
            .open_chat
            .and_then(|chat_id| self.histories.get(&chat_id.0))
        else {
            return Vec::new();
        };
        let mut ids = Vec::new();
        for message in history.messages.values() {
            for location in message_locations(&message.content) {
                if let Some(file_id) = self.media.map_thumbs.file_id(&location)
                    && self.should_download(FileId(file_id))
                    && !ids.contains(&FileId(file_id))
                {
                    ids.push(FileId(file_id));
                }
            }
        }
        ids
    }
}

#[cfg(test)]
mod cap_tests {
    use super::{MAP_THUMB_CAP, MapKey, MapThumbs};
    use crate::state::RequestId;

    fn key(n: i64) -> MapKey {
        MapKey {
            lat_e6: n,
            lon_e6: n,
        }
    }

    #[test]
    fn places_are_bounded() {
        let mut thumbs = MapThumbs::default();
        for n in 0..(MAP_THUMB_CAP as i64 * 3) {
            thumbs.expect(RequestId(n as u64), key(n));
            thumbs.answered(RequestId(n as u64), n as i32);
            assert!(thumbs.tiles.len() <= MAP_THUMB_CAP);
        }
        assert!(thumbs.has_asked(&key(MAP_THUMB_CAP as i64 * 3 - 1)));
        // Sweep 05-188: the cap drops one place per new one, not the map.
        assert_eq!(thumbs.tiles.len(), MAP_THUMB_CAP);
    }

    /// Sweep 05-187: a failed tile is not remembered as asked for the whole
    /// session; the chat it failed in does not re-ask it, a later visit does.
    #[test]
    fn a_failed_tile_is_asked_again_when_the_chat_is_opened_afresh() {
        use crate::ids::ChatId;
        let mut thumbs = MapThumbs::default();
        thumbs.expect(RequestId(1), key(1));
        thumbs.failed(RequestId(1), Some(ChatId(7)));
        assert!(thumbs.has_asked(&key(1)), "not again in the same chat");
        thumbs.retry_failed_outside(ChatId(7));
        assert!(thumbs.has_asked(&key(1)), "still the same chat");
        thumbs.retry_failed_outside(ChatId(8));
        assert!(!thumbs.has_asked(&key(1)), "another chat: ask again");
        thumbs.expect(RequestId(2), key(1));
        thumbs.answered(RequestId(2), 99);
        thumbs.retry_failed_outside(ChatId(9));
        assert!(thumbs.has_asked(&key(1)), "an answered tile stays");
    }
}
