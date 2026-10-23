//! Recent actions: one parsed event turned into what a row shows (the
//! service line and the affected message), in tdesktop's order and wording
//! (`history_admin_log_item.cpp`).
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::event_log_rows::{LogBody, LogEntry};
use quill::lang::{keys, tr, tr_with};
use quill::local_time::{civil_local, day_label, hhmm, now_unix};
use quill::telegram::envelope::{
    ChannelMemberStatus, ChatEventAction, MessageSender, ParsedChatEvent,
};

impl QuillApp {
    /// Rows for events listed oldest first. A day label goes on the first
    /// row of each day, as the chat history groups its messages.
    pub(super) fn event_log_entries(&self, events: &[ParsedChatEvent]) -> Vec<LogEntry> {
        let now = civil_local(now_unix());
        let mut entries = Vec::with_capacity(events.len());
        let mut last_day = None;
        for event in events {
            let at = civil_local(i64::from(event.date));
            let day = at.day_number();
            let label = (last_day != Some(day)).then(|| day_label(&at, &now));
            last_day = Some(day);
            let actor = self.group_call_participant_name(&event.member_id);
            let photo = match &event.member_id {
                MessageSender::User { user_id } => self
                    .session()
                    .and_then(|session| session.user_photo_path(*user_id))
                    .map(std::path::PathBuf::from),
                MessageSender::Chat { .. } => None,
            };
            let (phrase, body) = self.event_log_phrase(event);
            entries.push(LogEntry {
                id: event.id,
                day: label,
                time: hhmm(&at),
                line: format!("{actor} {phrase}"),
                actor_len: actor.len(),
                actor,
                photo,
                body,
            });
        }
        entries
    }

    fn event_log_user_name(&self, user_id: i64) -> String {
        self.group_call_participant_name(&MessageSender::User { user_id })
    }

