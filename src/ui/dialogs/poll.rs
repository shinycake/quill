use super::super::app::QuillApp;
use crate::ui::scheduled::TimePicker;
use gpui_kit::component::input::TextareaState;
use gpui_kit::*;
use quill::ids::{ChatId, MessageId};
use quill::lang::{keys, tr, tr_with};
use quill::poll::{
    DraftOption, POLL_DEFAULT_PERIOD_SECS, POLL_OPTIONS_MIN, PollClose, PollDraft, PollOptionInput,
};

/// What an option of the new poll carries beside its text.
#[derive(Default)]
pub(crate) enum OptionMediaSlot {
    #[default]
    None,
    /// A picked file being read (duration and size) off the UI thread.
    Preparing(Preparing),
    Ready(PollOptionInput),
}

/// The read of a picked file. The task is owned here: replacing or clearing
/// the slot, removing the row or closing the box cancels it, so its answer
/// can only reach a slot that still waits for it.
pub(crate) struct Preparing {
    pub(crate) path: String,
    _read: Task<()>,
}

impl Preparing {
    pub(crate) fn new(path: String, read: Task<()>) -> Self {
        Self { path, _read: read }
    }
}

impl OptionMediaSlot {
    pub(crate) fn is_none(&self) -> bool {
        matches!(self, Self::None)
    }

    /// The media, once it is ready to send.
    pub(crate) fn ready(&self) -> Option<&PollOptionInput> {
        match self {
            Self::Ready(media) => Some(media),
            _ => None,
        }
    }
}

/// What an option's attach area is doing: one thing at a time. Every mode
/// leaves to `Idle`: the pick, Done or Cancel, the media being set, the
/// sticker or emoji panel being chosen for (`PollPick`).
#[derive(Clone)]
pub(crate) enum AttachMode {
    Idle,
    /// The attach menu is open.
    Menu,
    /// The link field, while an address is being typed.
    Link(Entity<TextareaState>),
}

/// Where the next sticker or emoji picked in the media panel goes: one
/// recipient for the whole panel, named by the `OptionAttach::id` of the
/// option that asked (a row's index moves when an earlier row goes). It
/// leaves with the pick, Done or Cancel, or when the option is gone.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum PollPick {
    #[default]
    None,
    /// The next sticker clicked in the panel is this option's media.
    Sticker(u64),
    /// Emoji clicked in the panel go into this option's field.
    Emoji(u64),
}

impl OptionAttach {
    pub(crate) fn link_input(&self) -> Option<&Entity<TextareaState>> {
        match &self.mode {
            AttachMode::Link(input) => Some(input),
            _ => None,
        }
    }

    /// Back to `Idle` when in the mode `is` names; a different mode stays.
    pub(crate) fn leave(&mut self, is: impl Fn(&AttachMode) -> bool) {
        if is(&self.mode) {
            self.mode = AttachMode::Idle;
        }
    }
}

/// The media of one option being written (a row of the new poll, or the
/// option added to an open poll), and the little UI around it.
pub(crate) struct OptionAttach {
    /// Names this slot for as long as it lives: a row's index changes when
    /// an earlier row is removed, so an answer that comes later (the file
    /// panel) is matched by this instead.
    pub(crate) id: u64,
    pub(crate) media: OptionMediaSlot,
    /// What the attach area under the field is doing.
    pub(crate) mode: AttachMode,
    /// A frame of a picked video, drawn while it is attached.
    pub(crate) preview: Option<std::path::PathBuf>,
}

impl Default for OptionAttach {
    fn default() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        Self {
            id: NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            media: OptionMediaSlot::default(),
            mode: AttachMode::Idle,
            preview: None,
        }
    }
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

