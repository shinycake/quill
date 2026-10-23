//! Phase 4.2: pure poll logic shared by the driver, the UI, and the tests.
//!
//! `setPollAnswer` takes 0-based **indexes** into `poll.options`
//! (`option_ids:vector<int32>`, schema `td_api.tl:12932`) — not the string
//! `pollOption.id`. `Poll::chosen_indexes` (in `telegram::envelope`) bridges
//! the two.
#![deny(clippy::indexing_slicing)]

use crate::telegram::{Poll, PollVoteRestrictionReason};

/// Poll creation constraints (schema `inputMessagePoll`, `td_api.tl:6193`):
/// `@question` is 1–255 characters and `@options` is
/// 1–`getOption("poll_answer_count_max")` options; TDLib requires at least 2
/// usable options to create a poll through `inputMessagePoll`.
pub const POLL_QUESTION_MAX_CHARS: usize = 255;
pub const POLL_OPTIONS_MIN: usize = 2;
pub const POLL_OPTIONS_MAX: usize = 10;

/// Quiz explanation limits (schema `inputPollTypeQuiz`, `td_api.tl:488`):
/// "0-200 characters with at most 2 line feeds".
pub const QUIZ_EXPLANATION_MAX_CHARS: usize = 200;
pub const QUIZ_EXPLANATION_MAX_LINE_FEEDS: usize = 2;

/// Media for a new poll option (`inputPollOption.media`), as tdesktop's
/// create box offers it: a photo or video from a file, a sticker, a link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PollOptionInput {
    /// A picked image file.
    Photo { path: String },
    /// A picked video file, with what `inputVideo` asks for.
    Video {
        path: String,
        duration: i32,
        width: i32,
        height: i32,
    },
    /// A sticker TDLib already has (`file_id`); `emoji` is for the preview.
    Sticker {
        file_id: i32,
        width: i32,
        height: i32,
        emoji: String,
    },
    /// The address as typed (trimmed).
    Link { url: String },
}

/// Longest poll option text (`kMaxOptionLength` in tdesktop's poll box and
/// `getOption("poll_answer_length_max")`'s default).
pub const POLL_OPTION_MAX_CHARS: usize = 100;

/// An absolute poll deadline must be at least a minute ahead and within a
/// year (`create_poll_box.cpp` passes `min = now + 60`, `max = now + 365d`
/// to `ChooseDateTimeBox`).
pub const POLL_DEADLINE_MIN_SECS: i64 = 60;
pub const POLL_DEADLINE_MAX_SECS: i64 = 365 * 86_400;
/// The "Limit Duration" switch starts at a day (`closePeriod = 24 * 3600`).
pub const POLL_DEFAULT_PERIOD_SECS: i64 = 24 * 3600;
/// The "Poll ends" menu's presets (`create_poll_box.cpp`).
pub const POLL_PERIOD_PRESETS: [i64; 5] = [3600, 3 * 3600, 8 * 3600, 24 * 3600, 72 * 3600];

/// How far ahead a poll may close: TDLib's `poll_open_period_max` once it
/// reported it, tdesktop's year before that.
pub fn poll_close_max(option: Option<i64>) -> i64 {
    option
        .filter(|secs| *secs > POLL_DEADLINE_MIN_SECS)
        .unwrap_or(POLL_DEADLINE_MAX_SECS)
}

/// When a poll closes: after a preset span from creation, or at a picked
/// moment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PollClose {
    Period(i64),
    Date(i64),
}

impl PollClose {
    /// The closing moment if the poll were created at `now`.
    pub fn ends_at(self, now: i64) -> i64 {
        match self {
            Self::Period(secs) => now.saturating_add(secs),
            Self::Date(date) => date,
        }
    }
}

/// Fraction of the option's percentage bar, clamped to `[0.0, 1.0]`
/// (server `vote_percentage` can be stale or out of range).
pub fn poll_bar_fraction(vote_percentage: i32) -> f32 {
    (vote_percentage as f32 / 100.0).clamp(0.0, 1.0)
}

/// Human voter-count line under the poll question ("1 vote" / "N votes").
pub fn voter_count_label(total_voter_count: i32) -> String {
    if total_voter_count == 1 {
        "1 vote".to_string()
    } else {
        format!("{total_voter_count} votes")
    }
}

