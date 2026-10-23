//! Send Location: the attach menu's map picker (tdesktop
//! `Ui::LocationPicker`). The map is drawn from TDLib's static map tiles
//! instead of tdesktop's Mapbox web view; see
//! docs/decisions/location-picker.md.
//!
//! [`LocationSharing`] is the session-wide part (the venue bot, the last
//! exact fix, venue icons) and holds the open [`LocationPicker`].

pub mod country;
pub mod geo;
pub mod map_view;
mod picker;
pub mod venues;

pub use geo::{GeoOutcome, GeoPoint};
pub use map_view::{MapSize, MapView, TileKey};
pub use picker::{AddressLine, HereState, LocationPicker};
pub use venues::{Venue, VenueList};

use crate::ids::{ChatId, FileId};
use std::collections::HashMap;

/// In-flight requests of the picker; wrapped as
/// `MediaPurpose::LocationPicker`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocationPickerPurpose {
    /// `searchPublicChat` for the `venue_search_bot_username` option.
    ResolveVenueBot,
    /// `getInlineQueryResults` to the venue bot; `generation` names the
    /// picker's request so a stale answer is dropped.
    SearchVenues { generation: u64 },
    /// `getMapThumbnailFile` for the picker's map. Response is `file`.
    MapTile,
    /// `getRemoteFile` for a venue category icon. Response is `file`.
    VenueIcon,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PickerRequest {
    ResolveVenueBot {
        username: String,
    },
    Venues {
        generation: u64,
        bot_user_id: i64,
        chat_id: ChatId,
        point: GeoPoint,
        query: String,
    },
    MapTile {
        key: TileKey,
        chat_id: ChatId,
    },
    VenueIcon {
        venue_type: String,
        url: String,
    },
}

