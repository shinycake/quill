//! What the viewer may send in a group, and the sentence that says why not.
//!
//! Telegram Desktop decides this in `Data::CanSendAnyOf` and
//! `Data::RestrictionError` (data/data_chat_participant_status.cpp): a
//! right is allowed only when the group's default rights and the member's
//! own restriction both allow it, administrators are never restricted, and
//! the refusal text depends on whether everyone is restricted ("Sending
//! photos isn't allowed in this group."), only this member is ("The admins
//! of this group restricted you from sending photos here."), or the member
//! is restricted until a date. Pure logic; the UI reads it to replace the
//! composer, to refuse a recording or an attachment, and to explain why.

use crate::composer::AttachmentKind;
use crate::state::{ChatSummary, Session};
use crate::telegram::envelope::{
    ChannelMemberStatus, ChatKind, ChatPermissions, MemberRestriction,
};

/// One thing a member can be forbidden to send.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendKind {
    /// Plain text (`ChatRestriction::SendOther`, `can_send_basic_messages`).
    Message,
    Photos,
    Videos,
    Music,
    Files,
    VoiceMessages,
    VideoMessages,
    /// Stickers and GIFs share `can_send_other_messages` in TDLib.
    Stickers,
    Gifs,
    Inline,
    Polls,
}

impl SendKind {
    /// Every kind a member can lose, polls last.
    pub const ALL: [SendKind; 11] = [
        SendKind::Message,
        SendKind::Photos,
        SendKind::Videos,
        SendKind::Music,
        SendKind::Files,
        SendKind::VoiceMessages,
        SendKind::VideoMessages,
        SendKind::Stickers,
        SendKind::Gifs,
        SendKind::Inline,
        SendKind::Polls,
    ];

    /// The `chatPermissions` flag behind this kind (schema 1.8.67).
    pub fn granted_by(self, permissions: &ChatPermissions) -> bool {
        match self {
            SendKind::Message => permissions.can_send_basic_messages,
            SendKind::Photos => permissions.can_send_photos,
            SendKind::Videos => permissions.can_send_videos,
            SendKind::Music => permissions.can_send_audios,
            SendKind::Files => permissions.can_send_documents,
            SendKind::VoiceMessages => permissions.can_send_voice_notes,
            SendKind::VideoMessages => permissions.can_send_video_notes,
            SendKind::Stickers | SendKind::Gifs | SendKind::Inline => {
                permissions.can_send_other_messages
            }
            SendKind::Polls => permissions.can_send_polls,
        }
    }

    /// The kind an attachment is sent as.
    pub fn of_attachment(kind: AttachmentKind) -> Self {
        match kind {
            AttachmentKind::Photo => SendKind::Photos,
            AttachmentKind::Video => SendKind::Videos,
            AttachmentKind::Document => SendKind::Files,
            AttachmentKind::VideoNote => SendKind::VideoMessages,
        }
    }
}

/// A restriction this far ahead counts as permanent (Telegram: more than
/// 366 days, or zero).
const FOREVER_SECS: i64 = 366 * 24 * 3600;

/// The facts the decision reads.
#[derive(Debug, Clone, Copy)]
pub struct SendRights {
    /// A basic group or supergroup. Private chats and channels have no
    /// per-kind rights to enforce here.
    pub group: bool,
    /// The viewer's own status, once known.
    pub status: Option<ChannelMemberStatus>,
    /// The group's default rights; unknown rights allow everything.
    pub defaults: Option<ChatPermissions>,
    /// The viewer's own restriction.
    pub restriction: Option<MemberRestriction>,
    /// Now, in unix seconds.
    pub now: i64,
}

impl SendRights {
    /// The viewer's rights in `chat`. Basic groups take the status from the
    /// session (`updateBasicGroup`), supergroups from the chat itself.
    pub fn for_chat(session: &Session, chat: &ChatSummary, now: i64) -> Self {
        let (group, status) = match chat.kind {
            ChatKind::BasicGroup { basic_group_id } => (
                true,
                session
                    .groups
                    .basic_group_status
                    .get(&basic_group_id)
                    .copied(),
            ),
            // Broadcast channels: only admins post, the channel footer owns it.
            ChatKind::Supergroup {
                is_channel: false, ..
            } => (true, chat.my_member_status),
            _ => (false, None),
        };
        Self {
            group,
            status,
            defaults: chat.permissions,
            restriction: chat.my_restriction,
            now,
        }
    }

