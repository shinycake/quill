//! Send Location (tdesktop `Ui::LocationPicker`): a map with a fixed pin,
//! "Send This Location" with the pin's address, and the venues around it
//! or found by the title-bar search. The picker's state lives in
//! `Session::location` (`quill::location_picker`); this module turns input
//! into its events, runs the timers and platform lookups, and sends.
//! See docs/decisions/location-picker.md.
#![deny(clippy::indexing_slicing)]

mod demo;
mod dialog;
mod geolocation;
mod map;
pub(super) mod state;

use super::app::QuillApp;
use crate::ui::status_note::StatusNote;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::lang::{keys, tr};
use quill::location_picker::{GeoOutcome, GeoPoint, LocationSharing, MapView, Venue, country};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::{Duration, Instant};

/// tdesktop's map `move_end` fires this long after the map stops.
const SETTLE_DELAY: Duration = Duration::from_millis(500);
/// tdesktop `kResolveAddressDelay`.
const RESOLVE_ADDRESS_DELAY: Duration = Duration::from_secs(3);
/// tdesktop `kSearchDebounceDelay`.
const SEARCH_DEBOUNCE: Duration = Duration::from_millis(900);
const POLL: Duration = Duration::from_millis(100);
const LOCATE_TIMEOUT: Duration = Duration::from_secs(30);
const GEOCODE_TIMEOUT: Duration = Duration::from_secs(15);

async fn poll_answer<T>(receiver: Receiver<T>, timeout: Duration, cx: &mut AsyncApp) -> Option<T> {
    let started = Instant::now();
    loop {
        match receiver.try_recv() {
            Ok(answer) => return Some(answer),
            Err(TryRecvError::Disconnected) => return None,
            Err(TryRecvError::Empty) if started.elapsed() >= timeout => return None,
            Err(TryRecvError::Empty) => cx.background_executor().timer(POLL).await,
        }
    }
}

impl QuillApp {
    fn location_sharing(&self) -> Option<&LocationSharing> {
        self.session().map(|session| &session.location)
    }

    fn location_sharing_mut(&mut self) -> Option<&mut LocationSharing> {
        if let Some(live) = self.live.as_mut() {
            Some(&mut live.driver.session.location)
        } else {
            self.demo_session
                .as_mut()
                .map(|session| &mut session.location)
        }
    }

    pub(in crate::ui) fn location_picker_open(&self) -> bool {
        self.location_sharing()
            .is_some_and(|sharing| sharing.picker().is_some())
    }

    pub(in crate::ui) fn open_location_picker(&mut self, cx: &mut Context<Self>) {
        let Some(chat_id) = self.share_target() else {
            self.connection.set_note(StatusNote::error(tr(
                keys::Q_OPS_YOU_CANT_SEND_LOCATIONS_HERE,
            )));
            cx.notify();
            return;
        };
        let country = geolocation::system_country()
            .or_else(|| {
                self.session()
                    .and_then(|session| session.auth_state.guessed_country_iso.clone())
            })
            .and_then(|iso| country::country_center(&iso));
        self.location_ui.reset();
        self.location_ui.map_focus = Some(cx.focus_handle().tab_stop(true));
        if let Some(live) = self.live.as_mut() {
            if live.driver.open_location_picker(chat_id, country).is_err() {
                self.connection.set_note(StatusNote::error(tr(
                    keys::Q_OPS_COULD_NOT_SEND_THE_LOCATION,
                )));
                cx.notify();
                return;
            }
            self.locate_for_picker(cx);
        } else if let Some(sharing) = self.location_sharing_mut() {
            sharing.open(chat_id, country);
            demo::start(self);
        }
        self.sync_location_picker(cx);
    }

