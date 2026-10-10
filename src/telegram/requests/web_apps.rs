//! Mini-app requests (TDLib 1.8.68, `schema/td_api.tl`): `openWebApp`
//! (13505), `getWebAppUrl` (13484), `getMainWebApp` (13478),
//! `getWebAppLinkUrl` (13471), `searchWebApp` (13459), `closeWebApp`
//! (13508), `sendWebAppData` (13495), `canBotSendMessages` (15365),
//! `allowBotToSendMessages` (15368), `getAttachmentMenuBot` (14338),
//! `toggleBotIsAddedToAttachmentMenu` (14344), `getGrossingWebAppBots`
//! (13454), `sendWebAppCustomRequest`.

use crate::ids::{ChatId, RequestId};
use serde_json::{Value, json};

/// `webAppOpenParameters` (line 1152): the theme, Quill's name and the
/// panel mode. Quill shows every app in one window; `fullSize` is the
/// mode Telegram Desktop reports for its panel.
pub fn web_app_open_parameters(theme: Value) -> Value {
    json!({
        "@type": "webAppOpenParameters",
        "theme": theme,
        "application_name": "quill",
        "mode": { "@type": "webAppOpenModeFullSize" },
    })
}

/// `openWebApp`: a menu button, an inline button or an attachment-menu
/// bot (`url` empty) in `chat_id`.
pub fn open_web_app(
    extra: RequestId,
    chat_id: ChatId,
    bot_user_id: i64,
    url: &str,
    theme: Value,
) -> String {
    json!({
        "@type": "openWebApp",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "bot_user_id": bot_user_id,
        "url": url,
        "parameters": web_app_open_parameters(theme),
    })
    .to_string()
}

/// `getWebAppUrl`: a custom-keyboard `keyboardButtonTypeWebApp` ("simple"
/// web app, answers with `web_app_data_send`).
pub fn get_web_app_url(extra: RequestId, bot_user_id: i64, url: &str, theme: Value) -> String {
    json!({
        "@type": "getWebAppUrl",
        "@extra": extra.as_extra(),
        "bot_user_id": bot_user_id,
        "url": url,
        "parameters": web_app_open_parameters(theme),
    })
    .to_string()
}

/// `getMainWebApp`: the bot's main app (profile "Open App", Apps tab,
/// `t.me/bot?startapp=`).
pub fn get_main_web_app(
    extra: RequestId,
    chat_id: ChatId,
    bot_user_id: i64,
    start_parameter: &str,
    theme: Value,
) -> String {
    json!({
        "@type": "getMainWebApp",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "bot_user_id": bot_user_id,
        "start_parameter": start_parameter,
        "parameters": web_app_open_parameters(theme),
    })
    .to_string()
}

/// `getWebAppLinkUrl`: a named app link `t.me/bot/app`.
#[allow(clippy::too_many_arguments)]
pub fn get_web_app_link_url(
    extra: RequestId,
    chat_id: ChatId,
    bot_user_id: i64,
    short_name: &str,
    start_parameter: &str,
    allow_write_access: bool,
    theme: Value,
) -> String {
    json!({
        "@type": "getWebAppLinkUrl",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "bot_user_id": bot_user_id,
        "web_app_short_name": short_name,
        "start_parameter": start_parameter,
        "allow_write_access": allow_write_access,
        "parameters": web_app_open_parameters(theme),
    })
    .to_string()
}

pub fn search_web_app(extra: RequestId, bot_user_id: i64, short_name: &str) -> String {
    json!({
        "@type": "searchWebApp",
        "@extra": extra.as_extra(),
        "bot_user_id": bot_user_id,
        "web_app_short_name": short_name,
    })
    .to_string()
}

pub fn close_web_app(extra: RequestId, launch_id: i64) -> String {
    json!({
        "@type": "closeWebApp",
        "@extra": extra.as_extra(),
        "web_app_launch_id": launch_id,
    })
    .to_string()
}

/// `sendWebAppData`: the keyboard-button app's result, as a message in
/// the bot's chat (`messageWebAppDataSent` for the person).
pub fn send_web_app_data(
    extra: RequestId,
    bot_user_id: i64,
    button_text: &str,
    data: &str,
) -> String {
    json!({
        "@type": "sendWebAppData",
        "@extra": extra.as_extra(),
        "bot_user_id": bot_user_id,
        "button_text": button_text,
        "data": data,
    })
    .to_string()
}

pub fn can_bot_send_messages(extra: RequestId, bot_user_id: i64) -> String {
    json!({
        "@type": "canBotSendMessages",
        "@extra": extra.as_extra(),
        "bot_user_id": bot_user_id,
    })
    .to_string()
}

pub fn allow_bot_to_send_messages(extra: RequestId, bot_user_id: i64) -> String {
    json!({
        "@type": "allowBotToSendMessages",
        "@extra": extra.as_extra(),
        "bot_user_id": bot_user_id,
    })
    .to_string()
}

