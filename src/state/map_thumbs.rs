//! Static map thumbnails of location and venue messages
//! (`getMapThumbnailFile`, schema 1.8.67 line 15998; tdesktop draws the
//! same tile with `Data::LocationThumbnail`).
use super::*;
use crate::telegram::envelope::GeoLocation;
use std::collections::{HashMap, HashSet};

/// Map zoom of a message thumbnail (tdesktop `kZoom`).
pub const MAP_THUMB_ZOOM: i32 = 15;
/// Logical size of the tile in a bubble; the request asks for 2x pixels.
pub const MAP_THUMB_WIDTH: i32 = 320;
pub const MAP_THUMB_HEIGHT: i32 = 180;
/// `getMapThumbnailFile.scale` (1..=3): 2 keeps the tile sharp on Retina.
pub const MAP_THUMB_SCALE: i32 = 2;

/// Most places remembered in `files` / `asked`; past it both restart empty
/// and the open chat asks again for the tiles it still shows.
pub const MAP_THUMB_CAP: usize = 1024;

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

/// Thumbnails requested so far: which file shows which place, which
/// answers are awaited, and which requests came back with an error (not
/// asked again this session).
#[derive(Debug, Default)]
pub struct MapThumbs {
    files: HashMap<MapKey, i32>,
    awaiting: HashMap<u64, MapKey>,
    asked: HashSet<MapKey>,
}

impl MapThumbs {
    /// The file holding the tile for `location`, once TDLib answered.
    pub fn file_id(&self, location: &GeoLocation) -> Option<i32> {
        self.files.get(&MapKey::of(location)).copied()
    }

    /// Remember that request `extra` asks for `key`'s tile.
    pub fn expect(&mut self, extra: RequestId, key: MapKey) {
        if self.asked.len() >= MAP_THUMB_CAP && !self.asked.contains(&key) {
            self.asked.clear();
            self.files.clear();
        }
        self.asked.insert(key);
        self.awaiting.insert(extra.0, key);
    }

    /// The answer to `extra` named `file_id`.
    pub fn answered(&mut self, extra: RequestId, file_id: i32) {
        if let Some(key) = self.awaiting.remove(&extra.0) {
            self.files.insert(key, file_id);
        }
    }

    /// The request `extra` failed: the place stays tile-less.
    pub fn failed(&mut self, extra: RequestId) {
        self.awaiting.remove(&extra.0);
    }

    /// The request `extra` never left: the place may be asked again.
    pub fn unsent(&mut self, extra: RequestId) {
        if let Some(key) = self.awaiting.remove(&extra.0) {
            self.asked.remove(&key);
        }
    }

    pub fn has_asked(&self, key: &MapKey) -> bool {
        self.asked.contains(key)
    }
}

/// The place a location or venue message shows.
fn message_location(content: &MessageContent) -> Option<GeoLocation> {
    match content {
        MessageContent::Location(location) => Some(location.location),
        MessageContent::Venue(venue) => Some(venue.location),
        _ => None,
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
            if let Some(location) = message_location(&message.content) {
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
            if let Some(location) = message_location(&message.content)
                && let Some(file_id) = self.media.map_thumbs.file_id(&location)
                && self.should_download(FileId(file_id))
                && !ids.contains(&FileId(file_id))
            {
                ids.push(FileId(file_id));
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
            assert!(thumbs.asked.len() <= MAP_THUMB_CAP);
            assert!(thumbs.files.len() <= MAP_THUMB_CAP);
        }
        assert!(thumbs.has_asked(&key(MAP_THUMB_CAP as i64 * 3 - 1)));
    }
}
