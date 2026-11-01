use super::super::app::QuillApp;
use crate::ui::scheduled::TimePicker;
use gpui_kit::component::input::TextareaState;
use gpui_kit::*;
use quill::ids::{ChatId, MessageId};
use quill::lang::{keys, tr, tr_with};
use quill::poll::{
    POLL_DEFAULT_PERIOD_SECS, POLL_OPTIONS_MIN, PollClose, PollDraft, PollOptionInput,
};

/// What an option of the new poll carries beside its text.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) enum OptionMediaSlot {
    #[default]
    None,
    /// A picked video being read (duration and size) off the UI thread.
    Preparing(String),
    Ready(PollOptionInput),
}

impl OptionMediaSlot {
    /// The media, once it is ready to send.
    pub(crate) fn ready(&self) -> Option<&PollOptionInput> {
        match self {
            Self::Ready(media) => Some(media),
            _ => None,
        }
    }
}

/// The media of one option being written (a row of the new poll, or the
/// option added to an open poll), and the little UI around it.
#[derive(Default)]
pub(crate) struct OptionAttach {
    pub(crate) media: OptionMediaSlot,
    /// The attach menu is open.
    pub(crate) menu_open: bool,
    /// The link field, while an address is being typed.
    pub(crate) link_input: Option<Entity<TextareaState>>,
    /// Waiting for a sticker chosen in the sticker panel.
    pub(crate) sticker_wait: bool,
    /// The emoji panel is open for this option's field.
    pub(crate) emoji_wait: bool,
    /// A frame of a picked video, drawn while it is attached.
    pub(crate) preview: Option<std::path::PathBuf>,
}

/// What the box was opened with: tdesktop's `chosen`/`disabled` flags.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum QuizLock {
    /// A person's own poll: both types are open.
    #[default]
    Free,
    /// A bot's request button with `force_quiz`.
    Quiz,
    /// A bot's request button with `force_regular`.
    Regular,
}

impl QuizLock {
    /// What a request-poll button asks for (`force_quiz` wins).
    pub(crate) fn from_button(force_regular: bool, force_quiz: bool) -> Self {
        if force_quiz {
            Self::Quiz
        } else if force_regular {
            Self::Regular
        } else {
            Self::Free
        }
    }
}

/// The switches of the settings section, to tell an untouched box from one
/// with input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PollToggles {
    anonymous: bool,
    multiple: bool,
    add_options: bool,
    revoting: bool,
    shuffle: bool,
    quiz: bool,
    subscribers: bool,
    by_country: bool,
    limit_duration: bool,
    hide_results: bool,
}

/// The custom deadline box: a date and time picker under "Deadline".
pub(crate) struct DeadlineBox {
    pub(crate) picker: TimePicker,
}

/// Phase 4.2: poll creation dialog above the composer. Textarea entities are
/// created when the dialog opens (option rows are dynamic); the dialog
/// freezes into a validated `PollDraft` on "Create poll". The settings
/// follow tdesktop's `CreatePollBox`: switch rows, with "Limit Duration"
/// opening the "Poll ends" row and "Hide results".
pub struct PollDialog {
    pub(crate) description_input: Entity<TextareaState>,
    pub(crate) question_input: Entity<TextareaState>,
    pub(crate) option_inputs: Vec<Entity<TextareaState>>,
    /// The media of each option, by the same index as `option_inputs`.
    pub(crate) attach: Vec<OptionAttach>,
    pub(crate) explanation_input: Entity<TextareaState>,
    /// The allowed countries (ISO codes), picked in the country box.
    /// tdesktop's "Limit by Country" switch (channels); off keeps no codes.
    pub(crate) limit_by_country: bool,
    pub(crate) country_codes: Vec<String>,
    pub(crate) country_box: Option<crate::ui::country_box::CountryBox>,
    /// The opposite of tdesktop's "Show Who Voted".
    pub(crate) is_anonymous: bool,
    pub(crate) allows_multiple_answers: bool,
    pub(crate) is_quiz: bool,
    /// A bot's request button fixed the type.
    pub(crate) lock: QuizLock,
    /// A broadcast channel: votes are always anonymous and options cannot
    /// be added.
    pub(crate) in_channel: bool,
    /// Dialog-row indexes marked as the quiz's correct options.
    pub(crate) quiz_correct_rows: Vec<usize>,
    pub(crate) allows_revoting: bool,
    pub(crate) shuffle_options: bool,
    /// B15: "Allow Adding Options" (`inputPollTypeRegular.allow_adding_options`).
    pub(crate) allow_adding_options: bool,
    /// "Hide results" (`hide_results_until_closes`), under "Limit Duration".
    pub(crate) hide_results_until_closes: bool,
    /// B15: "Restrict to Subscribers" (`members_only`, channels only).
    pub(crate) members_only: bool,
    /// "Limit Duration" is on: the poll closes at `close`.
    pub(crate) limit_duration: bool,
    pub(crate) close: PollClose,
    /// The custom deadline box, while it is open.
    pub(crate) deadline_box: Option<DeadlineBox>,
    pub(crate) confirming_discard: bool,
    /// The window's height at the last frame, to cap the panel.
    pub(crate) viewport_height: Pixels,
    baseline: Option<PollToggles>,
}

/// B15: the inline "Add an Option" panel for an open poll that allows it
/// (`addPollOption`, schema 1.8.67 line 12920).
pub struct PollAddOption {
    pub(crate) chat_id: ChatId,
    pub(crate) message_id: MessageId,
    pub(crate) input: Entity<TextareaState>,
    pub(crate) attach: OptionAttach,
    pub(crate) error: Option<&'static str>,
}

