//! 1:1 call overlay rendering.

use super::app::QuillApp;
use super::chat_row::initials_avatar;
use super::*;
use gpui_kit::component::button::*;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::radio::{Radio, RadioGroup};
use gpui_kit::component::tag::Tag;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::state::{ActiveCall, CallSummary};
use quill::telegram::envelope::CallState;
use std::sync::Arc;
impl QuillApp {
    /// Phase C1: call overlay — incoming / outgoing / active / ended
    /// call UI above everything else. **Signaling only**: when a call
    /// would need media, the card says so honestly (audio transport is
    /// the C2 libtgvoip spike, not faked here).
    pub(super) fn call_overlay(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let session = self.session()?;
        // The call itself lives in the call window; the main window only
        // asks for a rating afterwards, or reports a failure to start.
        let rating = session
            .calls
            .summary
            .as_ref()
            .is_some_and(|summary| summary.need_rating && !summary.rating_sent)
            && session.calls.active_call.is_none();
        let failed = session.calls.error.is_some() && session.calls.active_call.is_none();
        if !rating && !failed {
            return None;
        }
        Some(self.call_card(cx).into_any_element())
    }

    /// Phase C1: format a call clock as `m:ss`.
    pub(super) fn call_clock(secs: u64) -> String {
        format!("{}:{:02}", secs / 60, secs % 60)
    }

    pub(super) fn call_peer_name(&self, user_id: i64) -> String {
        self.session()
            .and_then(|s| s.user(user_id))
            .map(|u| u.display_name())
            .unwrap_or_else(|| format!("User {user_id}"))
    }

    pub(super) fn call_card(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let session = self.session();
        let active = session.and_then(|s| s.calls.active_call.clone());
        let summary = session.and_then(|s| s.calls.summary.clone());
        let error = session.and_then(|s| s.calls.error.clone());

        let mut card = div()
            .id("call-card")
            .flex()
            .flex_col()
            .items_center()
            .gap_3()
            .p_6()
            .w(px(360.))
            .rounded_lg()
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().sidebar);

