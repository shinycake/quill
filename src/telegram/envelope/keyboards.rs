use super::*;
use serde_json::Value;

/// `buttonStyle*` (TDLib 1.8.67, `schema/td_api.tl:3696`). Unknown styles
/// fall back to `Default` — the keyboard must never fail to render.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InlineKeyboardButtonStyle {
    #[default]
    Default,
    Primary,
    Danger,
    Success,
    Link,
}

/// `targetChat*` for `inlineKeyboardButtonTypeSwitchInline`
/// (TDLib 1.8.67, `schema/td_api.tl:7476`). Only `Current` is actionable in
/// this slice; the rest insert into the current chat's composer too (chat
/// picker is out of scope).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InlineKeyboardTargetChat {
    Current,
    Chosen,
    InternalLink,
    Unknown,
}

/// `inlineKeyboardButtonType*` (TDLib 1.8.67, `schema/td_api.tl:3774`).
/// Types needing a TDLib round-trip or platform flow we do not have yet
/// (`LoginUrl` → `getLoginUrlInfo`, `WebApp` → `openWebApp`,
/// `CallbackWithPassword` → password prompt, `CallbackGame`, `Buy`,
/// `User`) are parsed and stored but render as disabled buttons. Anything
/// unrecognized becomes `Unknown` and also renders disabled — never a crash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InlineKeyboardButtonType {
    Url {
        url: String,
    },
    LoginUrl {
        url: String,
        /// B1: `id` (schema/td_api.tl:3780) — the button identifier for
        /// `getLoginUrlInfo`.
        id: i64,
    },
    WebApp {
        url: String,
    },
    Callback {
        data: Vec<u8>,
    },
    CallbackWithPassword {
        data: Vec<u8>,
    },
    CallbackGame,
    SwitchInline {
        query: String,
        target: InlineKeyboardTargetChat,
    },
    Buy,
    User {
        user_id: i64,
    },
    CopyText {
        text: String,
    },
    Disabled,
    Unknown {
        type_name: String,
    },
}

/// `inlineKeyboardButton` (TDLib 1.8.67, `schema/td_api.tl:3828`).
/// `icon_custom_emoji_id` is parsed (schema presence) but not rendered yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineKeyboardButton {
    pub text: String,
    pub style: InlineKeyboardButtonStyle,
    pub kind: InlineKeyboardButtonType,
}

/// `replyMarkupInlineKeyboard` (TDLib 1.8.67, `schema/td_api.tl:3855`):
/// `rows` is a vector of rows (`vector<vector<inlineKeyboardButton>>`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineKeyboard {
    pub rows: Vec<Vec<InlineKeyboardButton>>,
    pub force_reply: bool,
}

/// B1: `message.reply_markup` (TDLib 1.8.67, `schema/td_api.tl:3835-3855`).
/// The inline variant keeps the Phase 3.2 shape; the other three are the
/// bot custom-keyboard / force-reply / remove markups.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplyMarkup {
    InlineKeyboard(InlineKeyboard),
    ShowKeyboard(ReplyKeyboard),
    ForceReply { placeholder: String },
    RemoveKeyboard,
}

/// B1: `replyMarkupShowKeyboard` (TDLib 1.8.67, `schema/td_api.tl:3850`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplyKeyboard {
    pub rows: Vec<Vec<KeyboardButton>>,
    pub is_persistent: bool,
    pub resize_keyboard: bool,
    pub one_time: bool,
    pub is_personal: bool,
    pub force_reply: bool,
    pub placeholder: String,
}

/// B1: `keyboardButton` (TDLib 1.8.67, `schema/td_api.tl:3768`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyboardButton {
    pub text: String,
    pub kind: KeyboardButtonType,
}

/// B1: `keyboardButtonType*` (TDLib 1.8.67, `schema/td_api.tl:3714-3760`).
/// Only `Text` (tap → send the text) and `WebApp` (browser fallback) act;
/// the request variants stay disabled with honest tooltips.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyboardButtonType {
    Text,
    RequestPhoneNumber,
    RequestLocation,
    RequestPoll,
    /// `keyboardButtonTypeRequestUsers` (schema 1.8.67, line 3735).
    RequestUsers(RequestUsersSpec),
    /// `keyboardButtonTypeRequestChat` (schema 1.8.67, line 3751).
    RequestChat(RequestChatSpec),
    RequestManagedBot,
    WebApp {
        url: String,
    },
    Unknown {
        type_name: String,
    },
}

