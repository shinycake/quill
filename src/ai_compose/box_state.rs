//! The AI compose box (tdesktop `ComposeAiContent` and `ComposeAiBox` in
//! boxes/compose_ai_box.cpp) as a state machine. Every transition returns
//! the request to send, if any; answers come back through
//! [`AiComposeBox::accept`] / [`AiComposeBox::fail`] with the token they
//! were sent with, and an answer for any other token is ignored (tdesktop
//! cancels the older request instead).
use super::{DiffText, StyleList, fresh_token};
use crate::text::TextEntity;
use crate::text_split::utf16_len;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiComposeMode {
    Translate,
    Style,
    Fix,
}

/// A style tab, or none. The Translate picker's "Neutral" is `None`. The
/// main composer has no custom-prompt tab (tdesktop passes
/// `allowPrompt = false` for it), so there is no prompt choice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StyleChoice {
    None,
    Style(String),
}

impl StyleChoice {
    fn name(&self) -> &str {
        match self {
            Self::None => "",
            Self::Style(name) => name,
        }
    }
}

/// A successful answer: `formattedText` for Translate and Style,
/// `fixedText` for Fix (with its changes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiResult {
    pub text: String,
    pub entities: Vec<TextEntity>,
    pub diff: Option<DiffText>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CardState {
    /// Nothing asked yet (Style tab with no style picked).
    Waiting,
    Loading {
        token: u64,
    },
    Ready(AiResult),
    Failed,
}

/// What to send. `text` is the box's source text: composer markup, sent as
/// a `formattedText` by the request builders.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AiRequest {
    /// `composeTextWithAi` (`schema/td_api.tl:13094`).
    Compose {
        text: String,
        translate_to: String,
        style_name: String,
        add_emojis: bool,
    },
    /// `fixTextWithAi` (`schema/td_api.tl:13112`).
    Fix { text: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiSend {
    pub token: u64,
    pub request: AiRequest,
}

/// Why a box request failed (tdesktop `ComposeAiContent::showError`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AiBoxError {
    /// `AICOMPOSE_FLOOD_PREMIUM`: the free requests are used up; the footer
    /// becomes "Increase Limit".
    FloodPremium,
    /// `INPUT_TEXT_TOO_LONG`.
    TooLong,
    /// The style is gone or its name is bad (`AICOMPOSE_TONE_SLUG_INVALID`,
    /// `AICOMPOSE_TONE_INVALID`, `TONE_NOT_FOUND`); carries the error type.
    /// The apply-in-place shortcut clears its binding on it.
    StyleInvalid(String),
    /// Any other error, by its type (empty when the server gave none or
    /// the answer was empty: the generic error toast).
    Other(String),
}

/// The result card's title (tdesktop `updateTitles`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResultTitle {
    Original,
    Result,
    ToLanguage(String),
    /// "To {language} ({style})": the language code and the style's name.
    ToLanguageStyle(String, String),
}

/// The box's main button (tdesktop `rebuildButtons`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FooterAction {
    /// The free limit ran out: opens the Premium preview.
    IncreaseLimit,
    /// Style tab with nothing picked: a disabled "Select a Style".
    SelectStyle,
    /// "Apply Style" when `style`, else "Apply"; disabled until a result.
    Apply { style: bool, enabled: bool },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiComposeBox {
    mode: AiComposeMode,
    source_text: String,
    translate_to: String,
    translate_style: StyleChoice,
    style: StyleChoice,
    add_emojis: bool,
    card: CardState,
    premium_flooded: bool,
    original_expanded: bool,
}

impl AiComposeBox {
    /// A box on the Style tab for `source_text` (tdesktop
    /// `ComposeAiContent::_mode`). Call [`Self::start`] for the first
    /// request.
    pub fn new(source_text: String, detected_language: Option<&str>, app_language: &str) -> Self {
        Self {
            mode: AiComposeMode::Style,
            translate_to: default_translate_to(detected_language, app_language),
            source_text,
            translate_style: StyleChoice::None,
            style: StyleChoice::None,
            add_emojis: false,
            card: CardState::Waiting,
            premium_flooded: false,
            original_expanded: false,
        }
    }