/// Compute the `option_ids` to send with `setPollAnswer` after the user taps
/// option `index`, or `None` when the tap is a no-op.
///
/// Mirrors TDLib's client-side guards in `PollManager::set_poll_answer`
/// (tdlib/td, `PollManager.cpp`): a closed poll is never votable; with
/// revoting disabled, *any* `setPollAnswer` once an answer exists is
/// rejected (400 "Can't revote in a quiz" — the message predates regular
/// polls but the check covers them), and retracting (empty `option_ids`)
/// with revoting disabled is rejected ("Can't retract vote in the poll").
/// So with `allows_revoting == false` the client treats every tap after
/// the first vote as a no-op instead of sending a doomed request.
///
/// tdesktop-style semantics otherwise:
/// - quiz polls: single tap submits immediately (`Some([index])`);
/// - regular single-answer polls: tapping the chosen option retracts the
///   vote when `allows_revoting` (`Some([])`), otherwise a no-op;
/// - regular multiple-answer polls: the tap toggles the option's membership
///   in the current answer set.
pub fn poll_answer_for_tap(poll: &Poll, index: usize) -> Option<Vec<i32>> {
    // F5: restricted polls (closed / country / membership / ...) never
    // produce an answer — keeps the demo tap path in sync with the live
    // `send_poll_answer` gate.
    if poll.is_closed || poll.vote_restriction_reason.is_some() || index >= poll.options.len() {
        return None;
    }
    let index = index as i32;
    let chosen = poll.chosen_indexes();
    if !poll.allows_revoting && !chosen.is_empty() {
        // Matches TDLib: any `setPollAnswer` after a vote exists is
        // rejected when revoting is disabled.
        return None;
    }
    if matches!(poll.poll_type, crate::telegram::PollType::Quiz { .. }) {
        return Some(vec![index]);
    }
    if !poll.allows_multiple_answers {
        if chosen.contains(&index) {
            // Retract the vote (only reachable with `allows_revoting`).
            return Some(Vec::new());
        }
        return Some(vec![index]);
    }
    let was_chosen = chosen.contains(&index);
    let mut next = chosen;
    if was_chosen {
        next.retain(|&i| i != index);
    } else {
        next.push(index);
        next.sort_unstable();
    }
    Some(next)
}

/// Telegram Desktop's "Retract vote" item (`AddPollActions`): an open,
/// non-quiz poll you voted in, while revoting is allowed.
pub fn can_retract_vote(poll: &Poll) -> bool {
    !poll.is_closed
        && poll.vote_restriction_reason.is_none()
        && poll.allows_revoting
        && !matches!(poll.poll_type, crate::telegram::PollType::Quiz { .. })
        && !poll.chosen_indexes().is_empty()
}

/// Human reason for the given `pollVoteRestrictionReason*` (schema
/// `td_api.tl:494`-`:510`), surfaced where a vote tap would otherwise die
/// silently. Labels follow the schema descriptions verbatim-ish.
pub fn poll_vote_restriction_label(reason: &PollVoteRestrictionReason) -> String {
    match reason {
        PollVoteRestrictionReason::Closed => "Voting is closed".to_string(),
        PollVoteRestrictionReason::YetUnsent => "This poll hasn't been sent yet".to_string(),
        PollVoteRestrictionReason::Scheduled => {
            "Voting opens when the scheduled poll is sent".to_string()
        }
        PollVoteRestrictionReason::CountryRestricted { country_code }
            if !country_code.is_empty() =>
        {
            format!("Voting isn't available in your country ({country_code})")
        }
        PollVoteRestrictionReason::CountryRestricted { .. } => {
            "Voting isn't available in your country".to_string()
        }
        PollVoteRestrictionReason::MembershipRequired { .. } => {
            "You need to be a member of this chat for at least a day to vote".to_string()
        }
        PollVoteRestrictionReason::Other => {
            "Voting isn't available for you in this poll".to_string()
        }
    }
}

/// The quiz explanation to display on the poll card, if any. Per the
/// `pollTypeQuiz.explanation` doc (schema line 475-476) and TGX
/// (`TGMessagePoll.java:1085-1086`): auto-shown when the user chose an
/// *incorrect* answer. TGX also reveals it on demand via a lamp-icon tap;
/// Quill has no lamp affordance yet, so a correct answer shows no
/// explanation (documented deviation in DECISIONS.md).
pub fn quiz_explanation(poll: &Poll) -> Option<&str> {
    match &poll.poll_type {
        crate::telegram::PollType::Quiz {
            correct_option_ids,
            explanation,
        } => {
            let chosen = poll.chosen_indexes();
            let incorrect =
                !chosen.is_empty() && !chosen.iter().any(|i| correct_option_ids.contains(i));
            (incorrect && !explanation.is_empty()).then_some(explanation.as_str())
        }
        crate::telegram::PollType::Regular => None,
    }
}