    pub(in crate::ui) fn close_location_picker(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            live.driver.close_location_picker();
        } else if let Some(sharing) = self.location_sharing_mut() {
            sharing.close();
        }
        self.location_ui.reset();
        cx.notify();
    }

    fn locate_for_picker(&mut self, cx: &mut Context<Self>) {
        let receiver = geolocation::locate();
        self.location_ui.locate = Some(self.spawn_scoped(cx, async move |this, cx| {
            let outcome = poll_answer(receiver, LOCATE_TIMEOUT, cx)
                .await
                .unwrap_or(GeoOutcome::Unavailable);
            let _ = this.update(cx, |this, cx| this.location_picker_geolocated(outcome, cx));
        }));
    }

    /// One "where am I" lookup for a caller outside the picker (a bot's
    /// location request). `then` runs once with the answer; dropping the
    /// returned task cancels it. The screenshot demo answers from
    /// `QUILL_DEMO_LOCATION` instead of asking the system.
    pub(in crate::ui) fn locate_then(
        &mut self,
        cx: &mut Context<Self>,
        then: fn(&mut Self, GeoOutcome, &mut Context<Self>),
    ) -> Task<()> {
        if self.live.is_none() {
            let outcome = demo::locate_outcome();
            return self.spawn_scoped(cx, async move |this, cx| {
                if let Some(outcome) = outcome {
                    let _ = this.update(cx, |this, cx| then(this, outcome, cx));
                }
            });
        }
        let receiver = geolocation::locate();
        self.spawn_scoped(cx, async move |this, cx| {
            let outcome = poll_answer(receiver, LOCATE_TIMEOUT, cx)
                .await
                .unwrap_or(GeoOutcome::Unavailable);
            let _ = this.update(cx, |this, cx| then(this, outcome, cx));
        })
    }

    pub(in crate::ui) fn location_picker_locate_again(&mut self, cx: &mut Context<Self>) {
        if self.live.is_some() {
            self.locate_for_picker(cx);
        } else {
            demo::locate_again(self);
            self.sync_location_picker(cx);
        }
    }

    pub(in crate::ui) fn location_picker_geolocated(
        &mut self,
        outcome: GeoOutcome,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.location_picker_geolocated(outcome);
        } else if let Some(sharing) = self.location_sharing_mut() {
            sharing.geolocated(outcome);
        }
        self.sync_location_picker(cx);
    }

    /// After any change to the picker: send what it waits for, start the
    /// address lookup it asked for, and redraw.
    pub(in crate::ui) fn sync_location_picker(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.pump_location_picker();
        } else {
            demo::pump(self);
        }
        let job = self
            .location_sharing_mut()
            .and_then(|sharing| sharing.picker_mut())
            .and_then(|picker| picker.take_address_job());
        if let Some(point) = job {
            self.resolve_address(point, cx);
        }
        cx.notify();
    }

    fn resolve_address(&mut self, point: GeoPoint, cx: &mut Context<Self>) {
        let lookup = if self.live.is_some() {
            geolocation::reverse_geocode(point)
        } else {
            None
        };
        let Some(receiver) = lookup else {
            if let Some(picker) = self
                .location_sharing_mut()
                .and_then(|sharing| sharing.picker_mut())
            {
                picker.address_resolved(point, None);
            }
            return;
        };
        self.location_ui.geocode = Some(self.spawn_scoped(cx, async move |this, cx| {
            let address = poll_answer(receiver, GEOCODE_TIMEOUT, cx).await.flatten();
            let _ = this.update(cx, |this, cx| {
                if let Some(picker) = this
                    .location_sharing_mut()
                    .and_then(|sharing| sharing.picker_mut())
                {
                    picker.address_resolved(point, address);
                }
                cx.notify();
            });
        }));
    }

    /// The map moved to `view` (a drag step or a zoom step); it settles
    /// half a second after the last one.
    pub(in crate::ui) fn location_map_moved(&mut self, view: MapView, cx: &mut Context<Self>) {
        if let Some(picker) = self
            .location_sharing_mut()
            .and_then(|sharing| sharing.picker_mut())
        {
            picker.moved_to(view);
        }
        self.location_ui.resolve = None;
        self.location_ui.settle = Some(self.spawn_scoped(cx, async move |this, cx| {
            cx.background_executor().timer(SETTLE_DELAY).await;
            let _ = this.update(cx, |this, cx| this.location_map_settled(cx));
        }));
        cx.notify();
    }

    fn location_map_settled(&mut self, cx: &mut Context<Self>) {
        if self.location_ui.drag.is_some() {
            return;
        }
        let postponed = self
            .location_sharing_mut()
            .and_then(|sharing| sharing.picker_mut())
            .map(|picker| {
                picker.move_ended();
                picker.address_postponed()
            })
            .unwrap_or(false);
        if postponed {
            self.location_ui.resolve = Some(self.spawn_scoped(cx, async move |this, cx| {
                cx.background_executor().timer(RESOLVE_ADDRESS_DELAY).await;
                let _ = this.update(cx, |this, cx| {
                    if let Some(picker) = this
                        .location_sharing_mut()
                        .and_then(|sharing| sharing.picker_mut())
                    {
                        picker.resolve_postponed();
                    }
                    this.sync_location_picker(cx);
                });
            }));
        }
        self.sync_location_picker(cx);
    }

    pub(in crate::ui) fn location_map_measured(
        &mut self,
        size: quill::location_picker::MapSize,
        cx: &mut Context<Self>,
    ) {
        let changed = self
            .location_sharing_mut()
            .and_then(|sharing| sharing.picker_mut())
            .map(|picker| {
                let changed = picker.map_size() != Some(size);
                picker.set_map_size(size);
                changed
            })
            .unwrap_or(false);
        if changed {
            self.sync_location_picker(cx);
        }
    }

    pub(in crate::ui) fn location_places_in_area(&mut self, cx: &mut Context<Self>) {
        if let Some(picker) = self
            .location_sharing_mut()
            .and_then(|sharing| sharing.picker_mut())
        {
            picker.search_places_in_area();
        }
        self.sync_location_picker(cx);
    }

    pub(in crate::ui) fn location_search_changed(
        &mut self,
        query: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let pending = self
            .location_sharing_mut()
            .and_then(|sharing| sharing.picker_mut())
            .map(|picker| {
                picker.search_changed(query);
                picker.search_debounce_pending()
            })
            .unwrap_or(false);
        self.location_ui.search_timer = pending.then(|| {
            self.spawn_scoped(cx, async move |this, cx| {
                cx.background_executor().timer(SEARCH_DEBOUNCE).await;
                let _ = this.update(cx, |this, cx| {
                    if let Some(picker) = this
                        .location_sharing_mut()
                        .and_then(|sharing| sharing.picker_mut())
                    {
                        picker.search_debounce_fired();
                    }
                    this.sync_location_picker(cx);
                });
            })
        });
        self.sync_location_picker(cx);
    }

    pub(in crate::ui) fn send_picked_location(&mut self, cx: &mut Context<Self>) {
        let Some(point) = self
            .location_sharing()
            .and_then(|sharing| sharing.picker())
            .map(|picker| picker.selected_point())
        else {
            return;
        };
        self.send_from_picker(cx, |live, chat_id, reply_to, options| {
            live.driver.share_location_to_chat(
                chat_id,
                point.latitude(),
                point.longitude(),
                reply_to,
                options,
            )
        });
    }

    pub(in crate::ui) fn send_picked_venue(&mut self, venue: Venue, cx: &mut Context<Self>) {
        self.send_from_picker(cx, |live, chat_id, reply_to, options| {
            live.driver
                .share_venue_to_chat(chat_id, &venue, reply_to, options)
        });
    }

    fn send_from_picker(
        &mut self,
        cx: &mut Context<Self>,
        send: impl FnOnce(
            &mut quill::connect::LiveConnect,
            ChatId,
            Option<quill::telegram::requests::SendReply>,
            &quill::composer::SendOptions,
        ) -> Result<quill::ids::RequestId, quill::connect::ConnectSendError>,
    ) {
        let Some(chat_id) = self
            .location_sharing()
            .and_then(|sharing| sharing.picker())
            .map(|picker| picker.chat_id())
        else {
            return;
        };
        if self.slow_mode_blocked(chat_id, cx) {
            return;
        }
        let reply_to = self.send_reply_for(chat_id, cx);
        let options = self.composer_send_options();
        let Some(live) = self.live.as_mut() else {
            self.close_location_picker(cx);
            self.connection.set_note(StatusNote::info(tr(
                keys::Q_OPS_SHARING_NEEDS_A_LIVE_CONNECTION,
            )));
            cx.notify();
            return;
        };
        match send(live, chat_id, reply_to, &options) {
            Ok(_) => {
                self.close_location_picker(cx);
                self.composer_ui.set_reply(None);
                self.connection
                    .set_note(StatusNote::silent(tr(keys::Q_OPS_SENDING_LOCATION)));
            }
            Err(_) => self.connection.set_note(StatusNote::error(tr(
                keys::Q_OPS_COULD_NOT_SEND_THE_LOCATION,
            ))),
        }
        cx.notify();
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "tests.rs"]
mod tests;

crate::ui::shell::register_dialogs! {
    LocationPicker => DialogSpec::new(
        3795,
        |app| app.location_picker_open(),
        QuillApp::build_location_picker_dialog,
    ),
}