/// `keyboardButtonTypeRequestUsers`: what the bot asks the user to share.
/// `Option<bool>` restrictions are `None` when the bot does not restrict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestUsersSpec {
    pub id: i32,
    pub user_is_bot: Option<bool>,
    pub user_is_premium: Option<bool>,
    pub max_quantity: i32,
}

/// `keyboardButtonTypeRequestChat`: what chat the bot asks the user to share.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestChatSpec {
    pub id: i32,
    pub chat_is_channel: bool,
    pub chat_is_forum: Option<bool>,
    pub chat_has_username: Option<bool>,
    pub chat_is_created: bool,
    pub bot_is_member: bool,
}

/// B1: `LoginUrlInfo` (TDLib 1.8.67, `schema/td_api.tl:3862` /
/// `:3869`) — the `getLoginUrlInfo` answer for a login-URL button press.
/// `Failed` is Quill's own synthesis (not a TDLib constructor): on error
/// the button degrades to an ordinary URL button, per the `getLoginUrl`
/// schema doc comment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoginUrlInfo {
    Open {
        url: String,
    },
    RequestConfirmation {
        domain: String,
        /// B1: the bot asked for permission to message the user
        /// (schema/td_api.tl:3869) — shown in the consent dialog and passed
        /// as `allow_write_access` to `getLoginUrl` on consent.
        request_write_access: bool,
    },
    Failed {
        fallback_url: String,
    },
}

/// B1: whether a `replyMarkup*` demands a reply — `replyMarkupForceReply`
/// outright, or the `force_reply` flag on an inline / show-keyboard markup
/// (TDLib 1.8.67, `schema/td_api.tl:3850` / `:3855`). Pure logic: unit-tested.
pub fn reply_markup_demands_reply(markup: &ReplyMarkup) -> bool {
    match markup {
        ReplyMarkup::ForceReply { .. } => true,
        ReplyMarkup::ShowKeyboard(keyboard) => keyboard.force_reply,
        ReplyMarkup::InlineKeyboard(keyboard) => keyboard.force_reply,
        ReplyMarkup::RemoveKeyboard => false,
    }
}

/// `callbackQueryAnswer` (TDLib 1.8.67, `schema/td_api.tl:7747`): the bot's
/// answer to a callback query sent via `getCallbackQueryAnswer`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallbackQueryAnswer {
    pub text: String,
    pub show_alert: bool,
    pub url: String,
}

/// `replyMarkup*` (TDLib 1.8.67 lines 3835–3855). `reply_markup`
/// null/absent and unknown markup constructors → `None`. Rows and buttons
/// are parsed tolerantly: malformed rows are skipped, malformed buttons
/// become disabled `Unknown` placeholders — a hostile keyboard can never
/// crash the parse.
pub(crate) fn parse_reply_markup(value: Option<&Value>) -> Option<ReplyMarkup> {
    let value = value.filter(|v| !v.is_null())?;
    let type_name = value.get("@type").and_then(Value::as_str).unwrap_or("");
    match type_name {
        "replyMarkupInlineKeyboard" => {
            let rows = value
                .get("rows")
                .and_then(Value::as_array)
                .map(|rows| {
                    rows.iter()
                        .filter_map(Value::as_array)
                        .map(|row| {
                            row.iter()
                                .map(parse_inline_keyboard_button)
                                .collect::<Vec<_>>()
                        })
                        .collect()
                })
                .unwrap_or_default();
            Some(ReplyMarkup::InlineKeyboard(InlineKeyboard {
                rows,
                force_reply: value
                    .get("force_reply")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            }))
        }
        "replyMarkupShowKeyboard" => Some(ReplyMarkup::ShowKeyboard(parse_reply_keyboard(value))),
        "replyMarkupForceReply" => Some(ReplyMarkup::ForceReply {
            placeholder: json_field_str(value, "input_field_placeholder"),
        }),
        "replyMarkupRemoveKeyboard" => Some(ReplyMarkup::RemoveKeyboard),
        _ => None,
    }
}

