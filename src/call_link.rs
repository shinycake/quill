//! Voice chat, video chat and live stream links
//! (`parity:deeplink-voice-chat`).
//!
//! tdesktop `SessionNavigation::joinVoiceChatFromLink`: a `?voicechat=`,
//! `?videochat=` or `?livestream=` link on a public group or channel opens
//! that chat, then looks at its call. With no call the toast says the link is
//! invalid or expired; with one, the join confirmation
//! (`JoinConfirm::Always`) is shown and the link's hash goes out with the
//! join. Quill shows the call's join screen, whose Join button is the
//! confirmation, and sends the hash with `joinVideoChat`. Nothing joins until
//! that click, so the microphone never opens unasked.
//!
//! This module decides what the link does and holds the hash; the driver
//! (`connect::group_calls::link`) fetches the call.

use crate::telegram::envelope::ChatKind;

/// tdesktop `lng_group_invite_bad_link`.
pub const BAD_LINK_NOTE: &str = "This invite link is invalid or has expired.";

/// What a call link does once its chat is open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallLinkStep {
    /// Not a group or channel (tdesktop only follows the hash for
    /// channels): the chat just opens.
    JustOpen,
    /// The chat has no call to join.
    NoCall,
    /// The call is already joined here.
    AlreadyJoined,
    /// Another call (a private one, or a call in another chat) is on.
    Busy,
    /// Fetch the call and show its join screen with the hash.
    Offer { group_call_id: i32 },
}

/// The call that is tracked or joined right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CurrentCall {
    pub group_call_id: i32,
    pub joined: bool,
}

/// Decide what a call link does. `video_chat` is the chat's group call id,
/// `current` the tracked group call, and `private_call` says a 1:1 call is
/// on.
pub fn plan(
    kind: &ChatKind,
    video_chat: Option<i32>,
    current: Option<CurrentCall>,
    private_call: bool,
) -> CallLinkStep {
    if !matches!(kind, ChatKind::Supergroup { .. }) {
        return CallLinkStep::JustOpen;
    }
    let Some(id) = video_chat.filter(|id| *id != 0) else {
        return CallLinkStep::NoCall;
    };
    if private_call {
        return CallLinkStep::Busy;
    }
    match current {
        Some(call) if call.group_call_id == id && call.joined => CallLinkStep::AlreadyJoined,
        Some(call) if call.joined => CallLinkStep::Busy,
        _ => CallLinkStep::Offer { group_call_id: id },
    }
}

/// The line shown for a step that does not offer the call.
pub fn step_note(step: CallLinkStep, live_stream: bool) -> Option<String> {
    let what = if live_stream {
        "live stream"
    } else {
        "voice chat"
    };
    match step {
        CallLinkStep::JustOpen | CallLinkStep::Offer { .. } => None,
        CallLinkStep::NoCall => Some(BAD_LINK_NOTE.into()),
        CallLinkStep::AlreadyJoined => Some(format!("You are already in this {what}.")),
        // Busy asks first (`CallLinkConfirm`).
        CallLinkStep::Busy => None,
    }
}

/// What the viewer is in now, when a call link arrives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeaveKind {
    /// A 1:1 call.
    PrivateCall,
    /// A group call in a group (`current_is_channel` false) or channel.
    GroupCall { current_is_channel: bool },
}

/// tdesktop `Instance::confirmLeaveCurrent`: the question before leaving the
/// current call for the one a link offers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallLinkConfirm {
    pub chat_id: i64,
    pub invite_hash: String,
    pub live_stream: bool,
    pub target_is_channel: bool,
    pub leave: LeaveKind,
}

impl CallLinkConfirm {
    pub fn text(&self) -> &'static str {
        match (self.leave, self.target_is_channel) {
            (LeaveKind::PrivateCall, false) => "End your active call and join this video chat?",
            (LeaveKind::PrivateCall, true) => {
                "Do you want to end your active call and join a live stream in this channel?"
            }
            (
                LeaveKind::GroupCall {
                    current_is_channel: false,
                },
                false,
            ) => "Leave your currently active video chat and join this video chat?",
            (
                LeaveKind::GroupCall {
                    current_is_channel: false,
                },
                true,
            ) => {
                "Do you want to leave your active video chat and join a live stream in this channel?"
            }
            (
                LeaveKind::GroupCall {
                    current_is_channel: true,
                },
                false,
            ) => {
                "Do you want to leave your active live stream and join a video chat in this group?"
            }
            (
                LeaveKind::GroupCall {
                    current_is_channel: true,
                },
                true,
            ) => {
                "Do you want to leave your active live stream and join a live stream in this channel?"
            }
        }
    }

    /// The confirm button (`lng_call_bar_hangup`, `lng_group_call_leave`).
    pub fn button(&self) -> &'static str {
        match self.leave {
            LeaveKind::PrivateCall => "End call",
            LeaveKind::GroupCall { .. } => "Leave",
        }
    }
}

