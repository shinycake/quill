//! AI compose: the style list, the style limits from TDLib's options, and
//! the answers the compose box, the apply-in-place shortcut and the style
//! boxes wait for. Each answer slot is one-shot: the UI takes it.
use crate::ai_compose::{
    AiBoxError, AiResult, CUSTOM_STYLES_FALLBACK, CUSTOM_STYLES_LIMIT_TYPE, StyleExample,
    StyleLimitNotice, StyleList, TextCompositionStyle, style_limit_message,
};
use crate::premium_limits::{LimitPair, shown_limit};
use crate::state::*;
use crate::telegram::envelope::{AiComposePayload, ErrorClass, OptionValue, TdError};
use crate::text::TextEntity;

/// In-flight AI compose requests; wrapped as [`MessagesPurpose::AiCompose`].
/// Box and lookup purposes carry the token their answer is matched by; the
/// name behind a style operation rides [`AiComposeState::op_names`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiComposePurpose {
    /// `composeTextWithAi` from the box; answered with `formattedText`.
    BoxCompose {
        token: u64,
    },
    /// `fixTextWithAi` from the box; answered with `fixedText`.
    BoxFix {
        token: u64,
    },
    /// `composeTextWithAi` from the apply-in-place shortcut.
    ApplyInPlace {
        token: u64,
    },
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiBoxAnswer {
    pub token: u64,
    pub outcome: Result<AiResult, AiBoxError>,
}

/// The apply-in-place shortcut's answer: the text and entities that replace
/// the selection (or the whole field).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyInPlaceAnswer {
    pub token: u64,
    pub outcome: Result<(String, Vec<TextEntity>), AiBoxError>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StyleError {
    /// `TONES_SAVED_TOO_MANY`.
    SavedTooMany,
    /// The style was deleted or the name is bad.
    Invalid,
    /// Any other error, by its type.
    Other(String),
}

/// A request that runs, then answers once, with a value or the error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StyleReply<T> {
    /// Waiting for the answer of `request`; a lost request settles it as
    /// an error (`RequestPurpose::is_sweepable`).
    Running {
        request: RequestId,
    },
    Answered(Result<T, StyleError>),
}

impl<T> StyleReply<T> {
    /// Waiting for the answer of `request`.
    pub fn waits_for(&self, request: RequestId) -> bool {
        matches!(self, Self::Running { request: asked } if *asked == request)
    }

    pub fn is_done(&self) -> bool {
        matches!(self, Self::Answered(_))
    }

    /// The answer, once there is one.
    pub fn done(self) -> Option<Result<T, StyleError>> {
        match self {
            Self::Running { .. } => None,
            Self::Answered(outcome) => Some(outcome),
        }
    }

    pub fn answer(&self) -> Option<&Result<T, StyleError>> {
        match self {
            Self::Running { .. } => None,
            Self::Answered(outcome) => Some(outcome),
        }
    }
}

/// The `addstyle/<name>` lookup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyleLookup {
    pub token: u64,
    pub name: String,
    pub outcome: StyleReply<TextCompositionStyle>,
}

/// A style example for the preview box.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExampleAnswer {
    pub token: u64,
    pub style_name: String,
    pub outcome: StyleReply<StyleExample>,
}

/// What a Create or Save answered, for the editor that sent `request` (the
/// editor keeps the request it waits on; an answer to another one is not
/// its own).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyleEditAnswer {
    pub request: RequestId,
    pub outcome: Result<TextCompositionStyle, StyleError>,
}

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

#[derive(Debug, Default)]
pub struct AiComposeState {
    /// The Style tabs, from `updateTextCompositionStyles`.
    pub styles: StyleList,
    pub limits: AiComposeLimits,
    pub box_answer: Option<AiBoxAnswer>,
    pub apply_in_place: Option<ApplyInPlaceAnswer>,
    pub style_lookup: Option<StyleLookup>,
    pub example: Option<ExampleAnswer>,
    /// The newest Create or Save answer; the UI takes it and hands it to
    /// the editor that sent that request, if it is still open.
    pub style_edit: Option<StyleEditAnswer>,
    pub style_notice: Option<StyleNotice>,
    /// Add, remove and delete requests that failed: the preview that waits
    /// on one leaves its in-flight stage (a delete's failure is silent).
    pub style_op_failures: Vec<RequestId>,
    /// The style named by an in-flight delete, remove or add.
    pub(crate) op_names: HashMap<RequestId, String>,
}

/// An answer carrying the token of the request it answers.
trait Tokened {
    fn token(&self) -> u64;
}

impl Tokened for AiBoxAnswer {
    fn token(&self) -> u64 {
        self.token
    }
}

impl Tokened for ApplyInPlaceAnswer {
    fn token(&self) -> u64 {
        self.token
    }
}