/// The page the poll box shows. Every page leaves to `Form`: the pick, Esc
/// or Cancel in a picker, "Keep editing" on the discard question; the box
/// itself closes from any of them.
pub(crate) enum PollPage {
    /// Just opened: the form, and the question takes focus once the dialog
    /// has mounted (the kit dialog takes focus when it opens, so focusing at
    /// open time is lost). The first render moves on to `Form`.
    Opening,
    Form,
    /// The allowed countries picker.
    Country(crate::ui::country_box::CountryBox),
    /// The custom deadline box.
    Deadline(DeadlineBox),
    /// "Discard this poll?" over the form (Cancel or Esc with input).
    ConfirmDiscard,
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
    /// The options, in order: each row's field, media and quiz mark live
    /// together, so removing a row takes all of them.
    pub(crate) options: Vec<PollOptionRow>,
    pub(crate) explanation_input: Entity<TextareaState>,
    /// The allowed countries (ISO codes), picked in the country box.
    /// tdesktop's "Limit by Country" switch (channels); off keeps no codes.
    pub(crate) limit_by_country: bool,
    pub(crate) country_codes: Vec<String>,
    /// The opposite of tdesktop's "Show Who Voted".
    pub(crate) is_anonymous: bool,
    pub(crate) allows_multiple_answers: bool,
    pub(crate) is_quiz: bool,
    /// A bot's request button fixed the type.
    pub(crate) lock: QuizLock,
    /// A broadcast channel: votes are always anonymous and options cannot
    /// be added.
    pub(crate) in_channel: bool,
    pub(crate) allows_revoting: bool,
    pub(crate) shuffle_options: bool,
    /// B15: "Allow Adding Options" (`inputPollTypeRegular.allow_adding_options`).
    pub(crate) allow_adding_options: bool,
    /// "Hide results" (`hide_results_until_closes`), under "Limit Duration".
    pub(crate) hide_results_until_closes: bool,
    /// B15: "Restrict to Subscribers" (`members_only`, channels only).
    pub(crate) members_only: bool,
    /// Which page the box shows (the form, a picker over it, or the discard
    /// question).
    pub(crate) page: PollPage,
    /// "Limit Duration" is on: the poll closes at `close`.
    pub(crate) limit_duration: bool,
    pub(crate) close: PollClose,
    /// A bot's one-time keyboard whose Poll button opened this box: it
    /// goes when the poll is sent, not when the box opens.
    pub(crate) dismiss_keyboard_on_send: Option<(ChatId, MessageId)>,
    baseline: Option<PollToggles>,
}

/// One option of the new poll.
pub(crate) struct PollOptionRow {
    pub(crate) input: Entity<TextareaState>,
    pub(crate) attach: OptionAttach,
    /// Marked as a correct answer (a quiz).
    pub(crate) correct: bool,
}

/// B15: the inline "Add an Option" panel for an open poll that allows it
/// (`addPollOption`, schema 1.8.67 line 12920).
pub struct PollAddOption {
    pub(crate) chat_id: ChatId,
    pub(crate) message_id: MessageId,
    pub(crate) input: Entity<TextareaState>,
    pub(crate) attach: OptionAttach,
    pub(crate) stage: AddOptionStage,
}

/// Where the "Add an Option" box is. Just opened, the field takes focus once
/// the dialog has mounted (the kit dialog takes focus when it opens, so
/// focusing at open time is lost); the first render moves on to `Editing`,
/// which holds the refusal Add last got.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AddOptionStage {
    Opening,
    Editing { error: Option<&'static str> },
}

impl PollAddOption {
    /// The first render after open: whether the field takes focus now.
    pub(crate) fn take_focus(&mut self) -> bool {
        if self.stage == AddOptionStage::Opening {
            self.stage = AddOptionStage::Editing { error: None };
            true
        } else {
            false
        }
    }

    /// The reason Add was refused, drawn under the field.
    pub(crate) fn error(&self) -> Option<&'static str> {
        match self.stage {
            AddOptionStage::Editing { error } => error,
            AddOptionStage::Opening => None,
        }
    }

    pub(crate) fn refuse(&mut self, reason: &'static str) {
        self.stage = AddOptionStage::Editing {
            error: Some(reason),
        };
    }
}

impl PollDialog {
    pub(crate) fn country_box(&self) -> Option<&crate::ui::country_box::CountryBox> {
        match &self.page {
            PollPage::Country(country_box) => Some(country_box),
            _ => None,
        }
    }

