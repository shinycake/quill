//! group call overlay rendering.

use super::app::QuillApp;
use super::chat_row::initials_avatar;
use super::*;
use gpui_kit::component::button::*;
use gpui_kit::component::input::Textarea;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::state::{ActiveGroupCall, HistoryMessage};
use quill::telegram::envelope::{MessageSender, ParsedGroupCallParticipant};
impl QuillApp {
    /// Phase C3a: group-call overlay — the voice-chat card above
    /// everything else. **Signaling only**: no audio/video transport
    /// exists yet (Phase C2), so the card always carries the honest
    /// no-transport note.
    pub(super) fn group_call_overlay(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let call = self.session()?.active_group_call.clone()?;
        Some(self.group_call_card(&call, cx).into_any_element())
    }

    pub(super) fn group_call_participant_name(&self, sender: &MessageSender) -> String {
        match sender {
            MessageSender::User { user_id } => self
                .session()
                .and_then(|s| s.user(*user_id))
                .map(|u| u.display_name())
                .unwrap_or_else(|| format!("User {user_id}")),
            MessageSender::Chat { chat_id } => self
                .session()
                .and_then(|s| s.chats.get(chat_id))
                .map(|c| c.title.clone())
                .unwrap_or_else(|| format!("Chat {chat_id}")),
        }
    }

