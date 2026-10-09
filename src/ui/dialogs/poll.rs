use super::super::app::QuillApp;
use gpui_kit::component::date_picker::DatePickerState;
use gpui_kit::component::input::TextareaState;
use gpui_kit::*;
use quill::ids::{ChatId, MessageId};
use quill::poll::{POLL_OPTIONS_MIN, PollDraft};
/// Phase 4.2: poll creation dialog above the composer. Textarea entities are
/// created when the dialog opens (option rows are dynamic); the dialog
/// freezes into a validated `PollDraft` on "Create poll".
/// Slice B3: description, quiz mode (correct-option radio + explanation),
/// revoting/shuffle toggles, auto-close duration (hours), country
/// restriction, and a discard-confirmation when closing with unsent input.
pub struct PollDialog {
    pub(crate) description_input: Entity<TextareaState>,
    pub(crate) question_input: Entity<TextareaState>,
    pub(crate) option_inputs: Vec<Entity<TextareaState>>,
    pub(crate) explanation_input: Entity<TextareaState>,
    pub(crate) duration_input: Entity<TextareaState>,
    pub(crate) countries_input: Entity<TextareaState>,
    pub(crate) is_anonymous: bool,
    pub(crate) allows_multiple_answers: bool,
    pub(crate) is_quiz: bool,
    /// Dialog-row index marked as the quiz's correct option (`None` = unset).
    pub(crate) quiz_correct_row: Option<usize>,
    pub(crate) allows_revoting: bool,
    pub(crate) shuffle_options: bool,
    /// B15: "Allow Adding Options" (`inputPollTypeRegular.allow_adding_options`).
    pub(crate) allow_adding_options: bool,
    /// B15: "Hide results" (`hide_results_until_closes`).
    pub(crate) hide_results_until_closes: bool,
    /// B15: "Restrict to Subscribers" (`members_only`, channels only).
    pub(crate) members_only: bool,
    /// B15: absolute deadline picker (`close_date`); `Some` while the
    /// "Set a deadline" toggle is on.
    pub(crate) deadline: Option<Entity<DatePickerState>>,
    pub(crate) confirming_discard: bool,
}

/// B15: the inline "Add an Option" panel for an open poll that allows it
/// (`addPollOption`, schema 1.8.67 line 12920).
pub struct PollAddOption {
    pub(crate) chat_id: ChatId,
    pub(crate) message_id: MessageId,
    pub(crate) input: Entity<TextareaState>,
    pub(crate) error: Option<&'static str>,
}

impl PollDialog {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<QuillApp>) -> Self {
        let mut textarea = |cx: &mut Context<QuillApp>, placeholder: &str, rows: (usize, usize)| {
            cx.new(|cx| {
                TextareaState::new(window, cx)
                    .placeholder(placeholder)
                    .auto_grow(rows.0, rows.1)
                    .submit_on_enter(false)
            })
        };
        let question_input = textarea(cx, "Poll question", (1, 3));
        let description_input = textarea(cx, "Description (optional)", (1, 2));
        let explanation_input = textarea(cx, "Quiz explanation (optional, 200 max)", (1, 3));
        let duration_input = textarea(cx, "Close after, hours (optional)", (1, 1));
        let countries_input = textarea(cx, "Countries, e.g. US, GB (optional)", (1, 1));
        let option_inputs = (0..POLL_OPTIONS_MIN)
            .map(|index| textarea(cx, &format!("Option {}", index + 1), (1, 2)))
            .collect();
        Self {
            description_input,
            question_input,
            option_inputs,
            explanation_input,
            duration_input,
            countries_input,
            is_anonymous: true,
            allows_multiple_answers: false,
            is_quiz: false,
            quiz_correct_row: None,
            allows_revoting: true,
            shuffle_options: false,
            allow_adding_options: false,
            hide_results_until_closes: false,
            members_only: false,
            deadline: None,
            confirming_discard: false,
        }
    }

    /// Freeze the dialog inputs into a `PollDraft` (validated by the caller).
    pub(crate) fn draft(&self, cx: &App) -> PollDraft {
        let value = |input: &Entity<TextareaState>| input.read(cx).value().to_string();
        let options: Vec<String> = self.option_inputs.iter().map(&value).collect();
        // Map the marked row to its index among the usable (non-empty) options.
        let quiz_correct = self.quiz_correct_row.and_then(|row| {
            options
                .iter()
                .enumerate()
                .filter(|(_, option)| !option.trim().is_empty())
                .position(|(index, _)| index == row)
        });
        let country_codes = value(&self.countries_input)
            .split(|c: char| c == ',' || c.is_whitespace())
            .map(str::trim)
            .filter(|code| !code.is_empty())
            .map(|code| code.to_ascii_uppercase())
            .collect();
        PollDraft {
            question: value(&self.question_input),
            description: value(&self.description_input),
            options,
            is_anonymous: self.is_anonymous,
            allows_multiple_answers: self.allows_multiple_answers,
            is_quiz: self.is_quiz,
            quiz_correct,
            quiz_explanation: value(&self.explanation_input),
            allows_revoting: self.allows_revoting,
            shuffle_options: self.shuffle_options,
            duration_hours: value(&self.duration_input),
            country_codes,
            allow_adding_options: self.allow_adding_options,
            hide_results_until_closes: self.hide_results_until_closes,
            members_only: self.members_only,
            close_date: self.deadline_unix(cx).unwrap_or(0),
        }
    }

    /// The picked absolute deadline as unix seconds, if the toggle is on
    /// and the picker holds a readable date.
    pub(crate) fn deadline_unix(&self, cx: &App) -> Option<i64> {
        let picker = self.deadline.as_ref()?;
        picker
            .read(cx)
            .date_time()
            .start()
            .and_then(|value| {
                quill::schedule::parse_picker_stamp(&value.format("%Y-%m-%d %H:%M").to_string())
            })
            .map(|civil| quill::schedule::civil_to_unix(&civil))
    }

    /// `true` when closing would lose user input (drives the discard prompt).
    pub(crate) fn is_dirty(&self, cx: &App) -> bool {
        let value = |input: &Entity<TextareaState>| input.read(cx).value();
        !self.is_anonymous
            || self.allows_multiple_answers
            || self.is_quiz
            || !self.allows_revoting
            || self.shuffle_options
            || self.allow_adding_options
            || self.hide_results_until_closes
            || self.members_only
            || self.deadline.is_some()
            || !value(&self.question_input).trim().is_empty()
            || !value(&self.description_input).trim().is_empty()
            || !value(&self.explanation_input).trim().is_empty()
            || !value(&self.duration_input).trim().is_empty()
            || !value(&self.countries_input).trim().is_empty()
            || self
                .option_inputs
                .iter()
                .any(|input| !value(input).trim().is_empty())
    }
}
