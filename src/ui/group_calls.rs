//! group call state + actions.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::group_call_stream_layout::StreamJoinIntent;
use super::*;
use gpui_kit::*;
use quill::ids::{ChatId, MessageId};
use quill::lang::{keys, tr, tr_with};
use quill::telegram::envelope::MessageSender;

impl QuillApp {
    /// Phase C3a: header voice-chat affordance — join a live voice chat,
    /// or start one for a group/channel with none.
    pub(super) fn start_or_join_video_chat(
        &mut self,
        chat_id: ChatId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let group_call_id = self
            .session()
            .and_then(|s| s.chats.get(&chat_id.0))
            .and_then(|c| c.video_chat.clone())
            .map(|vc| vc.group_call_id);
        if let Some(group_call_id) = group_call_id {
            // A live stream joins in its window once it is tracked.
            self.note_stream_join_intent(StreamJoinIntent::Call(group_call_id));
            if let Some(live) = self.live.as_mut() {
                self.connection.status_note = match live.driver.fetch_group_call(group_call_id) {
                    Ok(_) => tr(keys::Q_AV_JOINING_VOICE_CHAT).into(),
                    Err(_) => tr(keys::Q_AV_COULDNT_REACH_THE_VOICE_CHAT).into(),
                };
            } else if let Some(session) = self.demo_session.as_mut() {
                // Screenshot demo: no live TDLib — the fixture already
                // tracks the call; just surface the overlay.
                let _ = session;
                self.connection.status_note = tr(keys::Q_AV_SCREENSHOT_DEMO_VOICE_CHAT_NO).into();
            }
        } else {
            // No call yet: tdesktop's create confirmation (`Context::Create`
            // in `startOrJoinGroupCall`).
            self.begin_group_call_create(
                chat_id,
                quill::calls::group_call_create::CreateKind::Now,
                window,
                cx,
            );
        }
        cx.notify();
    }

    /// Phase C3a: join the tracked voice chat.
    pub(super) fn join_active_group_call(&mut self, cx: &mut Context<Self>) {
        self.send_group_call_join();
        cx.notify();
    }

    /// Send the join for the tracked call. False when it could not be
    /// sent (no call, a 1:1 call is on, the connection refused it).
    pub(super) fn send_group_call_join(&mut self) -> bool {
        let id = self
            .session()
            .and_then(|s| s.calls.active_group_call.as_ref())
            .map(|c| c.id);
        let Some(id) = id else {
            self.connection.status_note = tr(keys::Q_AV_NO_VOICE_CHAT_TO_JOIN).into();
            return false;
        };
        // Push-to-talk joins with the microphone closed.
        let ptt_on = self
            .session()
            .is_some_and(|s| s.calls.prefs.push_to_talk.enabled);
        if ptt_on {
            if let Some(live) = self.live.as_mut() {
                live.driver.session.set_group_call_self_muted(true);
            } else if let Some(session) = self.demo_session.as_mut() {
                session.set_group_call_self_muted(true);
            }
        }
        if let Some(live) = self.live.as_mut() {
            let sent = live.driver.join_video_chat(id).is_ok();
            self.connection.status_note = if sent {
                tr(keys::Q_AV_JOINING_VOICE_CHAT).into()
            } else {
                tr(keys::Q_AV_COULDNT_JOIN_THE_VOICE_CHAT).into()
            };
            sent
        } else if let Some(session) = self.demo_session.as_mut() {
            if let Some(call) = session.calls.active_group_call.as_mut() {
                call.is_joined = true;
            }
            self.connection.status_note = tr(keys::Q_AV_SCREENSHOT_DEMO_VOICE_CHAT_NO).into();
            true
        } else {
            false
        }
    }

