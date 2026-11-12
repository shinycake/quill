//! AI compose: the style list, the style limits from TDLib's options, and
//! the answers the compose box, the apply-in-place shortcut and the style
//! boxes wait for. Every answer is filed under the request that asked and
//! taken by the stage that holds that request.
use crate::ai_compose::{
    AiBoxError, AiResult, CUSTOM_STYLES_FALLBACK, CUSTOM_STYLES_LIMIT_TYPE, StyleExample,
    StyleLimitNotice, StyleList, TextCompositionStyle, style_limit_message,
};
use crate::premium_limits::{LimitPair, shown_limit};
use crate::state::*;
use crate::telegram::envelope::{AiComposePayload, ErrorClass, OptionValue, TdError};
use crate::text::TextEntity;

/// In-flight AI compose requests; wrapped as [`MessagesPurpose::AiCompose`].
/// The stage that asked holds the request; the name behind a style
/// operation rides [`AiComposeState::op_names`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiComposePurpose {
    /// `composeTextWithAi` from the box; answered with `formattedText`.
    BoxCompose,
    /// `fixTextWithAi` from the box; answered with `fixedText`.
    BoxFix,
    /// `composeTextWithAi` from the apply-in-place shortcut.
    ApplyInPlace,
    /// `createTextCompositionStyle`; answered with `textCompositionStyle`.
    CreateStyle,
    /// `editTextCompositionStyle`; answered with `textCompositionStyle`.
    EditStyle,
    DeleteStyle,
    RemoveStyle,
    AddStyle,
    /// `searchTextCompositionStyle` (the `addstyle/<name>` link); its
    /// answer is taken by the request the lookup stage holds.
    SearchStyle,
    /// `getTextCompositionStyleExample`; its answer is taken by the request
    /// the example stage holds.
    GetExample,
}

/// Limits from `updateOption` (named in the method docs at
/// `schema/td_api.tl:13017`, `:13019`, `:13040` and `:13044`); `None`
/// until TDLib sends them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AiComposeLimits {
    /// `text_composition_style_title_length_max`.
    pub title_length_max: Option<usize>,
    /// `text_composition_style_prompt_length_max`.
    pub prompt_length_max: Option<usize>,
    /// `text_composition_style_example_count`.
    pub example_count: Option<usize>,
    /// `added_text_composition_style_count_max`.
    pub added_count_max: Option<usize>,
}

impl AiComposeLimits {
    fn apply_option(&mut self, name: &str, value: &OptionValue) {
        let slot = match name {
            "text_composition_style_title_length_max" => &mut self.title_length_max,
            "text_composition_style_prompt_length_max" => &mut self.prompt_length_max,
            "text_composition_style_example_count" => &mut self.example_count,
            "added_text_composition_style_count_max" => &mut self.added_count_max,
            _ => return,
        };
        *slot = match value {
            OptionValue::Integer(value) => usize::try_from(*value).ok(),
            _ => None,
        };
    }
}

/// The most answers kept for stages that are gone: an answer nobody takes
/// leaves when newer ones push it out.
const KEPT: usize = 8;

/// Answers filed under the request that asked, taken by the stage that
/// holds that request. One drain may hold several (two editors, two
/// boxes): each stage takes its own, so none overwrites another.
#[derive(Debug)]
pub struct Replies<T> {
    items: Vec<(RequestId, T)>,
}

impl<T> Default for Replies<T> {
    fn default() -> Self {
        Self { items: Vec::new() }
    }
}

impl<T> Replies<T> {
    /// File the answer of `request`.
    pub fn push(&mut self, request: RequestId, answer: T) {
        self.items.retain(|(held, _)| *held != request);
        self.items.push((request, answer));
        let extra = self.items.len().saturating_sub(KEPT);
        self.items.drain(..extra);
    }

    /// The answer of `request`, taken.
    pub fn take(&mut self, request: RequestId) -> Option<T> {
        let at = self.items.iter().position(|(held, _)| *held == request)?;
        Some(self.items.remove(at).1)
    }

    /// Whether `request` has answered.
    pub fn has(&self, request: RequestId) -> bool {
        self.items.iter().any(|(held, _)| *held == request)
    }

