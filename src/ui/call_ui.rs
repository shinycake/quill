//! 1:1 call overlay rendering.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::chat_row::initials_avatar;
use super::*;
use gpui_kit::component::button::*;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::radio::{Radio, RadioGroup};
use gpui_kit::component::*;
use gpui_kit::*;
use quill::lang::{keys, tr, tr_with};
use quill::state::{ActiveCall, CallSummary};
use quill::telegram::envelope::CallState;
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
            .unwrap_or_else(|| tr_with(keys::Q_OPS_USER, &[("user_id", &user_id.to_string())]))
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
            card = card
                .child(
                    div()
                        .text_sm()
                        .font_semibold()
                        .child(tr(keys::Q_OPS_CALL_FAILED)),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(error),
                )
                .child(
                    Button::new("call-error-dismiss")
                        .label(tr(keys::Q_OPS_DISMISS))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.dismiss_call_error(cx);
                        })),
                );
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

    pub(super) fn call_active_card(
        &mut self,
        card: Stateful<Div>,
        call: &ActiveCall,
        error: Option<&str>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let name = self.call_peer_name(call.user_id);
        let kind_line = if call.is_video {
            tr(keys::Q_OPS_VIDEO_CALL)
        } else {
            tr(keys::Q_OPS_VOICE_CALL)
        };
        let (status, clock): (String, Option<String>) = match &call.state {
            CallState::Pending { .. } => {
                let elapsed = call.started_at.elapsed().as_secs();
                if call.is_outgoing {
                    (
                        tr(keys::Q_OPS_CALLING).to_string(),
                        Some(Self::call_clock(elapsed)),
                    )
                } else {
                    (
                        tr(keys::CALL_INCOMING).to_string(),
                        Some(Self::call_clock(elapsed)),
                    )
                }
            }
            CallState::ExchangingKeys => (tr(keys::GROUP_CALL_CONNECTING).to_string(), None),
            CallState::Ready => {
                use quill::calls::engine::TransportState;
                let status = match call.transport {
                    Some(TransportState::Connected) if call.muted => {
                        tr(keys::Q_OPS_MUTED_MICROPHONE_OFF).to_string()
                    }
                    Some(TransportState::Connected) => tr(keys::Q_OPS_CONNECTED).to_string(),
                    Some(TransportState::Reconnecting) => {
                        tr(keys::Q_OPS_RECONNECTING_AUDIO).to_string()
                    }
                    Some(TransportState::Failed) => tr_with(
                        keys::Q_LEFTOVERS_AUDIO_START_FAILED,
                        &[(
                            "error",
                            (call
                                .transport_error
                                .as_deref()
                                .unwrap_or(tr(keys::Q_OPS_UNKNOWN_ERROR))),
                        )],
                    ),
                    Some(TransportState::Closed) => tr(keys::Q_OPS_AUDIO_DISCONNECTED).to_string(),
                    None | Some(TransportState::Connecting) => {
                        tr(keys::Q_OPS_CONNECTING_AUDIO).to_string()
                    }
                };
                (
                    status,
                    Some(Self::call_clock(call.connected_secs().max(0) as u64)),
                )
            }
            CallState::HangingUp => (tr(keys::Q_OPS_HANGING_UP).to_string(), None),
            // A future state the pinned schema doesn't know: label it
            // honestly instead of pretending it means something else.
            CallState::Unknown(type_name) => (
                tr_with(
                    keys::Q_OPS_CALL_STATE,
                    &[("type_name", &type_name.to_string())],
                ),
                None,
            ),
            // Terminal states end the call in the reducer, so this arm
            // is unreachable — but never crash the overlay on it.
            CallState::Discarded { .. } | CallState::Error { .. } => {
                (tr(keys::Q_OPS_ENDING).to_string(), None)
            }
        };
        let mut card = card;
        // The pictures are the call window's (`calls::call_video`).
        card = card.child(initials_avatar(&name, 72.));
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
                            .child(tr(keys::Q_OPS_END_TO_END_VERIFICATION)),
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
                    tr(keys::Q_OPS_VIDEO_CONNECTING_TILES_FILL_IN)
                } else {
                    tr(keys::Q_OPS_VIDEO_UNAVAILABLE_NO_CAMERA_FOUND)
                }
            } else if call.transport == Some(quill::calls::engine::TransportState::Connected) {
                tr(keys::Q_OPS_AUDIO_CONNECTED)
            } else {
                tr(keys::Q_OPS_WAITING_FOR_AUDIO_TRANSPORT)
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
                    .child(tr(keys::Q_OPS_MICROPHONE_MUTED)),
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
                    .child(tr(keys::Q_OPS_SHARING_YOUR_SCREEN)),
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
                        .child(tr(keys::Q_OPS_NO_AUDIO_CALL_ENGINE_UNAVAILABLE)),
                );
            } else if devices.is_empty() {
                card = card.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(tr(keys::Q_OPS_NO_AUDIO_DEVICES_FOUND_THE)),
                );
            } else {
                let mut pickers = div().w_full().flex().flex_col().gap_1();
                for (kind, label, selected) in [
                    (
                        quill::calls::engine::MediaDeviceKind::Microphone,
                        tr(keys::SETTINGS_CALL_SECTION_INPUT),
                        selected.0.as_deref(),
                    ),
                    (
                        quill::calls::engine::MediaDeviceKind::Speaker,
                        tr(keys::Q_OPS_SPEAKER),
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
                                if let Some(choice) = choices.get(ix) {
                                    this.select_call_device(kind, &choice.0, cx);
                                }
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
                    pickers = pickers.child(
                        div()
                            .text_xs()
                            .font_semibold()
                            .child(tr(keys::SETTINGS_CALL_CAMERA).to_string()),
                    );
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
                                .child(tr(keys::Q_OPS_NO_CAMERA_FOUND)),
                        );
                    } else {
                        let camera_active = camera_choices
                            .iter()
                            .position(|(id, _)| Some(id.as_str()) == camera_selected);
                        pickers = pickers.child(
                            RadioGroup::vertical(tr(keys::Q_OPS_CALL_DEVICE_CAMERA))
                                .selected_index(camera_active)
                                .children(camera_choices.iter().map(|(id, name)| {
                                    Radio::new(tr_with(
                                        keys::Q_OPS_CALL_DEVICE_CAMERA_2,
                                        &[("id", &id.to_string())],
                                    ))
                                    .label(name.clone())
                                }))
                                .on_click(cx.listener(move |this, &ix: &usize, _, cx| {
                                    if let Some(choice) = camera_choices.get(ix) {
                                        this.select_call_device(
                                            quill::calls::engine::MediaDeviceKind::Camera,
                                            &choice.0,
                                            cx,
                                        );
                                    }
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
                            .label(tr(keys::CALL_ACCEPT))
                            .success()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.accept_incoming_call(cx);
                            })),
                    )
                    .child(
                        Button::new("call-decline")
                            .label(tr(keys::CALL_DECLINE))
                            .danger()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.hang_up_call(cx);
                            })),
                    );
            }
            CallState::Pending { .. } | CallState::ExchangingKeys => {
                buttons = buttons.child(
                    Button::new("call-cancel")
                        .label(tr(keys::CALL_CANCEL))
                        .ghost()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.hang_up_call(cx);
                        })),
                );
            }
            CallState::Ready | CallState::Unknown(_) => {
                // Phase C1b: mute toggle — local-only state, no-op
                // without media transport (the card says so above).
                buttons = buttons.child(
                    Button::new("call-mute")
                        .label(if call.muted {
                            tr(keys::CALL_UNMUTE_AUDIO)
                        } else {
                            tr(keys::CALL_MUTE_AUDIO)
                        })
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
                                    tr(keys::Q_OPS_CAMERA_OFF)
                                } else {
                                    tr(keys::Q_OPS_CAMERA_ON)
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
                                .child(tr(keys::Q_OPS_NO_CAMERA_AVAILABLE)),
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
                        || self.live.is_none()
                        || self.uses_screen_chooser();
                    if screen_ready {
                        buttons = buttons.child(
                            Button::new("call-screenshare")
                                .label(if call.screen_sharing {
                                    tr(keys::GROUP_CALL_SCREEN_SHARE_STOP)
                                } else {
                                    tr(keys::GROUP_CALL_TOOLTIP_SCREEN)
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
                                .child(tr(keys::Q_OPS_NO_SCREEN_SOURCE_AVAILABLE)),
                        );
                    }
                }
                buttons = buttons.child(
                    Button::new("call-hangup")
                        .label(tr(keys::Q_OPS_HANG_UP))
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
                    .child(tr_with(
                        keys::Q_LEFTOVERS_CALL_CONNECTED_FOR,
                        &[(
                            "duration",
                            &(Self::call_clock(summary.duration_secs.max(0) as u64)).to_string(),
                        )],
                    )),
            );
        }
        card = card.child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(if summary.had_audio {
                    tr(keys::Q_OPS_AUDIO_WAS_CONNECTED_FOR_THIS)
                } else {
                    tr(keys::Q_OPS_NO_AUDIO_WAS_CARRIED)
                }),
        );
        if summary.need_debug_information && !summary.debug_information_sent {
            card = card.child(
                Button::new("call-upload-diagnostics")
                    .label(tr(keys::Q_OPS_UPLOAD_DIAGNOSTICS))
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
                    .child(tr(keys::Q_OPS_DIAGNOSTICS_SENT)),
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
                    .label(tr(keys::Q_OPS_UPLOAD_CALL_LOG))
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
                    .child(tr(keys::Q_OPS_CALL_LOG_SENT)),
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
            card = card.child(
                div()
                    .text_sm()
                    .child(tr(keys::Q_OPS_HOW_WAS_THE_CALL_QUALITY)),
            );
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
                            .label(tr(keys::Q_OPS_SKIP))
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
                        .child(tr(keys::Q_OPS_THANKS_FOR_YOUR_FEEDBACK)),
                );
            }
            card = card.child(
                Button::new("call-summary-close")
                    .label(tr(keys::GROUP_CALL_CLOSE))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.dismiss_call_summary(cx);
                    })),
            );
        }
        // Phase C2i: call again from the call-end card (honours the
        // confirm-before-calling pref).
        {
            let user_id = summary.user_id;
            let was_video = summary.is_video;
            card = card.child(
                Button::new("call-summary-again")
                    .label(tr(keys::Q_OPS_CALL_AGAIN))
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
        for (index, (_, label)) in CALL_PROBLEMS.iter().enumerate() {
            let selected = detail.problems.get(index).copied().unwrap_or(false);
            chips = chips.child(
                // Phase 6: kit Checkbox (was: ghost button with ☑/☐ label).
                Checkbox::new(format!("call-problem-{index}"))
                    .checked(selected)
                    .label(tr(*label))
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
                    .child(tr(keys::Q_OPS_WHAT_WENT_WRONG_OPTIONAL)),
            )
            .child(chips)
            .child(self.dialogs.rating_comment_input.clone())
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("call-rate-submit")
                            .label(tr(keys::Q_OPS_SUBMIT_RATING))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.submit_call_rating(cx);
                            })),
                    )
                    .child(
                        Button::new("call-rate-skip")
                            .label(tr(keys::Q_OPS_SKIP))
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
