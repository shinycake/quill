//! One open picker: the map, the pin's address and the venue list
//! (tdesktop `Ui::LocationPicker`, `location_picker.cpp`). Every input
//! (drag, zoom, a geolocation fix, a search keystroke, a TDLib answer)
//! is a method here; the UI draws from the readers and the driver sends
//! what [`super::LocationSharing::next_requests`] asks for.

use super::geo::{GeoOutcome, GeoPoint, same_place};
use super::map_view::{MapSize, MapView, START_ZOOM, TileKey};
use super::venues::{Venue, VenueList, normalize_query};
use crate::ids::{ChatId, FileId};

const TILE_CAP: usize = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HereState {
    Locating,
    Found,
    Denied,
    Unavailable,
}

/// The status line under "Send This Location" (tdesktop
/// `MakeChooseLocationButton`): the address, "Loading…" once an address
/// was shown and the pin moved, or nothing before the first address.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressLine<'a> {
    Hidden,
    Loading,
    Text(&'a str),
}

/// The address under the pin, from asking to showing. One value: the
/// place it is for travels with each stage, and an answer applies only
/// to the place asked.
#[derive(Debug, Clone, Default, PartialEq)]
enum AddressStage {
    /// Nothing is wanted or shown.
    #[default]
    Idle,
    /// Resolve this once the pin rests (tdesktop `kResolveAddressDelay`).
    Postponed(GeoPoint),
    /// The UI should look this place up ([`LocationPicker::take_address_job`]).
    Wanted(GeoPoint),
    /// The UI is looking this place up.
    Resolving(GeoPoint),
    /// The address for `point`. While the pin is lifted ([`Pin::Moving`])
    /// it is kept but not drawn, for a return to the same place.
    Shown { point: GeoPoint, text: String },
}

impl AddressStage {
    /// The place a stage is asking for or showing.
    fn point(&self) -> Option<GeoPoint> {
        match self {
            Self::Idle | Self::Postponed(_) => None,
            Self::Wanted(point) | Self::Resolving(point) => Some(*point),
            Self::Shown { point, .. } => Some(*point),
        }
    }
}

#[derive(Debug, Default)]
struct Address {
    stage: AddressStage,
    /// Some address was shown before: a blank line then reads "Loading…"
    /// instead of nothing. A latch of the history, not a stage.
    ever_shown: bool,
}

/// Where the venue query for a place and text stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AskStage {
    /// Not sent yet (the bot is still being looked up, or the driver has
    /// not picked it up).
    Unsent,
    InFlight,
    Answered,
    /// The lookup failed: asking for the same again sends it again.
    Failed,
}

/// tdesktop `_venuesRequestLocation` / `_venuesRequestQuery`, with the
/// stage of that request.
#[derive(Debug)]
struct VenueAsk {
    point: GeoPoint,
    query: String,
    stage: AskStage,
}

#[derive(Debug)]
struct VenueSearch {
    list: VenueList,
    request: Option<VenueAsk>,
    initial_query: String,
    generation: u64,
    cache: Vec<(String, GeoPoint, Vec<Venue>)>,
    /// Where a search looks (tdesktop `_venuesSearchLocation`); a search
    /// is offered once it is known.
    search_location: Option<GeoPoint>,
    no_search_location: Option<GeoPoint>,
    /// The search field (tdesktop `venuesSearchChanged`).
    input: SearchInput,
}

/// The venue search field: closed, open and blank, typing (the search
/// waits for typing to pause, tdesktop `kSearchDebounceDelay`), or
/// settled on a query.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
enum SearchInput {
    #[default]
    Closed,
    Blank(String),
    Typing(String),
    Settled(String),
}

impl SearchInput {
    fn query(&self) -> Option<&str> {
        match self {
            Self::Closed => None,
            Self::Blank(query) | Self::Typing(query) | Self::Settled(query) => Some(query),
        }
    }

