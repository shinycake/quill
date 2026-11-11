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

/// What happened to the process's last revoke request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Revoke {
    Idle,
    /// The `revoke` request waits for its answer.
    Asking(RequestId),
    /// It failed: the UI shows the server error once ([`RtmpStart::failure`]).
    Failed,
}

/// Where a "Stream with..." process stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RtmpStage {
    /// The first URL request waits for its answer; no box yet.
    Fetching { request: RequestId },
    /// It failed: the UI shows the server error once and drops the process.
    FetchFailed,
    /// The box is on screen (tdesktop creates it on the first answer).
    Ready { info: RtmpInfo, revoke: Revoke },
}

/// Which request of the process failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RtmpFailure {
    /// The first URL: no box was ever shown.
    Fetch,
    /// A revoke: the box keeps its old rows.
    Revoke,
}

/// One "Stream with..." process (tdesktop's `RtmpRequest`): the chat it is
/// for and what the server answered so far. Only the answers to the
/// requests it sent count: a late one from an earlier process for the same
/// chat does not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RtmpStart {
    pub chat_id: ChatId,
    pub stage: RtmpStage,
}

impl RtmpStart {
    /// The process sent `request` for the chat's URL.
    pub fn new(chat_id: ChatId, request: RequestId) -> Self {
        Self {
            chat_id,
            stage: RtmpStage::Fetching { request },
        }
    }

    /// The URL and key the box shows, once the server answered.
    pub fn info(&self) -> Option<&RtmpInfo> {
        match &self.stage {
            RtmpStage::Ready { info, .. } => Some(info),
            _ => None,
        }
    }

    /// A revoke request is out and not answered.
    pub fn revoke_in_flight(&self) -> bool {
        matches!(
            &self.stage,
            RtmpStage::Ready {
                revoke: Revoke::Asking(_),
                ..
            }
        )
    }

    /// The box is open with no revoke in flight: a new one may be sent.
    pub fn can_revoke(&self) -> bool {
        matches!(
            &self.stage,
            RtmpStage::Ready {
                revoke: Revoke::Idle | Revoke::Failed,
                ..
            }
        )
    }

    /// The process sent `request` for a new key (the URL with `revoke`).
    /// `false` when it is not recorded (no box, or another revoke is out):
    /// the caller checks [`Self::can_revoke`] before sending.
    pub fn revoking(&mut self, request: RequestId) -> bool {
        if !self.can_revoke() {
            return false;
        }
        if let RtmpStage::Ready { revoke, .. } = &mut self.stage {
            *revoke = Revoke::Asking(request);
        }
        true
    }

    /// The answer to `request`: the box has data to show.
    pub fn accept(&mut self, request: Option<RequestId>, answer: RtmpInfo) -> bool {
        let Some(request) = request else {
            return false;
        };
        match &mut self.stage {
            RtmpStage::Fetching { request: asked } if *asked == request => {
                self.stage = RtmpStage::Ready {
                    info: answer,
                    revoke: Revoke::Idle,
                };
                true
            }
            RtmpStage::Ready { info, revoke } if *revoke == Revoke::Asking(request) => {
                *info = answer;
                *revoke = Revoke::Idle;
                true
            }
            _ => false,
        }
    }

    /// `request` failed.
    pub fn fail(&mut self, request: Option<RequestId>) -> bool {
        let Some(request) = request else {
            return false;
        };
        match &mut self.stage {
            RtmpStage::Fetching { request: asked } if *asked == request => {
                self.stage = RtmpStage::FetchFailed;
                true
            }
            RtmpStage::Ready { revoke, .. } if *revoke == Revoke::Asking(request) => {
                *revoke = Revoke::Failed;
                true
            }
            _ => false,
        }
    }

    /// The failure to show: a failed revoke is taken once (the box keeps
    /// its rows), a failed first request stays until the UI drops the
    /// process.
    pub fn failure(&mut self) -> Option<RtmpFailure> {
        match &mut self.stage {
            RtmpStage::FetchFailed => Some(RtmpFailure::Fetch),
            RtmpStage::Ready { revoke, .. } if *revoke == Revoke::Failed => {
                *revoke = Revoke::Idle;
                Some(RtmpFailure::Revoke)
            }
            _ => None,
        }
    }

    /// The box is on screen (tdesktop creates it on the first answer).
    pub fn box_open(&self) -> bool {
        self.info().is_some()
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
        let info = RtmpInfo {
            url: "rtmps://x".into(),
            key: "k".into(),
        };
        let mut start = RtmpStart::new(ChatId(5), RequestId(1));
        // An answer to a request this process did not send.
        assert!(!start.accept(Some(RequestId(9)), info.clone()));
        assert!(!start.accept(None, info.clone()));
        assert!(!start.box_open());
        assert!(start.accept(Some(RequestId(1)), info.clone()));
        assert!(start.box_open());
        // A late failure of an earlier process's request.
        assert!(!start.fail(Some(RequestId(1))));
        assert_eq!(start.failure(), None);
        assert!(start.revoking(RequestId(2)));
        assert!(start.fail(Some(RequestId(2))));
        assert_eq!(start.failure(), Some(RtmpFailure::Revoke));
        assert_eq!(start.failure(), None, "taken once");
        assert!(start.box_open(), "the box keeps its rows");
    }

    #[test]
    fn a_second_revoke_does_not_replace_the_one_in_flight() {
        let mut start = RtmpStart::new(ChatId(5), RequestId(1));
        assert!(!start.can_revoke(), "no box yet");
        assert!(!start.revoking(RequestId(2)), "not recorded without a box");
        assert!(start.accept(Some(RequestId(1)), RtmpInfo::default()));
        assert!(start.can_revoke());
        assert!(start.revoking(RequestId(2)));
        assert!(start.revoke_in_flight());
        assert!(!start.can_revoke());
        assert!(!start.revoking(RequestId(3)), "the first stays");
        assert!(start.accept(Some(RequestId(2)), RtmpInfo::default()));
        assert!(!start.revoke_in_flight());
    }

    #[test]
    fn a_failed_first_request_leaves_the_fetching_stage() {
        let mut start = RtmpStart::new(ChatId(5), RequestId(1));
        assert!(start.fail(Some(RequestId(1))));
        assert_eq!(start.failure(), Some(RtmpFailure::Fetch));
        assert!(!start.box_open());
        assert!(!start.accept(Some(RequestId(1)), RtmpInfo::default()));
    }

    #[test]
    fn a_revoke_answer_swaps_the_key_only_for_its_request() {
        let info = |key: &str| RtmpInfo {
            url: "u".into(),
            key: key.into(),
        };
        let mut start = RtmpStart::new(ChatId(5), RequestId(1));
        start.accept(Some(RequestId(1)), info("old"));
        start.revoking(RequestId(2));
        assert!(!start.accept(Some(RequestId(3)), info("other")));
        assert!(start.accept(Some(RequestId(2)), info("new")));
        assert_eq!(start.info().map(|i| i.key.as_str()), Some("new"));
        assert!(!start.accept(Some(RequestId(2)), info("again")), "answered");
    }
}