    fn personal(&self) -> Option<&MemberRestriction> {
        // A date in the past means the restriction is over even if the
        // server has not told us yet.
        self.restriction
            .as_ref()
            .filter(|r| r.until_date == 0 || i64::from(r.until_date) > self.now)
    }

    fn exempt(&self) -> bool {
        !self.group
            || matches!(
                self.status,
                Some(ChannelMemberStatus::Creator | ChannelMemberStatus::Administrator)
            )
    }

    fn denied_for_everyone(&self, kind: SendKind) -> bool {
        self.defaults
            .as_ref()
            .is_some_and(|defaults| !kind.granted_by(defaults))
    }

    fn denied_for_me(&self, kind: SendKind) -> bool {
        self.personal()
            .is_some_and(|restriction| !kind.granted_by(&restriction.permissions))
    }

    /// `Data::CanSend` for one kind.
    pub fn allows(&self, kind: SendKind) -> bool {
        self.exempt() || !(self.denied_for_everyone(kind) || self.denied_for_me(kind))
    }

    /// `!Data::CanSendAnyOf(all but polls)`: nothing can be sent at all, so
    /// the composer gives way to the sentence.
    pub fn blocks_everything(&self) -> bool {
        !self.exempt()
            && SendKind::ALL
                .iter()
                .filter(|kind| **kind != SendKind::Polls)
                .all(|kind| !self.allows(*kind))
    }

    /// `Data::RestrictionError`: why `kind` is refused, or `None` when it
    /// is allowed. `until_text` formats a restriction's end date.
    pub fn denial(&self, kind: SendKind, until_text: &dyn Fn(i32) -> String) -> Option<String> {
        if self.allows(kind) {
            return None;
        }
        let everyone = self.denied_for_everyone(kind);
        if !everyone
            && let Some(restriction) = self.personal()
            && restriction.until_date > 0
            && i64::from(restriction.until_date) - self.now <= FOREVER_SECS
        {
            return Some(until_sentence(kind, &until_text(restriction.until_date)));
        }
        Some(
            if everyone {
                everyone_sentence(kind)
            } else {
                personal_sentence(kind)
            }
            .to_string(),
        )
    }

    /// The sentence that replaces the composer, if nothing can be sent
    /// (`HistoryWidget::computeSendRestriction`).
    pub fn composer_block(&self, until_text: &dyn Fn(i32) -> String) -> Option<String> {
        self.blocks_everything()
            .then(|| self.denial(SendKind::Message, until_text))
            .flatten()
    }

    /// The first refusal among the kinds an attachment list needs
    /// (`Data::FileRestrictionError`).
    pub fn attachment_denial(
        &self,
        kinds: impl IntoIterator<Item = AttachmentKind>,
        until_text: &dyn Fn(i32) -> String,
    ) -> Option<String> {
        kinds
            .into_iter()
            .find_map(|kind| self.denial(SendKind::of_attachment(kind), until_text))
    }
}

/// `lng_restricted_send_*_all`: everyone is restricted.
fn everyone_sentence(kind: SendKind) -> &'static str {
    match kind {
        SendKind::Message => "Sending messages is not allowed in this group.",
        SendKind::Photos => "Sending photos isn't allowed in this group.",
        SendKind::Videos => "Sending videos isn't allowed in this group.",
        SendKind::Music => "Sending music isn't allowed in this group.",
        SendKind::Files => "Sending files isn't allowed in this group.",
        SendKind::VoiceMessages => "Sending voice messages isn't allowed in this group.",
        SendKind::VideoMessages => "Sending video messages isn't allowed in this group.",
        SendKind::Stickers => "Stickers aren\u{2019}t allowed in this group.",
        SendKind::Gifs => "Sending GIFs isn't allowed in this group.",
        SendKind::Inline => "Sending inline content isn't allowed in this group.",
        SendKind::Polls => "Sorry, sending polls is not allowed in this group.",
    }
}

