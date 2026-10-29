//! group call overlay rendering.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::chat_row::initials_avatar;
use super::*;
use gpui_kit::component::button::*;
use gpui_kit::component::input::Textarea;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::lang::{keys, tr, tr_with};
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
                .unwrap_or_else(|| tr_with(keys::Q_AV_USER, &[("user_id", &user_id.to_string())])),
            MessageSender::Chat { chat_id } => self
                .session()
                .and_then(|s| s.chats.get(chat_id))
                .map(|c| c.title.clone())
                .unwrap_or_else(|| tr_with(keys::Q_AV_CHAT, &[("chat_id", &chat_id.to_string())])),
        }
    }

    /// Slice calls-group-self-tile: the current user's tile content in
    /// the group grid. Camera on with a frame → the live preview (a live
    /// player over the call's local feed); camera on with no frame yet →
    /// "Starting camera…"; camera off → the initials avatar, consistent
    /// with every other tile.
    pub(super) fn group_self_tile_content(
        &mut self,
        call: &ActiveGroupCall,
        participant: &ParsedGroupCallParticipant,
        name: &str,
        cx: &mut App,
    ) -> AnyElement {
        if !matches!(participant.participant_id, MessageSender::User { .. }) {
            return div().child(initials_avatar(name, 56.)).into_any_element();
        }
        if !call.is_my_video_enabled {
            return div().child(initials_avatar(name, 56.)).into_any_element();
        }
        let key = quill::calls::live_video::LiveKey::Local { call_id: call.id };
        match self
            .group_tile_player(key, cx)
            .filter(|player| player.has_live_frame())
        {
            Some(player) => div()
                .w_full()
                .h(px(90.))
                .child(super::video_player::video_frame(&player).object_fit(ObjectFit::Contain))
                .into_any_element(),
            None => div()
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
                        .child(tr(keys::Q_AV_STARTING_CAMERA)),
                )
                .into_any_element(),
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
            .child(div().text_xs().font_semibold().child(tr(keys::Q_AV_CHAT_2)));
        if !call.are_messages_allowed {
            section = section.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(tr(keys::Q_AV_CHAT_IS_TURNED_OFF_IN)),
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
                    Textarea::new(&self.group_call.composer)
                        .aria_label(tr(keys::Q_AV_VOICE_CHAT_MESSAGE))
                        .h(px(40.)),
                )
                .child(
                    Button::new("group-call-message-send")
                        .label(tr(keys::Q_AV_SEND))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.send_group_call_message(window, cx);
                        })),
                );
        } else if call.are_messages_allowed {
            section = section.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(tr(keys::Q_AV_YOU_CANT_SEND_MESSAGES_IN)),
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
                            .child(if call.is_conference() {
                                tr(keys::GROUP_CALL_INVITE_CONF)
                            } else {
                                tr(keys::Q_AV_INVITE_TO_VOICE_CHAT)
                            }),
                    )
                    .child(
                        Button::new("group-call-invite-close")
                            .icon(gpui_kit::assets::IconName::X)
                            .tooltip(tr(keys::GROUP_CALL_CLOSE))
                            .accessibility_label(tr(keys::GROUP_CALL_CLOSE))
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.group_call.invite_open = false;
                                this.close_conference_link();
                                cx.notify();
                            })),
                    ),
            )
            .children(self.conference_link_row(call, cx));
        let contacts = self
            .session()
            .and_then(|s| s.users_state.contacts.clone())
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
                .unwrap_or_else(|| tr_with(keys::Q_AV_USER, &[("user_id", &user_id.to_string())]));
            let uid = *user_id;
            rows = rows.child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(div().text_sm().child(name))
                    .child(
                        Button::new(format!("gc-invite-{uid}"))
                            .label(tr(keys::GROUP_CALL_INVITE_BUTTON))
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
                    .child(tr(keys::Q_AV_NO_CONTACTS_TO_INVITE)),
            );
        } else {
            panel = panel.child(rows);
        }
        panel
    }

    /// `messageGroupCall`: a conference call's entry in the chat (schema
    /// 1.8.69). Incoming, not joined and not missed: Join / Decline.
    /// Anything else reads as tdesktop's group call entries ("Outgoing
    /// group call", "Missed group call", ...); TDLib updates refresh the
    /// row when it is joined or missed.
    pub(super) fn group_call_invitation_row(
        message: &HistoryMessage,
        is_active: bool,
        was_missed: bool,
        cx: &mut Context<QuillApp>,
    ) -> impl IntoElement {
        let mut inner = div().flex().flex_col().items_center().gap_2();
        if !message.is_outgoing && !is_active && !was_missed {
            let chat_id = message.chat_id;
            let message_id = message.id;
            inner = inner
                .child(
                    div()
                        .text_sm()
                        .font_semibold()
                        .child(tr(keys::CALL_GROUP_INCOMING)),
                )
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(
                            Button::new(format!("gc-invite-accept-{}", message_id.0))
                                .label(tr(keys::GROUP_CALL_JOIN))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.accept_group_call_invitation(chat_id, message_id, cx);
                                })),
                        )
                        .child(
                            Button::new(format!("gc-invite-decline-{}", message_id.0))
                                .label(tr(keys::CALL_DECLINE))
                                .ghost()
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.decline_group_call_invitation(chat_id, message_id, cx);
                                })),
                        ),
                );
        } else {
            let label = match (message.is_outgoing, was_missed) {
                (true, true) => tr(keys::CALL_GROUP_DECLINED),
                (true, false) => tr(keys::CALL_GROUP_OUTGOING),
                (false, true) => tr(keys::CALL_GROUP_MISSED),
                (false, false) => tr(keys::CALL_GROUP_INCOMING),
            };
            let state = if is_active && !was_missed {
                tr(keys::Q_AV_IN_PROGRESS)
            } else {
                ""
            };
            inner = inner.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!("{label}{state}")),
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
