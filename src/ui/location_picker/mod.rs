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
use crate::ui::shell::DialogKind;
use crate::ui::status_note::StatusNote;
use geolocation::{PromptGate, Receiver};
use gpui_kit::*;
use quill::ids::ChatId;
use quill::lang::{keys, tr};
use quill::location_picker::{GeoOutcome, GeoPoint, LocationSharing, MapView, Venue, country};
use std::future::Future;
use std::pin::Pin;
use std::task::Poll;
use std::time::Duration;

/// tdesktop's map `move_end` fires this long after the map stops.
const SETTLE_DELAY: Duration = Duration::from_millis(500);
/// tdesktop `kResolveAddressDelay`.
const RESOLVE_ADDRESS_DELAY: Duration = Duration::from_secs(3);
/// tdesktop `kSearchDebounceDelay`.
const SEARCH_DEBOUNCE: Duration = Duration::from_millis(900);
const LOCATE_TIMEOUT: Duration = Duration::from_secs(30);
const GEOCODE_TIMEOUT: Duration = Duration::from_secs(15);

async fn poll_answer<T>(
    mut receiver: Receiver<T>,
    timeout: Duration,
    cx: &mut AsyncApp,
) -> Option<T> {
    match race_answer(&mut receiver, timeout, None, cx).await {
        Raced::Answered(answer) => answer,
        Raced::TimedOut | Raced::PromptClosed => None,
    }
}

/// The wait for a lookup's answer: the answer, the end of `timeout`, or the
/// permission prompt closing.
enum Raced<T> {
    Answered(Option<T>),
    TimedOut,
    PromptClosed,
}

/// Wait for the answer, which wakes this task when it comes, against one
/// timer of `timeout` and, when there is a `prompt`, its closing (which
/// wakes this task too).
async fn race_answer<T>(
    receiver: &mut Receiver<T>,
    timeout: Duration,
    prompt: Option<&PromptGate>,
    cx: &mut AsyncApp,
) -> Raced<T> {
    let mut timer = cx.background_executor().timer(timeout);
    let seen = prompt.map_or(0, PromptGate::closed);
    std::future::poll_fn(|task| {
        if let Poll::Ready(answer) = Pin::new(&mut *receiver).poll(task) {
            return Poll::Ready(Raced::Answered(answer.ok()));
        }
        if prompt.is_some_and(|gate| gate.poll_closed(seen, task).is_ready()) {
            return Poll::Ready(Raced::PromptClosed);
        }
        if Pin::new(&mut timer).poll(task).is_ready() {
            return Poll::Ready(Raced::TimedOut);
        }
        Poll::Pending
    })
    .await
}

/// [`poll_answer`] whose time limit does not run while the system's
/// permission prompt is open: the limit starts in full again when the prompt
/// closes, and one that ends while the prompt is still open starts again,
/// so the user's turn is not counted.
async fn poll_answer_unless<T>(
    receiver: &mut Receiver<T>,
    timeout: Duration,
    prompt: &PromptGate,
    cx: &mut AsyncApp,
) -> Option<T> {
    loop {
        match race_answer(receiver, timeout, Some(prompt), cx).await {
            Raced::Answered(answer) => return answer,
            Raced::PromptClosed => {}
            Raced::TimedOut if prompt.is_open() => {}
            Raced::TimedOut => return None,
        }
    }
}

