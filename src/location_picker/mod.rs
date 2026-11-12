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

use crate::ids::{ChatId, FileId, RequestId};
use std::collections::{HashMap, HashSet};

/// In-flight requests of the picker; wrapped as
/// `MediaPurpose::LocationPicker`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocationPickerPurpose {
    /// `searchPublicChat` for the `venue_search_bot_username` option.
    ResolveVenueBot,
    /// `getInlineQueryResults` to the venue bot. The picker's venue ask
    /// holds the request it waits for, so a stale answer is dropped.
    SearchVenues,
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
            Self::Venues { .. } => LocationPickerPurpose::SearchVenues,
            Self::MapTile { .. } => LocationPickerPurpose::MapTile,
            Self::VenueIcon { .. } => LocationPickerPurpose::VenueIcon,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum VenueBot {
    #[default]
    Unknown,
    /// The lookup is out as `request`; only that request's answer or error
    /// settles it, a request the session no longer holds ends it
    /// ([`LocationSharing::forget_dropped`]), and a new picker or username
    /// drops it.
    Resolving { request: RequestId },
    Found(i64),
    Missing,
    /// The lookup failed (FLOOD_WAIT, a server error): no venues in this
    /// picker, and the next picker asks again.
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IconState {
    Requested(RequestId),
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
        // Only a found bot is kept. A lookup still out (its request may
        // have been dropped), failed, or answered with a bot not yet
        // cached is asked again by this picker.
        if self.venue_bot != VenueBot::Unknown && !matches!(self.venue_bot, VenueBot::Found(_)) {
            self.venue_bot = VenueBot::Unknown;
        }
        // Icons that failed, or whose answer was dropped, are asked again
        // by this picker.
        self.icons
            .retain(|_, state| matches!(state, IconState::Ready(_)));
        self.picker = Some(LocationPicker::open(
            chat_id,
            self.last_exact,
            country.unwrap_or(GeoPoint::ORIGIN),
        ));
    }

    pub fn close(&mut self) {
        self.picker = None;
    }

    /// "My location" began a new lookup: the picker is locating again.
    pub fn locate_started(&mut self) {
        if let Some(picker) = self.picker.as_mut() {
            picker.locate_started();
        }
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

    /// What to send now. Nothing is marked asked here: the driver reports
    /// the request of each that left with [`Self::request_sent`], and a send
    /// that failed with [`Self::request_unsent`].
    pub fn next_requests(&mut self) -> Vec<PickerRequest> {
        let mut out = Vec::new();
        let Some(picker) = self.picker.as_mut() else {
            return out;
        };
        let chat_id = picker.chat_id();
        if let Some((point, query)) = picker.venue_query_to_send() {
            match self.venue_bot {
                VenueBot::Found(bot_user_id) => {
                    out.push(PickerRequest::Venues {
                        bot_user_id,
                        chat_id,
                        point,
                        query,
                    });
                }
                VenueBot::Unknown if !self.venue_bot_username.is_empty() => {
                    out.push(PickerRequest::ResolveVenueBot {
                        username: self.venue_bot_username.clone(),
                    });
                }
                VenueBot::Resolving { .. } => {}
                VenueBot::Unknown | VenueBot::Missing => picker.venues_unavailable(),
                VenueBot::Failed => picker.venues_unsent(),
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

    pub fn request_sent(&mut self, request: &PickerRequest, sent: RequestId) {
        match request {
            PickerRequest::MapTile { key, .. } => {
                if let Some(picker) = self.picker.as_mut() {
                    picker.tile_requested(*key, sent);
                }
            }
            PickerRequest::VenueIcon { venue_type, .. } => {
                self.icons
                    .insert(venue_type.clone(), IconState::Requested(sent));
            }
            PickerRequest::ResolveVenueBot { .. } => {
                self.venue_bot = VenueBot::Resolving { request: sent };
            }
            PickerRequest::Venues { .. } => {
                if let Some(picker) = self.picker.as_mut() {
                    picker.venue_query_requested(sent);
                }
            }
        }
    }

    pub fn request_unsent(&mut self, request: &PickerRequest) {
        match request {
            PickerRequest::ResolveVenueBot { .. } => {
                self.venue_bot = VenueBot::Unknown;
                self.settle_current_venues(true);
            }
            PickerRequest::Venues { .. } => {
                if let Some(picker) = self.picker.as_mut() {
                    picker.venues_unsent();
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

    /// The unsent venue query cannot go out: `failed` when the bot lookup
    /// went wrong (a retry may work), else there is no bot.
    fn settle_current_venues(&mut self, failed: bool) {
        if let Some(picker) = self.picker.as_mut() {
            if failed {
                picker.venues_unsent();
            } else {
                picker.venues_unavailable();
            }
        }
    }

    /// The Retry under a failed venue list: look the bot up again if that
    /// was what failed, then ask for the same places again.
    pub fn retry_venues(&mut self) {
        if self.venue_bot == VenueBot::Failed {
            self.venue_bot = VenueBot::Unknown;
        }
        if let Some(picker) = self.picker.as_mut() {
            picker.retry_venues();
        }
    }

    /// Whether the lookup out is `request`.
    fn resolving(&self, request: RequestId) -> bool {
        self.venue_bot == (VenueBot::Resolving { request })
    }

    /// `searchPublicChat` answered `request`: `bot_user_id` when the chat is
    /// a bot. An answer to a lookup that was dropped changes nothing.
    pub fn venue_bot_resolved(&mut self, request: RequestId, bot_user_id: Option<i64>) {
        if !self.resolving(request) {
            return;
        }
        self.venue_bot = bot_user_id.map_or(VenueBot::Missing, VenueBot::Found);
        if self.venue_bot == VenueBot::Missing {
            self.settle_current_venues(false);
        }
    }

    /// The lookup failed: not a missing bot, so the next picker asks
    /// again instead of showing no venues for the session.
    pub fn venue_bot_failed(&mut self, request: RequestId) {
        if !self.resolving(request) {
            return;
        }
        self.venue_bot = VenueBot::Failed;
        self.settle_current_venues(true);
    }

    /// `request` answered the venue query with `venues`.
    pub fn venues_answered(&mut self, request: RequestId, venues: Vec<Venue>) {
        if let Some(picker) = self.picker.as_mut() {
            picker.venues_answered(request, venues);
        }
    }

    /// The venue query sent as `request` failed.
    pub fn venues_failed(&mut self, request: RequestId) {
        if let Some(picker) = self.picker.as_mut() {
            picker.venues_failed(request);
        }
    }

    /// A `file` (or an error, `None`) answered `request`, sent for a tile or
    /// an icon.
    pub fn file_answered(&mut self, request: RequestId, file_id: Option<FileId>) {
        if let Some(picker) = self.picker.as_mut()
            && picker.tile_answered(request, file_id)
        {
            return;
        }
        if let Some(state) = self
            .icons
            .values_mut()
            .find(|state| **state == IconState::Requested(request))
        {
            *state = file_id.map_or(IconState::Failed, IconState::Ready);
        }
    }

    /// Requests of the picker the session no longer holds and nothing
    /// answered (a reconnect or a client restart dropped them) end here, as
    /// the failure TDLib never sent: the bot lookup fails (the venue list
    /// with it), the venue query fails (Retry asks again), a tile counts one
    /// failed try and an icon is asked again by the next picker. Run after
    /// each ingest, once the answers that did arrive were applied.
    pub fn forget_dropped(&mut self, pending: &HashSet<RequestId>) {
        if let VenueBot::Resolving { request } = self.venue_bot
            && !pending.contains(&request)
        {
            self.venue_bot = VenueBot::Failed;
            self.settle_current_venues(true);
        }
        if let Some(picker) = self.picker.as_mut() {
            picker.forget_dropped(pending);
        }
        self.icons.retain(|_, state| match state {
            IconState::Requested(request) => pending.contains(request),
            IconState::Ready(_) | IconState::Failed => true,
        });
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
