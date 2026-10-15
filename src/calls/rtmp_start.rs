//! Starting a live stream with another app (tdesktop `StartRtmpProcess`,
//! calls/group/calls_group_rtmp.cpp, and the start entries of
//! `FillVideoChatMenu`, window/window_peer_menu.cpp).
//!
//! A manager of a chat with no video chat opens "Stream with..." from the
//! chat's call button. tdesktop asks for the RTMP URL first
//! (`phone.getGroupCallStreamRtmpUrl`, TDLib `getVideoChatRtmpUrl`) and
//! shows the box once the answer is in; the box offers Revoke Stream Key
//! (the same request with `revoke`, TDLib `replaceVideoChatRtmpUrl`) and
//! "Start Streaming", which creates the call with the RTMP flag
//! (`createVideoChat` with `is_rtmp_stream`). A failed request only shows
//! the server error as a toast.
#![deny(clippy::indexing_slicing)]

use crate::ids::ChatId;

/// The Server URL and Stream Key of a chat's stream (tdesktop `RtmpInfo`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RtmpInfo {
    pub url: String,
    pub key: String,
}

/// One "Stream with..." process (tdesktop's `RtmpRequest`): the chat it is
/// for and what the server answered so far.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RtmpStart {
    pub chat_id: ChatId,
    /// `None` until the first answer; the box shows from then on.
    pub info: Option<RtmpInfo>,
    /// A request failed: the UI shows the server error once and clears it.
    pub failed: bool,
}

impl RtmpStart {
    pub fn new(chat_id: ChatId) -> Self {
        Self {
            chat_id,
            info: None,
            failed: false,
        }
    }

    /// An answer for `chat_id`: the box has data to show.
    pub fn accept(&mut self, chat_id: ChatId, info: RtmpInfo) -> bool {
        if self.chat_id != chat_id {
            return false;
        }
        self.info = Some(info);
        self.failed = false;
        true
    }

    /// A request for `chat_id` failed.
    pub fn fail(&mut self, chat_id: ChatId) -> bool {
        if self.chat_id != chat_id {
            return false;
        }
        self.failed = true;
        true
    }

    /// The box is on screen (tdesktop creates it on the first answer).
    pub fn box_open(&self) -> bool {
        self.info.is_some()
    }
}

/// The entries of the call menu for a chat without a video chat
/// (`FillVideoChatMenu`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartEntry {
    /// "Start video chat" / "Start live stream".
    Start,
    /// "Schedule video chat" / "Schedule live stream".
    Schedule,
    /// "Stream with...".
    StreamWith,
}

/// What `FillVideoChatMenu` and `TopBarWidget::groupCall` read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StartFacts {
    /// A group or a channel, not a user.
    pub is_group_chat: bool,
    /// `canManageGroupCall`.
    pub manager: bool,
    /// The chat has a video chat already.
    pub has_call: bool,
}

/// `HasGroupCallMenu`: the call button opens a menu instead of joining.
pub fn call_button_has_menu(facts: StartFacts) -> bool {
    facts.is_group_chat && !facts.has_call && facts.manager
}

/// The menu's entries (`FillVideoChatMenu`, without the Join entry that a
/// chat with a call gets; Quill's button joins directly).
pub fn start_entries(facts: StartFacts) -> Vec<StartEntry> {
    if !call_button_has_menu(facts) {
        return Vec::new();
    }
    vec![
        StartEntry::Start,
        StartEntry::Schedule,
        StartEntry::StreamWith,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(manager: bool, has_call: bool) -> StartFacts {
        StartFacts {
            is_group_chat: true,
            manager,
            has_call,
        }
    }

    #[test]
    fn managers_without_a_call_get_all_three_entries() {
        assert_eq!(
            start_entries(facts(true, false)),
            vec![
                StartEntry::Start,
                StartEntry::Schedule,
                StartEntry::StreamWith
            ]
        );
    }

    #[test]
    fn nobody_else_gets_a_menu() {
        assert!(start_entries(facts(false, false)).is_empty());
        assert!(start_entries(facts(true, true)).is_empty());
        assert!(
            start_entries(StartFacts {
                is_group_chat: false,
                manager: true,
                has_call: false
            })
            .is_empty()
        );
    }

    #[test]
    fn answers_for_another_chat_are_ignored() {
        let mut start = RtmpStart::new(ChatId(5));
        assert!(!start.accept(ChatId(6), RtmpInfo::default()));
        assert!(!start.box_open());
        assert!(start.accept(
            ChatId(5),
            RtmpInfo {
                url: "rtmps://x".into(),
                key: "k".into()
            }
        ));
        assert!(start.box_open());
        assert!(!start.fail(ChatId(6)));
        assert!(start.fail(ChatId(5)));
        assert!(start.failed);
    }
}
