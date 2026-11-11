//! Story state types.
use super::*;

/// Phase 9.3: honest `postStory` UI states — pending while TDLib uploads,
/// succeeded / failed when `updateStoryPostSucceeded` /
/// `updateStoryPostFailed` arrive.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum StoryPostOutcome {
    #[default]
    None,
    /// `postStory` answered with a `story`; `story_id` is the temporary
    /// id the succeeded/failed updates correlate against
    /// (`old_story_id` / `story.id`).
    Posting {
        story_id: i32,
    },
    Succeeded,
    Failed(String),
}

/// Phase 9.3: the composer's server-side state — latest `canPostStory`
/// answer (checked before every post), its last error, and the
/// `postStory` outcome.
#[derive(Debug, Clone, Default)]
pub struct StoryPostState {
    pub eligibility: Option<CanPostStoryResult>,
    pub check_error: Option<String>,
    pub outcome: StoryPostOutcome,
}

/// Phase 9.5: honest pending/failed states for `editStory` /
/// Phase 9.5: the viewers panel's accumulated `getStoryInteractions`
/// pages for one story. `next_offset` empty = no more pages.
#[derive(Debug, Clone, Default)]
pub struct StoryViewersState {
    pub chat_id: i64,
    pub story_id: i32,
    pub total_count: i32,
    pub rows: Vec<StoryInteractionView>,
    pub next_offset: String,
    /// The page in flight, or why it failed. Only the answer of the
    /// request it holds lands in `rows`.
    pub page: PageFetch,
}

impl StoryViewersState {
    /// Whether scrolling near the end should ask for the next page: one is
    /// left, none is on its way, and the last one did not fail (a failed
    /// page waits for the Retry button, not for the next scroll).
    pub fn wants_next_page(&self) -> bool {
        self.page == PageFetch::Idle && !self.next_offset.is_empty()
    }
}

/// Phase 9.5: honest `reportStory` UI states. The initial request carries
/// an empty option id; TDLib answers `OptionRequired` (reason picker),
/// then `TextRequired` (optional details), then `Ok`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoryReportStage {
    /// The initial `reportStory` is in flight.
    Checking,
    /// The user must pick a reason before the flow can continue.
    PickOption {
        title: String,
        options: Vec<ReportOption>,
    },
    /// A follow-up `reportStory` (option picked, or details submitted)
    /// is in flight.
    Sending,
    /// The server wants extra text details for `option_id`.
    TextRequired {
        option_id: String,
        is_optional: bool,
    },
    Reported,
    Failed(String),
}

/// Phase 9.5: the in-progress `reportStory` flow for one story.
#[derive(Debug, Clone)]
pub struct StoryReportFlow {
    pub chat_id: i64,
    pub story_id: i32,
    pub stage: StoryReportStage,
}

impl StoryReportFlow {
    /// Story ids are per poster, so a flow is the story's only when both match.
    pub fn is_for(&self, chat_id: i64, story_id: i32) -> bool {
        self.chat_id == chat_id && self.story_id == story_id
    }
}

/// Phase 9.5: story stealth-mode state (`updateStoryStealthMode`,
/// TDLib 1.8.67, `schema/td_api.tl:10919`). Unix timestamps; 0 = the
/// corresponding state is off.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StoryStealthMode {
    pub active_until_date: i32,
    pub cooldown_until_date: i32,
}

impl StoryStealthMode {
    /// `true` while stealth hides the user's story views (`now` is a
    /// Unix timestamp).
    pub fn is_active(&self, now: i64) -> bool {
        i64::from(self.active_until_date) > now
    }

    /// `true` while stealth cannot be re-enabled (and is not active).
    pub fn is_cooling_down(&self, now: i64) -> bool {
        !self.is_active(now) && i64::from(self.cooldown_until_date) > now
    }
}

/// `editStoryCover` / `setStoryPrivacySettings` — all `= Ok` calls, so
/// success only clears the spinner (the edited story itself arrives via
/// `updateStory`); failures surface the sanitized TDLib error. One slot:
/// a new call takes it over, and only the answer to the call it holds
/// (`request`) settles it. `story` is the story the call or the error is
/// about, so the viewer shows them under that story only.
#[derive(Debug, Clone, Default)]
pub struct StoryManageState {
    pub pending: bool,
    pub error: Option<String>,
    /// `(chat_id, story_id)` of the story the call or error is about.
    pub story: Option<(i64, i32)>,
    /// The call in flight.
    pub request: Option<RequestId>,
    /// The last call to settle and its failure, so whoever sent it can
    /// tell its own answer from a later call's.
    pub settled: Option<(RequestId, Option<String>)>,
}