    pub(super) fn group_call_participant_tile(
        &mut self,
        call: &ActiveGroupCall,
        participant: &ParsedGroupCallParticipant,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let name = self.group_call_participant_name(&participant.participant_id);
        let mut badges: Vec<String> = Vec::new();
        if participant.is_speaking {
            badges.push("🔊 speaking".to_string());
        }
        if participant.is_muted_for_all_users
            || participant.is_muted_for_current_user
            || (participant.is_current_user && call.is_muted_self)
        {
            // Slice calls-group-self-tile: the self tile shows the LOCAL
            // mute state (`is_muted_self`), which the participant flags
            // alone don't reliably carry for the current user.
            badges.push("🔇 muted".to_string());
        }
        if participant.is_hand_raised {
            badges.push("✋ hand raised".to_string());
        }
        if participant.video_enabled {
            badges.push("📹 video".to_string());
        }
        // `groupCallParticipantVideoInfo.is_paused` (schema 1.8.67,
        // :7163): TDLib clears the flag when new frames arrive, so the
        // badge only needs to read it.
        if participant
            .video_info
            .as_ref()
            .is_some_and(|info| info.is_paused)
        {
            badges.push("⏸ paused".to_string());
        }
        if participant.screen_sharing_enabled {
            badges.push("🖥 sharing".to_string());
        }
        // Phase C2g: live video tile when a frame is retained for this
        // participant (camera, or screen share preferred when sharing);
        // the avatar placeholder stays for everyone else. Slice
        // calls-group-self-tile: the current user's tile renders the
        // local camera preview from the driver's group-local slot.
        let video_or_avatar: AnyElement = if participant.is_current_user {
            self.group_self_tile_content(call, participant, &name)
        } else {
            match self.group_participant_frame(call.id, participant) {
                Some((user_id, screen, frame)) => {
                    match self.cached_group_video_image(call.id, user_id, screen, &frame) {
                        Some(image) => img(ImageSource::from(image))
                            .w_full()
                            .h(px(90.))
                            .object_fit(ObjectFit::Contain)
                            .into_any_element(),
                        None => div().child(initials_avatar(&name, 56.)).into_any_element(),
                    }
                }
                None => div().child(initials_avatar(&name, 56.)).into_any_element(),
            }
        };
        let mut tile = div()
            .w(px(124.))
            .flex()
            .flex_col()
            .items_center()
            .gap_1()
            .p_2()
            .rounded_md()
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().background)
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_md()
                    .bg(bg_video())
                    .child(video_or_avatar),
            )
            .child(
                div()
                    .text_xs()
                    .font_semibold()
                    .child(if participant.is_current_user {
                        format!("{name} (you)")
                    } else {
                        name
                    }),
            );
        if !badges.is_empty() {
            tile = tile.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(badges.join(" · ")),
            );
        }
        // Phase C3a: admin participant controls, gated on the actual
        // TDLib flags — mute for all, lower a raised hand. One wrapped
        // row keeps tiles short (the grid scrolls past 320px).
        let sender = participant.participant_id.clone();
        if call.can_be_managed && !participant.is_current_user {
            let mut admin_row = div().flex().flex_wrap().justify_center().gap_1();
            let mut admin_count = 0;
            if participant.is_hand_raised {
                let sender2 = sender.clone();
                admin_row = admin_row.child(
                    Button::new(format!("gc-lower-hand-{sender2:?}"))
                        .label("Lower hand")
                        .ghost()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.toggle_group_call_participant_hand(sender2.clone(), false, cx);
                        })),
                );
                admin_count += 1;
            }
            if participant.can_be_muted_for_all_users || participant.can_be_unmuted_for_all_users {
                let mute = !participant.is_muted_for_all_users;
                let sender2 = sender.clone();
                admin_row = admin_row.child(
                    Button::new(format!("gc-mute-participant-{sender2:?}"))
                        .label(if mute { "Mute" } else { "Unmute" })
                        .ghost()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.toggle_group_call_participant_muted(sender2.clone(), mute, cx);
                        })),
                );
                admin_count += 1;
            }
            if admin_count > 0 {
                tile = tile.child(admin_row);
            }
        }
        // Phase C2f: ban (`banGroupCallParticipants` takes user ids
        // only — `messageSenderChat` participants have no button).
        // Owner-gated: schema requires `groupCall.is_owned`
        // (`can_be_managed` is "for video chats and live stories
        // only"), so this is its own block, not part of the admin row.
        if call.is_owned && !participant.is_current_user {
            if let MessageSender::User { user_id } = participant.participant_id {
                tile = tile.child(
                    div().flex().flex_wrap().justify_center().gap_1().child(
                        Button::new(format!("gc-ban-participant-{user_id}"))
                            .label("Ban")
                            .ghost()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.ban_group_call_participant(user_id, cx);
                            })),
                    ),
                );
            }
        }
        // Phase C2f: per-participant volume stepper
        // (`setGroupCallParticipantVolumeLevel`; 1-20000 in hundreds
        // of percents, stepped ±10%). Local playback volume — no
        // admin right needed; self has no button.
        if !participant.is_current_user {
            let sender_down = sender;
            let sender_up = sender;
            tile = tile.child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(
                        Button::new(format!("gc-vol-down-{sender_down:?}"))
                            .label("−")
                            .ghost()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.adjust_group_call_participant_volume(sender_down, -1000, cx);
                            })),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!("🔊 {}%", participant.volume_level / 100)),
                    )
                    .child(
                        Button::new(format!("gc-vol-up-{sender_up:?}"))
                            .label("+")
                            .ghost()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.adjust_group_call_participant_volume(sender_up, 1000, cx);
                            })),
                    ),
            );
        }
        tile
    }

    /// Slice calls-group-self-tile: the current user's tile content in
    /// the group grid. Camera on + a retained local frame → the live
    /// local preview (decoded through the same per-participant cache as
    /// the remote tiles); camera on with no frame yet → "Starting
    /// camera…"; camera off → the initials avatar, consistent with every
    /// other tile. Live mode reads the driver's group-local slot (the
    /// engine delivers group CAPTURE frames as `is_local` with no
    /// participant); demo mode reuses the 1:1 local fixture frame.
    pub(super) fn group_self_tile_content(
        &mut self,
        call: &ActiveGroupCall,
        participant: &ParsedGroupCallParticipant,
        name: &str,
    ) -> AnyElement {
        let frame = if let Some(live) = self.live.as_ref() {
            live.driver.latest_video_frame(call.id, true)
        } else {
            self.demo_local_frame.clone()
        };
        let MessageSender::User { user_id } = participant.participant_id else {
            return div().child(initials_avatar(name, 56.)).into_any_element();
        };
        match (call.is_my_video_enabled, frame.as_ref()) {
            (true, Some(frame)) => {
                match self.cached_group_video_image(call.id, user_id, false, frame) {
                    Some(image) => img(ImageSource::from(image))
                        .w_full()
                        .h(px(90.))
                        .object_fit(ObjectFit::Contain)
                        .into_any_element(),
                    None => div().child(initials_avatar(name, 56.)).into_any_element(),
                }
            }
            (true, None) => div()
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
                        .child("Starting camera…"),
                )
                .into_any_element(),
            (false, _) => div().child(initials_avatar(name, 56.)).into_any_element(),
        }
    }

    pub(super) fn group_call_card(
        &mut self,
        call: &ActiveGroupCall,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let mut card = div()
            .id("group-call-card")
            .flex()
            .flex_col()
            .items_center()
            .gap_3()
            .p_6()
            .w(px(600.))
            .max_w(relative(0.9))
            .max_h(relative(0.85))
            .overflow_y_scroll()
            .rounded_lg()
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().sidebar);

        // Title + kind line.
        card = card
            .child(
                div()
                    .text_lg()
                    .font_semibold()
                    .child(if call.title.is_empty() {
                        "Voice chat".to_string()
                    } else {
                        call.title.clone()
                    }),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!(
                        "{} participant{}",
                        call.participant_count,
                        if call.participant_count == 1 { "" } else { "s" }
                    )),
            );

        // Phase C2f: group-call request failures (`group_call_error`)
        // were write-only before — surface them on the overlay with a
        // dismiss, so invite/ban/volume/rejoin failures are honest.
        if let Some(error) = self.session().and_then(|s| s.group_call_error.clone()) {
            card = card.child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .p_2()
                    .rounded_md()
                    .bg(danger_bg_deep())
                    .child(div().text_sm().text_color(danger_pale()).child(error))
                    .child(
                        Button::new("group-call-error-dismiss")
                            .label("Dismiss")
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.dismiss_group_call_error(cx);
                            })),
                    ),
            );
        }

        if call.scheduled_start_date > 0 && !call.is_joined {
            // Phase C2h: scheduled (not yet started) video chat —
            // admins (`can_be_managed`) get a Start-now button
            // (`startScheduledVideoChat`, schema 1.8.67 :14277); Join
            // appears once TDLib activates the call.
            card = card.child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_2()
                    .p_4()
                    .rounded_md()
                    .border_1()
                    .border_color(cx.theme().border)
                    .child(
                        div()
                            .text_sm()
                            .font_semibold()
                            .child("Scheduled voice chat"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!(
                                "Starts {}",
                                format_starts_in(call.scheduled_start_date as i64)
                            )),
                    )
                    .when(call.can_be_managed, |this| {
                        this.child(
                            Button::new("group-call-start-now")
                                .label("Start now")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.start_scheduled_video_chat(cx);
                                })),
                        )
                    })
                    // "Notify me when it starts":
                    // `toggleVideoChatEnabledStartNotification` (schema
                    // 1.8.67, :14282). Any viewer can set it — no admin
                    // right required. The driver sends the flipped flag;
                    // the new value arrives back as `updateGroupCall`.
                    .child(
                        Button::new("group-call-start-notify-toggle")
                            .label(if call.enabled_start_notification {
                                "🔔 Notifying — tap to turn off"
                            } else {
                                "🔕 Notify me when it starts"
                            })
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.toggle_video_chat_start_notification(cx);
                            })),
                    ),
            );
        } else if !call.is_joined {
            card = card.child(self.group_call_join_prompt(call, cx));
        } else {
            card = self.group_call_joined_card(card, call, cx);
        }

        // Backdrop, like the 1:1 call overlay.
        div()
            .id("group-call-overlay")
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
                    .id("group-call-backdrop")
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .bottom_0()
                    .bg(scrim()),
            )
            .child(card)
    }

    /// Phase C3a: the joined voice-chat card body — reconnect banner,
    /// verification emojis, participant grid, self/admin controls.
    pub(super) fn group_call_joined_card(
        &mut self,
        mut card: Stateful<Div>,
        call: &ActiveGroupCall,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        // Reconnect banner (`need_rejoin`): the call dropped and must be
        // rejoined. Live driver re-issues `joinVideoChat`.
        if call.reconnecting {
            card = card
                .child(
                    div()
                        .w_full()
                        .p_2()
                        .rounded_md()
                        .bg(warning_bg_deep())
                        .child(
                            div()
                                .text_sm()
                                .font_semibold()
                                .text_color(warning_soft())
                                .child("Connection lost — the voice chat needs to be rejoined."),
                        ),
                )
                .child(
                    Button::new("group-call-rejoin")
                        .label("Rejoin")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.rejoin_active_group_call(cx);
                        })),
                );
        }

        // E2E verification emojis, straight from the update.
        if let Some(verification) = &call.verification {
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
                    .child(div().text_2xl().child(verification.emojis.join(" "))),
            );
        }

        // Participant grid. Scrolls internally so the self/admin
        // controls below never clip when the roster is tall.
        // `flex_shrink_0`: inside the scrolling card the grid must keep
        // its own height instead of collapsing.
        let mut grid = div()
            .id("group-call-participants")
            .w_full()
            .flex()
            .flex_wrap()
            .justify_center()
            .gap_2()
            .max_h(px(320.))
            .flex_shrink_0()
            .overflow_y_scroll();
        for participant in &call.participants {
            grid = grid.child(self.group_call_participant_tile(call, participant, cx));
        }
        card = card.child(grid);
        if !call.loaded_all_participants && call.participants.len() >= 100 {
            card = card.child(
                Button::new("group-call-load-more")
                    .label("Load more participants")
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.load_more_group_call_participants(cx);
                    })),
            );
        }

        // The always-visible transport state note — honest about the
        // native group transport (Phase C2g connects it on the
        // `joinVideoChat` answer). The slice carries no audio
        // (microphone/speaker sources are null), so the copy says
        // video, not voice.
        let transport_note = if call.transport_ready {
            "Video connected."
        } else if call.transport_error.is_some() {
            "Video failed to connect."
        } else if self
            .live
            .as_ref()
            .is_some_and(|live| live.driver.group_call_engine_available())
        {
            "Connecting…"
        } else {
            "Video unavailable — call engine unavailable."
        };
        card = card.child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(transport_note),
        );

        // Self controls. Self mute is local-only: TDLib group calls
        // have no "mute self" outside the join parameters, so the flag
        // rides on the next join — the label says so.
        let self_muted = call.is_muted_self;
        let self_video = call.is_my_video_enabled;
        let self_hand = call
            .participants
            .iter()
            .any(|p| p.is_current_user && p.is_hand_raised);
        let mut controls = div().flex().gap_2().flex_wrap().justify_center();
        controls = controls.child(
            Button::new("group-call-mute")
                .label(if self_muted { "Unmute" } else { "Mute" })
                .on_click(cx.listener(|this, _, _, cx| {
                    this.toggle_group_call_self_mute(cx);
                })),
        );
        controls = controls.child(
            Button::new("group-call-hand")
                .label(if self_hand {
                    "Lower hand"
                } else {
                    "Raise hand"
                })
                .on_click(cx.listener({
                    let raise = !self_hand;
                    move |this, _, _, cx| {
                        this.toggle_group_call_self_hand(raise, cx);
                    }
                })),
        );
        if call.can_enable_video {
            controls = controls.child(
                Button::new("group-call-video")
                    .label(if self_video {
                        "Stop video"
                    } else {
                        "Start video"
                    })
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.toggle_group_call_video(cx);
                    })),
            );
        }
        // Phase C2g: screen sharing toggle, gated on an enumerated
        // screen-capture source — without one the honest note shows
        // instead of a dead button.
        let screen_source = self
            .live
            .as_ref()
            .is_some_and(|live| live.driver.group_call_screen_source_available());
        let sharing = call.screen_sharing || call.screen_share_pending;
        if screen_source {
            controls = controls.child(
                Button::new("group-call-share-screen")
                    .label(if sharing {
                        "Stop sharing"
                    } else {
                        "Share screen"
                    })
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.toggle_group_call_screen_share(cx);
                    })),
            );
        } else {
            controls = controls.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("No screen source available."),
            );
        }
        controls = controls.child(Button::new("group-call-leave").label("Leave").on_click(
            cx.listener(|this, _, _, cx| {
                this.leave_active_group_call(cx);
            }),
        ));
        // Phase C2f: invite participants (`inviteGroupCallParticipant`).
        controls = controls.child(
            Button::new("group-call-invite-participant")
                .label("Invite")
                .on_click(cx.listener(|this, _, _, cx| {
                    this.open_group_call_invite(cx);
                })),
        );
        card = card.child(controls);
        card = card.child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child("Mute is local-only: it applies on your next join."),
        );

        // Admin controls, gated on the actual TDLib flags.
        if call.can_be_managed || call.can_toggle_mute_new_participants {
            let mut admin = div().flex().gap_2().flex_wrap().justify_center();
            if call.can_be_managed {
                admin = admin.child(
                    Button::new("group-call-invite")
                        .label("Get invite link")
                        .ghost()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.fetch_group_call_invite_link(cx);
                        })),
                );
                admin = admin.child(
                    Button::new("group-call-rename")
                        .label("Rename")
                        .ghost()
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.open_group_call_title_dialog(window, cx);
                        })),
                );
                // Phase C2h: recording toggle (`startGroupCallRecording`
                // / `endGroupCallRecording`) — video chats only.
                if call.is_video_chat {
                    let recording = call.record_duration > 0;
                    admin = admin.child(
                        Button::new("group-call-record")
                            .label(if recording {
                                "■ Stop recording"
                            } else {
                                "● Record"
                            })
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.toggle_group_call_recording(cx);
                            })),
                    );
                }
                // Phase C2h: RTMP stream key (`getVideoChatRtmpUrl`).
                admin = admin.child(
                    Button::new("group-call-rtmp")
                        .label("Stream key")
                        .ghost()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.fetch_video_chat_rtmp_url(cx);
                        })),
                );
                admin = admin.child(
                    Button::new("group-call-end")
                        .label("End voice chat")
                        .ghost()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.end_active_group_call(cx);
                        })),
                );
            }
            // Phase C2h: in-call chat on/off
            // (`toggleGroupCallAreMessagesAllowed`).
            if call.can_toggle_are_messages_allowed {
                let label = if call.are_messages_allowed {
                    "Chat: on"
                } else {
                    "Chat: off"
                };
                admin = admin.child(
                    Button::new("group-call-chat-toggle")
                        .label(label)
                        .ghost()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.toggle_group_call_chat(cx);
                        })),
                );
            }
            if call.can_toggle_mute_new_participants {
                let label = if call.mute_new_participants {
                    "Mute new: on"
                } else {
                    "Mute new: off"
                };
                admin = admin.child(
                    Button::new("group-call-mute-new")
                        .label(label)
                        .ghost()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.toggle_video_chat_mute_new(cx);
                        })),
                );
            }
            card = card.child(admin);
        }

        // Phase C2h: recording indicator — `record_duration` from
        // `updateGroupCall` (0 = not recording).
        if call.record_duration > 0 {
            let label = if call.is_video_recorded {
                "video"
            } else {
                "audio"
            };
            card = card.child(
                div()
                    .text_sm()
                    .font_semibold()
                    .text_color(danger_vivid())
                    .child(format!(
                        "● Recording ({}) {}",
                        label,
                        format_record_duration(call.record_duration)
                    )),
            );
        }

        // Invite link once fetched — show, copy, revoke.
        if let Some(link) = &call.invite_link {
            let link = link.clone();
            card = card.child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!("Invite link: {link}")),
                    )
                    .child(
                        Button::new("group-call-invite-copy")
                            .label("Copy")
                            .ghost()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.copy_video_chat_invite_link(&link, cx);
                            })),
                    )
                    .when(call.can_be_managed, |this| {
                        this.child(
                            Button::new("group-call-invite-revoke")
                                .label("Revoke")
                                .ghost()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.revoke_group_call_invite_link(cx);
                                })),
                        )
                    }),
            );
        }

        // Phase C2h: RTMP URL + stream key once fetched
        // (`getVideoChatRtmpUrl` / `replaceVideoChatRtmpUrl`).
        if let (Some(url), Some(key)) = (&call.rtmp_url, &call.rtmp_stream_key) {
            let url = url.clone();
            let key = key.clone();
            let is_owned = call.is_owned;
            card = card.child(
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .p_2()
                    .rounded_md()
                    .border_1()
                    .border_color(cx.theme().border)
                    .child(
                        div()
                            .text_xs()
                            .font_semibold()
                            .child("RTMP stream (admins only)"),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .flex_1()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(format!("URL: {url}")),
                            )
                            .child(
                                Button::new("group-call-rtmp-copy-url")
                                    .label("Copy")
                                    .ghost()
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.copy_rtmp_value("Stream URL", &url, cx);
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .flex_1()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(format!("Key: {key}")),
                            )
                            .child(
                                Button::new("group-call-rtmp-copy-key")
                                    .label("Copy")
                                    .ghost()
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.copy_rtmp_value("Stream key", &key, cx);
                                    })),
                            ),
                    )
                    .when(is_owned, |this| {
                        this.child(
                            Button::new("group-call-rtmp-regenerate")
                                .label("Regenerate key")
                                .ghost()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.replace_video_chat_rtmp_url(cx);
                                })),
                        )
                    }),
            );
        }

        // Title rename dialog.
        if let Some(dialog) = &self.group_call_title_dialog {
            card = card.child(
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .p_3()
                    .rounded_md()
                    .border_1()
                    .border_color(cx.theme().border)
                    .child(div().text_sm().font_semibold().child("Rename voice chat"))
                    .child(
                        Textarea::new(&dialog.title_input)
                            .aria_label("Voice chat title")
                            .h(px(40.)),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(Button::new("group-call-title-save").label("Save").on_click(
                                cx.listener(|this, _, _, cx| {
                                    this.save_group_call_title(cx);
                                }),
                            ))
                            .child(
                                Button::new("group-call-title-cancel")
                                    .label("Cancel")
                                    .ghost()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.close_group_call_title_dialog(cx);
                                    })),
                            ),
                    ),
            );
        }

        // Phase C2f: invite picker — contacts not already in the call.
        if self.group_call_invite_open {
            card = card.child(self.group_call_invite_panel(call, cx));
        }

        // Phase C2h: in-call chat (`sendGroupCallMessage`) — a simple
        // message list + composer; TDLib has no history getter, so this
        // is the live feed only.
        card = self.group_call_messages_section(card, call, cx);

        card
    }

    /// Phase C2h: in-call group-chat section for the joined call card.
    pub(super) fn group_call_messages_section(
        &self,
        card: Stateful<Div>,
        call: &ActiveGroupCall,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        if !call.are_messages_allowed
            && call.messages.is_empty()
            && !call.can_toggle_are_messages_allowed
        {
            return card;
        }
        let mut section = div()
            .w_full()
            .flex()
            .flex_col()
            .gap_1()
            .p_2()
            .rounded_md()
            .border_1()
            .border_color(cx.theme().border)
            .child(div().text_xs().font_semibold().child("Chat"));
        if !call.are_messages_allowed {
            section = section.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("Chat is turned off in this voice chat."),
            );
        }
        if !call.messages.is_empty() {
            let mut list = div()
                .id("group-call-messages")
                .w_full()
                .flex()
                .flex_col()
                .gap_1()
                .max_h(px(180.))
                .overflow_y_scroll();
            for message in call.messages.iter().rev().take(50).rev() {
                let sender = self.group_call_participant_name(&message.sender_id);
                list = list.child(div().text_xs().child(format!("{sender}: {}", message.text)));
            }
            section = section.child(list);
        }
        if call.are_messages_allowed && call.can_send_messages {
            section = section
                .child(
                    Textarea::new(&self.group_call_composer)
                        .aria_label("Voice chat message")
                        .h(px(40.)),
                )
                .child(
                    Button::new("group-call-message-send")
                        .label("Send")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.send_group_call_message(window, cx);
                        })),
                );
        } else if call.are_messages_allowed {
            section = section.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("You can't send messages in this voice chat."),
            );
        }
        card.child(section)
    }

    /// Phase C2f: the invite picker panel — contact rows with Invite
    /// buttons (`inviteGroupCallParticipant`); contacts already in the
    /// call are excluded.
    pub(super) fn group_call_invite_panel(
        &self,
        call: &ActiveGroupCall,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let in_call: std::collections::HashSet<i64> = call
            .participants
            .iter()
            .filter_map(|p| match p.participant_id {
                MessageSender::User { user_id } => Some(user_id),
                _ => None,
            })
            .collect();
        let mut panel = div()
            .id("group-call-invite-panel")
            .w_full()
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .rounded_md()
            .border_1()
            .border_color(cx.theme().border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_sm()
                            .font_semibold()
                            .child("Invite to voice chat"),
                    )
                    .child(
                        Button::new("group-call-invite-close")
                            .label("Close")
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.group_call_invite_open = false;
                                cx.notify();
                            })),
                    ),
            );
        let contacts = self
            .session()
            .and_then(|s| s.contacts.clone())
            .unwrap_or_default();
        let mut rows = div()
            .id("group-call-invite-rows")
            .flex()
            .flex_col()
            .gap_1()
            .max_h(px(200.))
            .overflow_y_scroll();
        let mut shown = 0;
        for user_id in contacts.iter().filter(|id| !in_call.contains(id)).take(30) {
            let name = self
                .session()
                .and_then(|s| s.user(*user_id))
                .map(|u| u.display_name())
                .unwrap_or_else(|| format!("User {user_id}"));
            let uid = *user_id;
            rows = rows.child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(div().text_sm().child(name))
                    .child(
                        Button::new(format!("gc-invite-{uid}"))
                            .label("Invite")
                            .ghost()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.invite_group_call_participant(uid, cx);
                            })),
                    ),
            );
            shown += 1;
        }
        if shown == 0 {
            panel = panel.child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("No contacts to invite."),
            );
        } else {
            panel = panel.child(rows);
        }
        panel
    }

    /// Phase C2f: `messageGroupCall` invitation service row (schema
    /// 1.8.67, line 5288). Incoming and pending: Accept / Decline.
    /// Anything else (own sent, missed, active) is a neutral service
    /// notice — TDLib updates refresh the row when the call starts or
    /// ends. Free function: `session_history_row` is not a method.
    pub(super) fn group_call_invitation_row(
        message: &HistoryMessage,
        is_active: bool,
        was_missed: bool,
        is_video: bool,
        cx: &mut Context<QuillApp>,
    ) -> impl IntoElement {
        let kind = if is_video { "video chat" } else { "voice chat" };
        let mut inner = div().flex().flex_col().items_center().gap_2();
        if !message.is_outgoing && !is_active && !was_missed {
            let chat_id = message.chat_id;
            let message_id = message.id;
            inner = inner
                .child(
                    div()
                        .text_sm()
                        .font_semibold()
                        .child(format!("📞 Incoming {kind} invitation")),
                )
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(
                            Button::new(format!("gc-invite-accept-{}", message_id.0))
                                .label("Accept")
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.accept_group_call_invitation(chat_id, message_id, cx);
                                })),
                        )
                        .child(
                            Button::new(format!("gc-invite-decline-{}", message_id.0))
                                .label("Decline")
                                .ghost()
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.decline_group_call_invitation(chat_id, message_id, cx);
                                })),
                        ),
                );
        } else {
            let state = if was_missed {
                " · missed"
            } else if is_active {
                " · in progress"
            } else {
                ""
            };
            inner = inner.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!("📞 {kind} invitation{state}")),
            );
        }
        div()
            .id(("gc-invitation-row", message.id.0 as u64))
            .flex()
            .justify_center()
            .py_1()
            .child(inner)
    }

    // ── Phase C3a: group-call action handlers ──
}