pub fn get_attachment_menu_bot(extra: RequestId, bot_user_id: i64) -> String {
    json!({
        "@type": "getAttachmentMenuBot",
        "@extra": extra.as_extra(),
        "bot_user_id": bot_user_id,
    })
    .to_string()
}

pub fn toggle_bot_is_added_to_attachment_menu(
    extra: RequestId,
    bot_user_id: i64,
    is_added: bool,
    allow_write_access: bool,
) -> String {
    json!({
        "@type": "toggleBotIsAddedToAttachmentMenu",
        "@extra": extra.as_extra(),
        "bot_user_id": bot_user_id,
        "is_added": is_added,
        "allow_write_access": allow_write_access,
    })
    .to_string()
}

/// `getGrossingWebAppBots`: the Apps tab's list (first page).
pub fn get_grossing_web_app_bots(extra: RequestId, limit: i32) -> String {
    json!({
        "@type": "getGrossingWebAppBots",
        "@extra": extra.as_extra(),
        "offset": "",
        "limit": limit,
    })
    .to_string()
}

/// `sendWebAppCustomRequest` (`web_app_invoke_custom_method`):
/// `parameters` is the app's params as a JSON string.
pub fn send_web_app_custom_request(
    extra: RequestId,
    bot_user_id: i64,
    method: &str,
    parameters: &str,
) -> String {
    json!({
        "@type": "sendWebAppCustomRequest",
        "@extra": extra.as_extra(),
        "bot_user_id": bot_user_id,
        "method": method,
        "parameters": parameters,
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(json: String) -> Value {
        serde_json::from_str(&json).unwrap()
    }

    #[test]
    fn open_calls_carry_the_theme_and_mode() {
        let theme = json!({"@type": "themeParameters", "button_color": 1});
        let value = parse(open_web_app(
            RequestId(1),
            ChatId(5),
            42,
            "https://a/",
            theme.clone(),
        ));
        assert_eq!(value["@type"], "openWebApp");
        assert_eq!(value["chat_id"], 5);
        assert_eq!(value["bot_user_id"], 42);
        assert_eq!(value["url"], "https://a/");
        assert_eq!(value["parameters"]["@type"], "webAppOpenParameters");
        assert_eq!(value["parameters"]["theme"], theme);
        assert_eq!(
            value["parameters"]["mode"]["@type"],
            "webAppOpenModeFullSize"
        );
        assert_eq!(value["parameters"]["application_name"], "quill");

        let value = parse(get_web_app_url(RequestId(2), 42, "https://b/", json!({})));
        assert_eq!(value["@type"], "getWebAppUrl");
        assert!(value.get("chat_id").is_none());

        let value = parse(get_main_web_app(
            RequestId(3),
            ChatId(5),
            42,
            "promo",
            json!({}),
        ));
        assert_eq!(value["@type"], "getMainWebApp");
        assert_eq!(value["start_parameter"], "promo");

        let value = parse(get_web_app_link_url(
            RequestId(4),
            ChatId(5),
            42,
            "shop",
            "x",
            true,
            json!({}),
        ));
        assert_eq!(value["@type"], "getWebAppLinkUrl");
        assert_eq!(value["web_app_short_name"], "shop");
        assert_eq!(value["allow_write_access"], true);
    }

    #[test]
    fn the_small_calls_have_their_fields() {
        assert_eq!(
            parse(search_web_app(RequestId(1), 42, "shop"))["web_app_short_name"],
            "shop"
        );
        assert_eq!(
            parse(close_web_app(RequestId(1), 77))["web_app_launch_id"],
            77
        );
        let value = parse(send_web_app_data(RequestId(1), 42, "Order", "{\"a\":1}"));
        assert_eq!(value["@type"], "sendWebAppData");
        assert_eq!(value["button_text"], "Order");
        assert_eq!(value["data"], "{\"a\":1}");
        assert_eq!(
            parse(can_bot_send_messages(RequestId(1), 42))["@type"],
            "canBotSendMessages"
        );
        assert_eq!(
            parse(allow_bot_to_send_messages(RequestId(1), 42))["@type"],
            "allowBotToSendMessages"
        );
        assert_eq!(
            parse(get_attachment_menu_bot(RequestId(1), 42))["@type"],
            "getAttachmentMenuBot"
        );
        let value = parse(toggle_bot_is_added_to_attachment_menu(
            RequestId(1),
            42,
            true,
            false,
        ));
        assert_eq!(value["is_added"], true);
        assert_eq!(value["allow_write_access"], false);
        let value = parse(get_grossing_web_app_bots(RequestId(1), 30));
        assert_eq!(value["limit"], 30);
        assert_eq!(value["offset"], "");
        let value = parse(send_web_app_custom_request(RequestId(1), 42, "m", "{}"));
        assert_eq!(value["method"], "m");
        assert_eq!(value["parameters"], "{}");
    }
}
