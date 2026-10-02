//! group call state + actions.

use super::app::QuillApp;
use super::shell::{DialogKind, QuillShell};
use super::*;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::Textarea;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::ids::{ChatId, MessageId};
use quill::state::ActiveGroupCall;
use quill::telegram::envelope::MessageSender;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
/// Phase C2g: synthetic per-participant frames for the ready-group-call
/// demo fixture — a camera frame for Zed (user 41) and a screen-share
/// frame for Mia (user 42), keyed `(user_id, is_screen)`. Deterministic
/// test-pattern pixels, not real media. Injected demo data.
pub(super) fn demo_group_video_frames() -> HashMap<(i64, bool), quill::calls::engine::VideoFrame> {
    fn pattern(width: u16, height: u16, base: [u8; 3]) -> Vec<u8> {
        let mut rgba = Vec::with_capacity(width as usize * height as usize * 4);
        for y in 0..height {
            for x in 0..width {
                let stripe = (((x / 16 + y / 16) % 2) * 40) as u8;
                rgba.push(base[0].saturating_add(stripe));
                rgba.push(base[1].saturating_add(stripe));
                rgba.push(base[2].saturating_add(stripe));
                rgba.push(255);
            }
        }
        rgba
    }
    let camera = quill::calls::engine::VideoFrame {
        seq: 1,
        width: 96,
        height: 72,
        rgba: pattern(96, 72, [60, 120, 200]),
        is_local: false,
        participant_user_id: Some(41),
        is_screen: false,
    };
    let screen = quill::calls::engine::VideoFrame {
        seq: 1,
        width: 128,
        height: 72,
        rgba: pattern(128, 72, [200, 170, 60]),
        is_local: false,
        participant_user_id: Some(42),
        is_screen: true,
    };
    HashMap::from([((41i64, false), camera), ((42i64, true), screen)])
}