/// Wait for a "where am I" answer; `Unavailable` when it never comes.
async fn locate_outcome(mut locating: geolocation::Locating, cx: &mut AsyncApp) -> GeoOutcome {
    let prompt = locating.permission_prompt.clone();
    poll_answer_unless(&mut locating.answer, LOCATE_TIMEOUT, &prompt, cx)
        .await
        .unwrap_or(GeoOutcome::Unavailable)
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
            self.set_note(StatusNote::error(tr(
                keys::Q_OPS_YOU_CANT_SEND_LOCATIONS_HERE,
            )));
            cx.notify();
            return;
        };
        let country = geolocation::system_country()
            .or_else(|| {
                self.session().and_then(|session| {
                    session.auth_state.guessed_country_iso().map(str::to_string)
                })
            })
            .and_then(|iso| country::country_center(&iso));
        // One picker per account: opened from another window, it moves
        // there (its host is the window that asked, `open_box`), starts
        // over for this chat, and its tasks and search field are this
        // window's. The window it showed in closes it.
        self.location_ui.reset();
        self.open_box(DialogKind::LocationPicker);
        self.location_ui.map_focus = Some(cx.focus_handle().tab_stop(true));
        if let Some(live) = self.live.as_mut() {
            if live.driver.open_location_picker(chat_id, country).is_err() {
                self.set_note(StatusNote::error(tr(
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

    /// `chat`'s window closed: a picker it shows goes with it, instead of
    /// coming back in the main window for a chat it does not show, with
    /// its locate and search tasks gone.
    pub(in crate::ui) fn close_location_picker_of_window(
        &mut self,
        chat: quill::ids::ChatId,
        cx: &mut Context<Self>,
    ) {
        if self.location_picker_open()
            && self.windows.window_of_box(DialogKind::LocationPicker) == Some(chat)
        {
            self.close_location_picker(cx);
        }
    }

    fn locate_for_picker(&mut self, cx: &mut Context<Self>) {
        let locating = geolocation::locate();
        if let Some(sharing) = self.location_sharing_mut() {
            sharing.locate_started();
        }
        self.location_ui.locate =
            state::Locate::locating(self.spawn_scoped(cx, async move |this, cx| {
                let outcome = locate_outcome(locating, cx).await;
                let _ = this.update(cx, |this, cx| {
                    this.location_ui.locate = state::Locate::Idle;
                    this.location_picker_geolocated(outcome, cx)
                });
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
        let locating = geolocation::locate();
        self.spawn_scoped(cx, async move |this, cx| {
            let outcome = locate_outcome(locating, cx).await;
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
            .and_then(|picker| picker.address_job());
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
        let task = self.spawn_scoped(cx, async move |this, cx| {
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
        });
        if let Some(picker) = self
            .location_sharing_mut()
            .and_then(|sharing| sharing.picker_mut())
        {
            picker.address_started(point, quill::task_hold::TaskHold::new(task));
        }
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
        self.location_ui.map_phase =
            state::MapPhase::settling(self.spawn_scoped(cx, async move |this, cx| {
                cx.background_executor().timer(SETTLE_DELAY).await;
                let _ = this.update(cx, |this, cx| this.location_map_settled(cx));
            }));
        cx.notify();
    }

    fn location_map_settled(&mut self, cx: &mut Context<Self>) {
        self.location_ui.map_phase = state::MapPhase::Idle;
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
            self.location_ui.map_phase =
                state::MapPhase::resolving(self.spawn_scoped(cx, async move |this, cx| {
                    cx.background_executor().timer(RESOLVE_ADDRESS_DELAY).await;
                    let _ = this.update(cx, |this, cx| {
                        this.location_ui.map_phase = state::MapPhase::Idle;
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

    pub(in crate::ui) fn location_retry_venues(&mut self, cx: &mut Context<Self>) {
        if let Some(sharing) = self.location_sharing_mut() {
            sharing.retry_venues();
        }
        self.sync_location_picker(cx);
    }

    fn location_search_changed(&mut self, edit: state::SearchEdit, cx: &mut Context<Self>) {
        // The field is open exactly while the picker holds a query: this
        // one transition opens, edits and closes both, and an edit with no
        // field open reaches neither.
        let query = match edit {
            state::SearchEdit::Open(input) => {
                self.location_ui.search = state::Search::Open {
                    input,
                    debounce: state::Wait::Idle,
                };
                Some(String::new())
            }
            state::SearchEdit::Text(text) => {
                if self.location_ui.search.input().is_none() {
                    return;
                }
                Some(text)
            }
            state::SearchEdit::Close => {
                self.location_ui.search = state::Search::Closed;
                None
            }
        };
        let pending = self
            .location_sharing_mut()
            .and_then(|sharing| sharing.picker_mut())
            .map(|picker| {
                picker.search_changed(query);
                picker.search_debounce_pending()
            })
            .unwrap_or(false);
        let waiting = pending.then(|| {
            self.spawn_scoped(cx, async move |this, cx| {
                cx.background_executor().timer(SEARCH_DEBOUNCE).await;
                let _ = this.update(cx, |this, cx| {
                    if let state::Search::Open { debounce, .. } = &mut this.location_ui.search {
                        *debounce = state::Wait::Idle;
                    }
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
        if let state::Search::Open { debounce, .. } = &mut self.location_ui.search {
            *debounce = waiting.map_or(state::Wait::Idle, state::Wait::waiting);
        }
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
        let options = self.send_options_to(chat_id);
        let Some(live) = self.live.as_mut() else {
            self.close_location_picker(cx);
            self.set_note(StatusNote::info(tr(
                keys::Q_OPS_SHARING_NEEDS_A_LIVE_CONNECTION,
            )));
            cx.notify();
            return;
        };
        match send(live, chat_id, reply_to, &options) {
            Ok(_) => {
                self.close_location_picker(cx);
                self.chat_ui.set_reply(None);
                self.set_note(StatusNote::silent(tr(keys::Q_OPS_SENDING_LOCATION)));
            }
            Err(_) => self.set_note(StatusNote::error(tr(
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
    )
    .close(|app, _, cx| app.close_location_picker(cx)),
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
mod wait_tests;
