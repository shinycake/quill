//! Story state types.
use super::*;

/// Phase 9.3: honest `postStory` UI states — pending while TDLib uploads,
/// succeeded / failed when `updateStoryPostSucceeded` /
/// `updateStoryPostFailed` arrive. Every state names the `postStory`
/// request it is about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoryPostOutcome {
    /// `postStory` answered with a `story`; `story_id` is the temporary
    /// id the succeeded/failed updates correlate against
    /// (`old_story_id` / `story.id`).
    Posting {
        request: RequestId,
        story_id: i32,
    },
    Succeeded {
        request: RequestId,
    },
    Failed {
        request: RequestId,
        message: String,
    },
}

impl StoryPostOutcome {
    /// The `postStory` call this outcome is about.
    pub fn request(&self) -> RequestId {
        match self {
            Self::Posting { request, .. }
            | Self::Succeeded { request }
            | Self::Failed { request, .. } => *request,
        }
    }
}

/// The `postStory` outcomes by request, so a composer reads only the
/// outcome of its own post and another post's outcome never replaces it.
/// `clear` runs when a composer starts a post, so it stays small.
#[derive(Debug, Clone, Default)]
pub struct StoryPostOutcomes {
    by_request: HashMap<RequestId, StoryPostOutcome>,
}

impl StoryPostOutcomes {
    pub fn get(&self, request: RequestId) -> Option<&StoryPostOutcome> {
        self.by_request.get(&request)
    }

    pub fn record(&mut self, outcome: StoryPostOutcome) {
        self.by_request.insert(outcome.request(), outcome);
    }

    /// The post that is `Posting` as the temporary story `story_id`.
    pub fn posting_as(&self, story_id: i32) -> Option<RequestId> {
        self.by_request.values().find_map(|outcome| match outcome {
            StoryPostOutcome::Posting {
                request,
                story_id: id,
            } if *id == story_id => Some(*request),
            _ => None,
        })
    }

    pub fn clear(&mut self) {
        self.by_request.clear();
    }
}

/// The `canPostStory` answers by the call they answer, so a late answer to
/// an earlier check never hides the answer the composer waits for.
#[derive(Debug, Clone, Default)]
pub struct StoryChecks {
    answers: HashMap<RequestId, Result<CanPostStoryResult, String>>,
}

impl StoryChecks {
    /// The eligibility, or why the check failed, for `request`.
    pub fn answer_to(&self, request: RequestId) -> Option<&Result<CanPostStoryResult, String>> {
        self.answers.get(&request)
    }

    pub fn record(&mut self, request: RequestId, result: Result<CanPostStoryResult, String>) {
        self.answers.insert(request, result);
    }

    pub fn clear(&mut self) {
        self.answers.clear();
    }
}

/// Phase 9.3: the composer's server-side state — the `canPostStory`
/// answers (checked before every post) and the `postStory` outcomes. The
/// "Post as" list is `StoriesState::post_as`.
#[derive(Debug, Clone, Default)]
pub struct StoryPostState {
    pub check: StoryChecks,
    pub outcomes: StoryPostOutcomes,
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
    /// A page is on its way.
    pub fn is_loading(&self) -> bool {
        self.page.is_loading()
    }

