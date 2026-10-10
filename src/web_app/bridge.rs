//! The `Telegram.WebView` bridge, host side: what a mini app may ask for,
//! parsed and validated before anything acts on it, and the replies it
//! gets (`Telegram.WebView.receiveEvent`).
//!
//! The event names and payloads follow Telegram's bridge as Telegram
//! Desktop implements it (`attach_bot_webview.cpp`, `Panel::processMessage`
//! and the `web_app_*` handlers). Anything not listed here is ignored;
//! features Quill does not have answer with the bridge's own failure
//! events ([`refusal`]) so the app does not wait forever.

use quill_webview_protocol::MAX_WEB_APP_EVENT_BYTES;
use serde_json::{Map, Value, json};

/// Longest `web_app_data_send` payload Telegram accepts (bytes).
pub const MAX_DATA_SEND_BYTES: usize = 4096;
/// `web_app_open_popup` limits (Telegram's bot API docs for `showPopup`).
pub const MAX_POPUP_TITLE_CHARS: usize = 64;
pub const MAX_POPUP_MESSAGE_CHARS: usize = 256;
pub const MAX_POPUP_BUTTONS: usize = 3;
pub const MAX_POPUP_BUTTON_TEXT_CHARS: usize = 64;
pub const MAX_POPUP_BUTTON_ID_CHARS: usize = 64;
/// Longest custom-method name and parameters (`sendWebAppCustomRequest`).
pub const MAX_CUSTOM_METHOD_CHARS: usize = 64;
pub const MAX_CUSTOM_PARAMS_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BridgeError {
    /// Not a bridge event Quill knows; dropped.
    UnknownEvent(String),
    /// A known event with arguments that do not fit the bridge.
    BadArguments(&'static str),
    /// The payload is larger than anything the bridge sends.
    TooLarge,
}

/// A theme color key an app may name instead of a color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorKey {
    BgColor,
    SecondaryBgColor,
    BottomBarBgColor,
}

/// A color for the header, background or bottom bar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ColorSpec {
    Key(ColorKey),
    /// `#rrggbb`, lowercase.
    Rgb(String),
}

/// `web_app_setup_main_button` / `web_app_setup_secondary_button`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ButtonSetup {
    pub visible: bool,
    pub active: bool,
    pub text: String,
    pub color: Option<String>,
    pub text_color: Option<String>,
    pub progress: bool,
    pub shine: bool,
    /// Secondary button only: `left`, `right`, `top` or `bottom`.
    pub position: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PopupButtonSpec {
    pub id: String,
    /// `default`, `ok`, `close`, `cancel` or `destructive`.
    pub kind: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PopupSpec {
    pub title: String,
    pub message: String,
    pub buttons: Vec<PopupButtonSpec>,
}

/// A validated request from the page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WebAppEvent {
    Ready,
    Close,
    /// `web_app_data_send`: a keyboard-button app's result message.
    DataSend {
        data: String,
    },
    /// `web_app_switch_inline_query`: put `@bot query` into a composer.
    /// Empty `chat_types` means the current chat.
    SwitchInlineQuery {
        query: String,
        chat_types: Vec<String>,
    },
    SetupMainButton(ButtonSetup),
    SetupSecondaryButton(ButtonSetup),
    SetupBackButton {
        visible: bool,
    },
    SetupSettingsButton {
        visible: bool,
    },
    /// Answered by the shell; listed so the names are known.
    RequestTheme,
    RequestViewport,
    /// `web_app_open_link`: an `http(s)` URL for the system browser.
    OpenLink {
        url: String,
        instant_view: bool,
    },
    /// `web_app_open_tg_link`: a `t.me` path (`/durov`, `/+hash`), to go
    /// through Quill's deep-link handling.
    OpenTgLink {
        url: String,
    },
    /// `web_app_open_invoice`: refused (payments are out of scope).
    OpenInvoice {
        slug: String,
    },
    OpenPopup(PopupSpec),
    RequestWriteAccess,
    RequestPhone,
    ReadTextFromClipboard {
        req_id: String,
    },
    SetupClosingBehavior {
        need_confirmation: bool,
    },
    SetHeaderColor(ColorSpec),
    SetBackgroundColor(ColorSpec),
    SetBottomBarColor(ColorSpec),
    /// `web_app_invoke_custom_method` → `sendWebAppCustomRequest`.
    InvokeCustomMethod {
        req_id: String,
        method: String,
        params: Value,
    },
    /// `web_app_trigger_haptic_feedback`: nothing to shake on a desktop.
    Haptic,
    /// A known event Quill cannot serve; `reply` is the failure event
    /// the app expects, if the bridge has one.
    Unsupported {
        name: String,
        reply: Option<BridgeReply>,
    },
}