impl PollDialog {
    /// A fresh box with tdesktop's defaults (`kDefaultPollCreateFlags`):
    /// who voted shown, several answers, adding options and shuffling on.
    pub(crate) fn new(
        window: &mut Window,
        cx: &mut Context<QuillApp>,
        in_channel: bool,
        lock: QuizLock,
    ) -> Self {
        let mut textarea = |cx: &mut Context<QuillApp>, placeholder: &str, rows: (usize, usize)| {
            cx.new(|cx| {
                TextareaState::new(window, cx)
                    .placeholder(placeholder)
                    .auto_grow(rows.0, rows.1)
                    .submit_on_enter(false)
            })
        };
        let question_input = textarea(cx, tr(keys::Q_MEDIA_POLL_QUESTION), (1, 3));
        let description_input = textarea(cx, tr(keys::Q_MEDIA_DESCRIPTION_OPTIONAL), (1, 2));
        let explanation_input = textarea(
            cx,
            tr(keys::Q_MEDIA_QUIZ_EXPLANATION_OPTIONAL_200_MAX),
            (1, 3),
        );
        let option_inputs = (0..POLL_OPTIONS_MIN)
            .map(|index| {
                textarea(
                    cx,
                    &tr_with(
                        keys::Q_MEDIA_OPTION,
                        &[("index_1", &(index + 1).to_string())],
                    ),
                    (1, 2),
                )
            })
            .collect();
        let mut dialog = Self {
            description_input,
            question_input,
            attach: (0..POLL_OPTIONS_MIN)
                .map(|_| OptionAttach::default())
                .collect(),
            option_inputs,
            explanation_input,
            limit_by_country: false,
            country_codes: Vec::new(),
            country_box: None,
            is_anonymous: in_channel,
            allows_multiple_answers: true,
            is_quiz: lock == QuizLock::Quiz,
            lock,
            in_channel,
            quiz_correct_rows: Vec::new(),
            allows_revoting: lock != QuizLock::Quiz,
            shuffle_options: true,
            allow_adding_options: !in_channel && lock != QuizLock::Quiz,
            hide_results_until_closes: false,
            members_only: false,
            limit_duration: false,
            close: PollClose::Period(POLL_DEFAULT_PERIOD_SECS),
            deadline_box: None,
            confirming_discard: false,
            viewport_height: window.viewport_size().height,
            baseline: None,
        };
        dialog.baseline = Some(dialog.toggles());
        dialog
    }

    fn toggles(&self) -> PollToggles {
        PollToggles {
            anonymous: self.is_anonymous,
            multiple: self.allows_multiple_answers,
            add_options: self.allow_adding_options,
            revoting: self.allows_revoting,
            shuffle: self.shuffle_options,
            quiz: self.is_quiz,
            subscribers: self.members_only,
            by_country: self.limit_by_country,
            limit_duration: self.limit_duration,
            hide_results: self.hide_results_until_closes,
        }
    }

    /// Freeze the dialog inputs into a `PollDraft` (validated by the caller).
    pub(crate) fn draft(&self, cx: &App) -> PollDraft {
        let value = |input: &Entity<TextareaState>| input.read(cx).value().to_string();
        // Options carry their custom emoji as composer markup.
        let options: Vec<String> = self
            .option_inputs
            .iter()
            .map(|input| crate::ui::composer_field::field_doc(input.read(cx)).to_markup())
            .collect();
        // Map the marked rows to their indexes among the usable
        // (non-empty) options.
        let usable: Vec<usize> = options
            .iter()
            .enumerate()
            .filter(|(_, option)| !option.trim().is_empty())
            .map(|(index, _)| index)
            .collect();
        let quiz_correct = self
            .quiz_correct_rows
            .iter()
            .filter_map(|row| usable.iter().position(|index| index == row))
            .collect();
        let country_codes = if self.limit_by_country {
            self.country_codes.clone()
        } else {
            Vec::new()
        };
        let (open_period, close_date) = match (self.limit_duration, self.close) {
            (true, PollClose::Period(secs)) => (secs, 0),
            (true, PollClose::Date(date)) => (0, date),
            (false, _) => (0, 0),
        };
        PollDraft {
            question: value(&self.question_input),
            description: value(&self.description_input),
            option_media: self
                .attach
                .iter()
                .map(|attach| attach.media.ready().cloned())
                .collect(),
            options,
            is_anonymous: self.is_anonymous,
            allows_multiple_answers: self.allows_multiple_answers,
            is_quiz: self.is_quiz,
            quiz_correct,
            quiz_explanation: value(&self.explanation_input),
            allows_revoting: self.allows_revoting,
            shuffle_options: self.shuffle_options,
            country_codes,
            allow_adding_options: self.allow_adding_options,
            hide_results_until_closes: self.limit_duration && self.hide_results_until_closes,
            members_only: self.members_only,
            open_period,
            close_date,
            close_max: 0,
        }
    }

    /// A picked video is still being read: sending now would drop it.
    pub(crate) fn media_preparing(&self) -> bool {
        self.attach
            .iter()
            .any(|attach| matches!(attach.media, OptionMediaSlot::Preparing(_)))
    }

    /// `true` when closing would lose user input (drives the discard prompt).
    pub(crate) fn is_dirty(&self, cx: &App) -> bool {
        let value = |input: &Entity<TextareaState>| input.read(cx).value();
        Some(self.toggles()) != self.baseline
            || self
                .attach
                .iter()
                .any(|attach| attach.media != OptionMediaSlot::None)
            || !value(&self.question_input).trim().is_empty()
            || !value(&self.description_input).trim().is_empty()
            || !value(&self.explanation_input).trim().is_empty()
            || self
                .option_inputs
                .iter()
                .any(|input| !value(input).trim().is_empty())
    }
}