    /// A non-blank query is on.
    fn is_shown(&self) -> bool {
        matches!(self, Self::Typing(_) | Self::Settled(_))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TileState {
    /// Asked under request `extra`, after `failed` asks for it failed.
    Requested {
        extra: u64,
        failed: u8,
    },
    Ready(FileId),
    /// This many asks in a row failed. Under [`TILE_TRIES`] the tile is
    /// asked again; at it the tile waits for the map to move or resize.
    Failed(u8),
}

/// How many times in a row a tile is asked for before it waits for the map
/// to move or change size.
const TILE_TRIES: u8 = 3;

/// The pin: lifted while the map moves; once it rests away from the place
/// the venues were asked for, the "search this area" button shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pin {
    Resting,
    Moving,
    RestedAway,
}

#[derive(Debug)]
pub struct LocationPicker {
    chat_id: ChatId,
    view: MapView,
    pin: Pin,
    map_size: Option<MapSize>,
    here: HereState,
    address: Address,
    venues: VenueSearch,
    tiles: Vec<(TileKey, TileState)>,
}

impl LocationPicker {
    /// tdesktop `LocationPicker::setup`: start at the last exact fix when
    /// there is one (and look for places and the address there), else at
    /// `fallback` (the system country) or Null Island.
    pub(super) fn open(
        chat_id: ChatId,
        last_exact: Option<GeoPoint>,
        fallback: GeoPoint,
        first_generation: u64,
    ) -> Self {
        let view = match last_exact {
            Some(point) => MapView::new(point, START_ZOOM),
            None => MapView::new(fallback, 0),
        };
        let mut picker = Self {
            chat_id,
            view,
            pin: Pin::Resting,
            map_size: None,
            here: HereState::Locating,
            address: Address::default(),
            venues: VenueSearch {
                list: VenueList::Loading,
                request: None,
                initial_query: String::new(),
                generation: first_generation,
                cache: Vec::new(),
                search_location: None,
                no_search_location: None,
                input: SearchInput::Closed,
            },
            tiles: Vec::new(),
        };
        if let Some(point) = last_exact {
            picker.request_venues(point, "");
            picker.resolve_address(point);
            picker.venues.search_location = Some(point);
        }
        picker
    }

    pub fn chat_id(&self) -> ChatId {
        self.chat_id
    }

    pub fn view(&self) -> MapView {
        self.view
    }

    pub fn is_moving(&self) -> bool {
        self.pin == Pin::Moving
    }

    pub fn here(&self) -> HereState {
        self.here
    }

    pub fn venues(&self) -> &VenueList {
        &self.venues.list
    }

    pub fn places_in_area_shown(&self) -> bool {
        self.pin == Pin::RestedAway
    }

    pub fn can_search(&self) -> bool {
        self.venues.search_location.is_some()
    }

    /// The search field is open (tdesktop `venuesSearchChanged` got a
    /// query, not `None`). The UI's field exists exactly while this holds.
    pub fn search_open(&self) -> bool {
        self.venues.input.query().is_some()
    }

    pub fn search_shown(&self) -> bool {
        self.venues.input.is_shown()
    }

    /// The search timer is armed; the UI fires [`Self::search_debounce_fired`].
    pub fn search_debounce_pending(&self) -> bool {
        matches!(self.venues.input, SearchInput::Typing(_))
    }

    pub fn address_line(&self) -> AddressLine<'_> {
        if let AddressStage::Shown { text, .. } = &self.address.stage
            && self.pin != Pin::Moving
        {
            AddressLine::Text(text)
        } else if self.address.ever_shown {
            AddressLine::Loading
        } else {
            AddressLine::Hidden
        }
    }

    /// The point "Send This Location" sends: the place under the pin.
    pub fn selected_point(&self) -> GeoPoint {
        self.view.center()
    }

    /// The UI's map area changed size (tiles are asked at this size).
    pub fn set_map_size(&mut self, size: MapSize) {
        if self.map_size != Some(size) {
            self.tiles
                .retain(|(_, state)| !matches!(state, TileState::Failed(_)));
        }
        self.map_size = Some(size);
    }

    pub fn map_size(&self) -> Option<MapSize> {
        self.map_size
    }

    /// tdesktop `move_start`: the pin lifts, the address hides (kept for a
    /// return to the same place), the venue button hides.
    pub fn move_started(&mut self) {
        if self.pin == Pin::Moving {
            return;
        }
        self.pin = Pin::Moving;
        // A lookup not started is dropped; one under way goes on, and a
        // shown address is kept (not drawn while the pin is lifted).
        if matches!(self.address.stage, AddressStage::Postponed(_)) {
            self.address.stage = AddressStage::Idle;
        }
    }