/// An event for the page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BridgeReply {
    pub event: String,
    pub data: Value,
}

impl BridgeReply {
    fn new(event: &str, data: Value) -> Self {
        Self {
            event: event.to_string(),
            data,
        }
    }

    pub fn write_access_requested(allowed: bool) -> Self {
        Self::new(
            "write_access_requested",
            json!({ "status": if allowed { "allowed" } else { "cancelled" } }),
        )
    }

    pub fn phone_requested(shared: bool) -> Self {
        Self::new(
            "phone_requested",
            json!({ "status": if shared { "sent" } else { "cancelled" } }),
        )
    }

    /// `None` text means the person declined (or nothing was there).
    pub fn clipboard_text_received(req_id: &str, text: Option<&str>) -> Self {
        let mut data = Map::new();
        data.insert("req_id".into(), Value::String(req_id.to_string()));
        if let Some(text) = text {
            data.insert("data".into(), Value::String(text.to_string()));
        }
        Self::new("clipboard_text_received", Value::Object(data))
    }

    /// `button_id` empty: dismissed without a button.
    pub fn popup_closed(button_id: &str) -> Self {
        let mut data = Map::new();
        if !button_id.is_empty() {
            data.insert("button_id".into(), Value::String(button_id.to_string()));
        }
        Self::new("popup_closed", Value::Object(data))
    }

    pub fn invoice_closed(slug: &str, status: &str) -> Self {
        Self::new("invoice_closed", json!({ "slug": slug, "status": status }))
    }

    pub fn custom_method_result(req_id: &str, result: Value) -> Self {
        Self::new(
            "custom_method_invoked",
            json!({ "req_id": req_id, "result": result }),
        )
    }

    pub fn custom_method_error(req_id: &str, error: &str) -> Self {
        Self::new(
            "custom_method_invoked",
            json!({ "req_id": req_id, "error": error }),
        )
    }

    pub fn theme_changed(theme_params: Value) -> Self {
        Self::new("theme_changed", json!({ "theme_params": theme_params }))
    }

    pub fn main_button_pressed() -> Self {
        Self::new("main_button_pressed", json!({}))
    }

    pub fn settings_button_pressed() -> Self {
        Self::new("settings_button_pressed", json!({}))
    }
}