/// Whether the message menu should offer "Stop poll" / "Stop quiz".
/// Mirrors TGX (`MessageView.java:707`): the item appears on an open poll
/// the user can stop — `stopPoll` is gated by
/// `messageProperties.can_be_edited` (schema line 12951), which for
/// non-admins means the user's own polls.
pub fn can_stop_poll(is_outgoing: bool, poll: &Poll) -> bool {
    is_outgoing && !poll.is_closed
}

/// Whether the "View Votes" button may be offered. `getPollVoters`
/// is only valid when `poll.can_get_voters` (schema line 12941).
pub fn can_view_poll_voters(poll: &Poll) -> bool {
    poll.can_get_voters
}

/// Poll creation entry gating: `can_send_polls` (`chatPermissions`,
/// schema `td_api.tl:1061` / `:1070`). An absent permissions block means
/// unknown (e.g. not loaded yet) — never block on unknown, the send
/// itself would 400.
pub fn chat_allows_polls(permissions: Option<&crate::telegram::ChatPermissions>) -> bool {
    permissions.is_none_or(|permissions| permissions.can_send_polls)
}

/// Results are withheld until the poll closes (`hide_results_until_closes`
/// on creation, `poll.can_see_results == false` on the wire). tdesktop shows
/// `lng_polls_results_after_close` instead of the bars.
pub fn poll_results_hidden(poll: &Poll) -> bool {
    !poll.can_see_results && !poll.is_closed
}

/// Whether the "Add an Option" row is offered: the server said
/// `messagePoll.can_add_option` and the poll is still open.
pub fn can_offer_add_option(can_add_option: bool, poll: &Poll) -> bool {
    can_add_option && !poll.is_closed
}

/// Validate a new option's text the way tdesktop's add-option field does
/// (`lng_polls_add_option_duplicate`): non-empty, within
/// [`POLL_OPTION_MAX_CHARS`], and not already present (case-insensitive).
/// Returns the trimmed text to send.
pub fn validate_new_option(poll: &Poll, text: &str) -> Result<String, &'static str> {
    let text = text.trim();
    if text.is_empty() {
        return Err("Type the option text.");
    }
    if text.chars().count() > POLL_OPTION_MAX_CHARS {
        return Err("The option is too long (100 max).");
    }
    if poll.options.len() >= POLL_OPTIONS_MAX {
        return Err("This poll already has the maximum number of options.");
    }
    let lowered = text.to_lowercase();
    if poll
        .options
        .iter()
        .any(|option| option.text.trim().to_lowercase() == lowered)
    {
        return Err("This option already exists.");
    }
    Ok(text.to_string())
}

/// Remaining time as tdesktop's poll "results in …" caption:
/// `lng_polls_results_in_days` for ≥ 1 day, otherwise a short
/// `Hh Mm` / `Mm` countdown. `None` when there is no deadline or it passed.
pub fn poll_ends_in_label(poll: &Poll, now: i64) -> Option<String> {
    if poll.is_closed || poll.close_date <= 0 {
        return None;
    }
    let left = i64::from(poll.close_date) - now;
    if left <= 0 {
        return None;
    }
    let days = left / 86_400;
    Some(if days >= 1 {
        if days == 1 {
            "ends in 1 day".to_string()
        } else {
            format!("ends in {days} days")
        }
    } else {
        let hours = left / 3600;
        let minutes = (left % 3600 + 59) / 60;
        if hours >= 1 {
            format!("ends in {hours}h {}m", minutes.min(59))
        } else {
            format!("ends in {}m", minutes.max(1))
        }
    })
}