/// B1: `replyMarkupShowKeyboard` body (TDLib 1.8.67, `schema/td_api.tl:3850`).
pub(crate) fn parse_reply_keyboard(value: &Value) -> ReplyKeyboard {
    let rows = value
        .get("rows")
        .and_then(Value::as_array)
        .map(|rows| {
            rows.iter()
                .filter_map(Value::as_array)
                .map(|row| row.iter().map(parse_keyboard_button).collect::<Vec<_>>())
                .collect()
        })
        .unwrap_or_default();
    let flag = |name: &str| value.get(name).and_then(Value::as_bool).unwrap_or(false);
    ReplyKeyboard {
        rows,
        is_persistent: flag("is_persistent"),
        resize_keyboard: flag("resize_keyboard"),
        one_time: flag("one_time"),
        is_personal: flag("is_personal"),
        force_reply: flag("force_reply"),
        placeholder: json_field_str(value, "input_field_placeholder"),
    }
}

/// B1: `keyboardButton` (TDLib 1.8.67, `schema/td_api.tl:3768`). A missing
/// `type` degrades to `Unknown` (disabled), never a crash.
pub(crate) fn parse_keyboard_button(value: &Value) -> KeyboardButton {
    let text = value
        .get("text")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let type_name = value
        .get("type")
        .filter(|v| !v.is_null())
        .and_then(|v| v.get("@type"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let kind = match type_name {
        "keyboardButtonTypeText" => KeyboardButtonType::Text,
        "keyboardButtonTypeRequestPhoneNumber" => KeyboardButtonType::RequestPhoneNumber,
        "keyboardButtonTypeRequestLocation" => KeyboardButtonType::RequestLocation,
        "keyboardButtonTypeRequestPoll" => KeyboardButtonType::RequestPoll,
        "keyboardButtonTypeRequestUsers" => {
            let t = value.get("type");
            let flag = |name: &str| t.and_then(|t| t.get(name)).and_then(Value::as_bool);
            let restricted = |restrict: &str, value: &str| {
                flag(restrict)
                    .unwrap_or(false)
                    .then(|| flag(value).unwrap_or(false))
            };
            KeyboardButtonType::RequestUsers(RequestUsersSpec {
                id: t
                    .and_then(|t| t.get("id"))
                    .and_then(Value::as_i64)
                    .unwrap_or(0)
                    .sat_i32(),
                user_is_bot: restricted("restrict_user_is_bot", "user_is_bot"),
                user_is_premium: restricted("restrict_user_is_premium", "user_is_premium"),
                max_quantity: t
                    .and_then(|t| t.get("max_quantity"))
                    .and_then(Value::as_i64)
                    .unwrap_or(1)
                    .max(1)
                    .sat_i32(),
            })
        }
        "keyboardButtonTypeRequestChat" => {
            let t = value.get("type");
            let flag = |name: &str| t.and_then(|t| t.get(name)).and_then(Value::as_bool);
            let restricted = |restrict: &str, value: &str| {
                flag(restrict)
                    .unwrap_or(false)
                    .then(|| flag(value).unwrap_or(false))
            };
            KeyboardButtonType::RequestChat(RequestChatSpec {
                id: t
                    .and_then(|t| t.get("id"))
                    .and_then(Value::as_i64)
                    .unwrap_or(0)
                    .sat_i32(),
                chat_is_channel: flag("chat_is_channel").unwrap_or(false),
                chat_is_forum: restricted("restrict_chat_is_forum", "chat_is_forum"),
                chat_has_username: restricted("restrict_chat_has_username", "chat_has_username"),
                chat_is_created: flag("chat_is_created").unwrap_or(false),
                bot_is_member: flag("bot_is_member").unwrap_or(false),
            })
        }
        "keyboardButtonTypeRequestManagedBot" => KeyboardButtonType::RequestManagedBot,
        "keyboardButtonTypeWebApp" => KeyboardButtonType::WebApp {
            url: value
                .get("type")
                .and_then(|v| v.get("url"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        },
        _ => KeyboardButtonType::Unknown {
            type_name: type_name.to_string(),
        },
    };
    KeyboardButton { text, kind }
}

pub(crate) fn parse_inline_keyboard_button(value: &Value) -> InlineKeyboardButton {
    let text = value
        .get("text")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let style = match value
        .get("style")
        .and_then(|style| style.get("@type"))
        .and_then(Value::as_str)
    {
        Some("buttonStylePrimary") => InlineKeyboardButtonStyle::Primary,
        Some("buttonStyleDanger") => InlineKeyboardButtonStyle::Danger,
        Some("buttonStyleSuccess") => InlineKeyboardButtonStyle::Success,
        Some("buttonStyleLink") => InlineKeyboardButtonStyle::Link,
        _ => InlineKeyboardButtonStyle::Default,
    };
    InlineKeyboardButton {
        text,
        style,
        kind: parse_inline_keyboard_button_type(value.get("type")),
    }
}

pub(crate) fn parse_inline_keyboard_button_type(value: Option<&Value>) -> InlineKeyboardButtonType {
    let value = value.filter(|v| !v.is_null());
    let Some(value) = value else {
        return InlineKeyboardButtonType::Unknown {
            type_name: String::new(),
        };
    };
    let type_name = value
        .get("@type")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    match type_name.as_str() {
        "inlineKeyboardButtonTypeUrl" => InlineKeyboardButtonType::Url {
            url: json_field_str(value, "url"),
        },
        "inlineKeyboardButtonTypeLoginUrl" => InlineKeyboardButtonType::LoginUrl {
            url: json_field_str(value, "url"),
            id: value.get("id").and_then(Value::as_i64).unwrap_or(0),
        },
        "inlineKeyboardButtonTypeWebApp" => InlineKeyboardButtonType::WebApp {
            url: json_field_str(value, "url"),
        },
        "inlineKeyboardButtonTypeCallback" => InlineKeyboardButtonType::Callback {
            data: parse_tdlib_bytes(value.get("data")),
        },
        "inlineKeyboardButtonTypeCallbackWithPassword" => {
            InlineKeyboardButtonType::CallbackWithPassword {
                data: parse_tdlib_bytes(value.get("data")),
            }
        }
        "inlineKeyboardButtonTypeCallbackGame" => InlineKeyboardButtonType::CallbackGame,
        "inlineKeyboardButtonTypeSwitchInline" => InlineKeyboardButtonType::SwitchInline {
            query: json_field_str(value, "query"),
            target: parse_inline_keyboard_target_chat(value.get("target_chat")),
        },
        "inlineKeyboardButtonTypeBuy" => InlineKeyboardButtonType::Buy,
        "inlineKeyboardButtonTypeUser" => InlineKeyboardButtonType::User {
            user_id: value.get("user_id").and_then(Value::as_i64).unwrap_or(0),
        },
        "inlineKeyboardButtonTypeCopyText" => InlineKeyboardButtonType::CopyText {
            text: json_field_str(value, "text"),
        },
        "inlineKeyboardButtonTypeDisabled" => InlineKeyboardButtonType::Disabled,
        _ => InlineKeyboardButtonType::Unknown { type_name },
    }
}

pub(crate) fn parse_inline_keyboard_target_chat(value: Option<&Value>) -> InlineKeyboardTargetChat {
    match value
        .filter(|v| !v.is_null())
        .and_then(|v| v.get("@type"))
        .and_then(Value::as_str)
    {
        Some("targetChatCurrent") => InlineKeyboardTargetChat::Current,
        Some("targetChatChosen") => InlineKeyboardTargetChat::Chosen,
        Some("targetChatInternalLink") => InlineKeyboardTargetChat::InternalLink,
        _ => InlineKeyboardTargetChat::Unknown,
    }
}

/// `callbackQueryAnswer` (TDLib 1.8.67 line 7747).
pub(crate) fn parse_callback_query_answer(value: &Value) -> CallbackQueryAnswer {
    CallbackQueryAnswer {
        text: json_field_str(value, "text"),
        show_alert: value
            .get("show_alert")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        url: json_field_str(value, "url"),
    }
}
