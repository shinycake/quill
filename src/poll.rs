//! Phase 4.2: pure poll logic shared by the driver, the UI, and the tests.
//!
//! `setPollAnswer` takes 0-based **indexes** into `poll.options`
//! (`option_ids:vector<int32>`, schema `td_api.tl:12932`) — not the string
//! `pollOption.id`. `Poll::chosen_indexes` (in `telegram::envelope`) bridges
//! the two.

use crate::telegram::Poll;

/// Poll creation constraints (schema `inputMessagePoll`, `td_api.tl:6193`):
/// `@question` is 1–255 characters and `@options` is
/// 1–`getOption("poll_answer_count_max")` options; TDLib requires at least 2
/// usable options to create a poll through `inputMessagePoll`.
pub const POLL_QUESTION_MAX_CHARS: usize = 255;
pub const POLL_OPTIONS_MIN: usize = 2;
pub const POLL_OPTIONS_MAX: usize = 10;

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
    if poll.is_closed || index >= poll.options.len() {
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

/// A composer poll dialog frozen at "Create poll" (validated before send).
#[derive(Debug, Clone, Default)]
pub struct PollDraft {
    pub question: String,
    pub options: Vec<String>,
    pub is_anonymous: bool,
    pub allows_multiple_answers: bool,
}

impl PollDraft {
    pub fn new() -> Self {
        Self {
            options: vec![String::new(), String::new()],
            is_anonymous: true,
            allows_multiple_answers: false,
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

    /// `None` when the draft is valid; otherwise the short user-facing reason.
    pub fn validate(&self) -> Option<&'static str> {
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
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::telegram::{PollContent, PollOption, PollType};

    fn option(chosen: bool, percentage: i32) -> PollOption {
        PollOption {
            id: "opt".to_string(),
            text: "option".to_string(),
            voter_count: 0,
            vote_percentage: percentage,
            is_chosen: chosen,
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
        }
    }

    fn quiz_poll(chosen: &[usize]) -> Poll {
        let mut poll = regular_poll(chosen, false, false, false);
        poll.poll_type = PollType::Quiz {
            correct_option_ids: vec![1],
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

    #[test]
    fn poll_content_used_in_message_roundtrip() {
        let content = PollContent {
            poll: regular_poll(&[0], false, true, false),
            description: String::new(),
        };
        assert_eq!(content.poll.chosen_indexes(), vec![0]);
        assert!(content.poll.can_vote());
    }
}