/// A composer poll dialog frozen at "Create poll" (validated before send).
///
/// `quiz_correct` is the 0-based index into the *usable* (non-empty,
/// dialog-order) options; the dialog maps its marked row to it when
/// freezing. `country_codes` are the dialog's parsed, uppercased codes.
#[derive(Debug, Clone, Default)]
pub struct PollDraft {
    pub question: String,
    pub description: String,
    pub options: Vec<String>,
    /// The picture, clip, sticker or link attached to each option, by the
    /// same index as `options` (may be shorter).
    pub option_media: Vec<Option<PollOptionInput>>,
    pub is_anonymous: bool,
    pub allows_multiple_answers: bool,
    pub is_quiz: bool,
    pub quiz_correct: Vec<usize>,
    pub quiz_explanation: String,
    pub allows_revoting: bool,
    pub shuffle_options: bool,
    pub country_codes: Vec<String>,
    /// B15: `inputPollTypeRegular.allow_adding_options` ("Allow Adding
    /// Options"); ignored for quizzes.
    pub allow_adding_options: bool,
    /// B15: `hide_results_until_closes` ("Hide results").
    pub hide_results_until_closes: bool,
    /// B15: `members_only` ("Restrict to Subscribers", channels only).
    pub members_only: bool,
    /// `open_period`: seconds the poll stays open after it is sent; 0 =
    /// none. Set by a preset of "Poll ends".
    pub open_period: i64,
    /// B15: absolute deadline (`close_date`, unix seconds); 0 = none. Set
    /// by "Custom"; TDLib takes it or `open_period`, never both.
    pub close_date: i64,
    /// `poll_open_period_max` as TDLib reported it (0 = not yet).
    pub close_max: i64,
}

impl PollDraft {
    pub fn new() -> Self {
        Self {
            options: vec![String::new(), String::new()],
            is_anonymous: true,
            allows_multiple_answers: false,
            allows_revoting: true,
            ..Default::default()
        }
    }

    /// Non-empty option texts (trimmed), in dialog order.
    pub fn usable_options(&self) -> Vec<&str> {
        self.options
            .iter()
            .map(|option| option.trim())
            .filter(|option| !option.is_empty())
            .collect()
    }

    /// The media of each usable option, in the order of
    /// [`Self::usable_options`].
    pub fn usable_option_media(&self) -> Vec<Option<&PollOptionInput>> {
        self.options
            .iter()
            .enumerate()
            .filter(|(_, option)| !option.trim().is_empty())
            .map(|(index, _)| self.option_media.get(index).and_then(Option::as_ref))
            .collect()
    }

    /// `open_period` in seconds for `inputMessagePoll` (0 = no auto-close).
    /// Only meaningful when `validate()` is `None`. TDLib accepts either
    /// `open_period` or `close_date`, so an absolute deadline wins.
    pub fn open_period_secs(&self) -> i32 {
        if self.close_date > 0 {
            return 0;
        }
        self.open_period.clamp(0, i64::from(i32::MAX)) as i32
    }