    /// Phase C3a: leave the tracked voice chat.
    pub(super) fn leave_active_group_call(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            let result = live.driver.leave_group_call();
            live.driver.drop_group_call_locally();
            self.connection.status_note = match result {
                Ok(_) => tr(keys::Q_AV_LEFT_THE_VOICE_CHAT).into(),
                Err(_) => tr(keys::Q_AV_LEFT_THE_VOICE_CHAT_LOCAL).into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            session.leave_group_call_local();
            self.connection.status_note = tr(keys::Q_AV_SCREENSHOT_DEMO_LEFT_THE_VOICE).into();
        }
        cx.notify();
    }

    /// Phase C3a: rejoin after `need_rejoin`.
    pub(super) fn rejoin_active_group_call(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.connection.status_note = match live.driver.rejoin_group_call(true) {
                Ok(_) => tr(keys::Q_AV_REJOINING_VOICE_CHAT).into(),
                Err(_) => tr(keys::Q_AV_COULDNT_REJOIN_THE_VOICE_CHAT).into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            session.clear_group_call_reconnecting();
            self.connection.status_note = tr(keys::Q_AV_SCREENSHOT_DEMO_REJOINED).into();
        }
        cx.notify();
    }

    /// Phase C3a: local-only self mute — no TDLib request (group calls
    /// have no "mute self" outside the join parameters, and no audio
    /// path exists yet). The state rides on the next (re)join.
    pub(super) fn toggle_group_call_self_mute(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            if live.driver.toggle_group_call_self_mute().is_err() {
                self.connection.status_note = tr(keys::Q_AV_COULDNT_CHANGE_THE_MICROPHONE).into();
            }
        } else if let Some(session) = self.demo_session.as_mut() {
            let muted = !session
                .calls
                .active_group_call
                .as_ref()
                .is_some_and(|c| c.is_muted_self);
            session.set_group_call_self_muted(muted);
        }
        cx.notify();
    }

    /// Phase C3a: raise/lower the self hand (live) or flip the demo
    /// fixture's participant flag.
    pub(super) fn toggle_group_call_self_hand(&mut self, raise: bool, cx: &mut Context<Self>) {
        let me = self
            .session()
            .and_then(|s| s.calls.active_group_call.as_ref())
            .and_then(|c| {
                c.participants
                    .iter()
                    .find(|p| p.is_current_user)
                    .map(|p| p.participant_id)
            });
        let Some(me) = me else {
            cx.notify();
            return;
        };
        if let Some(live) = self.live.as_mut() {
            self.connection.status_note =
                match live.driver.toggle_group_call_participant_hand(me, raise) {
                    Ok(_) => if raise {
                        tr(keys::Q_AV_HAND_RAISED)
                    } else {
                        tr(keys::Q_AV_HAND_LOWERED)
                    }
                    .into(),
                    Err(_) => tr(keys::Q_AV_COULDNT_CHANGE_THE_HAND_STATE).into(),
                };
        } else if let Some(session) = self.demo_session.as_mut()
            && let Some(call) = session.calls.active_group_call.as_mut()
            && let Some(p) = call
                .participants
                .iter_mut()
                .find(|p| p.participant_id == me)
        {
            p.is_hand_raised = raise;
        }
        cx.notify();
    }

    /// Phase C3a: `toggleGroupCallIsMyVideoEnabled` (live only). Phase
    /// C2g applies the flag to the native camera capture in the driver
    /// pump once the transport is connected.
    pub(super) fn toggle_group_call_video(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.connection.status_note = match live.driver.toggle_group_call_my_video() {
                Ok(_) => tr(keys::Q_AV_TOGGLING_VIDEO).into(),
                Err(_) => tr(keys::Q_AV_COULDNT_TOGGLE_VIDEO).into(),
            };
        } else {
            self.connection.status_note = tr(keys::Q_AV_VIDEO_NEEDS_A_LIVE_CONNECTION).into();
        }
        cx.notify();
    }

    /// Phase C2g: start/stop group-call screen sharing (live only).
    /// The driver runs the presentation handshake
    /// (`ntg_init_presentation` + `startGroupCallScreenSharing`);
    /// stopping pairs `endGroupCallScreenSharing` with
    /// `ntg_stop_presentation`.
    ///
    /// Where Quill captures the screen itself, starting opens the chooser
    /// first (tdesktop `chooseShareScreenSource`); the menu's Stop Sharing
    /// stops at once.
    pub(super) fn toggle_group_call_screen_share(&mut self, cx: &mut Context<Self>) {
        let sharing = self
            .session()
            .and_then(|session| session.calls.active_group_call.as_ref())
            .is_some_and(|call| call.screen_sharing || call.screen_share_pending);
        if !sharing && self.open_screen_chooser(super::screen_share_state::ScreenFor::Group, cx) {
            return;
        }
        self.share_group_call_screen(cx);
    }

    /// Start or stop sharing the screen in the voice chat (the driver
    /// toggles).
    pub(super) fn share_group_call_screen(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.connection.status_note = match live.driver.toggle_group_call_screen_share() {
                Ok(_) => tr(keys::Q_AV_TOGGLING_SCREEN_SHARING).into(),
                Err(_) => {
                    if live.driver.group_call_screen_source_available() {
                        tr(keys::Q_AV_COULDNT_TOGGLE_SCREEN_SHARING).into()
                    } else {
                        tr(keys::Q_AV_NO_SCREEN_SOURCE_AVAILABLE).into()
                    }
                }
            };
        } else {
            self.connection.status_note = tr(keys::Q_AV_SCREEN_SHARING_NEEDS_A_LIVE).into();
        }
        cx.notify();
    }

    /// Phase C3a: mute/unmute a participant for all users (admin).
    pub(super) fn toggle_group_call_participant_muted(
        &mut self,
        sender: MessageSender,
        mute: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            self.connection.status_note = match live
                .driver
                .toggle_group_call_participant_muted(sender, mute)
            {
                Ok(_) => if mute {
                    tr(keys::Q_AV_MUTING_PARTICIPANT)
                } else {
                    tr(keys::Q_AV_UNMUTING_PARTICIPANT)
                }
                .into(),
                Err(_) => tr(keys::Q_AV_COULDNT_CHANGE_THE_PARTICIPANT_MUTE).into(),
            };
        } else {
            self.connection.status_note = tr(keys::Q_AV_PARTICIPANT_MUTE_NEEDS_A_LIVE).into();
        }
        cx.notify();
    }

    /// Phase C3a: lower a participant's raised hand (admin).
    pub(super) fn toggle_group_call_participant_hand(
        &mut self,
        sender: MessageSender,
        raise: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            self.connection.status_note = match live
                .driver
                .toggle_group_call_participant_hand(sender, raise)
            {
                Ok(_) => tr(keys::Q_AV_HAND_STATE_SENT).into(),
                Err(_) => tr(keys::Q_AV_COULDNT_CHANGE_THE_HAND_STATE).into(),
            };
        } else {
            self.connection.status_note = tr(keys::Q_AV_HAND_CONTROLS_NEED_A_LIVE).into();
        }
        cx.notify();
    }

    /// Phase C2f: open the invite picker; fetch contacts first when
    /// the cache is empty.
    pub(super) fn open_group_call_invite(&mut self, cx: &mut Context<Self>) {
        self.group_call_needs_unlock(cx);
        self.group_call.invite_open = true;
        if let Some(live) = self.live.as_mut()
            && live.driver.session.users_state.contacts.is_none()
        {
            let _ = live.driver.fetch_contacts();
        }
        cx.notify();
    }

    /// Phase C2f: `inviteGroupCallParticipant` for a picked contact.
    /// The `InviteGroupCallParticipantResult` answer lands in
    /// `group_call_error` (shown on the overlay) when it is not a
    /// success.
    pub(super) fn invite_group_call_participant(&mut self, user_id: i64, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.connection.status_note = match live.driver.invite_group_call_participant(user_id) {
                Ok(_) => tr(keys::Q_AV_INVITATION_SENT).into(),
                Err(_) => tr(keys::Q_AV_COULDNT_INVITE_TO_THE_VOICE).into(),
            };
        } else if self.demo_session.is_some() {
            self.connection.status_note =
                tr(keys::Q_AV_SCREENSHOT_DEMO_INVITATION_SENT_INJECTED).into();
        } else {
            self.connection.status_note = tr(keys::Q_AV_INVITES_NEED_A_LIVE_CONNECTION).into();
        }
        cx.notify();
    }

    /// Phase C2f: `banGroupCallParticipants` for one participant
    /// (owner-gated by the driver on `groupCall.is_owned`).
    pub(super) fn ban_group_call_participant(&mut self, user_id: i64, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.connection.status_note = match live.driver.ban_group_call_participant(user_id) {
                Ok(_) => tr(keys::Q_AV_BANNING_PARTICIPANT).into(),
                Err(_) => tr(keys::Q_AV_COULDNT_BAN_THE_PARTICIPANT).into(),
            };
        } else if self.demo_session.is_some() {
            self.connection.status_note =
                tr(keys::Q_AV_SCREENSHOT_DEMO_PARTICIPANT_BANNED_INJECTED).into();
        } else {
            self.connection.status_note = tr(keys::Q_AV_BAN_NEEDS_A_LIVE_CONNECTION).into();
        }
        cx.notify();
    }

    /// Phase C2f: `setGroupCallParticipantVolumeLevel`, stepped by
    /// `delta` (hundreds of percents) from the participant's current
    /// level.
    pub(super) fn adjust_group_call_participant_volume(
        &mut self,
        sender: MessageSender,
        delta: i32,
        cx: &mut Context<Self>,
    ) {
        let current = self
            .session()
            .and_then(|s| s.calls.active_group_call.as_ref())
            .and_then(|call| {
                call.participants
                    .iter()
                    .find(|p| p.participant_id == sender)
            })
            .map(|p| p.volume_level)
            .unwrap_or(10000);
        let level = current.saturating_add(delta).clamp(1, 20000);
        self.set_group_call_participant_volume(sender, level, cx);
    }

    /// `setGroupCallParticipantVolumeLevel` to `level` (1 to 20000,
    /// 10000 being 100 %): the stage's slider and the menu's steps.
    pub(super) fn set_group_call_participant_volume(
        &mut self,
        sender: MessageSender,
        level: i32,
        cx: &mut Context<Self>,
    ) {
        let level = level.clamp(1, 20000);
        if let Some(live) = self.live.as_mut() {
            self.connection.status_note =
                match live.driver.set_group_call_participant_volume(sender, level) {
                    Ok(_) => tr_with(
                        keys::Q_AV_VOLUME,
                        &[("level_100", &(level / 100).to_string())],
                    ),
                    Err(_) => tr(keys::Q_AV_COULDNT_CHANGE_THE_PARTICIPANT_VOLUME).into(),
                };
        } else if self.demo_session.is_some() {
            self.connection.status_note = tr_with(
                keys::Q_AV_SCREENSHOT_DEMO_VOLUME_INJECTED_NO,
                &[("level_100", &(level / 100).to_string())],
            );
        } else {
            self.connection.status_note = tr(keys::Q_AV_VOLUME_NEEDS_A_LIVE_CONNECTION).into();
        }
        cx.notify();
    }

    /// Phase C2f: accept a `messageGroupCall` invitation via
    /// `joinGroupCall` (schema 1.8.67, line 5288).
    pub(super) fn accept_group_call_invitation(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            self.connection.status_note = match live
                .driver
                .accept_group_call_invitation(chat_id.0, message_id.0)
            {
                Ok(_) => tr(keys::Q_AV_JOINING_VOICE_CHAT).into(),
                Err(_) => tr(keys::Q_AV_COULDNT_JOIN_THE_VOICE_CHAT).into(),
            };
        } else if self.demo_session.is_some() {
            self.connection.status_note =
                tr(keys::Q_AV_SCREENSHOT_DEMO_INVITATION_ACCEPTED_INJECTED).into();
        } else {
            self.connection.status_note = tr(keys::Q_AV_VOICE_CHATS_NEED_A_LIVE).into();
        }
        cx.notify();
    }

    /// Phase C2f: decline a `messageGroupCall` invitation via
    /// `declineGroupCallInvitation`.
    pub(super) fn decline_group_call_invitation(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            self.connection.status_note = match live
                .driver
                .decline_group_call_invitation(chat_id.0, message_id.0)
            {
                Ok(_) => tr(keys::Q_AV_INVITATION_DECLINED).into(),
                Err(_) => tr(keys::Q_AV_COULDNT_DECLINE_THE_INVITATION).into(),
            };
        } else if self.demo_session.is_some() {
            self.connection.status_note =
                tr(keys::Q_AV_SCREENSHOT_DEMO_INVITATION_DECLINED_INJECTED).into();
        } else {
            self.connection.status_note = tr(keys::Q_AV_VOICE_CHATS_NEED_A_LIVE).into();
        }
        cx.notify();
    }

    /// Slice G1: dismiss the member-action error line on the
    /// member-management dialog.
    pub(super) fn dismiss_member_action_error(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            live.driver
                .session
                .groups
                .member_action_error
                .remove(&chat_id.0);
        } else if let Some(session) = self.demo_session.as_mut() {
            session.groups.member_action_error.remove(&chat_id.0);
        }
        cx.notify();
    }

    /// Phase C2f: dismiss the group-call error line on the overlay.
    pub(super) fn dismiss_group_call_error(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            live.driver.session.calls.group_call_error = None;
        } else if let Some(session) = self.demo_session.as_mut() {
            session.calls.group_call_error = None;
        }
        cx.notify();
    }

    /// Phase C3a: `endGroupCall` (admin).
    pub(super) fn end_active_group_call(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.connection.status_note = match live.driver.end_group_call() {
                Ok(_) => tr(keys::Q_AV_ENDING_VOICE_CHAT).into(),
                Err(_) => tr(keys::Q_AV_COULDNT_END_THE_VOICE_CHAT).into(),
            };
        } else {
            self.connection.status_note = tr(keys::Q_AV_ENDING_NEEDS_A_LIVE_CONNECTION).into();
        }
        cx.notify();
    }

    /// Phase C3a: `toggleVideoChatMuteNewParticipants`.
    pub(super) fn toggle_video_chat_mute_new(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.connection.status_note = match live.driver.toggle_video_chat_mute_new() {
                Ok(_) => tr(keys::Q_AV_TOGGLING_MUTE_NEW).into(),
                Err(_) => tr(keys::Q_AV_COULDNT_TOGGLE_MUTE_NEW).into(),
            };
        } else {
            self.connection.status_note = tr(keys::Q_AV_MUTE_NEW_NEEDS_A_LIVE).into();
        }
        cx.notify();
    }

    /// Phase C3a: `loadGroupCallParticipants` — page more participants.
    pub(super) fn load_more_group_call_participants(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.load_more_group_call_participants();
        }
        cx.notify();
    }

    /// Phase C3a: open the voice-chat rename dialog (`setVideoChatTitle`).
    pub(super) fn open_group_call_title_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let current = self
            .session()
            .and_then(|s| s.calls.active_group_call.as_ref())
            .map(|c| c.title.clone())
            .unwrap_or_default();
        let dialog = GroupCallTitleDialog::new(window, cx, &current);
        self.group_call.title_dialog = Some(dialog);
        cx.notify();
    }

    pub(super) fn close_group_call_title_dialog(&mut self, cx: &mut Context<Self>) {
        self.group_call.title_dialog = None;
        cx.notify();
    }

    pub(super) fn save_group_call_title(&mut self, cx: &mut Context<Self>) {
        let title = self
            .group_call
            .title_dialog
            .as_ref()
            .map(|d| d.title_input.read(cx).value().to_string())
            .unwrap_or_default();
        self.group_call.title_dialog = None;
        if let Some(live) = self.live.as_mut() {
            self.connection.status_note = match live.driver.set_video_chat_title(title) {
                Ok(_) => tr(keys::Q_AV_RENAMING_VOICE_CHAT).into(),
                Err(_) => tr(keys::Q_AV_COULDNT_RENAME_THE_VOICE_CHAT).into(),
            };
        } else {
            self.connection.status_note = tr(keys::Q_AV_RENAMING_NEEDS_A_LIVE_CONNECTION).into();
        }
        cx.notify();
    }

    /// Phase C2h: revoke the video-chat invite link
    /// (`revokeGroupCallInviteLink`); the cached link clears on `ok`.
    pub(super) fn revoke_group_call_invite_link(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.connection.status_note = match live.driver.revoke_video_chat_invite_link() {
                Ok(_) => tr(keys::Q_AV_REVOKING_INVITE_LINK).into(),
                Err(_) => tr(keys::Q_AV_COULDNT_REVOKE_THE_INVITE_LINK).into(),
            };
        } else {
            self.connection.status_note = tr(keys::Q_AV_INVITE_LINKS_NEED_A_LIVE).into();
        }
        cx.notify();
    }

    /// Phase C2h: toggle group-call recording
    /// (`startGroupCallRecording` / `endGroupCallRecording`); the
    /// `record_duration` indicator refreshes via `updateGroupCall`.
    pub(super) fn toggle_group_call_recording(&mut self, cx: &mut Context<Self>) {
        let recording = self
            .session()
            .and_then(|s| s.calls.active_group_call.as_ref())
            .is_some_and(|c| c.record_duration > 0);
        let title = self
            .session()
            .and_then(|s| s.calls.active_group_call.as_ref())
            .map(|c| c.title.clone())
            .unwrap_or_default();
        if let Some(live) = self.live.as_mut() {
            self.connection.status_note = if recording {
                match live.driver.stop_group_call_recording() {
                    Ok(_) => tr(keys::Q_AV_STOPPING_THE_RECORDING).into(),
                    Err(_) => tr(keys::Q_AV_COULDNT_STOP_THE_RECORDING).into(),
                }
            } else {
                match live.driver.start_group_call_recording(title, true) {
                    Ok(_) => tr(keys::Q_AV_STARTING_THE_RECORDING).into(),
                    Err(_) => tr(keys::Q_AV_COULDNT_START_THE_RECORDING).into(),
                }
            };
        } else {
            self.connection.status_note = tr(keys::Q_AV_RECORDING_NEEDS_A_LIVE_CONNECTION).into();
        }
        cx.notify();
    }

    /// Phase C2h: `startScheduledVideoChat` — start the tracked
    /// scheduled video chat now (admins only; driver-gated on
    /// `can_be_managed`). Failures surface via the group-call error
    /// line on the card.
    pub(super) fn start_scheduled_video_chat(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.connection.status_note = match live.driver.start_scheduled_video_chat() {
                Ok(_) => tr(keys::Q_AV_STARTING_THE_VIDEO_CHAT).into(),
                Err(_) => tr(keys::Q_AV_COULDNT_START_THE_VIDEO_CHAT).into(),
            };
        } else {
            self.connection.status_note = tr(keys::Q_AV_START_NOW_NEEDS_A_LIVE).into();
        }
        cx.notify();
    }

    /// `toggleVideoChatEnabledStartNotification` (:14282) — "notify me
    /// when this scheduled video chat starts". The driver flips the
    /// tracked `enabled_start_notification` flag; the honest value
    /// arrives back as `updateGroupCall`.
    pub(super) fn toggle_video_chat_start_notification(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.connection.status_note = match live.driver.toggle_video_chat_start_notification() {
                Ok(_) => tr(keys::Q_AV_UPDATING_YOUR_START_NOTIFICATION).into(),
                Err(_) => tr(keys::Q_AV_COULDNT_UPDATE_THE_START_NOTIFICATION).into(),
            };
        } else {
            self.connection.status_note = tr(keys::Q_AV_NOTIFY_ME_NEEDS_A_LIVE).into();
        }
        cx.notify();
    }

    /// Phase C2h: `getVideoChatRtmpUrl` — fetch the RTMP URL + key.
    pub(super) fn fetch_video_chat_rtmp_url(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.connection.status_note = match live.driver.fetch_video_chat_rtmp_url() {
                Ok(_) => tr(keys::Q_AV_FETCHING_THE_STREAM_KEY).into(),
                Err(_) => tr(keys::Q_AV_COULDNT_FETCH_THE_STREAM_KEY).into(),
            };
        } else {
            self.connection.status_note = tr(keys::Q_AV_STREAM_KEYS_NEED_A_LIVE).into();
        }
        cx.notify();
    }

    /// Phase C2h: `replaceVideoChatRtmpUrl` — regenerate URL + key
    /// (owner only).
    pub(super) fn replace_video_chat_rtmp_url(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.connection.status_note = match live.driver.replace_video_chat_rtmp_url() {
                Ok(_) => tr(keys::Q_AV_REGENERATING_THE_STREAM_KEY).into(),
                Err(_) => tr(keys::Q_AV_COULDNT_REGENERATE_THE_STREAM_KEY).into(),
            };
        } else {
            self.connection.status_note = tr(keys::Q_AV_STREAM_KEYS_NEED_A_LIVE).into();
        }
        cx.notify();
    }

    /// Phase C2h: copy the RTMP URL or stream key.
    pub(super) fn copy_rtmp_value(&mut self, label: &str, value: &str, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(value.to_string()));
        self.connection.status_note = format!("{label} copied");
        cx.notify();
    }

    /// Phase C2h: send the in-call chat composer
    /// (`sendGroupCallMessage`); the echo arrives as
    /// `updateNewGroupCallMessage`. Clears on send — a send failure
    /// surfaces via `group_call_error`.
    pub(super) fn send_group_call_message(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = quill::composer::send_text_on_enter(
            self.group_call.composer.read(cx).value().to_string(),
            self.chat_prefs.send_key_mode,
        );
        if text.trim().is_empty() {
            return;
        }
        if let Some(live) = self.live.as_mut() {
            match live.driver.send_group_call_message(text) {
                Ok(_) => {
                    self.group_call.composer.update(cx, |input, cx| {
                        input.set_value("", window, cx);
                    });
                }
                Err(_) => {
                    self.connection.status_note = tr(keys::Q_AV_COULDNT_SEND_THE_MESSAGE).into();
                }
            }
        } else {
            self.connection.status_note = tr(keys::Q_AV_MESSAGES_NEED_A_LIVE_CONNECTION).into();
        }
        cx.notify();
    }

    /// Phase C2h: `toggleGroupCallAreMessagesAllowed`.
    pub(super) fn toggle_group_call_chat(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.connection.status_note = match live.driver.toggle_group_call_are_messages_allowed()
            {
                Ok(_) => tr(keys::Q_AV_TOGGLING_IN_CALL_CHAT).into(),
                Err(_) => tr(keys::Q_AV_COULDNT_TOGGLE_IN_CALL_CHAT).into(),
            };
        } else {
            self.connection.status_note = tr(keys::Q_AV_IN_CALL_CHAT_NEEDS_A).into();
        }
        cx.notify();
    }
}
