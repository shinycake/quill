//! Mini apps: the TDLib calls behind opening, closing and talking to a
//! bot Web App (`docs/decisions/codex-miniapp-webview.md`). The UI side
//! (`ui/web_app_ui.rs`) owns the window; this only sends and marks the
//! session so answers find their context.
use super::*;
use crate::ids::{ChatId, RequestId};
use crate::state::{BotsPurpose, RequestPurpose, WebAppPending};
use crate::telegram::requests::{
    allow_bot_to_send_messages, can_bot_send_messages, close_web_app, get_attachment_menu_bot,
    get_grossing_web_app_bots, get_main_web_app, get_web_app_link_url, get_web_app_url,
    open_web_app, search_web_app, send_web_app_custom_request, send_web_app_data,
    toggle_bot_is_added_to_attachment_menu,
};
use crate::web_app::LaunchSource;
use crate::web_app::theme::ThemeParams;

/// How many bots the Apps tab asks for.
pub const GROSSING_APPS_LIMIT: i32 = 30;

impl<S: JsonSender> ConnectDriver<S> {
    fn web_app_extra(
        &mut self,
        purpose: RequestPurpose,
        chat_id: Option<ChatId>,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        Ok(self.session.request(purpose, chat_id))
    }

    fn mark_web_app_pending(
        &mut self,
        bot_user_id: i64,
        chat_id: Option<ChatId>,
        source: LaunchSource,
    ) {
        self.session.web_apps.pending = Some(WebAppPending {
            bot_user_id,
            chat_id: chat_id.map(|id| id.0),
            source,
        });
        self.session.web_apps.open_result = None;
    }

    /// `openWebApp` from a menu button, an inline `web_app` button or an
    /// attachment-menu bot (`url` empty) in `chat_id`.
    pub fn open_web_app(
        &mut self,
        chat_id: ChatId,
        bot_user_id: i64,
        url: &str,
        source: LaunchSource,
        theme: &ThemeParams,
    ) -> Result<RequestId, ConnectSendError> {
        let extra = self.web_app_extra(
            RequestPurpose::Bots(BotsPurpose::OpenWebApp { bot_user_id }),
            Some(chat_id),
        )?;
        self.mark_web_app_pending(bot_user_id, Some(chat_id), source);
        let json = open_web_app(extra, chat_id, bot_user_id, url, theme.td_json());
        self.send_json_request(extra, &json)
    }

    /// `getWebAppUrl` for a custom-keyboard `web_app` button; the app may
    /// answer with `web_app_data_send`.
    pub fn get_web_app_url(
        &mut self,
        chat_id: ChatId,
        bot_user_id: i64,
        url: &str,
        button_text: &str,
        theme: &ThemeParams,
    ) -> Result<RequestId, ConnectSendError> {
        let extra = self.web_app_extra(
            RequestPurpose::Bots(BotsPurpose::GetWebAppUrl { bot_user_id }),
            Some(chat_id),
        )?;
        self.mark_web_app_pending(
            bot_user_id,
            Some(chat_id),
            LaunchSource::KeyboardButton {
                button_text: button_text.to_string(),
            },
        );
        let json = get_web_app_url(extra, bot_user_id, url, theme.td_json());
        self.send_json_request(extra, &json)
    }

    /// `getMainWebApp`: the bot's main app.
    pub fn get_main_web_app(
        &mut self,
        chat_id: ChatId,
        bot_user_id: i64,
        start_parameter: &str,
        theme: &ThemeParams,
    ) -> Result<RequestId, ConnectSendError> {
        let extra = self.web_app_extra(
            RequestPurpose::Bots(BotsPurpose::GetMainWebApp { bot_user_id }),
            Some(chat_id),
        )?;
        self.mark_web_app_pending(bot_user_id, Some(chat_id), LaunchSource::MainApp);
        let json = get_main_web_app(
            extra,
            chat_id,
            bot_user_id,
            start_parameter,
            theme.td_json(),
        );
        self.send_json_request(extra, &json)
    }

    /// `getWebAppLinkUrl`: a named app link, after `searchWebApp` and the
    /// open box.
    pub fn get_web_app_link_url(
        &mut self,
        chat_id: ChatId,
        bot_user_id: i64,
        short_name: &str,
        start_parameter: &str,
        allow_write_access: bool,
        theme: &ThemeParams,
    ) -> Result<RequestId, ConnectSendError> {
        let extra = self.web_app_extra(
            RequestPurpose::Bots(BotsPurpose::GetWebAppLinkUrl { bot_user_id }),
            Some(chat_id),
        )?;
        self.mark_web_app_pending(
            bot_user_id,
            Some(chat_id),
            LaunchSource::Link {
                short_name: short_name.to_string(),
            },
        );
        let json = get_web_app_link_url(
            extra,
            chat_id,
            bot_user_id,
            short_name,
            start_parameter,
            allow_write_access,
            theme.td_json(),
        );
        self.send_json_request(extra, &json)
    }

    /// `searchWebApp`: what a `t.me/bot/app` link points at.
    pub fn search_web_app(
        &mut self,
        bot_user_id: i64,
        short_name: &str,
    ) -> Result<RequestId, ConnectSendError> {
        let extra = self.web_app_extra(
            RequestPurpose::Bots(BotsPurpose::SearchWebApp { bot_user_id }),
            None,
        )?;
        self.session.web_apps.found = None;
        let json = search_web_app(extra, bot_user_id, short_name);
        self.send_json_request(extra, &json)
    }