    /// `None` when the draft is valid; otherwise the short user-facing reason.
    pub fn validate(&self) -> Option<&'static str> {
        self.validate_at(crate::local_time::now_unix())
    }

    /// [`Self::validate`] against an explicit clock (for tests).
    pub fn validate_at(&self, now: i64) -> Option<&'static str> {
        let question = self.question.trim();
        let question_len = question.chars().count();
        if question_len == 0 {
            return Some("add a poll question");
        }
        if question_len > POLL_QUESTION_MAX_CHARS {
            return Some("the question is too long (255 max)");
        }
        let options = self.usable_options();
        if options.len() < POLL_OPTIONS_MIN {
            return Some("add at least 2 options");
        }
        if options.len() > POLL_OPTIONS_MAX {
            return Some("at most 10 options");
        }
        if self.is_quiz {
            let marked = !self.quiz_correct.is_empty()
                && self.quiz_correct.iter().all(|index| *index < options.len());
            if !marked {
                return Some("mark the correct option");
            }
            let explanation_len = self.quiz_explanation.chars().count();
            if explanation_len > QUIZ_EXPLANATION_MAX_CHARS {
                return Some("the quiz explanation is too long (200 max)");
            }
            let line_feeds = self.quiz_explanation.chars().filter(|&c| c == '\n').count();
            if line_feeds > QUIZ_EXPLANATION_MAX_LINE_FEEDS {
                return Some("the quiz explanation has too many line breaks (2 max)");
            }
        }
        if self.close_date > 0 {
            if self.close_date < now + POLL_DEADLINE_MIN_SECS {
                return Some("the deadline must be in the future");
            }
            if self.close_date
                > now + poll_close_max((self.close_max > 0).then_some(self.close_max))
            {
                return Some("the deadline is too far ahead");
            }
        } else if self.open_period > 0
            && self.open_period > poll_close_max((self.close_max > 0).then_some(self.close_max))
        {
            return Some("the poll stays open too long");
        }
        for code in &self.country_codes {
            if code.len() != 2 || !code.bytes().all(|b| b.is_ascii_uppercase()) {
                return Some("country codes must be 2-letter ISO codes");
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::telegram::{PollContent, PollOption, PollType};

    fn option(chosen: bool, percentage: i32) -> PollOption {
        PollOption {
            vote_percentage: percentage,
            is_chosen: chosen,
            ..PollOption::plain("opt".to_string(), "option".to_string())
        }
    }

    fn regular_poll(chosen: &[usize], multiple: bool, revote: bool, closed: bool) -> Poll {
        let mut options = vec![option(false, 40), option(false, 60), option(false, 0)];
        for &index in chosen {
            options[index].is_chosen = true;
        }
        Poll {
            id: 7,
            question: "lunch?".to_string(),
            options,
            total_voter_count: 5,
            is_anonymous: true,
            allows_multiple_answers: multiple,
            allows_revoting: revote,
            is_closed: closed,
            poll_type: PollType::Regular,
            can_get_voters: false,
            vote_restriction_reason: None,
            can_see_results: true,
            members_only: false,
            open_period: 0,
            close_date: 0,
        }
    }

    fn quiz_poll(chosen: &[usize]) -> Poll {
        let mut poll = regular_poll(chosen, false, false, false);
        poll.poll_type = PollType::Quiz {
            correct_option_ids: vec![1],
            explanation: "Paris is the capital of France".to_string(),
        };
        poll
    }

    #[test]
    fn bar_fraction_clamps() {
        assert_eq!(poll_bar_fraction(45), 0.45);
        assert_eq!(poll_bar_fraction(0), 0.0);
        assert_eq!(poll_bar_fraction(100), 1.0);
        assert_eq!(poll_bar_fraction(150), 1.0);
        assert_eq!(poll_bar_fraction(-5), 0.0);
    }

    #[test]
    fn voter_count_label_singular() {
        assert_eq!(voter_count_label(0), "0 votes");
        assert_eq!(voter_count_label(1), "1 vote");
        assert_eq!(voter_count_label(42), "42 votes");
    }

    #[test]
    fn tap_closed_poll_is_noop() {
        let poll = regular_poll(&[], false, true, true);
        assert_eq!(poll_answer_for_tap(&poll, 1), None);
    }

    #[test]
    fn tap_out_of_range_is_noop() {
        let poll = regular_poll(&[], false, true, false);
        assert_eq!(poll_answer_for_tap(&poll, 9), None);
    }

    #[test]
    fn single_answer_tap_votes() {
        let poll = regular_poll(&[], false, true, false);
        assert_eq!(poll_answer_for_tap(&poll, 2), Some(vec![2]));
    }

    #[test]
    fn single_answer_tap_on_chosen_retracts_with_revoting() {
        let poll = regular_poll(&[1], false, true, false);
        assert_eq!(poll_answer_for_tap(&poll, 1), Some(vec![]));
    }

    #[test]
    fn single_answer_tap_on_chosen_is_noop_without_revoting() {
        let poll = regular_poll(&[1], false, false, false);
        assert_eq!(poll_answer_for_tap(&poll, 1), None);
    }

    #[test]
    fn single_answer_tap_switch_is_noop_without_revoting() {
        // TDLib rejects any `setPollAnswer` once a vote exists and
        // revoting is disabled (400 "Can't revote in a quiz"), so the
        // client treats the tap as a no-op instead of sending it.
        let poll = regular_poll(&[0], false, false, false);
        assert_eq!(poll_answer_for_tap(&poll, 2), None);
    }

    #[test]
    fn multiple_answers_tap_toggles() {
        let poll = regular_poll(&[0], true, true, false);
        assert_eq!(poll_answer_for_tap(&poll, 2), Some(vec![0, 2]));
        let poll = regular_poll(&[0, 2], true, true, false);
        assert_eq!(poll_answer_for_tap(&poll, 2), Some(vec![0]));
    }

    #[test]
    fn multiple_answers_tap_deselects_last() {
        let poll = regular_poll(&[1], true, true, false);
        assert_eq!(poll_answer_for_tap(&poll, 1), Some(vec![]));
    }

    #[test]
    fn multiple_answers_change_is_noop_without_revoting() {
        // Same TDLib guard as single-answer: with revoting disabled, any
        // change (adding or removing an option) is rejected, so no request.
        let poll = regular_poll(&[0], true, false, false);
        assert_eq!(poll_answer_for_tap(&poll, 2), None);
        assert_eq!(poll_answer_for_tap(&poll, 0), None);
    }

    #[test]
    fn multiple_answers_first_vote_without_revoting() {
        // No vote exists yet, so the first tap still goes through.
        let poll = regular_poll(&[], true, false, false);
        assert_eq!(poll_answer_for_tap(&poll, 1), Some(vec![1]));
    }

    #[test]
    fn quiz_tap_submits_single_answer() {
        let poll = quiz_poll(&[]);
        assert_eq!(poll_answer_for_tap(&poll, 1), Some(vec![1]));
    }

    #[test]
    fn quiz_retap_without_revoting_is_noop() {
        let poll = quiz_poll(&[1]);
        assert_eq!(poll_answer_for_tap(&poll, 1), None);
    }

    #[test]
    fn quiz_change_answer_without_revoting_is_noop() {
        // TDLib's "Can't revote in a quiz" guard: any new answer once one
        // exists is rejected when revoting is disabled.
        let poll = quiz_poll(&[1]);
        assert_eq!(poll_answer_for_tap(&poll, 2), None);
    }

    #[test]
    fn restriction_labels_cover_all_reasons() {
        use PollVoteRestrictionReason::*;
        assert_eq!(poll_vote_restriction_label(&Closed), "Voting is closed");
        assert_eq!(
            poll_vote_restriction_label(&YetUnsent),
            "This poll hasn't been sent yet"
        );
        assert_eq!(
            poll_vote_restriction_label(&Scheduled),
            "Voting opens when the scheduled poll is sent"
        );
        assert_eq!(
            poll_vote_restriction_label(&CountryRestricted {
                country_code: "US".to_string()
            }),
            "Voting isn't available in your country (US)"
        );
        assert_eq!(
            poll_vote_restriction_label(&CountryRestricted {
                country_code: String::new()
            }),
            "Voting isn't available in your country"
        );
        assert_eq!(
            poll_vote_restriction_label(&MembershipRequired { chat_id: 9 }),
            "You need to be a member of this chat for at least a day to vote"
        );
        assert_eq!(
            poll_vote_restriction_label(&Other),
            "Voting isn't available for you in this poll"
        );
    }

    #[test]
    fn quiz_explanation_only_after_answering() {
        // Unanswered quiz with a server-side explanation: hidden (schema:
        // "empty for a yet unanswered poll").
        let poll = quiz_poll(&[]);
        assert_eq!(quiz_explanation(&poll), None);
        // Answered quiz: shown.
        let poll = quiz_poll(&[0]);
        assert_eq!(
            quiz_explanation(&poll),
            Some("Paris is the capital of France")
        );
        // Regular polls never have one.
        let poll = regular_poll(&[1], false, false, false);
        assert_eq!(quiz_explanation(&poll), None);
    }

    #[test]
    fn quiz_explanation_empty_is_hidden() {
        let mut poll = quiz_poll(&[0]);
        if let PollType::Quiz { explanation, .. } = &mut poll.poll_type {
            explanation.clear();
        }
        assert_eq!(quiz_explanation(&poll), None);
    }

    #[test]
    fn stop_offer_only_on_open_own_poll() {
        let open = regular_poll(&[], false, false, false);
        let closed = regular_poll(&[], false, false, true);
        assert!(can_stop_poll(true, &open));
        assert!(!can_stop_poll(true, &closed));
        assert!(!can_stop_poll(false, &open));
        assert!(!can_stop_poll(false, &closed));
    }

    #[test]
    fn retract_offer_needs_an_open_revotable_regular_poll_with_a_vote() {
        let voted = regular_poll(&[1], false, true, false);
        assert!(can_retract_vote(&voted));
        // No vote yet, revoting off, closed, or a quiz: nothing to retract.
        assert!(!can_retract_vote(&regular_poll(&[], false, true, false)));
        assert!(!can_retract_vote(&regular_poll(&[1], false, false, false)));
        assert!(!can_retract_vote(&regular_poll(&[1], false, true, true)));
        let mut quiz = quiz_poll(&[0]);
        quiz.allows_revoting = true;
        assert!(!can_retract_vote(&quiz));
        let mut restricted = regular_poll(&[1], false, true, false);
        restricted.vote_restriction_reason = Some(PollVoteRestrictionReason::Closed);
        assert!(!can_retract_vote(&restricted));
    }

    #[test]
    fn voters_offer_follows_can_get_voters() {
        let mut poll = regular_poll(&[], false, false, false);
        assert!(!can_view_poll_voters(&poll));
        poll.can_get_voters = true;
        assert!(can_view_poll_voters(&poll));
    }

    #[test]
    fn poll_entry_allowed_unless_permissions_deny() {
        use crate::telegram::ChatPermissions;
        // Unknown (not loaded yet) never blocks.
        assert!(chat_allows_polls(None));
        let mut permissions = ChatPermissions::all();
        assert!(chat_allows_polls(Some(&permissions)));
        permissions.can_send_polls = false;
        assert!(!chat_allows_polls(Some(&permissions)));
    }

    #[test]
    fn draft_needs_question_and_two_options() {
        let mut draft = PollDraft::new();
        assert_eq!(draft.validate(), Some("add a poll question"));
        draft.question = "pick".to_string();
        assert_eq!(draft.validate(), Some("add at least 2 options"));
        draft.options = vec!["a".to_string(), "b".to_string()];
        assert_eq!(draft.validate(), None);
    }

    #[test]
    fn draft_rejects_long_question_and_too_many_options() {
        let mut draft = PollDraft::new();
        draft.question = "x".repeat(256);
        draft.options = vec!["a".to_string(), "b".to_string()];
        assert_eq!(draft.validate(), Some("the question is too long (255 max)"));
        draft.question = "pick".to_string();
        draft.options = (0..11).map(|i| format!("opt{i}")).collect();
        assert_eq!(draft.validate(), Some("at most 10 options"));
    }

    fn quiz_draft() -> PollDraft {
        PollDraft {
            question: "capital of France?".to_string(),
            options: vec!["Paris".to_string(), "London".to_string()],
            is_quiz: true,
            quiz_correct: vec![0],
            allows_revoting: false,
            ..Default::default()
        }
    }

    #[test]
    fn quiz_draft_requires_correct_option() {
        let mut draft = quiz_draft();
        assert_eq!(draft.validate(), None);
        draft.quiz_correct = Vec::new();
        assert_eq!(draft.validate(), Some("mark the correct option"));
        // Marked row maps outside the usable options (e.g. the marked row
        // was emptied after marking).
        draft.quiz_correct = vec![7];
        assert_eq!(draft.validate(), Some("mark the correct option"));
    }

    #[test]
    fn quiz_explanation_length_and_line_feeds() {
        let mut draft = quiz_draft();
        draft.quiz_explanation = "x".repeat(200);
        assert_eq!(draft.validate(), None);
        draft.quiz_explanation = "x".repeat(201);
        assert_eq!(
            draft.validate(),
            Some("the quiz explanation is too long (200 max)")
        );
        draft.quiz_explanation = "a\nb\nc".to_string();
        assert_eq!(draft.validate(), None);
        draft.quiz_explanation = "a\nb\nc\nd".to_string();
        assert_eq!(
            draft.validate(),
            Some("the quiz explanation has too many line breaks (2 max)")
        );
    }

    #[test]
    fn duration_bounds() {
        let mut draft = quiz_draft();
        draft.is_quiz = false;
        draft.quiz_correct = Vec::new();
        assert_eq!(draft.validate(), None);
        assert_eq!(draft.open_period_secs(), 0);
        draft.open_period = 3 * 3600;
        assert_eq!(draft.validate(), None);
        assert_eq!(draft.open_period_secs(), 3 * 3600);
        // TDLib's reported cap wins over the year.
        draft.close_max = 3600;
        assert_eq!(draft.validate(), Some("the poll stays open too long"));
        draft.close_max = 0;
        draft.open_period = POLL_DEADLINE_MAX_SECS + 1;
        assert_eq!(draft.validate(), Some("the poll stays open too long"));
    }

    #[test]
    fn close_max_prefers_the_option() {
        assert_eq!(poll_close_max(None), POLL_DEADLINE_MAX_SECS);
        assert_eq!(poll_close_max(Some(0)), POLL_DEADLINE_MAX_SECS);
        assert_eq!(poll_close_max(Some(864_000)), 864_000);
    }

    #[test]
    fn close_resolves_against_the_clock() {
        assert_eq!(PollClose::Period(3600).ends_at(1000), 4600);
        assert_eq!(PollClose::Date(9000).ends_at(1000), 9000);
        assert!(POLL_PERIOD_PRESETS.contains(&POLL_DEFAULT_PERIOD_SECS));
    }

    #[test]
    fn country_codes_must_be_iso() {
        let mut draft = quiz_draft();
        draft.is_quiz = false;
        draft.quiz_correct = Vec::new();
        draft.country_codes = vec!["US".to_string(), "GB".to_string()];
        assert_eq!(draft.validate(), None);
        draft.country_codes = vec!["USA".to_string()];
        assert_eq!(
            draft.validate(),
            Some("country codes must be 2-letter ISO codes")
        );
        draft.country_codes = vec!["us".to_string()];
        assert_eq!(
            draft.validate(),
            Some("country codes must be 2-letter ISO codes")
        );
    }

    #[test]
    fn hidden_results_until_close() {
        let mut poll = regular_poll(&[], false, true, false);
        assert!(!poll_results_hidden(&poll));
        poll.can_see_results = false;
        assert!(poll_results_hidden(&poll));
        poll.is_closed = true;
        assert!(!poll_results_hidden(&poll));
    }

    #[test]
    fn add_option_offer_needs_open_poll() {
        let open = regular_poll(&[], false, true, false);
        let closed = regular_poll(&[], false, true, true);
        assert!(can_offer_add_option(true, &open));
        assert!(!can_offer_add_option(false, &open));
        assert!(!can_offer_add_option(true, &closed));
    }

    #[test]
    fn new_option_validation() {
        let poll = regular_poll(&[], false, true, false);
        assert_eq!(validate_new_option(&poll, "  Ramen "), Ok("Ramen".into()));
        assert_eq!(
            validate_new_option(&poll, "   "),
            Err("Type the option text.")
        );
        assert_eq!(
            validate_new_option(&poll, &"x".repeat(101)),
            Err("The option is too long (100 max).")
        );
        // "option" is the fixture's text for every option.
        assert_eq!(
            validate_new_option(&poll, " OPTION "),
            Err("This option already exists.")
        );
        let mut full = regular_poll(&[], false, true, false);
        full.options = (0..POLL_OPTIONS_MAX)
            .map(|i| PollOption {
                id: i.to_string(),
                text: format!("o{i}"),
                ..option(false, 0)
            })
            .collect();
        assert_eq!(
            validate_new_option(&full, "more"),
            Err("This poll already has the maximum number of options.")
        );
    }

    #[test]
    fn ends_in_label_buckets() {
        let mut poll = regular_poll(&[], false, true, false);
        assert_eq!(poll_ends_in_label(&poll, 1000), None);
        poll.close_date = 1000 + 90;
        assert_eq!(
            poll_ends_in_label(&poll, 1000).as_deref(),
            Some("ends in 2m")
        );
        poll.close_date = 1000 + 3 * 3600 + 20 * 60;
        assert_eq!(
            poll_ends_in_label(&poll, 1000).as_deref(),
            Some("ends in 3h 20m")
        );
        poll.close_date = 1000 + 86_400;
        assert_eq!(
            poll_ends_in_label(&poll, 1000).as_deref(),
            Some("ends in 1 day")
        );
        poll.close_date = 1000 + 5 * 86_400 + 5;
        assert_eq!(
            poll_ends_in_label(&poll, 1000).as_deref(),
            Some("ends in 5 days")
        );
        poll.close_date = 900;
        assert_eq!(poll_ends_in_label(&poll, 1000), None);
        poll.close_date = 5000;
        poll.is_closed = true;
        assert_eq!(poll_ends_in_label(&poll, 1000), None);
    }

    #[test]
    fn absolute_deadline_validation() {
        let now = 1_000_000;
        let mut draft = quiz_draft();
        draft.is_quiz = false;
        draft.quiz_correct = Vec::new();
        draft.close_date = now + 30;
        assert_eq!(
            draft.validate_at(now),
            Some("the deadline must be in the future")
        );
        draft.close_date = now + POLL_DEADLINE_MAX_SECS + 1;
        assert_eq!(
            draft.validate_at(now),
            Some("the deadline is too far ahead")
        );
        draft.close_date = now + 3600;
        assert_eq!(draft.validate_at(now), None);
        // An absolute deadline replaces the relative duration.
        draft.open_period = 5 * 3600;
        assert_eq!(draft.validate_at(now), None);
        assert_eq!(draft.open_period_secs(), 0);
    }

    #[test]
    fn option_media_follows_the_options_that_have_text() {
        let link = |url: &str| {
            Some(PollOptionInput::Link {
                url: url.to_string(),
            })
        };
        let draft = PollDraft {
            options: vec!["a".into(), "  ".into(), "b".into(), "c".into()],
            // The blank second option's media is dropped with it.
            option_media: vec![link("1"), link("2"), None],
            ..PollDraft::new()
        };
        assert_eq!(draft.usable_options(), ["a", "b", "c"]);
        assert_eq!(
            draft.usable_option_media(),
            vec![link("1").as_ref(), None, None]
        );
    }

    #[test]
    fn poll_content_used_in_message_roundtrip() {
        let content = PollContent {
            poll: regular_poll(&[0], false, true, false),
            description: String::new(),
            can_add_option: false,
        };
        assert_eq!(content.poll.chosen_indexes(), vec![0]);
        assert!(content.poll.can_vote());
    }
}