    /// Nothing waits for these any more (the box closed).
    pub fn clear(&mut self) {
        self.items.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

/// The box's answer: the result card, or the error for its toast.
pub type AiBoxOutcome = Result<AiResult, AiBoxError>;

/// The apply-in-place shortcut's answer: the text and entities that replace
/// the selection (or the whole field).
pub type ApplyInPlaceOutcome = Result<(String, Vec<TextEntity>), AiBoxError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StyleError {
    /// `TONES_SAVED_TOO_MANY`.
    SavedTooMany,
    /// The style was deleted or the name is bad.
    Invalid,
    /// Any other error, by its type.
    Other(String),
}

/// A request that runs as `request`, then answers once, with a value or the
/// error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StyleReply<T> {
    /// Waiting for the answer of `request`; a lost request settles it as
    /// an error (`RequestPurpose::is_sweepable`).
    Running { request: RequestId },
    Answered {
        request: RequestId,
        outcome: Result<T, StyleError>,
    },
}

impl<T> StyleReply<T> {
    /// Waiting for the answer of `request`.
    pub fn waits_for(&self, request: RequestId) -> bool {
        matches!(self, Self::Running { request: asked } if *asked == request)
    }

    pub fn is_done(&self) -> bool {
        matches!(self, Self::Answered { .. })
    }

    /// The request this reply is for.
    pub fn request(&self) -> RequestId {
        match self {
            Self::Running { request } | Self::Answered { request, .. } => *request,
        }
    }

    /// The answer, once there is one.
    pub fn done(self) -> Option<Result<T, StyleError>> {
        match self {
            Self::Running { .. } => None,
            Self::Answered { outcome, .. } => Some(outcome),
        }
    }

    pub fn answer(&self) -> Option<&Result<T, StyleError>> {
        match self {
            Self::Running { .. } => None,
            Self::Answered { outcome, .. } => Some(outcome),
        }
    }

    /// `request` answered: the reply becomes `Answered`, only while it
    /// runs as that very request.
    fn settle(&mut self, request: RequestId, outcome: Result<T, StyleError>) {
        if self.waits_for(request) {
            *self = Self::Answered { request, outcome };
        }
    }
}

/// The `addstyle/<name>` lookup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyleLookup {
    pub name: String,
    pub outcome: StyleReply<TextCompositionStyle>,
}

/// A style example for the preview box.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExampleAnswer {
    pub style_name: String,
    pub outcome: StyleReply<StyleExample>,
}

/// What a Create or Save answered, for the editor that sent the request
/// it is filed under.
pub type StyleEditOutcome = Result<TextCompositionStyle, StyleError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StyleNotice {
    /// The style of this name was added, by this request.
    Added {
        name: String,
        request: RequestId,
    },
    /// The style was removed by this request.
    Removed {
        request: RequestId,
    },
    /// The style was deleted by this request.
    Deleted {
        request: RequestId,
    },
    Limit(StyleLimitNotice),
    Invalid,
    /// Any other error, by its type.
    Error(String),
}

/// Notices and failures not yet shown: the most kept when the UI is slow.
const NOTICES_KEPT: usize = 8;

#[derive(Debug, Default)]
pub struct AiComposeState {
    /// The Style tabs, from `updateTextCompositionStyles`.
    pub styles: StyleList,
    pub limits: AiComposeLimits,
    /// The compose box's answers, by the request its card waits for.
    pub box_answers: Replies<AiBoxOutcome>,
    /// The apply-in-place shortcut's answers, by request.
    pub apply_answers: Replies<ApplyInPlaceOutcome>,
    pub style_lookup: Option<StyleLookup>,
    pub example: Option<ExampleAnswer>,
    /// The Create and Save answers, by request; each editor takes the one
    /// it sent.
    pub style_edits: Replies<StyleEditOutcome>,
    /// Toasts for style operations, oldest first: two that arrive before the
    /// UI drains are both shown.
    style_notices: Vec<StyleNotice>,
    /// Add, remove and delete requests that failed: the preview that waits
    /// on one leaves its in-flight stage (a delete's failure is silent).
    style_op_failures: Vec<RequestId>,
    /// The style named by an in-flight delete, remove or add.
    pub(crate) op_names: HashMap<RequestId, String>,
}

impl AiComposeState {
    fn push_notice(&mut self, notice: StyleNotice) {
        self.style_notices.push(notice);
        let extra = self.style_notices.len().saturating_sub(NOTICES_KEPT);
        self.style_notices.drain(..extra);
    }

    fn push_failure(&mut self, request: RequestId) {
        self.style_op_failures.push(request);
        let extra = self.style_op_failures.len().saturating_sub(NOTICES_KEPT);
        self.style_op_failures.drain(..extra);
    }

