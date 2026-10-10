//! Telegram mini apps (bot Web Apps), the pure half.
//!
//! A mini app is a web page a bot serves, opened from its menu button, an
//! inline or keyboard `web_app` button, the attachment menu, the profile's
//! "Open App", the Apps tab, or a `t.me/bot/app` link. Telegram Desktop
//! (`inline_bots/bot_attach_web_view.cpp`, `ui/chat/attach/attach_bot_webview.cpp`)
//! shows it in its own panel window with a header and talks to it through
//! the `Telegram.WebView` JavaScript bridge. Quill hosts the page in a
//! separate helper process (`crates/webview-host`, see
//! `docs/decisions/codex-miniapp-webview.md`); the UI side lives in
//! `ui/web_app_ui.rs`.
//!
//! This module holds what can be decided without a window:
//! - [`bridge`]: parsing and validation of the bridge events a page sends,
//!   and the replies Quill owes it;
//! - [`theme`]: the theme colors TDLib (`themeParameters`) and the page
//!   (`theme_params`) get;
//! - [`trust`]: which bots the person already agreed to open mini apps
//!   for (tdesktop's `isPeerTrustedOpenWebView`).

pub mod bridge;
pub mod theme;
pub mod trust;

/// Telegram's terms for mini apps, linked from the first-open box
/// (tdesktop `lng_profile_open_app_terms`).
pub const MINI_APP_TERMS_URL: &str = "https://telegram.org/tos/mini-apps";

/// How a mini app was launched; decides which TDLib call opened it and
/// what it may do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchSource {
    /// The bot's menu button (`botMenuButton.url`): `openWebApp`.
    MenuButton,
    /// An inline keyboard `inlineKeyboardButtonTypeWebApp`: `openWebApp`.
    InlineButton,
    /// A custom keyboard `keyboardButtonTypeWebApp` ("simple" web app,
    /// `getWebAppUrl`): the page may send data back with
    /// `web_app_data_send`, which becomes a `sendWebAppData` message.
    KeyboardButton { button_text: String },
    /// An attachment menu bot (`attachmentMenuBot`): `openWebApp` with an
    /// empty URL.
    AttachmentMenu,
    /// The bot's main app (`getMainWebApp`): profile "Open App", the
    /// Apps tab, `t.me/bot?startapp`.
    MainApp,
    /// A named app link `t.me/bot/app` (`getWebAppLinkUrl`).
    Link { short_name: String },
}

impl LaunchSource {
    /// Only a keyboard-button app may post data as a message
    /// (tdesktop `WebViewInstance::botSendData`: `button->simple`).
    pub fn allows_data_send(&self) -> bool {
        matches!(self, Self::KeyboardButton { .. })
    }
}

/// Everything the window needs to know about the app it shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebAppLaunch {
    pub bot_user_id: i64,
    pub bot_name: String,
    /// The chat the app was opened from (an `openWebApp` reply goes there).
    pub chat_id: Option<i64>,
    pub source: LaunchSource,
    /// `webAppInfo.launch_id`, for `closeWebApp`; 0 for simple apps.
    pub launch_id: i64,
    pub url: String,
}

/// The header menu, in tdesktop's order (`Panel::showMenu`): settings
/// when the app asked for it, open the bot, reload, the terms, the
/// privacy policy when the bot has one, and leaving the attachment menu
/// when the bot is in it.
pub fn menu_items(
    settings: bool,
    has_privacy_policy: bool,
    in_attachment_menu: bool,
) -> Vec<(&'static str, &'static str, bool)> {
    let mut items = Vec::new();
    if settings {
        items.push(("settings", "Settings", false));
    }
    items.push(("open_bot", "Open bot", false));
    items.push(("reload", "Reload page", false));
    items.push(("terms", "Terms of use", false));
    if has_privacy_policy {
        items.push(("privacy", "Privacy policy", false));
    }
    if in_attachment_menu {
        items.push(("remove_from_menu", "Remove from menu", true));
    }
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_keyboard_apps_send_data() {
        assert!(
            LaunchSource::KeyboardButton {
                button_text: "Order".into()
            }
            .allows_data_send()
        );
        for source in [
            LaunchSource::MenuButton,
            LaunchSource::InlineButton,
            LaunchSource::AttachmentMenu,
            LaunchSource::MainApp,
            LaunchSource::Link {
                short_name: "shop".into(),
            },
        ] {
            assert!(!source.allows_data_send(), "{source:?}");
        }
    }

    #[test]
    fn menu_follows_tdesktop_order() {
        let ids: Vec<&str> = menu_items(true, true, true)
            .into_iter()
            .map(|(id, _, _)| id)
            .collect();
        assert_eq!(
            ids,
            [
                "settings",
                "open_bot",
                "reload",
                "terms",
                "privacy",
                "remove_from_menu"
            ]
        );
        let ids: Vec<&str> = menu_items(false, false, false)
            .into_iter()
            .map(|(id, _, _)| id)
            .collect();
        assert_eq!(ids, ["open_bot", "reload", "terms"]);
        assert!(
            menu_items(false, false, true)[3].2,
            "removal is the attention item"
        );
    }
}