    /// The action as a phrase after the actor's name, and the message the
    /// action concerns where tdesktop shows one.
    fn event_log_phrase(&self, event: &ParsedChatEvent) -> (String, LogBody) {
        let name = |sender: &MessageSender| self.group_call_participant_name(sender);
        // Short quoted excerpt; long titles/descriptions are cut rather
        // than wrapping the row.
        let quote = |text: &str| {
            let mut short: String = text.chars().take(60).collect();
            if text.chars().count() > 60 {
                short.push('…');
            }
            format!("\"{short}\"")
        };
        let message = |text: &String| LogBody::Message(text.clone());
        let phrase = match &event.action {
            ChatEventAction::MessageEdited { text, old_text, .. } => {
                return (
                    tr(keys::Q_ADMIN_LOG_EDITED_MESSAGE).to_owned(),
                    LogBody::Edited {
                        original: old_text.clone(),
                        new: text.clone(),
                    },
                );
            }
            ChatEventAction::MessageDeleted { text, .. } => {
                return (
                    tr(keys::Q_ADMIN_LOG_DELETED_MESSAGE).to_owned(),
                    message(text),
                );
            }
            ChatEventAction::MessagePinned { text, .. } => {
                return (
                    tr(keys::Q_ADMIN_LOG_PINNED_MESSAGE).to_owned(),
                    message(text),
                );
            }
            ChatEventAction::MessageUnpinned { text, .. } => {
                return (
                    tr(keys::Q_ADMIN_LOG_UNPINNED_MESSAGE).to_owned(),
                    message(text),
                );
            }
            ChatEventAction::MemberJoined => tr(keys::Q_OPS_JOINED_THE_CHAT).to_owned(),
            // B4: `chatEventPollStopped` (schema 1.8.67 line 7776) — TGX
            // `EventLogPollStopped` / `EventLogQuizStopped` copy.
            ChatEventAction::PollStopped { is_quiz } => {
                if *is_quiz {
                    tr(keys::Q_OPS_STOPPED_THE_QUIZ).to_owned()
                } else {
                    tr(keys::Q_OPS_STOPPED_THE_POLL).to_owned()
                }
            }
            ChatEventAction::MemberJoinedByInviteLink {
                invite_link_name, ..
            } => {
                if invite_link_name.is_empty() {
                    tr(keys::Q_OPS_JOINED_VIA_AN_INVITE_LINK).to_owned()
                } else {
                    tr_with(
                        keys::Q_OPS_JOINED_VIA_INVITE_LINK,
                        &[("quote", &quote(invite_link_name))],
                    )
                }
            }
            ChatEventAction::MemberJoinedByRequest {
                approver_user_id, ..
            } => {
                if *approver_user_id == 0 {
                    tr(keys::Q_OPS_JOINED_VIA_AN_APPROVED_JOIN).to_owned()
                } else {
                    tr_with(
                        keys::Q_OPS_JOIN_REQUEST_APPROVED_BY,
                        &[("user_name", &self.event_log_user_name(*approver_user_id))],
                    )
                }
            }
            ChatEventAction::MemberInvited { user_id, .. } => tr_with(
                keys::Q_ADMIN_LOG_INVITED,
                &[("name", &self.event_log_user_name(*user_id))],
            ),
            ChatEventAction::MemberPromoted {
                user_id,
                old_status,
                new_status,
            } => {
                let who = self.event_log_user_name(*user_id);
                match (old_status, new_status) {
                    (_, ChannelMemberStatus::Creator) => {
                        tr_with(keys::ADMIN_LOG_TRANSFERRED, &[("user", &who)])
                    }
                    (_, ChannelMemberStatus::Administrator) => {
                        tr_with(keys::Q_OPS_PROMOTED_TO_ADMINISTRATOR, &[("name", &who)])
                    }
                    (ChannelMemberStatus::Administrator, ChannelMemberStatus::Member) => {
                        tr_with(keys::Q_OPS_REMOVED_AS_ADMINISTRATOR, &[("name", &who)])
                    }
                    _ => tr_with(keys::Q_OPS_CHANGED_S_ROLE, &[("name", &who)]),
                }
            }
            ChatEventAction::MemberRestricted {
                member_id,
                old_status,
                new_status,
            } => {
                let who = name(member_id);
                match (old_status, new_status) {
                    (_, ChannelMemberStatus::Banned) => {
                        tr_with(keys::Q_ADMIN_LOG_BANNED, &[("name", &who)])
                    }
                    (_, ChannelMemberStatus::Restricted) => {
                        tr_with(keys::Q_ADMIN_LOG_RESTRICTED, &[("name", &who)])
                    }
                    (ChannelMemberStatus::Banned, _) => {
                        tr_with(keys::Q_ADMIN_LOG_UNBANNED, &[("name", &who)])
                    }
                    (ChannelMemberStatus::Restricted, _) => {
                        tr_with(keys::Q_OPS_LIFTED_RESTRICTIONS_ON, &[("name", &who)])
                    }
                    _ => tr_with(keys::Q_OPS_CHANGED_S_RESTRICTIONS, &[("name", &who)]),
                }
            }
            ChatEventAction::DescriptionChanged {
                new_description, ..
            } => {
                if new_description.is_empty() {
                    tr(keys::Q_OPS_CLEARED_THE_DESCRIPTION).to_owned()
                } else {
                    tr_with(
                        keys::Q_OPS_CHANGED_THE_DESCRIPTION_TO,
                        &[("quote", &quote(new_description))],
                    )
                }
            }
            ChatEventAction::PhotoChanged => tr(keys::Q_OPS_CHANGED_THE_CHAT_PHOTO).to_owned(),
            ChatEventAction::TitleChanged { new_title, .. } => tr_with(
                keys::Q_OPS_CHANGED_THE_TITLE_TO,
                &[("quote", &quote(new_title))],
            ),
            ChatEventAction::InviteLinkEdited {
                old_name, new_name, ..
            } => {
                let link = if new_name.is_empty() {
                    old_name
                } else {
                    new_name
                };
                if link.is_empty() {
                    tr(keys::Q_OPS_EDITED_AN_INVITE_LINK).to_owned()
                } else {
                    tr_with(
                        keys::ADMIN_LOG_EDITED_INVITE_LINK,
                        &[("link", &quote(link))],
                    )
                }
            }
            ChatEventAction::InviteLinkRevoked { name, .. } => {
                if name.is_empty() {
                    tr(keys::Q_OPS_REVOKED_AN_INVITE_LINK).to_owned()
                } else {
                    tr_with(keys::Q_OPS_REVOKED_INVITE_LINK, &[("quote", &quote(name))])
                }
            }
            ChatEventAction::InviteLinkDeleted { name, .. } => {
                if name.is_empty() {
                    tr(keys::Q_OPS_DELETED_AN_INVITE_LINK).to_owned()
                } else {
                    tr_with(keys::Q_OPS_DELETED_INVITE_LINK, &[("quote", &quote(name))])
                }
            }
            ChatEventAction::Unsupported { .. } => tr(keys::Q_OPS_PERFORMED_AN_ACTION).to_owned(),
        };
        (phrase, LogBody::None)
    }
}