/// Tokens only grow, so an older answer landing in the same drain as a
/// newer one must not replace it.
fn keep_newest<T: Tokened>(slot: &mut Option<T>, answer: T) {
    if slot
        .as_ref()
        .is_none_or(|held| held.token() <= answer.token())
    {
        *slot = Some(answer);
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
        let state = &mut self.messages.ai_compose;
        match payload {
            AiComposePayload::UpdateStyles { styles } => {
                state.styles.set_server(styles);
            }
            AiComposePayload::Style(style) => match asked {
                Some(AiComposePurpose::SearchStyle) => {
                    if let Some(lookup) = state.style_lookup.as_mut()
                        && pending.is_some_and(|p| lookup.outcome.waits_for(p.id))
                    {
                        lookup.outcome = StyleReply::Answered(Ok(style));
                    }
                }
                Some(AiComposePurpose::CreateStyle | AiComposePurpose::EditStyle) => {
                    state.styles.promote_front(style.clone());
                    if let Some(request) = pending.map(|p| p.id) {
                        state.style_edit = Some(StyleEditAnswer {
                            request,
                            outcome: Ok(style),
                        });
                    }
                }
                _ => {}
            },
            AiComposePayload::StyleExample(example) => {
                if let Some(AiComposePurpose::GetExample) = asked
                    && let Some(answer) = state.example.as_mut()
                    && pending.is_some_and(|p| answer.outcome.waits_for(p.id))
                {
                    answer.outcome = StyleReply::Answered(Ok(example));
                }
            }
            AiComposePayload::FixedText {
                text,
                entities,
                diff,
            } => {
                if let Some(AiComposePurpose::BoxFix { token }) = asked {
                    keep_newest(
                        &mut state.box_answer,
                        AiBoxAnswer {
                            token,
                            outcome: Ok(AiResult {
                                text,
                                entities,
                                diff: Some(diff),
                            }),
                        },
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
        let state = &mut self.messages.ai_compose;
        match ai_purpose(pending) {
            Some(AiComposePurpose::BoxCompose { token }) => {
                keep_newest(
                    &mut state.box_answer,
                    AiBoxAnswer {
                        token,
                        outcome: Ok(AiResult {
                            text,
                            entities,
                            diff: None,
                        }),
                    },
                );
            }
            Some(AiComposePurpose::ApplyInPlace { token }) => {
                keep_newest(
                    &mut state.apply_in_place,
                    ApplyInPlaceAnswer {
                        token,
                        outcome: Ok((text, entities)),
                    },
                );
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
        state.style_notice = match purpose {
            AiComposePurpose::AddStyle => {
                state.styles.promote_name(&name);
                Some(StyleNotice::Added { name, request })
            }
            AiComposePurpose::RemoveStyle => {
                state.styles.forget(&name);
                Some(StyleNotice::Removed { request })
            }
            AiComposePurpose::DeleteStyle => {
                state.styles.forget(&name);
                Some(StyleNotice::Deleted { request })
            }
            _ => return,
        };
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
        if let Some(pending) = pending {
            self.messages.ai_compose.op_names.remove(&pending.id);
        }
        let error = style_error(err);
        let notice = match &error {
            StyleError::SavedTooMany => StyleNotice::Limit(self.style_limit_notice()),
            StyleError::Invalid => StyleNotice::Invalid,
            StyleError::Other(kind) => StyleNotice::Error(kind.clone()),
        };
        let state = &mut self.messages.ai_compose;
        match purpose {
            AiComposePurpose::BoxCompose { token } | AiComposePurpose::BoxFix { token } => {
                keep_newest(
                    &mut state.box_answer,
                    AiBoxAnswer {
                        token,
                        outcome: Err(box_error(err)),
                    },
                );
            }
            AiComposePurpose::ApplyInPlace { token } => {
                keep_newest(
                    &mut state.apply_in_place,
                    ApplyInPlaceAnswer {
                        token,
                        outcome: Err(box_error(err)),
                    },
                );
            }
            AiComposePurpose::CreateStyle | AiComposePurpose::EditStyle => {
                if error == StyleError::SavedTooMany {
                    state.style_notice = Some(notice);
                }
                if let Some(request) = pending.map(|p| p.id) {
                    state.style_edit = Some(StyleEditAnswer {
                        request,
                        outcome: Err(error),
                    });
                }
            }
            AiComposePurpose::AddStyle | AiComposePurpose::RemoveStyle => {
                state.style_notice = Some(notice);
                state.style_op_failures.extend(pending.map(|p| p.id));
            }
            AiComposePurpose::DeleteStyle => {
                state.style_op_failures.extend(pending.map(|p| p.id));
            }
            AiComposePurpose::SearchStyle => {
                if let Some(lookup) = state.style_lookup.as_mut()
                    && pending.is_some_and(|p| lookup.outcome.waits_for(p.id))
                {
                    lookup.outcome = StyleReply::Answered(Err(error));
                }
            }
            AiComposePurpose::GetExample => {
                if let Some(answer) = state.example.as_mut()
                    && pending.is_some_and(|p| answer.outcome.waits_for(p.id))
                {
                    answer.outcome = StyleReply::Answered(Err(error));
                }
            }
        }
    }
}

#[cfg(test)]
mod keep_newest_tests {
    use super::{AiBoxAnswer, keep_newest};
    use crate::ai_compose::AiBoxError;

    fn answer(token: u64) -> AiBoxAnswer {
        AiBoxAnswer {
            token,
            outcome: Err(AiBoxError::TooLong),
        }
    }

    #[test]
    fn an_older_answer_never_replaces_a_newer_one() {
        let mut slot = None;
        keep_newest(&mut slot, answer(2));
        keep_newest(&mut slot, answer(1));
        assert_eq!(slot.map(|held| held.token), Some(2));
        let mut slot = Some(answer(1));
        keep_newest(&mut slot, answer(3));
        assert_eq!(slot.map(|held| held.token), Some(3));
    }
}