/// `lng_restricted_send_*`: only this member is restricted.
fn personal_sentence(kind: SendKind) -> &'static str {
    match kind {
        SendKind::Message => {
            "The admins of this group have restricted your ability to send messages."
        }
        SendKind::Photos => "The admins of this group restricted you from sending photos here.",
        SendKind::Videos => "The admins of this group restricted you from sending videos here.",
        SendKind::Music => "The admins of this group restricted you from sending music here.",
        SendKind::Files => "The admins of this group restricted you from sending files here.",
        SendKind::VoiceMessages => {
            "The admins of this group restricted you from sending voice messages here."
        }
        SendKind::VideoMessages => {
            "The admins of this group restricted you from sending video messages here."
        }
        SendKind::Stickers => {
            "The admins of this group have restricted your ability to send stickers."
        }
        SendKind::Gifs => "The admins of this group have restricted your ability to send GIFs.",
        SendKind::Inline => {
            "The admins of this group have restricted your ability to send inline content."
        }
        SendKind::Polls => "The admins of this group have restricted your ability to send polls.",
    }
}

/// `lng_restricted_send_*_until`: restricted until `when`.
fn until_sentence(kind: SendKind, when: &str) -> String {
    match kind {
        SendKind::Message => {
            format!(
                "The admins of this group have restricted you from sending messages until {when}."
            )
        }
        SendKind::Photos => {
            format!(
                "The admins of this group restricted you from sending photos here until {when}."
            )
        }
        SendKind::Videos => {
            format!(
                "The admins of this group restricted you from sending videos here until {when}."
            )
        }
        SendKind::Music => {
            format!("The admins of this group restricted you from sending music here until {when}.")
        }
        SendKind::Files => {
            format!("The admins of this group restricted you from sending files here until {when}.")
        }
        SendKind::VoiceMessages => format!(
            "The admins of this group restricted you from sending voice messages here until {when}."
        ),
        SendKind::VideoMessages => format!(
            "The admins of this group restricted you from sending video messages here until {when}."
        ),
        SendKind::Stickers => format!(
            "The admins of this group have restricted your ability to send stickers until {when}."
        ),
        SendKind::Gifs => format!(
            "The admins of this group have restricted your ability to send GIFs until {when}."
        ),
        SendKind::Inline => format!(
            "The admins of this group have restricted your ability to send inline content until {when}."
        ),
        SendKind::Polls => format!(
            "The admins of this group have restricted your ability to send polls until {when}."
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_800_000_000;

    fn when(unix: i32) -> String {
        format!("T{unix}")
    }

    fn rights() -> SendRights {
        SendRights {
            group: true,
            status: Some(ChannelMemberStatus::Member),
            defaults: Some(ChatPermissions::all()),
            restriction: None,
            now: NOW,
        }
    }

    fn only(keep: impl FnOnce(&mut ChatPermissions)) -> ChatPermissions {
        let mut none = ChatPermissions::default();
        keep(&mut none);
        none
    }

    #[test]
    fn unrestricted_members_can_send_everything() {
        let r = rights();
        assert!(SendKind::ALL.iter().all(|kind| r.allows(*kind)));
        assert!(!r.blocks_everything());
        assert_eq!(r.composer_block(&when), None);
    }

    #[test]
    fn unknown_defaults_never_block() {
        let r = SendRights {
            defaults: None,
            ..rights()
        };
        assert!(SendKind::ALL.iter().all(|kind| r.allows(*kind)));
    }

    #[test]
    fn administrators_and_private_chats_are_exempt() {
        for status in [
            ChannelMemberStatus::Creator,
            ChannelMemberStatus::Administrator,
        ] {
            let r = SendRights {
                status: Some(status),
                defaults: Some(ChatPermissions::default()),
                ..rights()
            };
            assert!(
                SendKind::ALL.iter().all(|kind| r.allows(*kind)),
                "{status:?}"
            );
        }
        let private = SendRights {
            group: false,
            defaults: Some(ChatPermissions::default()),
            ..rights()
        };
        assert!(private.allows(SendKind::Photos));
    }

    #[test]
    fn default_restrictions_use_the_everyone_wording() {
        let r = SendRights {
            defaults: Some(only(|p| {
                p.can_send_basic_messages = true;
                p.can_send_other_messages = true;
            })),
            ..rights()
        };
        assert_eq!(
            r.denial(SendKind::Photos, &when).as_deref(),
            Some("Sending photos isn't allowed in this group.")
        );
        assert_eq!(r.denial(SendKind::Message, &when), None);
        assert_eq!(
            r.denial(SendKind::Polls, &when).as_deref(),
            Some("Sorry, sending polls is not allowed in this group.")
        );
        assert!(!r.blocks_everything());
    }

    #[test]
    fn a_personal_restriction_uses_the_admins_wording() {
        let r = SendRights {
            status: Some(ChannelMemberStatus::Restricted),
            restriction: Some(MemberRestriction {
                until_date: 0,
                permissions: only(|p| p.can_send_basic_messages = true),
            }),
            ..rights()
        };
        assert_eq!(r.denial(SendKind::Message, &when), None);
        assert_eq!(
            r.denial(SendKind::VoiceMessages, &when).as_deref(),
            Some("The admins of this group restricted you from sending voice messages here.")
        );
        assert_eq!(
            r.denial(SendKind::Stickers, &when).as_deref(),
            Some("The admins of this group have restricted your ability to send stickers.")
        );
    }

    #[test]
    fn a_temporary_restriction_names_the_end_date() {
        let until = (NOW + 3600) as i32;
        let r = SendRights {
            status: Some(ChannelMemberStatus::Restricted),
            restriction: Some(MemberRestriction {
                until_date: until,
                permissions: ChatPermissions::default(),
            }),
            ..rights()
        };
        assert_eq!(
            r.denial(SendKind::Message, &when),
            Some(format!(
                "The admins of this group have restricted you from sending messages until T{until}."
            ))
        );
        assert_eq!(
            r.composer_block(&when),
            r.denial(SendKind::Message, &when),
            "everything denied: the sentence replaces the composer"
        );
    }

    #[test]
    fn a_distant_or_zero_end_date_is_permanent() {
        for until in [0, (NOW + 400 * 24 * 3600) as i32] {
            let r = SendRights {
                restriction: Some(MemberRestriction {
                    until_date: until,
                    permissions: ChatPermissions::default(),
                }),
                ..rights()
            };
            assert_eq!(
                r.denial(SendKind::Message, &when).as_deref(),
                Some("The admins of this group have restricted your ability to send messages."),
                "until {until}"
            );
        }
    }

    #[test]
    fn an_expired_restriction_no_longer_applies() {
        let r = SendRights {
            restriction: Some(MemberRestriction {
                until_date: (NOW - 5) as i32,
                permissions: ChatPermissions::default(),
            }),
            ..rights()
        };
        assert!(SendKind::ALL.iter().all(|kind| r.allows(*kind)));
        assert!(!r.blocks_everything());
    }

    #[test]
    fn everyone_wording_wins_over_a_personal_date() {
        let r = SendRights {
            defaults: Some(ChatPermissions::default()),
            restriction: Some(MemberRestriction {
                until_date: (NOW + 60) as i32,
                permissions: ChatPermissions::default(),
            }),
            ..rights()
        };
        assert_eq!(
            r.composer_block(&when).as_deref(),
            Some("Sending messages is not allowed in this group.")
        );
    }

    #[test]
    fn polls_alone_do_not_keep_the_composer() {
        let r = SendRights {
            defaults: Some(only(|p| p.can_send_polls = true)),
            ..rights()
        };
        assert!(r.blocks_everything());
        // Any one kind besides polls keeps it.
        let r = SendRights {
            defaults: Some(only(|p| p.can_send_documents = true)),
            ..rights()
        };
        assert!(!r.blocks_everything());
    }

    #[test]
    fn both_the_defaults_and_the_member_must_allow() {
        let r = SendRights {
            defaults: Some(only(|p| p.can_send_photos = true)),
            restriction: Some(MemberRestriction {
                until_date: 0,
                permissions: only(|p| p.can_send_videos = true),
            }),
            ..rights()
        };
        assert!(!r.allows(SendKind::Photos));
        assert!(!r.allows(SendKind::Videos));
    }

    #[test]
    fn attachments_map_to_the_kind_they_are_sent_as() {
        let r = SendRights {
            defaults: Some(only(|p| p.can_send_photos = true)),
            ..rights()
        };
        assert_eq!(r.attachment_denial([AttachmentKind::Photo], &when), None);
        assert_eq!(
            r.attachment_denial([AttachmentKind::Photo, AttachmentKind::Video], &when)
                .as_deref(),
            Some("Sending videos isn't allowed in this group.")
        );
        assert_eq!(
            r.attachment_denial([AttachmentKind::Document], &when)
                .as_deref(),
            Some("Sending files isn't allowed in this group.")
        );
        assert_eq!(
            r.attachment_denial([AttachmentKind::VideoNote], &when)
                .as_deref(),
            Some("Sending video messages isn't allowed in this group.")
        );
    }
}