    /// The style notices not yet shown, taken.
    pub fn take_style_notices(&mut self) -> Vec<StyleNotice> {
        std::mem::take(&mut self.style_notices)
    }

    /// The add, remove and delete requests that failed, taken.
    pub fn take_style_op_failures(&mut self) -> Vec<RequestId> {
        std::mem::take(&mut self.style_op_failures)
    }

    /// Queue a notice (tests and demos).
    pub fn show_style_notice(&mut self, notice: StyleNotice) {
        self.push_notice(notice);
    }

    /// No box or editor is open: whatever answered for them is dropped.
    pub fn forget_stage_answers(&mut self) {
        self.box_answers.clear();
        self.style_edits.clear();
    }
}

fn ai_purpose(pending: Option<&PendingRequest>) -> Option<AiComposePurpose> {
    match pending.map(|p| p.purpose) {
        Some(RequestPurpose::Messages(MessagesPurpose::AiCompose(purpose))) => Some(purpose),
        _ => None,
    }
}

fn error_type(err: &TdError) -> String {
    err.server_type.clone().unwrap_or_default()
}

fn box_error(err: &TdError) -> AiBoxError {
    match err.class {
        ErrorClass::AiComposeFloodPremium => AiBoxError::FloodPremium,
        ErrorClass::InputTextTooLong => AiBoxError::TooLong,
        ErrorClass::StyleInvalid => AiBoxError::StyleInvalid(error_type(err)),
        _ => AiBoxError::Other(error_type(err)),
    }
}

fn style_error(err: &TdError) -> StyleError {
    match err.class {
        ErrorClass::StylesSavedTooMany => StyleError::SavedTooMany,
        ErrorClass::StyleInvalid | ErrorClass::NotFound => StyleError::Invalid,
        _ => StyleError::Other(error_type(err)),
    }
}

impl Session {
    /// The saved custom style limit: TDLib's Premium limit when the Premium
    /// info is loaded, else tdesktop's 5 / 20.
    pub fn custom_style_limit(&self) -> LimitPair {
        self.payments
            .hub
            .premium
            .data()
            .and_then(|info| info.limit_pair_for_type(CUSTOM_STYLES_LIMIT_TYPE))
            .unwrap_or(CUSTOM_STYLES_FALLBACK)
    }

    /// The toast for `TONES_SAVED_TOO_MANY`: the option's limit in force,
    /// else the free or Premium value.
    fn style_limit_notice(&self) -> StyleLimitNotice {
        let pair = self.custom_style_limit();
        let premium = self.my_is_premium();
        let count = self
            .messages
            .ai_compose
            .limits
            .added_count_max
            .and_then(|count| i64::try_from(count).ok())
            .unwrap_or_else(|| shown_limit(pair, premium));
        style_limit_message(count, pair.premium_value, premium, self.premium_possible())
    }

    pub(crate) fn apply_ai_compose_option(&mut self, name: &str, value: &OptionValue) {
        self.messages.ai_compose.limits.apply_option(name, value);
    }

    pub(crate) fn apply_ai_compose_payload(
        &mut self,
        payload: AiComposePayload,
        pending: Option<&PendingRequest>,
    ) {
        let asked = ai_purpose(pending);
        let request = pending.map(|p| p.id);
        let state = &mut self.messages.ai_compose;
        match payload {
            AiComposePayload::UpdateStyles { styles } => {
                state.styles.set_server(styles);
            }
            AiComposePayload::Style(style) => match (asked, request) {
                (Some(AiComposePurpose::SearchStyle), Some(request)) => {
                    if let Some(lookup) = state.style_lookup.as_mut() {
                        lookup.outcome.settle(request, Ok(style));
                    }
                }
                (Some(AiComposePurpose::CreateStyle | AiComposePurpose::EditStyle), request) => {
                    state.styles.promote_front(style.clone());
                    if let Some(request) = request {
                        state.style_edits.push(request, Ok(style));
                    }
                }
                _ => {}
            },
            AiComposePayload::StyleExample(example) => {
                if let (Some(AiComposePurpose::GetExample), Some(request)) = (asked, request)
                    && let Some(answer) = state.example.as_mut()
                {
                    answer.outcome.settle(request, Ok(example));
                }
            }
            AiComposePayload::FixedText {
                text,
                entities,
                diff,
            } => {
                if let (Some(AiComposePurpose::BoxFix), Some(request)) = (asked, request) {
                    state.box_answers.push(
                        request,
                        Ok(AiResult {
                            text,
                            entities,
                            diff: Some(diff),
                        }),
                    );
                }
            }
        }
    }