    /// The map moved to `view` (a drag or a zoom step); no request yet.
    pub fn moved_to(&mut self, view: MapView) {
        self.move_started();
        self.view = view;
    }

    /// tdesktop `move_end`, sent half a second after the map stops.
    pub fn move_ended(&mut self) {
        if self.pin != Pin::Moving {
            return;
        }
        self.pin = Pin::Resting;
        // A tile that failed is asked again once the map rests.
        self.tiles
            .retain(|(_, state)| !matches!(state, TileState::Failed(_)));
        let location = self.view.center();
        self.address.stage = match std::mem::take(&mut self.address.stage) {
            // The address, or a lookup already out, is for this place.
            kept @ (AddressStage::Shown { .. }
            | AddressStage::Wanted(_)
            | AddressStage::Resolving(_))
                if same_place(kept.point(), Some(location)) =>
            {
                kept
            }
            _ => AddressStage::Postponed(location),
        };
        if !same_place(
            self.venues.request.as_ref().map(|r| r.point),
            Some(location),
        ) {
            self.pin = Pin::RestedAway;
        }
        self.venues.search_location = Some(location);
    }

    /// The pin rested long enough: resolve its address.
    pub fn resolve_postponed(&mut self) {
        if let AddressStage::Postponed(point) = self.address.stage {
            self.address.stage = AddressStage::Idle;
            self.resolve_address(point);
        }
    }

    pub fn address_postponed(&self) -> bool {
        matches!(self.address.stage, AddressStage::Postponed(_))
    }

    fn resolve_address(&mut self, point: GeoPoint) {
        if same_place(self.address.stage.point(), Some(point)) {
            return;
        }
        self.address.stage = AddressStage::Wanted(point);
    }

    /// The place whose address the UI should look up now.
    pub fn take_address_job(&mut self) -> Option<GeoPoint> {
        let AddressStage::Wanted(point) = self.address.stage else {
            return None;
        };
        self.address.stage = AddressStage::Resolving(point);
        Some(point)
    }