impl PickerRequest {
    pub fn purpose(&self) -> LocationPickerPurpose {
        match self {
            Self::ResolveVenueBot { .. } => LocationPickerPurpose::ResolveVenueBot,
            Self::Venues { generation, .. } => LocationPickerPurpose::SearchVenues {
                generation: *generation,
            },
            Self::MapTile { .. } => LocationPickerPurpose::MapTile,
            Self::VenueIcon { .. } => LocationPickerPurpose::VenueIcon,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum VenueBot {
    #[default]
    Unknown,
    Resolving,
    Found(i64),
    Missing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IconState {
    Requested(u64),
    Ready(FileId),
    Failed,
}

#[derive(Debug, Default)]
pub struct LocationSharing {
    venue_bot_username: String,
    venue_bot: VenueBot,
    /// tdesktop `LastExactLocation`: the last fix, where the next picker starts.
    last_exact: Option<GeoPoint>,
    picker: Option<LocationPicker>,
    icons: HashMap<String, IconState>,
}

const ICON_CAP: usize = 256;

impl LocationSharing {
    /// The `venue_search_bot_username` option changed.
    pub fn set_venue_bot_username(&mut self, username: String) {
        if self.venue_bot_username != username {
            self.venue_bot_username = username;
            self.venue_bot = VenueBot::Unknown;
        }
    }

    pub fn picker(&self) -> Option<&LocationPicker> {
        self.picker.as_ref()
    }

    pub fn picker_mut(&mut self) -> Option<&mut LocationPicker> {
        self.picker.as_mut()
    }

    /// Open the picker for `chat_id`; `country` is the system country's
    /// centre, where the map starts without a previous fix.
    pub fn open(&mut self, chat_id: ChatId, country: Option<GeoPoint>) {
        self.picker = Some(LocationPicker::open(
            chat_id,
            self.last_exact,
            country.unwrap_or(GeoPoint::ORIGIN),
        ));
    }

    pub fn close(&mut self) {
        self.picker = None;
    }

    pub fn geolocated(&mut self, outcome: GeoOutcome) {
        let changed = match outcome {
            GeoOutcome::Found(point) => !geo::same_place(self.last_exact, Some(point)),
            _ => false,
        };
        if let GeoOutcome::Found(point) = outcome {
            self.last_exact = Some(point);
        }
        if let Some(picker) = self.picker.as_mut() {
            picker.geolocated(outcome, changed);
        }
    }

    /// What to send now. Marks each as sent; the driver reports a send
    /// that failed with [`Self::request_unsent`] and the extra of one that
    /// left with [`Self::request_sent`].
    pub fn next_requests(&mut self) -> Vec<PickerRequest> {
        let mut out = Vec::new();
        let Some(picker) = self.picker.as_mut() else {
            return out;
        };
        let chat_id = picker.chat_id();
        if let Some((generation, point, query)) = picker.venue_query_to_send() {
            match self.venue_bot {
                VenueBot::Found(bot_user_id) => {
                    picker.venue_query_sent(generation);
                    out.push(PickerRequest::Venues {
                        generation,
                        bot_user_id,
                        chat_id,
                        point,
                        query,
                    });
                }
                VenueBot::Unknown if !self.venue_bot_username.is_empty() => {
                    self.venue_bot = VenueBot::Resolving;
                    out.push(PickerRequest::ResolveVenueBot {
                        username: self.venue_bot_username.clone(),
                    });
                }
                VenueBot::Resolving => {}
                VenueBot::Unknown | VenueBot::Missing => picker.venues_failed(generation),
            }
        }
        if let Some(key) = picker.tile_to_request() {
            out.push(PickerRequest::MapTile { key, chat_id });
        }
        if self.icons.len() >= ICON_CAP {
            self.icons.clear();
        }
        for venue_type in picker.listed_venue_types() {
            if self.icons.contains_key(&venue_type) {
                continue;
            }
            match venues::venue_icon_url(&venue_type) {
                Some(url) => out.push(PickerRequest::VenueIcon { venue_type, url }),
                None => {
                    self.icons.insert(venue_type, IconState::Failed);
                }
            }
        }
        out
    }

    pub fn request_sent(&mut self, request: &PickerRequest, extra: u64) {
        match request {
            PickerRequest::MapTile { key, .. } => {
                if let Some(picker) = self.picker.as_mut() {
                    picker.tile_requested(*key, extra);
                }
            }
            PickerRequest::VenueIcon { venue_type, .. } => {
                self.icons
                    .insert(venue_type.clone(), IconState::Requested(extra));
            }
            PickerRequest::ResolveVenueBot { .. } | PickerRequest::Venues { .. } => {}
        }
    }

    pub fn request_unsent(&mut self, request: &PickerRequest) {
        match request {
            PickerRequest::ResolveVenueBot { .. } => {
                self.venue_bot = VenueBot::Unknown;
                self.fail_current_venues();
            }
            PickerRequest::Venues { generation, .. } => {
                if let Some(picker) = self.picker.as_mut() {
                    picker.venues_failed(*generation);
                }
            }
            PickerRequest::MapTile { key, .. } => {
                if let Some(picker) = self.picker.as_mut() {
                    picker.tile_unsent(*key);
                }
            }
            PickerRequest::VenueIcon { venue_type, .. } => {
                self.icons.insert(venue_type.clone(), IconState::Failed);
            }
        }
    }

    fn fail_current_venues(&mut self) {
        if let Some(picker) = self.picker.as_mut()
            && let Some((generation, ..)) = picker.venue_query_to_send()
        {
            picker.venues_failed(generation);
        }
    }

    /// `searchPublicChat` answered: `bot_user_id` when the chat is a bot.
    pub fn venue_bot_resolved(&mut self, bot_user_id: Option<i64>) {
        self.venue_bot = bot_user_id.map_or(VenueBot::Missing, VenueBot::Found);
        if self.venue_bot == VenueBot::Missing {
            self.fail_current_venues();
        }
    }

    pub fn venue_bot_failed(&mut self) {
        self.venue_bot_resolved(None);
    }

    pub fn venues_answered(&mut self, generation: u64, venues: Vec<Venue>) {
        if let Some(picker) = self.picker.as_mut() {
            picker.venues_answered(generation, venues);
        }
    }

    pub fn venues_failed(&mut self, generation: u64) {
        if let Some(picker) = self.picker.as_mut() {
            picker.venues_failed(generation);
        }
    }

    /// A `file` (or an error, `None`) answered request `extra`, sent for a
    /// tile or an icon.
    pub fn file_answered(&mut self, extra: u64, file_id: Option<FileId>) {
        if let Some(picker) = self.picker.as_mut()
            && picker.tile_answered(extra, file_id)
        {
            return;
        }
        if let Some(state) = self
            .icons
            .values_mut()
            .find(|state| **state == IconState::Requested(extra))
        {
            *state = file_id.map_or(IconState::Failed, IconState::Ready);
        }
    }

    /// The icon file of a venue category, once TDLib registered it.
    pub fn icon_file(&self, venue_type: &str) -> Option<FileId> {
        match self.icons.get(venue_type)? {
            IconState::Ready(file_id) => Some(*file_id),
            _ => None,
        }
    }

    /// Files the open picker shows: its tiles and the listed venues' icons.
    pub fn files_to_download(&self) -> Vec<FileId> {
        let Some(picker) = self.picker.as_ref() else {
            return Vec::new();
        };
        let mut files: Vec<FileId> = picker.tiles().into_iter().map(|(_, id)| id).collect();
        files.extend(
            picker
                .listed_venue_types()
                .iter()
                .filter_map(|venue_type| self.icon_file(venue_type)),
        );
        files
    }
}

#[cfg(test)]
#[path = "sharing_tests.rs"]
mod tests;