/// The invite hash of the call a link offered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingCallLink {
    pub group_call_id: i32,
    pub invite_hash: String,
}

impl PendingCallLink {
    /// The hash to send with a join of `group_call_id` (empty for any other
    /// call).
    pub fn hash_for(link: Option<&Self>, group_call_id: i32) -> &str {
        match link {
            Some(link) if link.group_call_id == group_call_id => &link.invite_hash,
            _ => "",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn channel() -> ChatKind {
        ChatKind::Supergroup {
            supergroup_id: 1,
            is_channel: true,
        }
    }

    fn call(id: i32) -> i32 {
        id
    }

    #[test]
    fn only_groups_and_channels_follow_the_link() {
        let user = ChatKind::Private {
            user_id: crate::ids::UserId(5),
        };
        assert_eq!(
            plan(&user, Some(call(4)), None, false),
            CallLinkStep::JustOpen
        );
    }

    #[test]
    fn no_call_is_a_bad_link() {
        assert_eq!(plan(&channel(), None, None, false), CallLinkStep::NoCall);
        assert_eq!(
            plan(&channel(), Some(call(0)), None, false),
            CallLinkStep::NoCall
        );
        assert_eq!(
            step_note(CallLinkStep::NoCall, false).as_deref(),
            Some(BAD_LINK_NOTE)
        );
    }

    #[test]
    fn a_call_is_offered_not_joined() {
        assert_eq!(
            plan(&channel(), Some(call(4)), None, false),
            CallLinkStep::Offer { group_call_id: 4 }
        );
        // The same call tracked but not joined yet still gets the offer.
        let tracked = CurrentCall {
            group_call_id: 4,
            joined: false,
        };
        assert_eq!(
            plan(&channel(), Some(call(4)), Some(tracked), false),
            CallLinkStep::Offer { group_call_id: 4 }
        );
    }

    #[test]
    fn a_running_call_blocks_the_offer() {
        let joined = |id| CurrentCall {
            group_call_id: id,
            joined: true,
        };
        assert_eq!(
            plan(&channel(), Some(call(4)), Some(joined(4)), false),
            CallLinkStep::AlreadyJoined
        );
        assert_eq!(
            plan(&channel(), Some(call(4)), Some(joined(9)), false),
            CallLinkStep::Busy
        );
        assert_eq!(
            plan(&channel(), Some(call(4)), None, true),
            CallLinkStep::Busy
        );
        assert_eq!(step_note(CallLinkStep::Busy, true), None);
    }

    #[test]
    fn the_confirmation_names_what_is_left() {
        let ask = |leave, target_is_channel| CallLinkConfirm {
            chat_id: 1,
            invite_hash: String::new(),
            live_stream: target_is_channel,
            target_is_channel,
            leave,
        };
        let private = ask(LeaveKind::PrivateCall, false);
        assert_eq!(private.button(), "End call");
        assert!(private.text().starts_with("End your active call"));
        let from_channel = ask(
            LeaveKind::GroupCall {
                current_is_channel: true,
            },
            true,
        );
        assert_eq!(from_channel.button(), "Leave");
        assert!(from_channel.text().contains("live stream in this channel"));
        let to_group = ask(
            LeaveKind::GroupCall {
                current_is_channel: false,
            },
            false,
        );
        assert!(to_group.text().ends_with("join this video chat?"));
    }

    #[test]
    fn the_hash_only_goes_with_its_own_call() {
        let link = PendingCallLink {
            group_call_id: 4,
            invite_hash: "abc".into(),
        };
        assert_eq!(PendingCallLink::hash_for(Some(&link), 4), "abc");
        assert_eq!(PendingCallLink::hash_for(Some(&link), 5), "");
        assert_eq!(PendingCallLink::hash_for(None, 4), "");
    }
}