    /// The geocoder answered for `point`; without a name the coordinates
    /// stand in (tdesktop `resolveAddress`'s fallback).
    pub fn address_resolved(&mut self, point: GeoPoint, name: Option<String>) {
        if !same_place(self.address.stage.point(), Some(point)) {
            return;
        }
        let text = name
            .map(|name| name.trim().to_string())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| point.coordinates_label());
        // An answer that lands while the pin is lifted is drawn on its return.
        self.address.stage = AddressStage::Shown { point, text };
        self.address.ever_shown = true;
    }

    /// A new "where am I" lookup began: the picker is locating again until
    /// [`Self::geolocated`] answers it.
    pub(super) fn locate_started(&mut self) {
        self.here = HereState::Locating;
    }

    /// tdesktop `resolveCurrentLocation`'s callback. A new exact fix flies
    /// the map there and looks around it; anything else leaves the list
    /// waiting for the user to choose a place.
    pub(super) fn geolocated(&mut self, outcome: GeoOutcome, changed: bool) {
        match outcome {
            GeoOutcome::Found(point) => {
                self.here = HereState::Found;
                if !changed {
                    self.wait_for_location();
                    return;
                }
                if self.venues.input.query().unwrap_or("").is_empty() {
                    self.request_venues(point, "");
                }
                self.resolve_address(point);
                self.moved_to(MapView::new(point, START_ZOOM));
                self.move_ended();
            }
            GeoOutcome::Denied => {
                self.here = HereState::Denied;
                self.wait_for_location();
            }
            GeoOutcome::Unavailable => {
                self.here = HereState::Unavailable;
                self.wait_for_location();
            }
        }
    }

    fn wait_for_location(&mut self) {
        if self.venues.search_location.is_none() {
            self.venues.list = VenueList::WaitingForLocation;
        }
    }

    pub fn search_places_in_area(&mut self) {
        if self.pin == Pin::RestedAway {
            self.pin = Pin::Resting;
        }
        self.request_venues(self.view.center(), "");
    }

    /// tdesktop `venuesSearchChanged`: `None` when the search field closed.
    pub fn search_changed(&mut self, query: Option<String>) {
        let shown = query.as_ref().is_some_and(|q| !q.trim().is_empty());
        if self.venues.input.is_shown() != shown {
            if shown {
                self.venues.no_search_location = self.venues.request.as_ref().map(|r| r.point);
            } else if let Some(point) = self.venues.no_search_location
                && !self.venues_from_cache(point, "")
            {
                self.request_venues(point, "");
            }
        }
        let text = query.clone().unwrap_or_default();
        let cached = match self.venues.search_location {
            Some(point) if shown => self.venues_from_cache(point, &text),
            _ => false,
        };
        self.venues.input = match query {
            None => SearchInput::Closed,
            Some(query) if !shown => SearchInput::Blank(query),
            Some(query) if cached => SearchInput::Settled(query),
            Some(query) => {
                self.venues.list = VenueList::Loading;
                SearchInput::Typing(query)
            }
        };
    }

    /// The search paused long enough: ask the bot.
    pub fn search_debounce_fired(&mut self) {
        let SearchInput::Typing(query) = std::mem::take(&mut self.venues.input) else {
            return;
        };
        self.venues.input = SearchInput::Settled(query.clone());
        if let Some(point) = self.venues.search_location {
            self.request_venues(point, &query);
        }
    }

    fn venues_from_cache(&mut self, point: GeoPoint, query: &str) -> bool {
        let normalized = normalize_query(query);
        let Some(list) = self
            .venues
            .cache
            .iter()
            .find(|(q, p, _)| *q == normalized && p.same_place(&point))
            .map(|(_, _, list)| list.clone())
        else {
            return false;
        };
        self.venues.request = Some(VenueAsk {
            point,
            query: normalized,
            stage: AskStage::Answered,
        });
        self.venues.initial_query = query.to_string();
        self.venues.generation += 1;
        self.apply_venues(list);
        true
    }

    fn request_venues(&mut self, point: GeoPoint, query: &str) {
        let normalized = normalize_query(query);
        // The same ask is not repeated while it is on its way, or when its
        // answer is on screen. After a failure, or when typing blanked the
        // list, it is asked again.
        if let Some(ask) = &self.venues.request
            && ask.point.same_place(&point)
            && ask.query == normalized
            && match ask.stage {
                AskStage::Unsent | AskStage::InFlight => true,
                AskStage::Answered => self.venues.list != VenueList::Loading,
                AskStage::Failed => false,
            }
        {
            return;
        }
        self.venues.generation += 1;
        self.venues.list = VenueList::Loading;
        self.venues.request = Some(VenueAsk {
            point,
            query: normalized,
            stage: AskStage::Unsent,
        });
        self.venues.initial_query = query.to_string();
    }

    /// The generation the next picker numbers its queries from, so a
    /// late answer of this one never matches there.
    pub(super) fn generation(&self) -> u64 {
        self.venues.generation
    }

    pub(super) fn venue_query_to_send(&self) -> Option<(u64, GeoPoint, String)> {
        let ask = self.venues.request.as_ref()?;
        (ask.stage == AskStage::Unsent)
            .then(|| (self.venues.generation, ask.point, ask.query.clone()))
    }

    pub(super) fn venue_query_sent(&mut self, generation: u64) {
        if generation == self.venues.generation
            && let Some(ask) = self.venues.request.as_mut()
            && ask.stage == AskStage::Unsent
        {
            ask.stage = AskStage::InFlight;
        }
    }

    pub(super) fn venues_answered(&mut self, generation: u64, venues: Vec<Venue>) {
        if generation != self.venues.generation {
            return;
        }
        if let Some(ask) = self.venues.request.as_mut() {
            ask.stage = AskStage::Answered;
            self.venues
                .cache
                .push((ask.query.clone(), ask.point, venues.clone()));
        }
        self.apply_venues(venues);
    }

    /// The lookup failed: the list says so and offers a retry.
    pub(super) fn venues_failed(&mut self, generation: u64) {
        if generation == self.venues.generation {
            if let Some(ask) = self.venues.request.as_mut() {
                ask.stage = AskStage::Failed;
            }
            self.venues.list = VenueList::Failed;
        }
    }

    /// There is no venue bot to ask: nothing to find, and nothing to retry.
    pub(super) fn venues_unavailable(&mut self, generation: u64) {
        if generation == self.venues.generation {
            if let Some(ask) = self.venues.request.as_mut() {
                ask.stage = AskStage::Answered;
            }
            self.apply_venues(Vec::new());
        }
    }

    /// The Retry under a failed list: ask for the same place and text again.
    pub fn retry_venues(&mut self) {
        if self.venues.list != VenueList::Failed {
            return;
        }
        let Some(point) = self.venues.request.as_ref().map(|ask| ask.point) else {
            return;
        };
        let query = self.venues.initial_query.clone();
        self.request_venues(point, &query);
    }

    fn apply_venues(&mut self, venues: Vec<Venue>) {
        self.venues.list = if venues.is_empty() {
            VenueList::NothingFound {
                query: self.venues.initial_query.clone(),
            }
        } else {
            VenueList::Found(venues)
        };
    }

    pub(super) fn tile_to_request(&self) -> Option<TileKey> {
        if self.pin == Pin::Moving {
            return None;
        }
        let key = TileKey::for_view(&self.view, self.map_size?);
        match self.tile_state(key) {
            None => Some(key),
            // A failed ask is repeated a few times, so a view the user
            // leaves alone does not stay blank after one lost answer.
            Some(TileState::Failed(failed)) if failed < TILE_TRIES => Some(key),
            Some(_) => None,
        }
    }

    fn tile_state(&self, key: TileKey) -> Option<TileState> {
        self.tiles
            .iter()
            .find(|(known, _)| *known == key)
            .map(|(_, state)| *state)
    }

    /// How many asks for `key` failed in a row so far.
    fn tile_failures(&self, key: TileKey) -> u8 {
        match self.tile_state(key) {
            Some(TileState::Failed(failed) | TileState::Requested { failed, .. }) => failed,
            Some(TileState::Ready(_)) | None => 0,
        }
    }

    pub(super) fn tile_requested(&mut self, key: TileKey, extra: u64) {
        let failed = self.tile_failures(key);
        self.tiles.retain(|(known, _)| *known != key);
        self.tiles
            .push((key, TileState::Requested { extra, failed }));
        while self.tiles.len() > TILE_CAP {
            let Some(index) = self
                .tiles
                .iter()
                .position(|(_, state)| !matches!(state, TileState::Requested { .. }))
            else {
                break;
            };
            self.tiles.remove(index);
        }
    }

    pub(super) fn tile_answered(&mut self, extra: u64, file_id: Option<FileId>) -> bool {
        for (_, state) in &mut self.tiles {
            if let TileState::Requested {
                extra: asked,
                failed,
            } = *state
                && asked == extra
            {
                *state = match file_id {
                    Some(file_id) => TileState::Ready(file_id),
                    None => TileState::Failed(failed.saturating_add(1)),
                };
                return true;
            }
        }
        false
    }

    /// The ask for `key` could not be sent: one more failed try, so a send
    /// that is refused every time is not repeated on every pass.
    pub(super) fn tile_unsent(&mut self, key: TileKey) {
        let failed = self.tile_failures(key).saturating_add(1);
        self.tiles.retain(|(known, _)| *known != key);
        self.tiles.push((key, TileState::Failed(failed)));
    }

    /// Tiles with a file, oldest first; the one for the current view last
    /// so it draws on top.
    pub fn tiles(&self) -> Vec<(TileKey, FileId)> {
        let current = self
            .map_size
            .map(|size| TileKey::for_view(&self.view, size));
        let mut tiles: Vec<(TileKey, FileId)> = self
            .tiles
            .iter()
            .filter_map(|(key, state)| match state {
                TileState::Ready(file_id) => Some((*key, *file_id)),
                _ => None,
            })
            .collect();
        tiles.sort_by_key(|(key, _)| (key.zoom == self.view.zoom(), Some(*key) == current));
        tiles
    }

    pub(super) fn listed_venue_types(&self) -> Vec<String> {
        let VenueList::Found(venues) = &self.venues.list else {
            return Vec::new();
        };
        let mut types: Vec<String> = Vec::new();
        for venue in venues {
            if !venue.venue_type.is_empty() && !types.contains(&venue.venue_type) {
                types.push(venue.venue_type.clone());
            }
        }
        types
    }
}

#[cfg(test)]
#[path = "picker_tests.rs"]
mod tests;
