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

use crate::ids::{ChatId, RequestId};

/// The Server URL and Stream Key of a chat's stream (tdesktop `RtmpInfo`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RtmpInfo {
    pub url: String,
    pub key: String,
}

/// One "Stream with..." process (tdesktop's `RtmpRequest`): the chat it is
/// for, the requests it sent and what the server answered so far.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RtmpStart {
    pub chat_id: ChatId,
    /// `None` until the first answer; the box shows from then on.
    pub info: Option<RtmpInfo>,
    /// A request failed: the UI shows the server error once and clears it.
    pub failed: bool,
    /// The URL requests this process sent and still waits for. Only their
    /// answers and failures count: a late one from an earlier process for
    /// the same chat does not.
    requests: Vec<RequestId>,
}

impl RtmpStart {
    pub fn new(chat_id: ChatId) -> Self {
        Self {
            chat_id,
            info: None,
            failed: false,
            requests: Vec::new(),
        }
    }

    /// The process sent `request` (the URL, or the URL with `revoke`).
    pub fn sent(&mut self, request: RequestId) {
        self.requests.push(request);
    }

    /// Whether `request` is one this process waits for.
    pub fn waits_for(&self, request: Option<RequestId>) -> bool {
        request.is_some_and(|request| self.requests.contains(&request))
    }

    /// The answer to `request`: the box has data to show.
    pub fn accept(&mut self, request: Option<RequestId>, info: RtmpInfo) -> bool {
        if !self.waits_for(request) {
            return false;
        }
        self.requests.retain(|sent| Some(*sent) != request);
        self.info = Some(info);
        self.failed = false;
        true
    }

    /// `request` failed.
    pub fn fail(&mut self, request: Option<RequestId>) -> bool {
        if !self.waits_for(request) {
            return false;
        }
        self.requests.retain(|sent| Some(*sent) != request);
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

/// Whether the chat header shows the call button at all
/// (`TopBarWidget::updateControlsVisibility`): a manager always sees it;
/// anyone else only for a call nobody is in yet (`fullCount() == 0`; a call
/// with people in it has the Join bar instead).
pub fn call_button_visible(facts: StartFacts, call_has_participants: bool) -> bool {
    facts.is_group_chat && (facts.manager || (facts.has_call && !call_has_participants))
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
    fn the_call_button_shows_for_managers_and_empty_calls() {
        assert!(call_button_visible(facts(true, false), false));
        assert!(call_button_visible(facts(true, true), true));
        assert!(!call_button_visible(facts(false, false), false));
        assert!(call_button_visible(facts(false, true), false));
        assert!(!call_button_visible(facts(false, true), true));
        let user = StartFacts {
            is_group_chat: false,
            manager: true,
            has_call: false,
        };
        assert!(!call_button_visible(user, false));
    }

    #[test]
    fn only_this_process_answers_count() {
        let mut start = RtmpStart::new(ChatId(5));
        let info = RtmpInfo {
            url: "rtmps://x".into(),
            key: "k".into(),
        };
        // An answer to a request this process did not send.
        assert!(!start.accept(Some(RequestId(9)), info.clone()));
        assert!(!start.accept(None, info.clone()));
        assert!(!start.box_open());
        start.sent(RequestId(1));
        assert!(start.accept(Some(RequestId(1)), info));
        assert!(start.box_open());
        // A late failure of an earlier process's request.
        assert!(!start.fail(Some(RequestId(1))));
        assert!(!start.failed);
        start.sent(RequestId(2));
        assert!(start.fail(Some(RequestId(2))));
        assert!(start.failed);
    }
}