    /// `closeWebApp` once the window is gone (`launch_id` from
    /// `webAppInfo`; simple and link apps have none).
    pub fn close_web_app(&mut self, launch_id: i64) -> Result<Option<RequestId>, ConnectSendError> {
        if launch_id == 0 {
            return Ok(None);
        }
        let extra = self.web_app_extra(RequestPurpose::CloseWebApp, None)?;
        let json = close_web_app(extra, launch_id);
        self.send_json_request(extra, &json).map(Some)
    }

    /// `sendWebAppData`: a keyboard-button app's result.
    pub fn send_web_app_data(
        &mut self,
        bot_user_id: i64,
        button_text: &str,
        data: &str,
    ) -> Result<RequestId, ConnectSendError> {
        let extra = self.web_app_extra(RequestPurpose::SendWebAppData, None)?;
        self.session.web_apps.data_sent = None;
        let json = send_web_app_data(extra, bot_user_id, button_text, data);
        self.send_json_request(extra, &json)
    }

    /// `canBotSendMessages`: ok, or 404 when the person must be asked.
    pub fn can_bot_send_messages(
        &mut self,
        bot_user_id: i64,
    ) -> Result<RequestId, ConnectSendError> {
        let extra = self.web_app_extra(
            RequestPurpose::Bots(BotsPurpose::CanBotSendMessages { bot_user_id }),
            None,
        )?;
        self.session.web_apps.write_access = None;
        let json = can_bot_send_messages(extra, bot_user_id);
        self.send_json_request(extra, &json)
    }

    /// `allowBotToSendMessages`, after the person said yes.
    pub fn allow_bot_to_send_messages(
        &mut self,
        bot_user_id: i64,
    ) -> Result<RequestId, ConnectSendError> {
        let extra = self.web_app_extra(
            RequestPurpose::Bots(BotsPurpose::AllowBotToSendMessages { bot_user_id }),
            None,
        )?;
        self.session.web_apps.write_access = None;
        let json = allow_bot_to_send_messages(extra, bot_user_id);
        self.send_json_request(extra, &json)
    }

    /// `getAttachmentMenuBot`: the terms of adding a bot to the menu.
    pub fn fetch_attachment_menu_bot(
        &mut self,
        bot_user_id: i64,
    ) -> Result<RequestId, ConnectSendError> {
        let extra = self.web_app_extra(
            RequestPurpose::Bots(BotsPurpose::GetAttachmentMenuBot { bot_user_id }),
            None,
        )?;
        self.session.web_apps.attachment_menu_bot = None;
        let json = get_attachment_menu_bot(extra, bot_user_id);
        self.send_json_request(extra, &json)
    }

    /// `toggleBotIsAddedToAttachmentMenu`, after the add (or remove) box.
    pub fn toggle_bot_in_attachment_menu(
        &mut self,
        bot_user_id: i64,
        added: bool,
        allow_write_access: bool,
    ) -> Result<RequestId, ConnectSendError> {
        let extra = self.web_app_extra(
            RequestPurpose::Bots(BotsPurpose::ToggleBotInAttachmentMenu { bot_user_id, added }),
            None,
        )?;
        self.session.web_apps.attachment_menu_toggled = None;
        let json =
            toggle_bot_is_added_to_attachment_menu(extra, bot_user_id, added, allow_write_access);
        self.send_json_request(extra, &json)
    }

    /// `getGrossingWebAppBots` for the Apps tab; once per session unless
    /// it failed.
    pub fn fetch_grossing_web_app_bots(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if self.session.web_apps.grossing_loading
            || self
                .session
                .web_apps
                .grossing_bots
                .as_ref()
                .is_some_and(|bots| !bots.is_empty())
        {
            return Ok(None);
        }
        let extra = self.web_app_extra(RequestPurpose::GetGrossingWebAppBots, None)?;
        self.session.web_apps.grossing_loading = true;
        let json = get_grossing_web_app_bots(extra, GROSSING_APPS_LIMIT);
        match self.send_json_request(extra, &json) {
            Ok(id) => Ok(Some(id)),
            Err(err) => {
                self.session.web_apps.grossing_loading = false;
                Err(err)
            }
        }
    }

    /// `sendWebAppCustomRequest` for `web_app_invoke_custom_method`;
    /// the answer comes back under the app's `req_id`.
    pub fn send_web_app_custom_request(
        &mut self,
        bot_user_id: i64,
        req_id: &str,
        method: &str,
        parameters: &str,
    ) -> Result<RequestId, ConnectSendError> {
        let extra = self.web_app_extra(RequestPurpose::SendWebAppCustomRequest, None)?;
        self.session
            .web_apps
            .custom_requests
            .insert(extra.0, req_id.to_string());
        let json = send_web_app_custom_request(extra, bot_user_id, method, parameters);
        match self.send_json_request(extra, &json) {
            Ok(id) => Ok(id),
            Err(err) => {
                self.session.web_apps.custom_requests.remove(&extra.0);
                Err(err)
            }
        }
    }
}