    /// The first request (tdesktop `ComposeAiContent::start`): none until
    /// a style or emojify is picked, since the box opens on the Style tab.
    pub fn start(&mut self) -> Option<AiSend> {
        self.request()
    }

    pub fn set_mode(&mut self, mode: AiComposeMode) -> Option<AiSend> {
        if self.mode == mode {
            return None;
        }
        if mode != AiComposeMode::Style {
            self.style = StyleChoice::None;
        }
        self.mode = mode;
        self.card = CardState::Waiting;
        self.request()
    }

    /// A Style tab click (also the style just created from the box). A
    /// click on the active tab asks again, as in tdesktop.
    pub fn select_style(&mut self, name: &str) -> Option<AiSend> {
        self.style = StyleChoice::Style(name.to_string());
        self.request_in(AiComposeMode::Style)
    }

    pub fn set_translate_to(&mut self, code: &str) -> Option<AiSend> {
        self.translate_to = code.to_string();
        self.request_in(AiComposeMode::Translate)
    }

    pub fn select_translate_style(&mut self, style: StyleChoice) -> Option<AiSend> {
        self.translate_style = style;
        self.request_in(AiComposeMode::Translate)
    }

    /// The "Emojify" checkbox; Fix ignores it.
    pub fn set_add_emojis(&mut self, add_emojis: bool) -> Option<AiSend> {
        self.add_emojis = add_emojis;
        if self.mode == AiComposeMode::Fix {
            return None;
        }
        self.request()
    }

    /// The style list changed (tdesktop `refreshTones`): a picked style the
    /// server no longer lists is dropped, and the Style tab asks again.
    pub fn refresh_styles(&mut self, styles: &StyleList) -> Option<AiSend> {
        if let StyleChoice::Style(name) = &self.translate_style
            && styles.find(name).is_none()
        {
            self.translate_style = StyleChoice::None;
        }
        let StyleChoice::Style(name) = &self.style else {
            return None;
        };
        if styles.find(name).is_some() {
            return None;
        }
        self.style = StyleChoice::None;
        self.request_in(AiComposeMode::Style)
    }

    pub fn toggle_original_expanded(&mut self) {
        self.original_expanded = !self.original_expanded;
    }

    /// The answer for `token`. An empty text fails the card with the
    /// generic error, returned for the toast.
    pub fn accept(&mut self, token: u64, result: AiResult) -> Option<AiBoxError> {
        if self.card != (CardState::Loading { token }) {
            return None;
        }
        if result.text.is_empty() {
            self.card = CardState::Failed;
            return Some(AiBoxError::Other(String::new()));
        }
        self.card = CardState::Ready(result);
        None
    }

    /// The error for `token`, returned for the toast when it is current.
    pub fn fail(&mut self, token: u64, error: AiBoxError) -> Option<AiBoxError> {
        if self.card != (CardState::Loading { token }) {
            return None;
        }
        self.card = CardState::Failed;
        if error == AiBoxError::FloodPremium {
            self.premium_flooded = true;
        }
        Some(error)
    }

    pub fn mode(&self) -> AiComposeMode {
        self.mode
    }

    pub fn source_text(&self) -> &str {
        &self.source_text
    }

    pub fn translate_to(&self) -> &str {
        &self.translate_to
    }

    pub fn translate_style(&self) -> &StyleChoice {
        &self.translate_style
    }

    pub fn style(&self) -> &StyleChoice {
        &self.style
    }

    pub fn add_emojis(&self) -> bool {
        self.add_emojis
    }

    pub fn card(&self) -> &CardState {
        &self.card
    }

    pub fn premium_flooded(&self) -> bool {
        self.premium_flooded
    }

    pub fn original_expanded(&self) -> bool {
        self.original_expanded
    }

    pub fn result(&self) -> Option<&AiResult> {
        match &self.card {
            CardState::Ready(result) => Some(result),
            _ => None,
        }
    }

    pub fn loading(&self) -> bool {
        matches!(self.card, CardState::Loading { .. })
    }

    /// The original text shows above the result while one is loading or
    /// ready.
    pub fn original_visible(&self) -> bool {
        self.has_result_card()
    }