impl QuillApp {
    /// kit Phase 2 (redo): group-call start hosted in a kit `Dialog` via
    /// `window.open_dialog`. Esc / backdrop / ✕ clear state via `on_close`.
    pub(super) fn build_group_call_start_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::GroupCallStart, |this, _, cx| {
                this.close_group_call_start_dialog(cx);
            });
        app.update(cx, |this, cx| {
            let Some(dialog_state) = this.group_call_start_dialog.as_ref() else {
                return dialog
                    .overlay(true)
                    .title(crate::ui::shell::dialog_title("Start voice chat"))
                    .on_close(on_close.clone());
            };
            let selected = dialog_state.schedule_offset;
            let mut presets = div().flex().flex_wrap().gap_2();
            for (offset, label) in GROUP_CALL_SCHEDULE_PRESETS {
                let active = offset == selected;
                presets = presets.child(
                    Button::new(format!("group-call-start-preset-{offset}"))
                        .label(label)
                        .when(!active, |b| b.ghost())
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.set_group_call_start_schedule(offset, cx);
                            this.close_kit_dialog_if_done(DialogKind::GroupCallStart, window, cx);
                        })),
                );
            }
            let body = div()
                .flex()
                .flex_col()
                .gap_3()
                .child(
                    Textarea::new(&dialog_state.title_input)
                        .aria_label("Voice chat title")
                        .h(px(40.)),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("When should it start?"),
                )
                .child(presets)
                .into_any_element();
            let footer = div()
                .flex()
                .gap_2()
                .child(
                    Button::new("group-call-start-confirm")
                        .label(if selected == 0 {
                            "Start now"
                        } else {
                            "Schedule"
                        })
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.confirm_group_call_start(cx);
                            this.close_kit_dialog_if_done(DialogKind::GroupCallStart, window, cx);
                        })),
                )
                .child(
                    Button::new("group-call-start-cancel")
                        .label("Cancel")
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.close_group_call_start_dialog(cx);
                            this.close_kit_dialog_if_done(DialogKind::GroupCallStart, window, cx);
                        })),
                );
            dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title("Start voice chat"))
                .content(crate::ui::shell::scrollable_dialog_content({
                    let body = Rc::new(RefCell::new(Some(body.into_any_element())));
                    move |content, _, _| {
                        let body = body
                            .borrow_mut()
                            .take()
                            .unwrap_or_else(|| div().into_any_element());
                        content.child(body)
                    }
                }))
                .footer(footer)
                .on_close(on_close)
        })
    }

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
            if let Some(live) = self.live.as_mut() {
                self.status_note = match live.driver.fetch_group_call(group_call_id) {
                    Ok(_) => "Joining voice chat…".into(),
                    Err(_) => "Couldn't reach the voice chat.".into(),
                };
            } else if let Some(session) = self.demo_session.as_mut() {
                // Screenshot demo: no live TDLib — the fixture already
                // tracks the call; just surface the overlay.
                let _ = session;
                self.status_note = "screenshot demo — voice chat (no audio yet)".into();
            }
        } else if self.live.is_some() {
            // Phase C2h: starting goes through the title/schedule
            // dialog (`createVideoChat` with `start_date`).
            self.open_group_call_start_dialog(chat_id, window, cx);
        } else {
            self.status_note = "Voice chats need a live connection.".into();
        }
        cx.notify();
    }

    /// Phase C3a: join the tracked voice chat.
    pub(super) fn join_active_group_call(&mut self, cx: &mut Context<Self>) {
        let id = self
            .session()
            .and_then(|s| s.active_group_call.as_ref())
            .map(|c| c.id);
        let Some(id) = id else {
            self.status_note = "No voice chat to join.".into();
            cx.notify();
            return;
        };
        if let Some(live) = self.live.as_mut() {
            self.status_note = match live.driver.join_video_chat(id) {
                Ok(_) => "Joining voice chat…".into(),
                Err(_) => "Couldn't join the voice chat.".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            if let Some(call) = session.active_group_call.as_mut() {
                call.is_joined = true;
            }
            self.status_note = "screenshot demo — voice chat (no audio yet)".into();
        }
        cx.notify();
    }

    /// Phase C3a: leave the tracked voice chat.
    pub(super) fn leave_active_group_call(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            let result = live.driver.leave_group_call();
            live.driver.session.leave_group_call_local();
            self.status_note = match result {
                Ok(_) => "Left the voice chat.".into(),
                Err(_) => "Left the voice chat (local).".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            session.leave_group_call_local();
            self.status_note = "screenshot demo — left the voice chat".into();
        }
        cx.notify();
    }

    /// Phase C3a: rejoin after `need_rejoin`.
    pub(super) fn rejoin_active_group_call(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.status_note = match live.driver.rejoin_group_call(true) {
                Ok(_) => "Rejoining voice chat…".into(),
                Err(_) => "Couldn't rejoin the voice chat.".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            session.clear_group_call_reconnecting();
            self.status_note = "screenshot demo — rejoined".into();
        }
        cx.notify();
    }

    /// Phase C3a: local-only self mute — no TDLib request (group calls
    /// have no "mute self" outside the join parameters, and no audio
    /// path exists yet). The state rides on the next (re)join.
    pub(super) fn toggle_group_call_self_mute(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            live.driver.toggle_group_call_self_mute();
            self.status_note = "Muted (local — no audio path yet).".into();
        } else if let Some(session) = self.demo_session.as_mut() {
            let muted = !session
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
            .and_then(|s| s.active_group_call.as_ref())
            .and_then(|c| {
                c.participants
                    .iter()
                    .find(|p| p.is_current_user)
                    .map(|p| p.participant_id.clone())
            });
        let Some(me) = me else {
            cx.notify();
            return;
        };
        if let Some(live) = self.live.as_mut() {
            self.status_note = match live.driver.toggle_group_call_participant_hand(me, raise) {
                Ok(_) => if raise {
                    "Hand raised."
                } else {
                    "Hand lowered."
                }
                .into(),
                Err(_) => "Couldn't change the hand state.".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            if let Some(call) = session.active_group_call.as_mut() {
                if let Some(p) = call
                    .participants
                    .iter_mut()
                    .find(|p| p.participant_id == me)
                {
                    p.is_hand_raised = raise;
                }
            }
        }
        cx.notify();
    }

    /// Phase C3a: `toggleGroupCallIsMyVideoEnabled` (live only). Phase
    /// C2g applies the flag to the native camera capture in the driver
    /// pump once the transport is connected.
    pub(super) fn toggle_group_call_video(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.status_note = match live.driver.toggle_group_call_my_video() {
                Ok(_) => "Toggling video…".into(),
                Err(_) => "Couldn't toggle video.".into(),
            };
        } else {
            self.status_note = "Video needs a live connection.".into();
        }
        cx.notify();
    }

    /// Phase C2g: start/stop group-call screen sharing (live only).
    /// The driver runs the presentation handshake
    /// (`ntg_init_presentation` + `startGroupCallScreenSharing`);
    /// stopping pairs `endGroupCallScreenSharing` with
    /// `ntg_stop_presentation`.
    pub(super) fn toggle_group_call_screen_share(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.status_note = match live.driver.toggle_group_call_screen_share() {
                Ok(_) => "Toggling screen sharing…".into(),
                Err(_) => {
                    if live.driver.group_call_screen_source_available() {
                        "Couldn't toggle screen sharing.".into()
                    } else {
                        "No screen source available.".into()
                    }
                }
            };
        } else {
            self.status_note = "Screen sharing needs a live connection.".into();
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
            self.status_note = match live
                .driver
                .toggle_group_call_participant_muted(sender, mute)
            {
                Ok(_) => if mute {
                    "Muting participant…"
                } else {
                    "Unmuting participant…"
                }
                .into(),
                Err(_) => "Couldn't change the participant mute.".into(),
            };
        } else {
            self.status_note = "Participant mute needs a live connection.".into();
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
            self.status_note = match live
                .driver
                .toggle_group_call_participant_hand(sender, raise)
            {
                Ok(_) => "Hand state sent.".into(),
                Err(_) => "Couldn't change the hand state.".into(),
            };
        } else {
            self.status_note = "Hand controls need a live connection.".into();
        }
        cx.notify();
    }

    /// Phase C2f: open the invite picker; fetch contacts first when
    /// the cache is empty.
    pub(super) fn open_group_call_invite(&mut self, cx: &mut Context<Self>) {
        self.group_call_invite_open = true;
        if let Some(live) = self.live.as_mut()
            && live.driver.session.contacts.is_none()
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
            self.status_note = match live.driver.invite_group_call_participant(user_id) {
                Ok(_) => "Invitation sent…".into(),
                Err(_) => "Couldn't invite to the voice chat.".into(),
            };
        } else if self.demo_session.is_some() {
            self.status_note =
                "screenshot demo — invitation sent (injected, no live Telegram)".into();
        } else {
            self.status_note = "Invites need a live connection.".into();
        }
        cx.notify();
    }

    /// Phase C2f: `banGroupCallParticipants` for one participant
    /// (owner-gated by the driver on `groupCall.is_owned`).
    pub(super) fn ban_group_call_participant(&mut self, user_id: i64, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.status_note = match live.driver.ban_group_call_participant(user_id) {
                Ok(_) => "Banning participant…".into(),
                Err(_) => "Couldn't ban the participant.".into(),
            };
        } else if self.demo_session.is_some() {
            self.status_note =
                "screenshot demo — participant banned (injected, no live Telegram)".into();
        } else {
            self.status_note = "Ban needs a live connection.".into();
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
            .and_then(|s| s.active_group_call.as_ref())
            .and_then(|call| {
                call.participants
                    .iter()
                    .find(|p| p.participant_id == sender)
            })
            .map(|p| p.volume_level)
            .unwrap_or(10000);
        let level = (current + delta).clamp(1, 20000);
        if let Some(live) = self.live.as_mut() {
            self.status_note = match live.driver.set_group_call_participant_volume(sender, level) {
                Ok(_) => format!("Volume {}%.", level / 100),
                Err(_) => "Couldn't change the participant volume.".into(),
            };
        } else if self.demo_session.is_some() {
            self.status_note = format!(
                "screenshot demo — volume {}% (injected, no live Telegram)",
                level / 100
            );
        } else {
            self.status_note = "Volume needs a live connection.".into();
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
            self.status_note = match live
                .driver
                .accept_group_call_invitation(chat_id.0, message_id.0)
            {
                Ok(_) => "Joining voice chat…".into(),
                Err(_) => "Couldn't join the voice chat.".into(),
            };
        } else if self.demo_session.is_some() {
            self.status_note =
                "screenshot demo — invitation accepted (injected, no live Telegram)".into();
        } else {
            self.status_note = "Voice chats need a live connection.".into();
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
            self.status_note = match live
                .driver
                .decline_group_call_invitation(chat_id.0, message_id.0)
            {
                Ok(_) => "Invitation declined.".into(),
                Err(_) => "Couldn't decline the invitation.".into(),
            };
        } else if self.demo_session.is_some() {
            self.status_note =
                "screenshot demo — invitation declined (injected, no live Telegram)".into();
        } else {
            self.status_note = "Voice chats need a live connection.".into();
        }
        cx.notify();
    }

    /// Slice G1: dismiss the member-action error line on the
    /// member-management dialog.
    pub(super) fn dismiss_member_action_error(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            live.driver.session.member_action_error.remove(&chat_id.0);
        } else if let Some(session) = self.demo_session.as_mut() {
            session.member_action_error.remove(&chat_id.0);
        }
        cx.notify();
    }

    /// Phase C2f: dismiss the group-call error line on the overlay.
    pub(super) fn dismiss_group_call_error(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            live.driver.session.group_call_error = None;
        } else if let Some(session) = self.demo_session.as_mut() {
            session.group_call_error = None;
        }
        cx.notify();
    }

    /// Phase C3a: `endGroupCall` (admin).
    pub(super) fn end_active_group_call(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.status_note = match live.driver.end_group_call() {
                Ok(_) => "Ending voice chat…".into(),
                Err(_) => "Couldn't end the voice chat.".into(),
            };
        } else {
            self.status_note = "Ending needs a live connection.".into();
        }
        cx.notify();
    }

    /// Phase C3a: `toggleVideoChatMuteNewParticipants`.
    pub(super) fn toggle_video_chat_mute_new(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.status_note = match live.driver.toggle_video_chat_mute_new() {
                Ok(_) => "Toggling mute-new…".into(),
                Err(_) => "Couldn't toggle mute-new.".into(),
            };
        } else {
            self.status_note = "Mute-new needs a live connection.".into();
        }
        cx.notify();
    }

    /// Phase C3a: `getVideoChatInviteLink` — the `HttpUrl` answer lands
    /// on the tracked call via the state router.
    pub(super) fn fetch_group_call_invite_link(&mut self, cx: &mut Context<Self>) {
        let can_self_unmute = self
            .session()
            .and_then(|s| s.active_group_call.as_ref())
            .is_some_and(|c| c.can_be_managed);
        if let Some(live) = self.live.as_mut() {
            self.status_note = match live.driver.fetch_video_chat_invite_link(can_self_unmute) {
                Ok(_) => "Fetching invite link…".into(),
                Err(_) => "Couldn't fetch the invite link.".into(),
            };
        } else {
            self.status_note = "Invite links need a live connection.".into();
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
            .and_then(|s| s.active_group_call.as_ref())
            .map(|c| c.title.clone())
            .unwrap_or_default();
        let dialog = GroupCallTitleDialog::new(window, cx, &current);
        self.group_call_title_dialog = Some(dialog);
        cx.notify();
    }

    pub(super) fn close_group_call_title_dialog(&mut self, cx: &mut Context<Self>) {
        self.group_call_title_dialog = None;
        cx.notify();
    }

    pub(super) fn save_group_call_title(&mut self, cx: &mut Context<Self>) {
        let title = self
            .group_call_title_dialog
            .as_ref()
            .map(|d| d.title_input.read(cx).value().to_string())
            .unwrap_or_default();
        self.group_call_title_dialog = None;
        if let Some(live) = self.live.as_mut() {
            self.status_note = match live.driver.set_video_chat_title(title) {
                Ok(_) => "Renaming voice chat…".into(),
                Err(_) => "Couldn't rename the voice chat.".into(),
            };
        } else {
            self.status_note = "Renaming needs a live connection.".into();
        }
        cx.notify();
    }

    /// Phase C2h: open the start/schedule dialog (`createVideoChat`).
    pub(super) fn open_group_call_start_dialog(
        &mut self,
        chat_id: ChatId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let chat_title = self
            .session()
            .and_then(|s| s.chats.get(&chat_id.0))
            .map(|c| c.title.clone())
            .unwrap_or_default();
        self.group_call_start_dialog =
            Some(GroupCallStartDialog::new(window, cx, chat_id, &chat_title));
        cx.notify();
    }

    pub(super) fn close_group_call_start_dialog(&mut self, cx: &mut Context<Self>) {
        self.group_call_start_dialog = None;
        cx.notify();
    }

    pub(super) fn set_group_call_start_schedule(&mut self, offset: i64, cx: &mut Context<Self>) {
        if let Some(dialog) = self.group_call_start_dialog.as_mut() {
            dialog.schedule_offset = offset;
        }
        cx.notify();
    }

    /// Phase C2h: confirm the start dialog — `createVideoChat` with
    /// the chosen title and `start_date` (0 = immediate).
    pub(super) fn confirm_group_call_start(&mut self, cx: &mut Context<Self>) {
        let (chat_id, title, offset) = match self.group_call_start_dialog.as_ref() {
            Some(d) => (
                d.chat_id,
                d.title_input.read(cx).value().to_string(),
                d.schedule_offset,
            ),
            None => return,
        };
        self.group_call_start_dialog = None;
        let start_date = if offset == 0 {
            0
        } else {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64 + offset)
                .unwrap_or(0)
        };
        if let Some(live) = self.live.as_mut() {
            self.status_note = match live.driver.start_video_chat(chat_id.0, title, start_date) {
                Ok(_) => {
                    if offset == 0 {
                        "Starting voice chat…".into()
                    } else {
                        "Scheduling voice chat…".into()
                    }
                }
                Err(_) => "Couldn't start a voice chat here.".into(),
            };
        } else {
            self.status_note = "Voice chats need a live connection.".into();
        }
        cx.notify();
    }

    /// Phase C2h: revoke the video-chat invite link
    /// (`revokeGroupCallInviteLink`); the cached link clears on `ok`.
    pub(super) fn revoke_group_call_invite_link(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.status_note = match live.driver.revoke_video_chat_invite_link() {
                Ok(_) => "Revoking invite link…".into(),
                Err(_) => "Couldn't revoke the invite link.".into(),
            };
        } else {
            self.status_note = "Invite links need a live connection.".into();
        }
        cx.notify();
    }

    /// Phase C2h: copy the video-chat invite link.
    pub(super) fn copy_video_chat_invite_link(&mut self, link: &str, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(link.to_string()));
        self.status_note = "Invite link copied".into();
        cx.notify();
    }

    /// Phase C2h: toggle group-call recording
    /// (`startGroupCallRecording` / `endGroupCallRecording`); the
    /// `record_duration` indicator refreshes via `updateGroupCall`.
    pub(super) fn toggle_group_call_recording(&mut self, cx: &mut Context<Self>) {
        let recording = self
            .session()
            .and_then(|s| s.active_group_call.as_ref())
            .is_some_and(|c| c.record_duration > 0);
        let title = self
            .session()
            .and_then(|s| s.active_group_call.as_ref())
            .map(|c| c.title.clone())
            .unwrap_or_default();
        if let Some(live) = self.live.as_mut() {
            self.status_note = if recording {
                match live.driver.stop_group_call_recording() {
                    Ok(_) => "Stopping the recording…".into(),
                    Err(_) => "Couldn't stop the recording.".into(),
                }
            } else {
                match live.driver.start_group_call_recording(title, true) {
                    Ok(_) => "Starting the recording…".into(),
                    Err(_) => "Couldn't start the recording.".into(),
                }
            };
        } else {
            self.status_note = "Recording needs a live connection.".into();
        }
        cx.notify();
    }

    /// Phase C2h: `startScheduledVideoChat` — start the tracked
    /// scheduled video chat now (admins only; driver-gated on
    /// `can_be_managed`). Failures surface via the group-call error
    /// line on the card.
    pub(super) fn start_scheduled_video_chat(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.status_note = match live.driver.start_scheduled_video_chat() {
                Ok(_) => "Starting the video chat…".into(),
                Err(_) => "Couldn't start the video chat.".into(),
            };
        } else {
            self.status_note = "Start now needs a live connection.".into();
        }
        cx.notify();
    }

    /// `toggleVideoChatEnabledStartNotification` (:14282) — "notify me
    /// when this scheduled video chat starts". The driver flips the
    /// tracked `enabled_start_notification` flag; the honest value
    /// arrives back as `updateGroupCall`.
    pub(super) fn toggle_video_chat_start_notification(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.status_note = match live.driver.toggle_video_chat_start_notification() {
                Ok(_) => "Updating your start notification…".into(),
                Err(_) => "Couldn't update the start notification.".into(),
            };
        } else {
            self.status_note = "Notify-me needs a live connection.".into();
        }
        cx.notify();
    }

    /// Phase C2h: `getVideoChatRtmpUrl` — fetch the RTMP URL + key.
    pub(super) fn fetch_video_chat_rtmp_url(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.status_note = match live.driver.fetch_video_chat_rtmp_url() {
                Ok(_) => "Fetching the stream key…".into(),
                Err(_) => "Couldn't fetch the stream key.".into(),
            };
        } else {
            self.status_note = "Stream keys need a live connection.".into();
        }
        cx.notify();
    }

    /// Phase C2h: `replaceVideoChatRtmpUrl` — regenerate URL + key
    /// (owner only).
    pub(super) fn replace_video_chat_rtmp_url(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.status_note = match live.driver.replace_video_chat_rtmp_url() {
                Ok(_) => "Regenerating the stream key…".into(),
                Err(_) => "Couldn't regenerate the stream key.".into(),
            };
        } else {
            self.status_note = "Stream keys need a live connection.".into();
        }
        cx.notify();
    }

    /// Phase C2h: copy the RTMP URL or stream key.
    pub(super) fn copy_rtmp_value(&mut self, label: &str, value: &str, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(value.to_string()));
        self.status_note = format!("{label} copied");
        cx.notify();
    }

    /// Phase C2h: send the in-call chat composer
    /// (`sendGroupCallMessage`); the echo arrives as
    /// `updateNewGroupCallMessage`. Clears on send — a send failure
    /// surfaces via `group_call_error`.
    pub(super) fn send_group_call_message(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = quill::composer::send_text_on_enter(
            self.group_call_composer.read(cx).value().to_string(),
            self.chat_prefs.send_key_mode,
        );
        if text.trim().is_empty() {
            return;
        }
        if let Some(live) = self.live.as_mut() {
            match live.driver.send_group_call_message(text) {
                Ok(_) => {
                    self.group_call_composer.update(cx, |input, cx| {
                        input.set_value("", window, cx);
                    });
                }
                Err(_) => {
                    self.status_note = "Couldn't send the message.".into();
                }
            }
        } else {
            self.status_note = "Messages need a live connection.".into();
        }
        cx.notify();
    }

    /// Phase C2h: `toggleGroupCallAreMessagesAllowed`.
    pub(super) fn toggle_group_call_chat(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.status_note = match live.driver.toggle_group_call_are_messages_allowed() {
                Ok(_) => "Toggling in-call chat…".into(),
                Err(_) => "Couldn't toggle in-call chat.".into(),
            };
        } else {
            self.status_note = "In-call chat needs a live connection.".into();
        }
        cx.notify();
    }

    /// Phase C3a: not-joined tracked call — show the join prompt card.
    /// (Handled inside `group_call_card` via `call.is_joined`.)
    pub(super) fn group_call_join_prompt(
        &self,
        call: &ActiveGroupCall,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .id("group-call-join")
            .flex()
            .flex_col()
            .items_center()
            .gap_2()
            .p_4()
            .rounded_md()
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().background)
            .child(
                div()
                    .text_sm()
                    .font_semibold()
                    .child("A voice chat is live"),
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
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("Join to connect video."),
            )
            .child(
                Button::new("group-call-join-btn")
                    .label("Join voice chat")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.join_active_group_call(cx);
                    })),
            )
            .into_any_element()
    }
}
