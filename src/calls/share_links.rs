//! Speaker and listener invite links of a video chat, and what the
//! members list's invite row does.
//!
//! tdesktop `ShareInviteLinkAction` (calls/group/calls_group_settings.cpp)
//! fetches two links for a video chat in a public chat: a listener link
//! (`phone.exportGroupCallInvite` without flags) and, for whoever can
//! manage the call, a speaker link (`can_self_unmute`). An RTMP stream has
//! no speaker link, and a failed speaker request leaves it out. Once both
//! answers are in, the share box opens with a "Users with this link can
//! speak" checkbox that picks which link "Copy … Link" copies. In TDLib
//! both are `getVideoChatInviteLink` (schema 1.8.68), which "Returns invite
//! link to a video chat in a public chat".

/// One of the two links: not asked for, being fetched, or answered.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum ShareLink {
    #[default]
    Idle,
    /// Its request is in flight.
    Fetching,
    /// The answer. For the speaker link `""` means there is none (no right
    /// to manage the call, an RTMP stream, or the request failed), as in
    /// tdesktop's `std::optional<QString> linkSpeaker`.
    Got(String),
}

impl ShareLink {
    fn text(&self) -> Option<&str> {
        match self {
            Self::Got(link) => Some(link),
            Self::Idle | Self::Fetching => None,
        }
    }
}

/// The two links of the tracked video chat, as far as they are known.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VideoChatShareLinks {
    pub listener: ShareLink,
    pub speaker: ShareLink,
}

/// What a click on "Share Invite Link" needs sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LinkRequests {
    pub listener: bool,
    pub speaker: bool,
}

impl VideoChatShareLinks {
    /// The listener link, once answered (possibly empty).
    pub fn listener_link(&self) -> Option<&str> {
        self.listener.text().filter(|link| !link.is_empty())
    }

    /// Requests are in flight (tdesktop `generatingLink`).
    pub fn generating(&self) -> bool {
        self.listener == ShareLink::Fetching || self.speaker == ShareLink::Fetching
    }

    /// Both answers are in: the share box can open (tdesktop `shareReady`).
    pub fn ready(&self) -> bool {
        self.listener_link().is_some() && self.speaker.text().is_some()
    }

    /// The speaker link, when there is one.
    pub fn speaker_link(&self) -> Option<&str> {
        self.speaker.text().filter(|link| !link.is_empty())
    }

    /// The link "Copy … Link" copies: the speaker link while the checkbox
    /// is ticked and one exists, otherwise the listener link.
    pub fn current(&self, speaker_checked: bool) -> Option<&str> {
        match self.speaker_link() {
            Some(link) if speaker_checked => Some(link),
            _ => self.listener_link(),
        }
    }

    /// Plan a click (tdesktop's callback): nothing while ready or already
    /// generating; otherwise the listener link and, when the viewer can
    /// manage a non-RTMP call, the speaker link. Settles the speaker link
    /// to "none" when it cannot exist and marks the requests in flight.
    pub fn plan(&mut self, can_manage: bool, rtmp: bool) -> LinkRequests {
        if self.ready() || self.generating() {
            return LinkRequests::default();
        }
        if !can_manage || rtmp {
            self.speaker = ShareLink::Got(String::new());
        }
        let wanted = LinkRequests {
            listener: self.listener_link().is_none(),
            speaker: self.speaker == ShareLink::Idle,
        };
        if wanted.listener {
            self.listener = ShareLink::Fetching;
        }
        if wanted.speaker {
            self.speaker = ShareLink::Fetching;
        }
        wanted
    }

    /// One answer arrived (`speaker` = the request had `can_self_unmute`).
    pub fn answered(&mut self, speaker: bool, link: String) {
        let slot = if speaker {
            &mut self.speaker
        } else {
            &mut self.listener
        };
        *slot = ShareLink::Got(link);
    }

    /// A request failed. A failed speaker link is left out (tdesktop's
    /// `fail` handler); a failed listener link goes back to idle so a later
    /// click can try again.
    pub fn failed(&mut self, speaker: bool) {
        if speaker {
            self.speaker = ShareLink::Got(String::new());
        } else {
            self.listener = ShareLink::Idle;
        }
    }

    /// A planned request was never sent: back to idle, to be asked again.
    pub fn unsent(&mut self, speaker: bool) {
        let slot = if speaker {
            &mut self.speaker
        } else {
            &mut self.listener
        };
        if *slot == ShareLink::Fetching {
            *slot = ShareLink::Idle;
        }
    }
}

/// The share box's copy button (tdesktop `copyLinkText`).
pub fn copy_label(links: &VideoChatShareLinks, speaker_checked: bool) -> &'static str {
    if speaker_checked && links.speaker_link().is_some() {
        "Copy Speaker Link"
    } else {
        "Copy Listener Link"
    }
}

/// The facts the members list's invite row reads (tdesktop
/// `Members::setupAddMember` and `Panel` `addMembersRequests`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InviteRowFacts {
    /// The call is bound to a chat; otherwise it is a group call that
    /// isn't bound to one (tdesktop `conference()`).
    pub is_video_chat: bool,
    /// The chat is a broadcast channel.
    pub is_broadcast: bool,
    /// The chat is a supergroup or channel with a public username.
    pub has_username: bool,
    /// The viewer may send "other" messages there (tdesktop
    /// `CanSend(peer, ChatRestriction::SendOther)`).
    pub can_send_other: bool,
    /// The call is joined as yourself, not as a channel.
    pub joined_as_self: bool,
}