        if let Some(call) = active {
            card = self.call_active_card(card, &call, error.as_deref(), cx);
        } else if let Some(summary) = summary {
            card = self.call_summary_card(card, &summary, cx);
        } else if let Some(error) = error {
            card =
                card.child(div().text_sm().font_semibold().child("Call failed"))
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(error),
                    )
                    .child(Button::new("call-error-dismiss").label("Dismiss").on_click(
                        cx.listener(|this, _, _, cx| {
                            this.dismiss_call_error(cx);
                        }),
                    ));
        }

        div()
            .id("call-overlay")
            .occlude()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .bottom_0()
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .id("call-backdrop")
                    .occlude()
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .bottom_0()
                    .bg(scrim()),
            )
            .child(card)
    }

    /// Phase C2e: the video stage for a connected video call — the peer's
    /// camera as the main tile, the local preview as a 160x120 PiP
    /// anchored bottom-right. Phase C2l: while the peer shares their
    /// screen the share takes the main tile (with a badge), same
    /// preference as the group tiles. Frames come from the driver
    /// (live) or the demo fixture (screenshot mode); both tiles
    /// degrade to honest status text when a feed is missing (a paused
    /// peer share states itself rather than freezing), and decoded
    /// tiles are cached by (frame seq, is_screen) so re-renders don't
    /// re-decode.
    pub(super) fn call_video_stage(&mut self, call: &ActiveCall, name: &str) -> Div {
        let (remote_frame, local_frame): (
            Option<quill::calls::engine::VideoFrame>,
            Option<quill::calls::engine::VideoFrame>,
        ) = if let Some(live) = self.live.as_ref() {
            (
                live.driver.latest_video_frame(call.id, false),
                live.driver.latest_video_frame(call.id, true),
            )
        } else {
            (
                self.demo_ui.remote_frame.clone(),
                self.demo_ui.local_frame.clone(),
            )
        };
        // Phase C2l: the peer's 1:1 screen share. The driver drops the
        // retained frames when the share goes inactive, and
        // `remote_screen` closes the late-frame race — the tile shows
        // only while the share is not inactive.
        let screen_frame: Option<quill::calls::engine::VideoFrame> =
            if call.remote_screen == quill::calls::engine::RemoteVideoState::Inactive {
                None
            } else if let Some(live) = self.live.as_ref() {
                live.driver.latest_screen_frame(call.id)
            } else {
                self.demo_ui.screen_frame.clone()
            };

        let status_text = |text: &str| {
            div()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap_1()
                .child(div().text_2xl().child("📹"))
                .child(
                    div()
                        .text_xs()
                        .text_color(text_faint())
                        .child(text.to_string()),
                )
        };
        let video_img = |image: Arc<RenderImage>| {
            img(ImageSource::from(image))
                .w_full()
                .h_full()
                .object_fit(ObjectFit::Contain)
        };

        let remote: AnyElement = match (
            screen_frame.as_ref(),
            call.remote_video,
            remote_frame.as_ref(),
        ) {
            // Phase C2l: a paused peer share states itself instead of
            // rendering — same honest-state rule as the camera arm; a
            // stale frozen frame would mislead.
            (Some(_), _, _)
                if call.remote_screen == quill::calls::engine::RemoteVideoState::Paused =>
            {
                status_text("Screen share paused by peer").into_any_element()
            }
            // Phase C2l: the peer's screen share takes the main tile
            // while the share is Active (same preference as the group
            // tiles); the camera resumes the tile when the share ends.
            (Some(frame), _, _) => match self.cached_video_image(false, frame) {
                Some(image) => div()
                    .w_full()
                    .h_full()
                    .relative()
                    .child(video_img(image))
                    .child(
                        div()
                            .absolute()
                            .top_2()
                            .left_2()
                            .child(Tag::info().child("🖥 Peer's screen")),
                    )
                    .into_any_element(),
                None => status_text("Couldn't decode the peer's screen").into_any_element(),
            },
            (None, quill::calls::engine::RemoteVideoState::Active, Some(frame)) => {
                match self.cached_video_image(false, frame) {
                    Some(image) => video_img(image).into_any_element(),
                    None => status_text("Couldn't decode the peer's video").into_any_element(),
                }
            }
            (None, quill::calls::engine::RemoteVideoState::Active, None) => {
                status_text("Connecting video…").into_any_element()
            }
            (None, quill::calls::engine::RemoteVideoState::Paused, _) => {
                status_text("Video paused by peer").into_any_element()
            }
            (None, quill::calls::engine::RemoteVideoState::Inactive, _) => div()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap_1()
                .child(initials_avatar(name, 48.))
                .child(
                    div()
                        .text_xs()
                        .font_semibold()
                        .text_color(text_on_fill())
                        .child(name.to_string()),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(text_faint())
                        .child("Peer's camera is off"),
                )
                .into_any_element(),
        };

        let camera_on = self.call_camera_effective(call);
        let local: AnyElement = match (camera_on, local_frame.as_ref()) {
            (true, Some(frame)) => match self.cached_video_image(true, frame) {
                Some(image) => video_img(image).into_any_element(),
                None => status_text("Couldn't decode the camera preview").into_any_element(),
            },
            (true, None) => status_text("Starting camera…").into_any_element(),
            // Phase C2i: screen share replaces the camera track, so the
            // local preview honestly says what's being sent.
            (false, _) => status_text(if call.screen_sharing {
                "Sharing screen"
            } else {
                "Camera off"
            })
            .into_any_element(),
        };

        div()
            .w_full()
            .relative()
            .child(
                div()
                    .w_full()
                    .h(px(240.))
                    .rounded_md()
                    .bg(bg_video())
                    .overflow_hidden()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(remote),
            )
            .child(
                div()
                    .absolute()
                    .bottom_2()
                    .right_2()
                    .w(px(160.))
                    .h(px(120.))
                    .rounded_md()
                    .bg(bg_video())
                    .overflow_hidden()
                    .border_1()
                    .border_color(border_video())
                    .child(local),
            )
    }

    pub(super) fn call_active_card(
        &mut self,
        card: Stateful<Div>,
        call: &ActiveCall,
        error: Option<&str>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let name = self.call_peer_name(call.user_id);
        let kind_line = if call.is_video {
            "📹 Video call"
        } else {
            "Voice call"
        };
        let (status, clock): (String, Option<String>) = match &call.state {
            CallState::Pending { .. } => {
                let elapsed = call.started_at.elapsed().as_secs();
                if call.is_outgoing {
                    ("Calling…".to_string(), Some(Self::call_clock(elapsed)))
                } else {
                    ("Incoming call".to_string(), Some(Self::call_clock(elapsed)))
                }
            }
            CallState::ExchangingKeys => ("Connecting…".to_string(), None),
            CallState::Ready => {
                use quill::calls::engine::TransportState;
                let status = match call.transport {
                    Some(TransportState::Connected) if call.muted => {
                        "Muted - microphone off".to_string()
                    }
                    Some(TransportState::Connected) => "Connected".to_string(),
                    Some(TransportState::Reconnecting) => "Reconnecting audio…".to_string(),
                    Some(TransportState::Failed) => format!(
                        "Couldn't start audio: {}. The call is up but carries no sound.",
                        call.transport_error.as_deref().unwrap_or("unknown error")
                    ),
                    Some(TransportState::Closed) => "Audio disconnected.".to_string(),
                    None | Some(TransportState::Connecting) => "Connecting audio...".to_string(),
                };
                (
                    status,
                    Some(Self::call_clock(call.connected_secs().max(0) as u64)),
                )
            }
            CallState::HangingUp => ("Hanging up…".to_string(), None),
            // A future state the pinned schema doesn't know: label it
            // honestly instead of pretending it means something else.
            CallState::Unknown(type_name) => (format!("Call state: {type_name}"), None),
            // Terminal states end the call in the reducer, so this arm
            // is unreachable — but never crash the overlay on it.
            CallState::Discarded { .. } | CallState::Error { .. } => ("Ending…".to_string(), None),
        };
        let mut card = card;
        // Phase C2e: a connected video call shows the video stage
        // (peer's camera as the main tile, local preview as a PiP);
        // every other state keeps the avatar.
        if call.is_video && matches!(call.state, CallState::Ready) {
            card = card.child(self.call_video_stage(call, &name));
        } else {
            card = card.child(initials_avatar(&name, 72.));
        }
        card = card
            .child(div().text_lg().font_semibold().child(name))
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(kind_line),
            )
            .child(div().text_sm().child(status));
        if let Some(clock) = clock {
            card = card.child(div().text_2xl().font_semibold().child(clock));
        }
        // E2E verification emojis, straight from `callStateReady`
        // (:7068) — same rendering as the group-call card. Hidden
        // until TDLib sends the 4-emoji fingerprint.
        if let Some(ready) = &call.ready
            && !ready.emojis.is_empty()
        {
            card = card.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("End-to-end verification:"),
                    )
                    .child(div().text_2xl().child(ready.emojis.join(" "))),
            );
        }
        // Honest no-transport note: the call can be "Connected" at the
        // signaling level while carrying no audio or video. Never fake
        // a live call. Every pre-connected card carries it — incoming
        // ringing, outgoing "Calling…", connecting — and the end screen
        // repeats the note below; accepting/placing starts no media in
        // this build.
        let no_transport_note = matches!(
            call.state,
            CallState::Ready | CallState::ExchangingKeys | CallState::Pending { .. }
        );
        if no_transport_note {
            // Phase C2e: video is real once the driver is ready (engine
            // + camera); otherwise say exactly what's missing instead of
            // the old "ships in a later slice" note.
            let video_ready = self
                .live
                .as_ref()
                .is_some_and(|live| live.driver.call_video_ready())
                || self.live.is_none();
            let note = if call.is_video {
                if video_ready {
                    "Video connecting — tiles fill in as cameras stream."
                } else {
                    "Video unavailable — no camera found."
                }
            } else if call.transport == Some(quill::calls::engine::TransportState::Connected) {
                "Audio connected"
            } else {
                "Waiting for audio transport"
            };
            card = card.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(note),
            );
        }
        // Phase C1b: local-only mute state. Tracked on the call but a
        // no-op without media transport — labeled honestly.
        let show_mute_note =
            call.muted && matches!(call.state, CallState::Ready | CallState::Unknown(_));
        if show_mute_note {
            card = card.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("Microphone muted"),
            );
        }
        // Phase C2i: local screen-share send state — the engine sends
        // the desktop track instead of the camera (ntgcalls no-mix
        // rule); the flag is the tracked toggle intent.
        let show_screen_share_note =
            call.screen_sharing && matches!(call.state, CallState::Ready | CallState::Unknown(_));
        if show_screen_share_note {
            card = card.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("Sharing your screen"),
            );
        }
        if let Some(error) = error {
            card = card.child(
                div()
                    .text_xs()
                    .text_color(danger_soft())
                    .child(error.to_string()),
            );
        }
        if let Some(error) = &call.transport_error {
            card = card.child(
                div()
                    .text_xs()
                    .text_color(danger_soft())
                    .child(error.clone()),
            );
        }
        if matches!(call.state, CallState::Ready) {
            // Phase C2c: microphone/speaker pickers. Live state comes
            // from the driver; the screenshot demo injects demo devices
            // only (no live Telegram, no real hardware). Empty means the
            // engine reported none — never fabricated.
            let no_engine = self
                .live
                .as_ref()
                .is_some_and(|live| !live.driver.has_call_engine());
            let (devices, selected): (
                Vec<quill::calls::engine::MediaDevice>,
                (Option<String>, Option<String>),
            ) = if let Some(live) = self.live.as_ref() {
                let (microphone, speaker) = live.driver.selected_call_devices();
                (
                    live.driver.call_devices().to_vec(),
                    (microphone.map(str::to_owned), speaker.map(str::to_owned)),
                )
            } else {
                (
                    self.demo_ui.call_devices.clone().unwrap_or_default(),
                    self.demo_ui.selected_devices.clone(),
                )
            };
            if no_engine {
                card = card.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("No audio - call engine unavailable."),
                );
            } else if devices.is_empty() {
                card = card.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("No audio devices found - the engine reported none."),
                );
            } else {
                let mut pickers = div().w_full().flex().flex_col().gap_1();
                for (kind, label, selected) in [
                    (
                        quill::calls::engine::MediaDeviceKind::Microphone,
                        "Microphone",
                        selected.0.as_deref(),
                    ),
                    (
                        quill::calls::engine::MediaDeviceKind::Speaker,
                        "Speaker",
                        selected.1.as_deref(),
                    ),
                ] {
                    pickers =
                        pickers.child(div().text_xs().font_semibold().child(label.to_string()));
                    // Phase 6: kit RadioGroup (was: buttons with a ●/○
                    // prefix). Controlled: the chosen index writes the value.
                    let choices: Vec<(String, String)> = devices
                        .iter()
                        .filter(|device| device.kind == kind)
                        .map(|device| (device.id.clone(), device.name.clone()))
                        .collect();
                    let active = choices
                        .iter()
                        .position(|(id, _)| Some(id.as_str()) == selected);
                    pickers = pickers.child(
                        RadioGroup::vertical(format!("call-device-{label}"))
                            .selected_index(active)
                            .children(choices.iter().map(|(id, name)| {
                                Radio::new(format!("call-device-{label}-{id}")).label(name.clone())
                            }))
                            .on_click(cx.listener(move |this, &ix: &usize, _, cx| {
                                this.select_call_device(kind, &choices[ix].0, cx);
                            })),
                    );
                }
                // Phase C2e: camera picker, video calls only. When the
                // engine reported other devices but no camera, the row
                // says so honestly instead of vanishing.
                if call.is_video {
                    let camera_selected: Option<&str> = if let Some(live) = self.live.as_ref() {
                        live.driver.selected_call_camera()
                    } else {
                        self.demo_ui.selected_camera.as_deref()
                    };
                    pickers =
                        pickers.child(div().text_xs().font_semibold().child("Camera".to_string()));
                    // Phase 6: kit RadioGroup (was: buttons with a ●/○
                    // prefix). Controlled: the chosen index writes the value.
                    let camera_choices: Vec<(String, String)> = devices
                        .iter()
                        .filter(|device| {
                            device.kind == quill::calls::engine::MediaDeviceKind::Camera
                        })
                        .map(|device| (device.id.clone(), device.name.clone()))
                        .collect();
                    if camera_choices.is_empty() {
                        pickers = pickers.child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child("No camera found."),
                        );
                    } else {
                        let camera_active = camera_choices
                            .iter()
                            .position(|(id, _)| Some(id.as_str()) == camera_selected);
                        pickers = pickers.child(
                            RadioGroup::vertical("call-device-Camera")
                                .selected_index(camera_active)
                                .children(camera_choices.iter().map(|(id, name)| {
                                    Radio::new(format!("call-device-Camera-{id}"))
                                        .label(name.clone())
                                }))
                                .on_click(cx.listener(move |this, &ix: &usize, _, cx| {
                                    this.select_call_device(
                                        quill::calls::engine::MediaDeviceKind::Camera,
                                        &camera_choices[ix].0,
                                        cx,
                                    );
                                })),
                        );
                    }
                }
                card = card.child(pickers);
            }
        }
        let mut buttons = div().flex().gap_2();
        match &call.state {
            CallState::Pending { .. } if !call.is_outgoing => {
                buttons = buttons
                    .child(
                        Button::new("call-accept")
                            .label("Accept")
                            .success()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.accept_incoming_call(cx);
                            })),
                    )
                    .child(
                        Button::new("call-decline")
                            .label("Decline")
                            .danger()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.hang_up_call(cx);
                            })),
                    );
            }
            CallState::Pending { .. } | CallState::ExchangingKeys => {
                buttons =
                    buttons.child(Button::new("call-cancel").label("Cancel").ghost().on_click(
                        cx.listener(|this, _, _, cx| {
                            this.hang_up_call(cx);
                        }),
                    ));
            }
            CallState::Ready | CallState::Unknown(_) => {
                // Phase C1b: mute toggle — local-only state, no-op
                // without media transport (the card says so above).
                buttons = buttons.child(
                    Button::new("call-mute")
                        .label(if call.muted { "Unmute" } else { "Mute" })
                        .ghost()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.toggle_call_mute(cx);
                        })),
                );
                // Phase C2e: camera toggle for video calls — shown only
                // when video can actually run (live: driver ready; demo:
                // fixtures). Otherwise an honest "No camera available".
                if call.is_video {
                    let video_ready = self
                        .live
                        .as_ref()
                        .is_some_and(|live| live.driver.call_video_ready())
                        || self.live.is_none();
                    if video_ready {
                        buttons = buttons.child(
                            Button::new("call-camera")
                                .label(if call.camera_on {
                                    "Camera off"
                                } else {
                                    "Camera on"
                                })
                                .ghost()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.toggle_call_camera(cx);
                                })),
                        );
                    } else {
                        buttons = buttons.child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child("No camera available"),
                        );
                    }
                    // Phase C2i: screen-share send toggle for video
                    // calls — shown only when a screen source can
                    // actually be captured (live: enumerated by the
                    // engine; demo: fixtures). Otherwise an honest
                    // "No screen source available".
                    let screen_ready = self
                        .live
                        .as_ref()
                        .is_some_and(|live| live.driver.call_screen_source_available())
                        || self.live.is_none();
                    if screen_ready {
                        buttons = buttons.child(
                            Button::new("call-screenshare")
                                .label(if call.screen_sharing {
                                    "Stop sharing"
                                } else {
                                    "Share screen"
                                })
                                .ghost()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.toggle_call_screen_share(cx);
                                })),
                        );
                    } else {
                        buttons = buttons.child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child("No screen source available"),
                        );
                    }
                }
                buttons = buttons.child(
                    Button::new("call-hangup")
                        .label("Hang up")
                        .danger()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.hang_up_call(cx);
                        })),
                );
            }
            CallState::HangingUp | CallState::Discarded { .. } | CallState::Error { .. } => {}
        }
        card.child(buttons)
    }

    /// Phase C1: the call-end screen — reason line, duration, and the
    /// optional 1–5 rating card (`callStateDiscarded.need_rating`,
    /// schema 1.8.67, line 7078). Phase C1 shows the
    /// `need_debug_information` upload; phase C2i adds the `need_log`
    /// (`sendCallLog`) upload and the rating-detail editor (problems +
    /// comment).
    pub(super) fn call_summary_card(
        &self,
        card: Stateful<Div>,
        summary: &CallSummary,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let name = self.call_peer_name(summary.user_id);
        let mut card = card
            .child(initials_avatar(&name, 72.))
            .child(div().text_lg().font_semibold().child(name))
            .child(div().text_sm().child(summary.end_line.clone()));
        if summary.duration_secs > 0 {
            card = card.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!(
                        "Connected for {}",
                        Self::call_clock(summary.duration_secs.max(0) as u64)
                    )),
            );
        }
        card = card.child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(if summary.had_audio {
                    "Audio was connected for this call."
                } else {
                    "No audio was carried."
                }),
        );
        if summary.need_debug_information && !summary.debug_information_sent {
            card = card.child(
                Button::new("call-upload-diagnostics")
                    .label("Upload diagnostics")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.upload_call_diagnostics(cx);
                    })),
            );
        }
        if summary.debug_information_sent {
            card = card.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("Diagnostics sent."),
            );
        }
        if let Some(error) = &summary.debug_information_error {
            card = card.child(
                div()
                    .text_xs()
                    .text_color(danger_soft())
                    .child(error.clone()),
            );
        }
        if summary.need_log && !summary.log_sent {
            card = card.child(
                Button::new("call-upload-log")
                    .label("Upload call log")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.upload_call_log(cx);
                    })),
            );
        }
        if summary.log_sent {
            card = card.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("Call log sent."),
            );
        }
        if let Some(error) = &summary.log_error {
            card = card.child(
                div()
                    .text_xs()
                    .text_color(danger_soft())
                    .child(error.clone()),
            );
        }
        if summary.need_rating && !summary.rating_sent {
            card = card.child(div().text_sm().child("How was the call quality?"));
            match &self.dialogs.rating_detail {
                // Phase C2i: star tap opens the detail editor (problems
                // + comment) — nothing is sent until Submit.
                None => {
                    let mut stars = div().flex().gap_2();
                    for star in 1..=5 {
                        stars = stars.child(
                            Button::new(format!("call-rate-{star}"))
                                .label(format!("{star} ★"))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.open_rating_detail(star, window, cx);
                                })),
                        );
                    }
                    card = card.child(stars).child(
                        Button::new("call-rate-skip")
                            .label("Skip")
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.dismiss_call_summary(cx);
                            })),
                    );
                }
                Some(detail) => {
                    card = card.child(self.rating_detail_editor(detail, cx));
                }
            }
        } else {
            if summary.rating_sent {
                card = card.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("Thanks for your feedback"),
                );
            }
            card = card.child(Button::new("call-summary-close").label("Close").on_click(
                cx.listener(|this, _, _, cx| {
                    this.dismiss_call_summary(cx);
                }),
            ));
        }
        // Phase C2i: call again from the call-end card (honours the
        // confirm-before-calling pref).
        {
            let user_id = summary.user_id;
            let was_video = summary.is_video;
            card = card.child(
                Button::new("call-summary-again")
                    .label("Call again")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.dismiss_call_summary(cx);
                        this.call_again(user_id, was_video, cx);
                    })),
            );
        }
        card
    }

    /// Phase C2i: the rating-detail editor — re-pickable stars, the
    /// nine `CallProblem` chips (schema 1.8.67 `:7253`-`:7277`), and
    /// an optional comment. Nothing leaves the machine until Submit.
    pub(super) fn rating_detail_editor(
        &self,
        detail: &RatingDetail,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut stars = div().flex().gap_2();
        for star in 1..=5 {
            stars = stars.child(
                Button::new(format!("call-rate-pick-{star}"))
                    .label(format!("{star} ★"))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(detail) = this.dialogs.rating_detail.as_mut() {
                            detail.stars = star;
                        }
                        cx.notify();
                    })),
            );
        }
        let mut chips = div().flex().flex_wrap().gap_1();
        for (index, (_, description)) in CALL_PROBLEMS.iter().enumerate() {
            let selected = detail.problems[index];
            chips = chips.child(
                // Phase 6: kit Checkbox (was: ghost button with ☑/☐ label).
                Checkbox::new(format!("call-problem-{index}"))
                    .checked(selected)
                    .label(*description)
                    .on_click(cx.listener(move |this, &on, _, cx| {
                        if on != selected {
                            this.toggle_rating_problem(index, cx);
                        }
                    })),
            );
        }
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(stars)
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("What went wrong? (optional)"),
            )
            .child(chips)
            .child(self.dialogs.rating_comment_input.clone())
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("call-rate-submit")
                            .label("Submit rating")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.submit_call_rating(cx);
                            })),
                    )
                    .child(
                        Button::new("call-rate-skip")
                            .label("Skip")
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.dismiss_call_summary(cx);
                            })),
                    ),
            )
            .into_any_element()
    }

    // ── Phase C3a: group-call (voice chat) signaling UI ──
}
