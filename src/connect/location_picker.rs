//! Connect driver: Send Location's map picker (`crate::location_picker`).
//! The session decides what the picker needs; this sends it.
use super::*;
use crate::ids::{ChatId, RequestId};
use crate::location_picker::{GeoOutcome, GeoPoint, PickerRequest};
use crate::state::{MediaPurpose, RequestPurpose};
use crate::telegram::requests::{
    get_map_thumbnail_file, get_remote_thumbnail_file, get_venue_results, search_public_chat,
};

impl<S: JsonSender> ConnectDriver<S> {
    /// Open the picker for `chat_id`; `country` is where the map starts
    /// without a previous fix.
    pub fn open_location_picker(
        &mut self,
        chat_id: ChatId,
        country: Option<GeoPoint>,
    ) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.location.open(chat_id, country);
        self.pump_location_picker()
    }

    pub fn close_location_picker(&mut self) {
        self.session.location.close();
    }

    /// The platform's answer to "where am I".
    pub fn location_picker_geolocated(
        &mut self,
        outcome: GeoOutcome,
    ) -> Result<(), ConnectSendError> {
        self.session.location.geolocated(outcome);
        self.pump_location_picker()
    }

    /// Send what the open picker is waiting for: the venue bot lookup, the
    /// venue query, the tile for the resting map, venue icons, and the
    /// downloads of the files it shows. Runs after every ingest and after
    /// the UI changes the picker.
    pub fn pump_location_picker(&mut self) -> Result<(), ConnectSendError> {
        if self.session.location.picker().is_none() || !self.chats_path_active() {
            return Ok(());
        }
        for request in self.session.location.next_requests() {
            let chat_id = match &request {
                PickerRequest::Venues { chat_id, .. } | PickerRequest::MapTile { chat_id, .. } => {
                    Some(*chat_id)
                }
                _ => None,
            };
            let extra = self.session.request(
                RequestPurpose::Media(MediaPurpose::LocationPicker(request.purpose())),
                chat_id,
            );
            let json = picker_request_json(extra, &request);
            match self.sender.send_json(&json) {
                Ok(()) => self.session.location.request_sent(&request, extra.0),
                Err(err) => {
                    self.session.requests.take(extra);
                    self.session.location.request_unsent(&request);
                    return Err(err);
                }
            }
        }
        for file_id in self.session.location.files_to_download() {
            self.download_file(file_id, INLINE_RESULTS_DOWNLOAD_PRIORITY)?;
        }
        Ok(())
    }
}

fn picker_request_json(extra: RequestId, request: &PickerRequest) -> String {
    match request {
        PickerRequest::ResolveVenueBot { username } => search_public_chat(extra, username),
        PickerRequest::Venues {
            bot_user_id,
            chat_id,
            point,
            query,
            ..
        } => get_venue_results(extra, *bot_user_id, *chat_id, *point, query),
        PickerRequest::MapTile { key, chat_id } => {
            let center = key.request_center();
            get_map_thumbnail_file(
                extra,
                center.latitude(),
                center.longitude(),
                i32::from(key.zoom),
                i32::try_from(key.size.width).unwrap_or(i32::MAX),
                i32::try_from(key.size.height).unwrap_or(i32::MAX),
                i32::from(key.size.scale),
                *chat_id,
            )
        }
        PickerRequest::VenueIcon { url, .. } => get_remote_thumbnail_file(extra, url),
    }
}