    /// A bare `formattedText` answer for the box or the shortcut; any
    /// other purpose is left to its own handler.
    pub(crate) fn apply_ai_compose_formatted_text(
        &mut self,
        pending: Option<&PendingRequest>,
        text: String,
        entities: Vec<TextEntity>,
    ) {
        let Some(request) = pending.map(|p| p.id) else {
            return;
        };
        let state = &mut self.messages.ai_compose;
        match ai_purpose(pending) {
            Some(AiComposePurpose::BoxCompose) => {
                state.box_answers.push(
                    request,
                    Ok(AiResult {
                        text,
                        entities,
                        diff: None,
                    }),
                );
            }
            Some(AiComposePurpose::ApplyInPlace) => {
                state.apply_answers.push(request, Ok((text, entities)));
            }
            _ => {}
        }
    }

    /// `ok` for an add, remove or delete (tdesktop `AiComposeTones::save`
    /// and `remove`): the list changes at once; TDLib's next
    /// `updateTextCompositionStyles` confirms it.
    pub(crate) fn apply_ai_compose_ok(&mut self, pending: Option<&PendingRequest>) {
        let Some(purpose) = ai_purpose(pending) else {
            return;
        };
        let Some(request) = pending.map(|pending| pending.id) else {
            return;
        };
        let state = &mut self.messages.ai_compose;
        let name = state.op_names.remove(&request).unwrap_or_default();
        let notice = match purpose {
            AiComposePurpose::AddStyle => {
                state.styles.promote_name(&name);
                StyleNotice::Added { name, request }
            }
            AiComposePurpose::RemoveStyle => {
                state.styles.forget(&name);
                StyleNotice::Removed { request }
            }
            AiComposePurpose::DeleteStyle => {
                state.styles.forget(&name);
                StyleNotice::Deleted { request }
            }
            _ => return,
        };
        state.push_notice(notice);
    }

    /// A failed AI compose request. The box and the shortcut get the error
    /// for their toast; a saved-style limit becomes tdesktop's limit toast
    /// (`ShowAiComposeToneLimitError`); a failed delete is silent, as in
    /// tdesktop.
    pub(crate) fn fail_ai_compose_request(
        &mut self,
        purpose: AiComposePurpose,
        err: &TdError,
        pending: Option<&PendingRequest>,
    ) {
        let request = pending.map(|p| p.id);
        if let Some(request) = request {
            self.messages.ai_compose.op_names.remove(&request);
        }
        let error = style_error(err);
        let notice = match &error {
            StyleError::SavedTooMany => StyleNotice::Limit(self.style_limit_notice()),
            StyleError::Invalid => StyleNotice::Invalid,
            StyleError::Other(kind) => StyleNotice::Error(kind.clone()),
        };
        let state = &mut self.messages.ai_compose;
        match purpose {
            AiComposePurpose::BoxCompose | AiComposePurpose::BoxFix => {
                if let Some(request) = request {
                    state.box_answers.push(request, Err(box_error(err)));
                }
            }
            AiComposePurpose::ApplyInPlace => {
                if let Some(request) = request {
                    state.apply_answers.push(request, Err(box_error(err)));
                }
            }
            AiComposePurpose::CreateStyle | AiComposePurpose::EditStyle => {
                if error == StyleError::SavedTooMany {
                    state.push_notice(notice);
                }
                if let Some(request) = request {
                    state.style_edits.push(request, Err(error));
                }
            }
            AiComposePurpose::AddStyle | AiComposePurpose::RemoveStyle => {
                state.push_notice(notice);
                if let Some(request) = request {
                    state.push_failure(request);
                }
            }
            AiComposePurpose::DeleteStyle => {
                if let Some(request) = request {
                    state.push_failure(request);
                }
            }
            AiComposePurpose::SearchStyle => {
                if let (Some(request), Some(lookup)) = (request, state.style_lookup.as_mut()) {
                    lookup.outcome.settle(request, Err(error));
                }
            }
            AiComposePurpose::GetExample => {
                if let (Some(request), Some(answer)) = (request, state.example.as_mut()) {
                    answer.outcome.settle(request, Err(error));
                }
            }
        }
    }
}