    /// Why the last page failed.
    pub fn error(&self) -> Option<&str> {
        self.page.error()
    }

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
    /// The initial `reportStory` is in flight as `request`; only its
    /// answer moves the flow.
    Checking {
        request: RequestId,
    },
    /// The user must pick a reason before the flow can continue.
    PickOption {
        title: String,
        options: Vec<ReportOption>,
    },
    /// A follow-up `reportStory` (option picked, or details submitted)
    /// is in flight as `request`; only its answer moves the flow.
    Sending {
        request: RequestId,
    },
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

impl StoryReportStage {
    /// The `reportStory` call the flow waits for, in `Checking` or `Sending`.
    pub fn waiting_for(&self) -> Option<RequestId> {
        match self {
            Self::Checking { request } | Self::Sending { request } => Some(*request),
            _ => None,
        }
    }
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

/// `editStory` / `editStoryCover` / `setStoryPrivacySettings` — all `= Ok`
/// calls, so success only clears the spinner (the edited story itself
/// arrives via `updateStory`); failures surface the sanitized TDLib error.
/// The slot (`pending`, `error`, `story`) is the status line of the latest
/// call, shown under its story. Each call is also tracked by its own
/// request (`in_flight`, `settled`): whoever sent it (the composer's Save,
/// the cover picker, the privacy editor) reads the answer to its own
/// request with `outcome`, whatever call took the slot after it.
#[derive(Debug, Clone, Default)]
pub struct StoryManageState {
    pub error: Option<String>,
    /// `(chat_id, story_id)` of the story the call or error is about.
    pub story: Option<(i64, i32)>,
    /// The call the slot shows.
    pub request: Option<RequestId>,
    /// Every call still on its way.
    in_flight: Vec<RequestId>,
    /// The latest calls to settle and their failures.
    settled: Vec<(RequestId, Option<String>)>,
}

impl StoryManageState {
    /// The account is going away: no call still on its way will be answered,
    /// so each reads as `Lost` (its sender leaves its stage) instead of
    /// `InFlight` for good.
    pub fn forget_in_flight(&mut self) {
        self.in_flight.clear();
    }
}

/// What became of one management call (`StoryManageState::outcome`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoryManageOutcome {
    /// Still on its way.
    InFlight,
    Saved,
    /// Refused; the reason is `StoryManageState::failure`.
    Failed,
    /// Never answered in this session (the account went away).
    Lost,
}

/// Settled calls kept for their senders to read.
const STORY_MANAGE_SETTLED_CAP: usize = 16;

impl StoryManageState {
    /// A call for the story went out as `request`.
    pub fn start(&mut self, request: RequestId, chat_id: ChatId, story_id: i32) {
        self.error = None;
        self.story = Some((chat_id.0, story_id));
        self.request = Some(request);
        self.in_flight.push(request);
    }

    /// The answer to `request` landed; `error` is its failure. The slot
    /// follows only when it still shows that call, but the call's own
    /// sender always gets its answer.
    pub fn settle(&mut self, request: RequestId, error: Option<String>) {
        let was_in_flight = self.in_flight.len();
        self.in_flight.retain(|id| *id != request);
        if self.in_flight.len() == was_in_flight {
            return;
        }
        if self.settled.len() >= STORY_MANAGE_SETTLED_CAP {
            self.settled.remove(0);
        }
        self.settled.push((request, error.clone()));
        if self.request == Some(request) {
            self.request = None;
            self.error = error;
        }
    }

    /// What became of `request`, matched by its id: a later call's
    /// answer is never taken for it.
    pub fn outcome(&self, request: RequestId) -> StoryManageOutcome {
        if self.in_flight.contains(&request) {
            return StoryManageOutcome::InFlight;
        }
        match self.settled.iter().find(|(id, _)| *id == request) {
            Some((_, None)) => StoryManageOutcome::Saved,
            Some((_, Some(_))) => StoryManageOutcome::Failed,
            None => StoryManageOutcome::Lost,
        }
    }

    /// Why `request` failed.
    pub fn failure(&self, request: RequestId) -> Option<&str> {
        self.settled
            .iter()
            .find(|(id, _)| *id == request)
            .and_then(|(_, error)| error.as_deref())
    }

    /// The story's call could not be made: `error` says why. The slot
    /// shows it under the story and lets go of the call it showed, so
    /// that call's answer stays off this story's line.
    pub fn fail_locally(&mut self, chat_id: i64, story_id: i32, error: String) {
        self.request = None;
        self.story = Some((chat_id, story_id));
        self.error = Some(error);
    }

    /// The slot shows a call still on its way.
    pub fn is_pending(&self) -> bool {
        self.request.is_some()
    }

    /// A call for this story is in flight.
    pub fn pending_for(&self, chat_id: i64, story_id: i32) -> bool {
        self.request.is_some() && self.story == Some((chat_id, story_id))
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