    pub fn emojify_visible(&self) -> bool {
        let emojify_only = !self.has_result_card()
            && self.mode == AiComposeMode::Style
            && self.style == StyleChoice::None;
        (self.has_result_card() && self.mode != AiComposeMode::Fix) || emojify_only
    }

    pub fn result_title(&self) -> ResultTitle {
        if !self.has_result_card() {
            return ResultTitle::Original;
        }
        match (self.mode, &self.translate_style) {
            (AiComposeMode::Translate, StyleChoice::None) => {
                ResultTitle::ToLanguage(self.translate_to.clone())
            }
            (AiComposeMode::Translate, StyleChoice::Style(name)) => {
                ResultTitle::ToLanguageStyle(self.translate_to.clone(), name.clone())
            }
            _ => ResultTitle::Result,
        }
    }

    pub fn footer(&self) -> FooterAction {
        let style = self.mode == AiComposeMode::Style;
        let ready = self.result().is_some();
        if self.premium_flooded {
            FooterAction::IncreaseLimit
        } else if style && self.style == StyleChoice::None && !ready {
            FooterAction::SelectStyle
        } else {
            FooterAction::Apply {
                style,
                enabled: ready,
            }
        }
    }

    /// "Style by {user}" under a Style result whose style names its
    /// creator.
    pub fn author_user_id(&self, styles: &StyleList) -> Option<i64> {
        let (AiComposeMode::Style, StyleChoice::Style(name), Some(_)) =
            (self.mode, &self.style, self.result())
        else {
            return None;
        };
        styles
            .find(name)
            .map(|style| style.creator_user_id)
            .filter(|id| *id != 0)
    }

    fn has_result_card(&self) -> bool {
        matches!(self.card, CardState::Loading { .. } | CardState::Ready(_))
    }

    fn request_in(&mut self, mode: AiComposeMode) -> Option<AiSend> {
        if self.mode == mode {
            self.request()
        } else {
            None
        }
    }

    fn request(&mut self) -> Option<AiSend> {
        let style_waiting = self.style == StyleChoice::None && !self.add_emojis;
        if self.mode == AiComposeMode::Style && style_waiting {
            self.card = CardState::Waiting;
            return None;
        }
        let token = fresh_token();
        self.card = CardState::Loading { token };
        let text = self.source_text.clone();
        let request = match self.mode {
            AiComposeMode::Translate => AiRequest::Compose {
                text,
                translate_to: self.translate_to.clone(),
                style_name: self.translate_style.name().to_string(),
                add_emojis: self.add_emojis,
            },
            AiComposeMode::Style => AiRequest::Compose {
                text,
                translate_to: String::new(),
                style_name: self.style.name().to_string(),
                add_emojis: self.add_emojis,
            },
            AiComposeMode::Fix => AiRequest::Fix { text },
        };
        Some(AiSend { token, request })
    }
}

/// The Translate tab's first target (tdesktop `DefaultAiTranslateTo`): the
/// app's language when the text is in another one, else English, else
/// Spanish. Codes compare by their primary subtag (`pt-br` is `pt`), as
/// tdesktop compares `QLocale::Language` values.
pub fn default_translate_to(detected: Option<&str>, app_language: &str) -> String {
    let detected = detected.map(primary_subtag);
    let current = primary_subtag(app_language);
    if !current.is_empty() && detected.as_deref() != Some(current.as_str()) {
        current
    } else if detected.as_deref() != Some("en") {
        "en".to_string()
    } else {
        "es".to_string()
    }
}

fn primary_subtag(code: &str) -> String {
    code.split(['-', '_'])
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
}

/// Whether the composer shows its AI button (tdesktop
/// compose_ai_button_factory.cpp `HasEnoughLinesForAi`): styles are known,
/// the field is at least three lines tall, the text fits the message length
/// limit (UTF-16 units) and has something besides whitespace.
pub fn ai_button_visible(
    text: &str,
    line_count: usize,
    has_styles: bool,
    text_length_max: usize,
) -> bool {
    has_styles
        && line_count >= 3
        && utf16_len(text) <= text_length_max
        && text.chars().any(|c| !c.is_whitespace() && !c.is_control())
}

#[cfg(test)]
#[path = "box_state_tests.rs"]
mod tests;