/// Parse one event the page posted. `data` is the raw JSON (an object,
/// or empty).
pub fn parse_event(name: &str, data: &str) -> Result<WebAppEvent, BridgeError> {
    if data.len() > MAX_WEB_APP_EVENT_BYTES {
        return Err(BridgeError::TooLarge);
    }
    let args: Map<String, Value> = if data.trim().is_empty() {
        Map::new()
    } else {
        match serde_json::from_str::<Value>(data) {
            Ok(Value::Object(map)) => map,
            // The bridge always sends an object; anything else is noise.
            Ok(_) => Map::new(),
            Err(_) => return Err(BridgeError::BadArguments("data is not JSON")),
        }
    };
    let text = |key: &str| args.get(key).and_then(Value::as_str).unwrap_or("");
    let flag = |key: &str| args.get(key).and_then(Value::as_bool).unwrap_or(false);
    Ok(match name {
        "web_app_ready" => WebAppEvent::Ready,
        "web_app_close" => WebAppEvent::Close,
        "web_app_data_send" => {
            let data = text("data");
            if data.is_empty() {
                return Err(BridgeError::BadArguments("data_send without data"));
            }
            if data.len() > MAX_DATA_SEND_BYTES {
                return Err(BridgeError::BadArguments("data_send over 4096 bytes"));
            }
            WebAppEvent::DataSend {
                data: data.to_string(),
            }
        }
        "web_app_switch_inline_query" => WebAppEvent::SwitchInlineQuery {
            query: text("query").chars().take(256).collect(),
            chat_types: args
                .get("chat_types")
                .and_then(Value::as_array)
                .map(|types| {
                    types
                        .iter()
                        .filter_map(Value::as_str)
                        .filter(|t| matches!(*t, "users" | "bots" | "groups" | "channels"))
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default(),
        },
        "web_app_setup_main_button" => WebAppEvent::SetupMainButton(button_setup(&args)?),
        "web_app_setup_secondary_button" => WebAppEvent::SetupSecondaryButton(button_setup(&args)?),
        "web_app_setup_back_button" => WebAppEvent::SetupBackButton {
            visible: flag("is_visible"),
        },
        "web_app_setup_settings_button" => WebAppEvent::SetupSettingsButton {
            visible: flag("is_visible"),
        },
        "web_app_request_theme" => WebAppEvent::RequestTheme,
        "web_app_request_viewport" | "web_app_expand" => WebAppEvent::RequestViewport,
        "web_app_open_link" => {
            let url = text("url").trim();
            if !crate::text::openable_http_url(url) {
                return Err(BridgeError::BadArguments("open_link needs an http(s) URL"));
            }
            WebAppEvent::OpenLink {
                url: url.to_string(),
                instant_view: flag("try_instant_view"),
            }
        }
        "web_app_open_tg_link" => match tg_link_url(text("path_full")) {
            Some(url) => WebAppEvent::OpenTgLink { url },
            None => return Err(BridgeError::BadArguments("open_tg_link needs a /path")),
        },
        "web_app_open_invoice" => {
            let slug = text("slug").trim();
            if slug.is_empty() {
                return Err(BridgeError::BadArguments("open_invoice without slug"));
            }
            WebAppEvent::OpenInvoice {
                slug: slug.chars().take(128).collect(),
            }
        }
        "web_app_open_popup" => WebAppEvent::OpenPopup(popup_spec(&args)?),
        "web_app_request_write_access" => WebAppEvent::RequestWriteAccess,
        "web_app_request_phone" => WebAppEvent::RequestPhone,
        "web_app_read_text_from_clipboard" => WebAppEvent::ReadTextFromClipboard {
            req_id: text("req_id").chars().take(64).collect(),
        },
        "web_app_setup_closing_behavior" => WebAppEvent::SetupClosingBehavior {
            need_confirmation: flag("need_confirmation"),
        },
        "web_app_set_header_color" => WebAppEvent::SetHeaderColor(color_spec(&args)?),
        "web_app_set_background_color" => WebAppEvent::SetBackgroundColor(color_spec(&args)?),
        "web_app_set_bottom_bar_color" => WebAppEvent::SetBottomBarColor(color_spec(&args)?),
        "web_app_invoke_custom_method" => {
            let method = text("method").trim();
            if method.is_empty() || method.chars().count() > MAX_CUSTOM_METHOD_CHARS {
                return Err(BridgeError::BadArguments("custom method name"));
            }
            let params = args.get("params").cloned().unwrap_or(Value::Null);
            if params.to_string().len() > MAX_CUSTOM_PARAMS_BYTES {
                return Err(BridgeError::BadArguments("custom method params too large"));
            }
            WebAppEvent::InvokeCustomMethod {
                req_id: text("req_id").chars().take(64).collect(),
                method: method.to_string(),
                params,
            }
        }
        "web_app_trigger_haptic_feedback" => WebAppEvent::Haptic,
        other => match refusal(other, &args) {
            Some(reply) => WebAppEvent::Unsupported {
                name: other.to_string(),
                reply: Some(reply),
            },
            None if is_known_but_ignored(other) => WebAppEvent::Unsupported {
                name: other.to_string(),
                reply: None,
            },
            None => return Err(BridgeError::UnknownEvent(other.to_string())),
        },
    })
}

/// `https://t.me` + `path_full`, when the path is one (tdesktop
/// `Panel::openTgLink`: must start with `/`). A scheme or host smuggled
/// into the path is refused.
pub fn tg_link_url(path_full: &str) -> Option<String> {
    let path = path_full.trim();
    if !path.starts_with('/') || path.starts_with("//") {
        return None;
    }
    if path.contains(['\\', '@', ' ']) || path.contains("://") {
        return None;
    }
    if path.chars().any(|c| c.is_control()) {
        return None;
    }
    Some(format!("https://t.me{path}"))
}

/// A `#rrggbb` (or `#rgb`) color as lowercase `#rrggbb`.
pub fn parse_color(text: &str) -> Option<String> {
    let hex = text.trim().strip_prefix('#')?;
    if !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    match hex.len() {
        6 => Some(format!("#{}", hex.to_ascii_lowercase())),
        3 => {
            let expanded: String = hex
                .chars()
                .flat_map(|c| [c, c])
                .collect::<String>()
                .to_ascii_lowercase();
            Some(format!("#{expanded}"))
        }
        _ => None,
    }
}

fn color_spec(args: &Map<String, Value>) -> Result<ColorSpec, BridgeError> {
    if let Some(key) = args.get("color_key").and_then(Value::as_str) {
        return match key {
            "bg_color" => Ok(ColorSpec::Key(ColorKey::BgColor)),
            "secondary_bg_color" => Ok(ColorSpec::Key(ColorKey::SecondaryBgColor)),
            "bottom_bar_bg_color" => Ok(ColorSpec::Key(ColorKey::BottomBarBgColor)),
            _ => Err(BridgeError::BadArguments("unknown color key")),
        };
    }
    args.get("color")
        .and_then(Value::as_str)
        .and_then(parse_color)
        .map(ColorSpec::Rgb)
        .ok_or(BridgeError::BadArguments("color is not #rrggbb"))
}

fn button_setup(args: &Map<String, Value>) -> Result<ButtonSetup, BridgeError> {
    let flag = |key: &str| args.get(key).and_then(Value::as_bool).unwrap_or(false);
    let color = |key: &str| args.get(key).and_then(Value::as_str).and_then(parse_color);
    let text: String = args
        .get("text")
        .and_then(Value::as_str)
        .unwrap_or("")
        .chars()
        .take(64)
        .collect();
    let visible = flag("is_visible") && !text.trim().is_empty();
    let position = match args.get("position").and_then(Value::as_str) {
        Some(p @ ("left" | "right" | "top" | "bottom")) => p.to_string(),
        _ => "left".to_string(),
    };
    Ok(ButtonSetup {
        visible,
        active: args
            .get("is_active")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        text,
        color: color("color"),
        text_color: color("text_color"),
        progress: flag("is_progress_visible"),
        shine: flag("has_shine_effect"),
        position,
    })
}

fn popup_spec(args: &Map<String, Value>) -> Result<PopupSpec, BridgeError> {
    let title: String = args
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    let message: String = args
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    if title.chars().count() > MAX_POPUP_TITLE_CHARS {
        return Err(BridgeError::BadArguments("popup title over 64 characters"));
    }
    if message.is_empty() || message.chars().count() > MAX_POPUP_MESSAGE_CHARS {
        return Err(BridgeError::BadArguments("popup message empty or over 256"));
    }
    let raw = args.get("buttons").and_then(Value::as_array);
    let mut buttons = Vec::new();
    for button in raw.into_iter().flatten() {
        let kind = match button
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("default")
        {
            kind @ ("default" | "ok" | "close" | "cancel" | "destructive") => kind,
            _ => return Err(BridgeError::BadArguments("popup button type")),
        };
        let id: String = button
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        if id.chars().count() > MAX_POPUP_BUTTON_ID_CHARS {
            return Err(BridgeError::BadArguments("popup button id"));
        }
        let text: String = match kind {
            "ok" => "OK".into(),
            "close" => "Close".into(),
            "cancel" => "Cancel".into(),
            _ => button
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim()
                .to_string(),
        };
        if text.is_empty() || text.chars().count() > MAX_POPUP_BUTTON_TEXT_CHARS {
            return Err(BridgeError::BadArguments("popup button text"));
        }
        buttons.push(PopupButtonSpec {
            id,
            kind: kind.to_string(),
            text,
        });
    }
    if buttons.is_empty() {
        buttons.push(PopupButtonSpec {
            id: String::new(),
            kind: "close".into(),
            text: "Close".into(),
        });
    }
    if buttons.len() > MAX_POPUP_BUTTONS {
        return Err(BridgeError::BadArguments("popup with more than 3 buttons"));
    }
    Ok(PopupSpec {
        title,
        message,
        buttons,
    })
}

/// Events Quill knows about but does not act on, and that expect no reply.
fn is_known_but_ignored(name: &str) -> bool {
    matches!(
        name,
        "web_app_share_to_story"
            | "web_app_request_chat"
            | "web_app_verify_age"
            | "web_app_set_viewport"
            | "web_app_hide_keyboard"
            | "web_app_toggle_orientation_lock"
            | "web_app_setup_swipe_behavior"
            | "iframe_ready"
            | "iframe_will_reload"
    )
}

/// The failure reply for a bridge feature Quill does not have, so the app
/// learns at once instead of waiting (tdesktop answers the same way for
/// what it lacks, e.g. `location_checked` with `available: false`).
pub fn refusal(name: &str, args: &Map<String, Value>) -> Option<BridgeReply> {
    let req_id = args.get("req_id").and_then(Value::as_str).unwrap_or("");
    let unsupported =
        |event: &str| Some(BridgeReply::new(event, json!({ "error": "UNSUPPORTED" })));
    let unsupported_req = |event: &str| {
        Some(BridgeReply::new(
            event,
            json!({ "req_id": req_id, "error": "UNSUPPORTED" }),
        ))
    };
    match name {
        "web_app_request_fullscreen" => unsupported("fullscreen_failed"),
        "web_app_exit_fullscreen" => Some(BridgeReply::new(
            "fullscreen_changed",
            json!({ "is_fullscreen": false }),
        )),
        "web_app_check_home_screen" => Some(BridgeReply::new(
            "home_screen_checked",
            json!({ "status": "unsupported" }),
        )),
        "web_app_add_to_home_screen" => unsupported("home_screen_failed"),
        "web_app_start_accelerometer" => unsupported("accelerometer_failed"),
        "web_app_start_device_orientation" => unsupported("device_orientation_failed"),
        "web_app_start_gyroscope" => unsupported("gyroscope_failed"),
        "web_app_stop_accelerometer" => Some(BridgeReply::new("accelerometer_stopped", json!({}))),
        "web_app_stop_device_orientation" => {
            Some(BridgeReply::new("device_orientation_stopped", json!({})))
        }
        "web_app_stop_gyroscope" => Some(BridgeReply::new("gyroscope_stopped", json!({}))),
        "web_app_check_location" => Some(BridgeReply::new(
            "location_checked",
            json!({ "available": false }),
        )),
        "web_app_request_location" => Some(BridgeReply::new(
            "location_requested",
            json!({ "available": false }),
        )),
        "web_app_biometry_get_info" | "web_app_biometry_request_access" => Some(BridgeReply::new(
            "biometry_info_received",
            json!({ "available": false }),
        )),
        "web_app_biometry_request_auth" => Some(BridgeReply::new(
            "biometry_auth_requested",
            json!({ "status": "failed" }),
        )),
        "web_app_biometry_update_token" => Some(BridgeReply::new(
            "biometry_token_updated",
            json!({ "status": "failed" }),
        )),
        "web_app_biometry_open_settings" => None,
        "web_app_open_scan_qr_popup" => Some(BridgeReply::new("scan_qr_popup_closed", json!({}))),
        "web_app_close_scan_qr_popup" => None,
        "web_app_request_file_download" => Some(BridgeReply::new(
            "file_download_requested",
            json!({ "status": "cancelled" }),
        )),
        "web_app_send_prepared_message" => unsupported("prepared_message_failed"),
        "web_app_set_emoji_status" => unsupported("emoji_status_failed"),
        "web_app_request_emoji_status_access" => Some(BridgeReply::new(
            "emoji_status_access_requested",
            json!({ "status": "cancelled" }),
        )),
        "web_app_device_storage_save_key"
        | "web_app_device_storage_get_key"
        | "web_app_device_storage_clear" => unsupported_req("device_storage_failed"),
        "web_app_secure_storage_save_key"
        | "web_app_secure_storage_get_key"
        | "web_app_secure_storage_restore_key"
        | "web_app_secure_storage_clear" => unsupported_req("secure_storage_failed"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(name: &str, data: &str) -> WebAppEvent {
        parse_event(name, data).unwrap_or_else(|e| panic!("{name}: {e:?}"))
    }

    #[test]
    fn simple_events_parse_with_and_without_data() {
        assert_eq!(parse("web_app_ready", ""), WebAppEvent::Ready);
        assert_eq!(parse("web_app_close", "{}"), WebAppEvent::Close);
        assert_eq!(
            parse("web_app_expand", "null"),
            WebAppEvent::RequestViewport
        );
        assert_eq!(
            parse("web_app_trigger_haptic_feedback", r##"{"type":"impact"}"##),
            WebAppEvent::Haptic
        );
    }

    #[test]
    fn unknown_and_malformed_events_are_errors() {
        assert_eq!(
            parse_event("web_app_launch_missiles", "{}"),
            Err(BridgeError::UnknownEvent("web_app_launch_missiles".into()))
        );
        assert_eq!(
            parse_event("web_app_close", "{not json"),
            Err(BridgeError::BadArguments("data is not JSON"))
        );
        let big = "x".repeat(MAX_WEB_APP_EVENT_BYTES + 1);
        assert_eq!(
            parse_event("web_app_close", &big),
            Err(BridgeError::TooLarge)
        );
    }

    #[test]
    fn data_send_is_bounded() {
        assert_eq!(
            parse("web_app_data_send", r##"{"data":"hello"}"##),
            WebAppEvent::DataSend {
                data: "hello".into()
            }
        );
        assert!(parse_event("web_app_data_send", "{}").is_err());
        let data = format!(r##"{{"data":"{}"}}"##, "x".repeat(MAX_DATA_SEND_BYTES + 1));
        assert!(parse_event("web_app_data_send", &data).is_err());
    }

    #[test]
    fn buttons_need_text_to_show() {
        let event = parse(
            "web_app_setup_main_button",
            r##"{"is_visible":true,"is_active":false,"text":"Pay","color":"#31B545","text_color":"#fff","is_progress_visible":true,"has_shine_effect":true}"##,
        );
        assert_eq!(
            event,
            WebAppEvent::SetupMainButton(ButtonSetup {
                visible: true,
                active: false,
                text: "Pay".into(),
                color: Some("#31b545".into()),
                text_color: Some("#ffffff".into()),
                progress: true,
                shine: true,
                position: "left".into(),
            })
        );
        let WebAppEvent::SetupSecondaryButton(button) = parse(
            "web_app_setup_secondary_button",
            r##"{"is_visible":true,"text":"  ","position":"bottom"}"##,
        ) else {
            panic!()
        };
        assert!(!button.visible, "no text, nothing to show");
        assert_eq!(button.position, "bottom");
        assert!(button.active, "active defaults to true");
        let WebAppEvent::SetupMainButton(button) = parse(
            "web_app_setup_main_button",
            r##"{"is_visible":true,"text":"Go","color":"red"}"##,
        ) else {
            panic!()
        };
        assert_eq!(button.color, None, "a named color is not a color");
    }

    #[test]
    fn links_are_http_only_and_tg_paths_stay_on_t_me() {
        assert_eq!(
            parse(
                "web_app_open_link",
                r##"{"url":"https://example.com/x","try_instant_view":true}"##
            ),
            WebAppEvent::OpenLink {
                url: "https://example.com/x".into(),
                instant_view: true
            }
        );
        assert!(parse_event("web_app_open_link", r##"{"url":"file:///etc/passwd"}"##).is_err());
        assert!(parse_event("web_app_open_link", r##"{"url":"javascript:alert(1)"}"##).is_err());
        assert_eq!(
            parse(
                "web_app_open_tg_link",
                r##"{"path_full":"/durov?start=1"}"##
            ),
            WebAppEvent::OpenTgLink {
                url: "https://t.me/durov?start=1".into()
            }
        );
        assert!(parse_event("web_app_open_tg_link", r##"{"path_full":"//evil.com/x"}"##).is_err());
        assert!(parse_event("web_app_open_tg_link", r##"{"path_full":"durov"}"##).is_err());
        assert!(parse_event("web_app_open_tg_link", r##"{"path_full":"/a@evil.com"}"##).is_err());
        assert!(parse_event("web_app_open_tg_link", r##"{"path_full":"/x/http://e"}"##).is_err());
    }

    #[test]
    fn popups_follow_telegrams_limits() {
        let WebAppEvent::OpenPopup(popup) = parse(
            "web_app_open_popup",
            r##"{"title":"Hi","message":"Continue?","buttons":[{"id":"y","type":"default","text":"Yes"},{"id":"n","type":"cancel"},{"id":"d","type":"destructive","text":"Delete"}]}"##,
        ) else {
            panic!()
        };
        assert_eq!(popup.buttons.len(), 3);
        assert_eq!(popup.buttons[1].text, "Cancel");
        assert_eq!(popup.buttons[2].kind, "destructive");
        // Defaults: no buttons means one Close.
        let WebAppEvent::OpenPopup(popup) = parse("web_app_open_popup", r##"{"message":"Hi"}"##)
        else {
            panic!()
        };
        assert_eq!(popup.buttons.len(), 1);
        assert_eq!(popup.buttons[0].kind, "close");
        assert!(parse_event("web_app_open_popup", r##"{"message":""}"##).is_err());
        let long = "m".repeat(MAX_POPUP_MESSAGE_CHARS + 1);
        assert!(
            parse_event(
                "web_app_open_popup",
                &format!(r##"{{"message":"{long}"}}"##)
            )
            .is_err()
        );
        let four = r##"{"message":"x","buttons":[{"type":"ok"},{"type":"ok"},{"type":"ok"},{"type":"ok"}]}"##;
        assert!(parse_event("web_app_open_popup", four).is_err());
        let bad_type = r##"{"message":"x","buttons":[{"type":"submit","text":"Go"}]}"##;
        assert!(parse_event("web_app_open_popup", bad_type).is_err());
        let no_text = r##"{"message":"x","buttons":[{"id":"a","type":"default"}]}"##;
        assert!(parse_event("web_app_open_popup", no_text).is_err());
    }

    #[test]
    fn colors_come_as_keys_or_hex() {
        assert_eq!(
            parse(
                "web_app_set_header_color",
                r##"{"color_key":"secondary_bg_color"}"##
            ),
            WebAppEvent::SetHeaderColor(ColorSpec::Key(ColorKey::SecondaryBgColor))
        );
        assert_eq!(
            parse("web_app_set_background_color", r##"{"color":"#ABC"}"##),
            WebAppEvent::SetBackgroundColor(ColorSpec::Rgb("#aabbcc".into()))
        );
        assert_eq!(
            parse(
                "web_app_set_bottom_bar_color",
                r##"{"color_key":"bottom_bar_bg_color"}"##
            ),
            WebAppEvent::SetBottomBarColor(ColorSpec::Key(ColorKey::BottomBarBgColor))
        );
        assert!(parse_event("web_app_set_header_color", r##"{"color":"url(x)"}"##).is_err());
        assert!(
            parse_event(
                "web_app_set_header_color",
                r##"{"color_key":"text_color"}"##
            )
            .is_err()
        );
        assert_eq!(parse_color("#12AB34"), Some("#12ab34".into()));
        assert_eq!(parse_color("12ab34"), None);
        assert_eq!(parse_color("#12ab3"), None);
    }

    #[test]
    fn consent_and_clipboard_events_carry_their_ids() {
        assert_eq!(
            parse("web_app_request_write_access", "{}"),
            WebAppEvent::RequestWriteAccess
        );
        assert_eq!(
            parse("web_app_request_phone", "{}"),
            WebAppEvent::RequestPhone
        );
        assert_eq!(
            parse("web_app_read_text_from_clipboard", r##"{"req_id":"r1"}"##),
            WebAppEvent::ReadTextFromClipboard {
                req_id: "r1".into()
            }
        );
        assert_eq!(
            parse(
                "web_app_setup_closing_behavior",
                r##"{"need_confirmation":true}"##
            ),
            WebAppEvent::SetupClosingBehavior {
                need_confirmation: true
            }
        );
        assert_eq!(
            parse("web_app_open_invoice", r##"{"slug":"abc"}"##),
            WebAppEvent::OpenInvoice { slug: "abc".into() }
        );
    }

    #[test]
    fn custom_methods_are_bounded() {
        assert_eq!(
            parse(
                "web_app_invoke_custom_method",
                r##"{"req_id":"7","method":"getRequestedContact","params":{"a":1}}"##
            ),
            WebAppEvent::InvokeCustomMethod {
                req_id: "7".into(),
                method: "getRequestedContact".into(),
                params: json!({ "a": 1 }),
            }
        );
        assert!(parse_event("web_app_invoke_custom_method", r##"{"req_id":"7"}"##).is_err());
    }

    #[test]
    fn unsupported_features_answer_honestly() {
        let WebAppEvent::Unsupported { name, reply } = parse("web_app_check_location", "{}") else {
            panic!()
        };
        assert_eq!(name, "web_app_check_location");
        assert_eq!(
            reply,
            Some(BridgeReply {
                event: "location_checked".into(),
                data: json!({ "available": false })
            })
        );
        let WebAppEvent::Unsupported { reply, .. } = parse(
            "web_app_device_storage_get_key",
            r##"{"req_id":"k1","key":"a"}"##,
        ) else {
            panic!()
        };
        assert_eq!(reply.unwrap().data["req_id"], "k1");
        let WebAppEvent::Unsupported { reply, .. } = parse("web_app_share_to_story", "{}") else {
            panic!()
        };
        assert_eq!(reply, None);
    }

    #[test]
    fn replies_have_the_bridge_shapes() {
        assert_eq!(
            BridgeReply::write_access_requested(true).data,
            json!({ "status": "allowed" })
        );
        assert_eq!(
            BridgeReply::clipboard_text_received("r", None).data,
            json!({ "req_id": "r" })
        );
        assert_eq!(
            BridgeReply::clipboard_text_received("r", Some("hi")).data,
            json!({ "req_id": "r", "data": "hi" })
        );
        assert_eq!(BridgeReply::popup_closed("").data, json!({}));
        assert_eq!(
            BridgeReply::popup_closed("ok").data,
            json!({ "button_id": "ok" })
        );
        assert_eq!(
            BridgeReply::invoice_closed("s", "cancelled").data,
            json!({ "slug": "s", "status": "cancelled" })
        );
    }
}
