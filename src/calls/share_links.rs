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

use crate::ids::RequestId;

/// One of the two links: not asked for, being fetched, or answered.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum ShareLink {
    #[default]
    Idle,
    /// This request for it is in flight; only its answer or error settles
    /// the slot.
    Fetching(RequestId),
    /// The speaker link cannot exist for this viewer or call (no right to
    /// manage it, an RTMP stream). Settled from the call's facts at each
    /// click, so it is asked for once they change (promoted to admin).
    Unavailable,
    /// The answer. For the speaker link `""` means the server gave none or
    /// the request failed, as in tdesktop's `std::optional<QString>
    /// linkSpeaker`.
    Got(String),
}

impl ShareLink {
    fn text(&self) -> Option<&str> {
        match self {
            Self::Got(link) => Some(link),
            Self::Unavailable => Some(""),
            Self::Idle | Self::Fetching(_) => None,
        }
    }

    fn fetching(&self) -> bool {
        matches!(self, Self::Fetching(_))
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
        self.listener.fetching() || self.speaker.fetching()
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
    /// to "none" when it cannot exist. A planned request is in flight only
    /// once it was sent ([`Self::sent`]); one that never left stays idle
    /// and is asked for by the next click.
    pub fn plan(&mut self, can_manage: bool, rtmp: bool) -> LinkRequests {
        let possible = can_manage && !rtmp;
        if !possible {
            if matches!(self.speaker, ShareLink::Idle | ShareLink::Got(_)) {
                self.speaker = ShareLink::Unavailable;
            }
        } else if self.speaker == ShareLink::Unavailable {
            self.speaker = ShareLink::Idle;
        }
        if self.ready() || self.generating() {
            return LinkRequests::default();
        }
        LinkRequests {
            listener: self.listener_link().is_none(),
            speaker: self.speaker == ShareLink::Idle,
        }
    }

    /// A planned request went out as `request` (`speaker` = it had
    /// `can_self_unmute`).
    pub fn sent(&mut self, speaker: bool, request: RequestId) {
        let slot = if speaker {
            &mut self.speaker
        } else {
            &mut self.listener
        };
        *slot = ShareLink::Fetching(request);
    }

    /// `request` was answered with `link`. Only the slot that waits for
    /// that request takes it: the answer of a request whose links were
    /// reset meanwhile (a revoke), also when a newer click is already
    /// fetching again, is dropped.
    pub fn answered(&mut self, request: RequestId, link: String) {
        for slot in [&mut self.listener, &mut self.speaker] {
            if *slot == ShareLink::Fetching(request) {
                *slot = ShareLink::Got(link);
                return;
            }
        }
    }

    /// `request` failed. A failed speaker link is left out (tdesktop's
    /// `fail` handler); a failed listener link goes back to idle so a later
    /// click can try again. Whether it was the listener link's request,
    /// which is the failure the user is told about.
    pub fn failed(&mut self, request: RequestId) -> bool {
        if self.listener == ShareLink::Fetching(request) {
            self.listener = ShareLink::Idle;
            return true;
        }
        if self.speaker == ShareLink::Fetching(request) {
            self.speaker = ShareLink::Got(String::new());
        }
        false
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

    /// The listener and speaker requests of one click.
    const L: RequestId = RequestId(1);
    const S: RequestId = RequestId(2);

    /// A click whose planned requests all went out as `L` and `S`.
    fn click(links: &mut VideoChatShareLinks, can_manage: bool, rtmp: bool) -> LinkRequests {
        click_as(links, can_manage, rtmp, L, S)
    }

    fn click_as(
        links: &mut VideoChatShareLinks,
        can_manage: bool,
        rtmp: bool,
        listener: RequestId,
        speaker: RequestId,
    ) -> LinkRequests {
        let wanted = links.plan(can_manage, rtmp);
        if wanted.listener {
            links.sent(false, listener);
        }
        if wanted.speaker {
            links.sent(true, speaker);
        }
        wanted
    }

    const BOTH: LinkRequests = LinkRequests {
        listener: true,
        speaker: true,
    };
    const LISTENER: LinkRequests = LinkRequests {
        listener: true,
        speaker: false,
    };

    #[test]
    fn admin_asks_for_both_links_once() {
        let mut links = VideoChatShareLinks::default();
        assert_eq!(click(&mut links, true, false), BOTH);
        assert!(links.generating());
        // A second click while generating sends nothing.
        assert_eq!(links.plan(true, false), LinkRequests::default());
        links.answered(L, "https://t.me/q?videochat".into());
        assert!(!links.ready());
        links.answered(S, "https://t.me/q?videochat=hash".into());
        assert!(links.ready());
        assert!(!links.generating());
        // Ready: a click just opens the box.
        assert_eq!(links.plan(true, false), LinkRequests::default());
    }

    #[test]
    fn members_and_streams_get_only_the_listener_link() {
        let mut links = VideoChatShareLinks::default();
        assert_eq!(click(&mut links, false, false), LISTENER);
        links.answered(L, "https://t.me/q?videochat".into());
        assert!(links.ready());
        assert_eq!(links.speaker_link(), None);

        let mut stream = VideoChatShareLinks::default();
        assert!(!stream.plan(true, true).speaker);
        assert_eq!(stream.speaker, ShareLink::Unavailable);
    }

    #[test]
    fn a_member_promoted_to_admin_gets_the_speaker_link_asked_for() {
        let mut links = VideoChatShareLinks::default();
        click(&mut links, false, false);
        links.answered(L, "L".into());
        assert!(links.ready());
        // Promoted in the same call: the next click asks for the speaker link.
        assert_eq!(
            click(&mut links, true, false),
            LinkRequests {
                listener: false,
                speaker: true
            }
        );
        links.answered(S, "S".into());
        assert_eq!(links.speaker_link(), Some("S"));
    }

    #[test]
    fn an_answer_for_links_reset_meanwhile_is_dropped() {
        let mut links = VideoChatShareLinks::default();
        click(&mut links, true, false);
        // A revoke resets the links while both requests are out.
        links = VideoChatShareLinks::default();
        links.answered(L, "old".into());
        assert!(!links.failed(S));
        assert_eq!(links, VideoChatShareLinks::default());
    }

    /// The revoked link's late answer must not fill the slot a newer click
    /// is fetching, in either order of the two answers.
    #[test]
    fn a_late_answer_does_not_fill_the_next_clicks_slot() {
        let (new_l, new_s) = (RequestId(3), RequestId(4));
        for old_first in [true, false] {
            let mut links = VideoChatShareLinks::default();
            click(&mut links, true, false);
            links = VideoChatShareLinks::default();
            assert_eq!(click_as(&mut links, true, false, new_l, new_s), BOTH);
            if old_first {
                links.answered(L, "revoked".into());
                links.answered(S, "revoked-speaker".into());
                assert!(links.generating(), "the new requests are still out");
            }
            links.answered(new_l, "fresh".into());
            links.answered(new_s, "fresh-speaker".into());
            if !old_first {
                links.answered(L, "revoked".into());
                links.answered(S, "revoked-speaker".into());
            }
            assert_eq!(links.current(false), Some("fresh"));
            assert_eq!(links.current(true), Some("fresh-speaker"));
        }
    }

    #[test]
    fn an_answer_of_a_request_asked_again_after_a_reset_is_dropped() {
        let mut links = VideoChatShareLinks::default();
        click(&mut links, true, false);
        links = VideoChatShareLinks::default();
        // The revoke is followed by a new click, with new request ids.
        let wanted = links.plan(true, false);
        assert_eq!(wanted, both());
        links.sent(false, RequestId(10));
        links.sent(true, RequestId(11));
        links.answered(false, LISTENER, "revoked".into());
        links.failed(true, SPEAKER);
        assert!(links.generating(), "the old requests settle nothing");
        links.answered(false, RequestId(10), "fresh".into());
        assert_eq!(links.listener_link(), Some("fresh"));
    }

    #[test]
    fn failed_speaker_link_falls_back_to_listener() {
        let mut links = VideoChatShareLinks::default();
        click(&mut links, true, false);
        links.answered(L, "L".into());
        assert!(!links.failed(S), "not the failure the user is told about");
        assert!(links.ready());
        assert!(!links.generating());
        assert_eq!(links.current(true), Some("L"));
        assert_eq!(copy_label(&links, true), "Copy Listener Link");
    }

    #[test]
    fn failed_listener_link_allows_a_retry() {
        let mut links = VideoChatShareLinks::default();
        click(&mut links, true, false);
        links.answered(S, "S".into());
        assert!(links.failed(L));
        assert!(!links.generating());
        assert!(!links.ready());
        // Only the listener link is asked for again.
        assert_eq!(links.plan(true, false), LISTENER);
    }

    #[test]
    fn a_failed_listener_does_not_resend_the_speaker_in_flight() {
        let mut links = VideoChatShareLinks::default();
        click(&mut links, true, false);
        links.failed(L);
        // The speaker request is still out: a click asks for the listener
        // link only, and the box is not ready until both answer.
        assert!(links.generating());
        assert_eq!(links.plan(true, false), LinkRequests::default());
        links.answered(S, "S".into());
        assert_eq!(links.plan(true, false), LISTENER);
    }

    #[test]
    fn a_request_that_never_left_is_asked_again() {
        let mut links = VideoChatShareLinks::default();
        // The listener request went out and failed; the speaker request
        // was planned and never sent.
        assert_eq!(links.plan(true, false), BOTH);
        links.sent(false, L);
        links.failed(L);
        assert!(!links.generating());
        assert_eq!(links.plan(true, false), BOTH);
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