/// What the "Invite Members" row does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InviteRowAction {
    /// The invite picker.
    Invite,
    /// The speaker / listener link share box.
    ShareLink,
}

/// The row's action, or `None` when tdesktop shows no row.
pub fn invite_row_action(facts: InviteRowFacts) -> Option<InviteRowAction> {
    if !facts.is_video_chat {
        return Some(InviteRowAction::Invite);
    }
    if !facts.is_broadcast && facts.can_send_other && facts.joined_as_self {
        Some(InviteRowAction::Invite)
    } else if facts.has_username {
        Some(InviteRowAction::ShareLink)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admin_asks_for_both_links_once() {
        let mut links = VideoChatShareLinks::default();
        assert_eq!(
            links.plan(true, false),
            LinkRequests {
                listener: true,
                speaker: true
            }
        );
        assert!(links.generating());
        // A second click while generating sends nothing.
        assert_eq!(links.plan(true, false), LinkRequests::default());
        links.answered(false, "https://t.me/q?videochat".into());
        assert!(!links.ready());
        links.answered(true, "https://t.me/q?videochat=hash".into());
        assert!(links.ready());
        assert!(!links.generating());
        // Ready: a click just opens the box.
        assert_eq!(links.plan(true, false), LinkRequests::default());
    }

    #[test]
    fn members_and_streams_get_only_the_listener_link() {
        let mut links = VideoChatShareLinks::default();
        assert_eq!(
            links.plan(false, false),
            LinkRequests {
                listener: true,
                speaker: false
            }
        );
        links.answered(false, "https://t.me/q?videochat".into());
        assert!(links.ready());
        assert_eq!(links.speaker_link(), None);

        let mut stream = VideoChatShareLinks::default();
        assert!(!stream.plan(true, true).speaker);
        assert_eq!(stream.speaker, ShareLink::Got(String::new()));
    }

    #[test]
    fn failed_speaker_link_falls_back_to_listener() {
        let mut links = VideoChatShareLinks::default();
        links.plan(true, false);
        links.answered(false, "L".into());
        links.failed(true);
        assert!(links.ready());
        assert!(!links.generating());
        assert_eq!(links.current(true), Some("L"));
        assert_eq!(copy_label(&links, true), "Copy Listener Link");
    }

    #[test]
    fn failed_listener_link_allows_a_retry() {
        let mut links = VideoChatShareLinks::default();
        links.plan(true, false);
        links.answered(true, "S".into());
        links.failed(false);
        assert!(!links.generating());
        assert!(!links.ready());
        // Only the listener link is asked for again.
        assert_eq!(
            links.plan(true, false),
            LinkRequests {
                listener: true,
                speaker: false
            }
        );
    }

    #[test]
    fn a_failed_listener_does_not_resend_the_speaker_in_flight() {
        let mut links = VideoChatShareLinks::default();
        links.plan(true, false);
        links.failed(false);
        // The speaker request is still out: a click asks for the listener
        // link only, and the box is not ready until both answer.
        assert!(links.generating());
        assert_eq!(links.plan(true, false), LinkRequests::default());
        links.answered(true, "S".into());
        assert_eq!(
            links.plan(true, false),
            LinkRequests {
                listener: true,
                speaker: false
            }
        );
    }

    #[test]
    fn an_unsent_request_can_be_asked_again() {
        let mut links = VideoChatShareLinks::default();
        links.plan(true, false);
        links.failed(false);
        links.unsent(true);
        assert!(!links.generating());
        assert_eq!(
            links.plan(true, false),
            LinkRequests {
                listener: true,
                speaker: true
            }
        );
    }

    #[test]
    fn checkbox_picks_the_copied_link() {
        let links = VideoChatShareLinks {
            listener: ShareLink::Got("L".into()),
            speaker: ShareLink::Got("S".into()),
        };
        assert_eq!(links.current(true), Some("S"));
        assert_eq!(copy_label(&links, true), "Copy Speaker Link");
        assert_eq!(links.current(false), Some("L"));
        assert_eq!(copy_label(&links, false), "Copy Listener Link");
    }

    #[test]
    fn invite_row_follows_tdesktop() {
        let group = InviteRowFacts {
            is_video_chat: true,
            is_broadcast: false,
            has_username: false,
            can_send_other: true,
            joined_as_self: true,
        };
        assert_eq!(invite_row_action(group), Some(InviteRowAction::Invite));
        // Joined as a channel, or restricted: public chats share the link.
        let as_channel = InviteRowFacts {
            joined_as_self: false,
            ..group
        };
        assert_eq!(invite_row_action(as_channel), None);
        assert_eq!(
            invite_row_action(InviteRowFacts {
                has_username: true,
                ..as_channel
            }),
            Some(InviteRowAction::ShareLink)
        );
        // Broadcast channels never get the picker.
        let channel = InviteRowFacts {
            is_broadcast: true,
            ..group
        };
        assert_eq!(invite_row_action(channel), None);
        assert_eq!(
            invite_row_action(InviteRowFacts {
                has_username: true,
                ..channel
            }),
            Some(InviteRowAction::ShareLink)
        );
        // A call that isn't bound to a chat always invites.
        assert_eq!(
            invite_row_action(InviteRowFacts {
                is_video_chat: false,
                is_broadcast: false,
                has_username: false,
                can_send_other: false,
                joined_as_self: false,
            }),
            Some(InviteRowAction::Invite)
        );
    }
}