    pub(crate) fn country_box_mut(&mut self) -> Option<&mut crate::ui::country_box::CountryBox> {
        match &mut self.page {
            PollPage::Country(country_box) => Some(country_box),
            _ => None,
        }
    }

    pub(crate) fn deadline_box(&self) -> Option<&DeadlineBox> {
        match &self.page {
            PollPage::Deadline(deadline) => Some(deadline),
            _ => None,
        }
    }

    pub(crate) fn confirming_discard(&self) -> bool {
        matches!(self.page, PollPage::ConfirmDiscard)
    }

    /// A picker is up (the form's own buttons give way to the picker's).
    pub(crate) fn picker_open(&self) -> bool {
        matches!(self.page, PollPage::Country(_) | PollPage::Deadline(_))
    }

    /// Back to the form; the focus request of a fresh box is spent.
    pub(crate) fn show_form(&mut self) {
        self.page = PollPage::Form;
    }

    /// The first render after open: whether the question takes focus now.
    pub(crate) fn take_focus_question(&mut self) -> bool {
        if matches!(self.page, PollPage::Opening) {
            self.page = PollPage::Form;
            true
        } else {
            false
        }
    }
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
        let options = (0..POLL_OPTIONS_MIN)
            .map(|index| PollOptionRow {
                input: textarea(
                    cx,
                    &tr_with(
                        keys::Q_MEDIA_OPTION,
                        &[("index_1", &(index + 1).to_string())],
                    ),
                    (1, 2),
                ),
                attach: OptionAttach::default(),
                correct: false,
            })
            .collect();
        let mut dialog = Self {
            description_input,
            question_input,
            options,
            explanation_input,
            limit_by_country: false,
            country_codes: Vec::new(),
            is_anonymous: in_channel,
            allows_multiple_answers: true,
            is_quiz: lock == QuizLock::Quiz,
            lock,
            in_channel,
            allows_revoting: lock != QuizLock::Quiz,
            shuffle_options: true,
            allow_adding_options: !in_channel && lock != QuizLock::Quiz,
            hide_results_until_closes: false,
            members_only: false,
            limit_duration: false,
            close: PollClose::Period(POLL_DEFAULT_PERIOD_SECS),
            page: PollPage::Opening,
            dismiss_keyboard_on_send: None,
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
        let options: Vec<DraftOption> = self
            .options
            .iter()
            .map(|row| DraftOption {
                text: crate::ui::composer_field::field_doc(row.input.read(cx)).to_markup(),
                media: row.attach.media.ready().cloned(),
                correct: row.correct,
            })
            .collect();
        let country_codes = if self.limit_by_country {
            self.country_codes.clone()
        } else {
            Vec::new()
        };
        let deadline = self.limit_duration.then_some(self.close);
        PollDraft {
            question: value(&self.question_input),
            description: value(&self.description_input),
            options,
            is_anonymous: self.is_anonymous,
            allows_multiple_answers: self.allows_multiple_answers,
            is_quiz: self.is_quiz,
            quiz_explanation: value(&self.explanation_input),
            allows_revoting: self.allows_revoting,
            shuffle_options: self.shuffle_options,
            country_codes,
            allow_adding_options: self.allow_adding_options,
            hide_results_until_closes: self.limit_duration && self.hide_results_until_closes,
            members_only: self.members_only,
            deadline,
            close_max: 0,
        }
    }

    /// A picked video is still being read: sending now would drop it.
    pub(crate) fn media_preparing(&self) -> bool {
        self.options
            .iter()
            .any(|row| matches!(row.attach.media, OptionMediaSlot::Preparing(_)))
    }

    /// `true` when closing would lose user input (drives the discard prompt).
    pub(crate) fn is_dirty(&self, cx: &App) -> bool {
        let value = |input: &Entity<TextareaState>| input.read(cx).value();
        Some(self.toggles()) != self.baseline
            || self.options.iter().any(|row| !row.attach.media.is_none())
            || !value(&self.question_input).trim().is_empty()
            || !value(&self.description_input).trim().is_empty()
            || !value(&self.explanation_input).trim().is_empty()
            || self
                .options
                .iter()
                .any(|row| !value(&row.input).trim().is_empty())
    }
}
