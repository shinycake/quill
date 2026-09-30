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
    pub loading: bool,
    pub error: Option<String>,
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
/// `updateStory`); failures surface the sanitized TDLib error. One
/// shared slot: the viewer disables its management buttons while
/// `pending`, so only one call is ever in flight.
#[derive(Debug, Clone, Default)]
pub struct StoryManageState {
    pub pending: bool,
    pub error: Option<String>,
}

/// Phase 6: which info panel is open in the side panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InfoPanelTarget {
    User(i64),
    Supergroup(i64),
    /// Slice G1: basic-group info panel, keyed by basic group id.
    BasicGroup(i64),
    /// Phase D2: channel/group statistics view, keyed by chat id. The
    /// `getChatStatistics` fetch is gated on
    /// `supergroupFullInfo.can_get_statistics` before opening.
    Statistics(i64),
    /// Slice G10: community info panel, keyed by community id. The
    /// `loadCommunityFullInfo` fetch fires on open; name edits go
    /// through `TextPromptKind::CommunityName`.
    Community(i64),
}
