//! group call overlay rendering.

use super::app::QuillApp;
use super::chat_row::initials_avatar;
use super::*;
use gpui_kit::component::button::*;
use gpui_kit::component::input::Textarea;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::state::{ActiveGroupCall, HistoryMessage};
use quill::telegram::envelope::{MessageSender, ParsedGroupCallParticipant};
impl QuillApp {
    /// Phase C3a: group-call overlay — the voice-chat card above
    /// everything else. **Signaling only**: no audio/video transport
    /// exists yet (Phase C2), so the card always carries the honest
    /// no-transport note.
    /// The group call lives in its own window now (`group_call_panel`).
    pub(super) fn group_call_overlay(&mut self, _cx: &mut Context<Self>) -> Option<AnyElement> {
        None
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
                            .icon(gpui_kit::assets::IconName::X)
                            .tooltip("Close")
                            .accessibility_label("Close")
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