/// What became of one management call (`StoryManageState::outcome`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoryManageOutcome {
    /// Still on its way.
    InFlight,
    Saved,
    /// Refused; the reason is in `StoryManageState::error`.
    Failed,
    /// Let go without an answer (another call took the slot).
    Lost,
}

impl StoryManageState {
    /// A call for the story went out as `request`.
    pub(crate) fn start(&mut self, request: RequestId, chat_id: ChatId, story_id: i32) {
        self.pending = true;
        self.error = None;
        self.story = Some((chat_id.0, story_id));
        self.request = Some(request);
    }

    /// The answer to `request` landed; `error` is its failure. An answer
    /// to a call the slot no longer holds changes nothing.
    pub(crate) fn settle(&mut self, request: RequestId, error: Option<String>) {
        if self.request != Some(request) {
            return;
        }
        self.pending = false;
        self.request = None;
        self.settled = Some((request, error.clone()));
        self.error = error;
    }

    /// What became of `request`, matched by its id: a later call's
    /// answer is never taken for it.
    pub fn outcome(&self, request: RequestId) -> StoryManageOutcome {
        if self.request == Some(request) {
            return StoryManageOutcome::InFlight;
        }
        match &self.settled {
            Some((id, None)) if *id == request => StoryManageOutcome::Saved,
            Some((id, Some(_))) if *id == request => StoryManageOutcome::Failed,
            _ => StoryManageOutcome::Lost,
        }
    }

    /// The story's call could not be made: `error` says why. Any call in
    /// flight for another story is let go, so its answer stays silent.
    pub fn fail_locally(&mut self, chat_id: i64, story_id: i32, error: String) {
        self.pending = false;
        self.request = None;
        self.story = Some((chat_id, story_id));
        self.error = Some(error);
    }

    /// A call for this story is in flight.
    pub fn pending_for(&self, chat_id: i64, story_id: i32) -> bool {
        self.pending && self.story == Some((chat_id, story_id))
    }

    /// This story's failure line.
    pub fn error_for(&self, chat_id: i64, story_id: i32) -> Option<&str> {
        self.error
            .as_deref()
            .filter(|_| self.story == Some((chat_id, story_id)))
    }
}

/// Phase 6: which info panel is open in the side panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InfoPanelTarget {
    User(i64),
    /// The Saved Messages chat's own info (tdesktop `Info::Key` for
    /// `Data::SavedMessages`), as opposed to your user profile.
    SavedMessages,
    Supergroup(i64),
    /// Slice G1: basic-group info panel, keyed by basic group id.
    BasicGroup(i64),
    /// Phase D2: channel/group statistics view, keyed by chat id. The
    /// `getChatStatistics` fetch is gated on
    /// `supergroupFullInfo.can_get_statistics` before opening.
    Statistics(i64),
    /// Slice G10: community info panel, keyed by community id. The
    /// `getCommunityFullInfo` fetch fires on open; name edits go
    /// through `TextPromptKind::CommunityName`.
    Community(i64),
    /// Revenue and affiliate pages (`crate::earn`).
    Earn(crate::earn::EarnPanel),
}

#[cfg(test)]
mod viewers_page_tests {
    use super::*;

    fn viewers(page: PageFetch, next: &str) -> StoryViewersState {
        StoryViewersState {
            chat_id: 1,
            story_id: 2,
            total_count: 10,
            rows: Vec::new(),
            next_offset: next.to_string(),
            page,
        }
    }

    /// A failed viewers page waits for Retry instead of being asked again
    /// by the next scroll (sweep 13-144).
    #[test]
    fn a_failed_page_does_not_ask_again_on_scroll() {
        assert!(viewers(PageFetch::Idle, "n").wants_next_page());
        assert!(!viewers(PageFetch::Loading(None), "n").wants_next_page());
        assert!(!viewers(PageFetch::Failed("boom".into()), "n").wants_next_page());
        assert!(!viewers(PageFetch::Idle, "").wants_next_page());
    }
}
