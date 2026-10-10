//! Mini apps: routing of the TDLib answers into [`WebApps`].
use super::*;
use crate::telegram::envelope::{EnvelopePayload, TdError};

impl Session {
    /// A mini-app payload (`webAppInfo`, `webAppUrl`, `mainWebApp`,
    /// `foundWebApp`, `attachmentMenuBot`, `updateAttachmentMenuBots`,
    /// `customRequestResult`). Answers are matched by `@extra`, so only
    /// Quill's own calls write the one-shot slots.
    pub(crate) fn apply_web_app_payload(
        &mut self,
        payload: EnvelopePayload,
        pending: Option<&PendingRequest>,
    ) {
        let purpose = pending.map(|p| p.purpose);
        match payload {
            EnvelopePayload::WebAppInfo { launch_id, url } => {
                if let Some(RequestPurpose::OpenWebApp { bot_user_id }) = purpose {
                    self.finish_web_app_open(bot_user_id, launch_id, url);
                }
            }
            EnvelopePayload::WebAppUrl { url } => {
                if let Some(
                    RequestPurpose::GetWebAppUrl { bot_user_id }
                    | RequestPurpose::GetWebAppLinkUrl { bot_user_id },
                ) = purpose
                {
                    self.finish_web_app_open(bot_user_id, 0, url);
                }
            }
            EnvelopePayload::MainWebApp { url } => {
                if let Some(RequestPurpose::GetMainWebApp { bot_user_id }) = purpose {
                    self.finish_web_app_open(bot_user_id, 0, url);
                }
            }
            EnvelopePayload::FoundWebApp(app) => {
                if let Some(RequestPurpose::SearchWebApp { bot_user_id }) = purpose {
                    self.web_apps.found = Some(Ok((bot_user_id, app)));
                }
            }
            EnvelopePayload::AttachmentMenuBot(bot) => {
                if let Some(RequestPurpose::GetAttachmentMenuBot { .. }) = purpose {
                    self.web_apps.attachment_menu_bot = Some(Ok(bot));
                }
            }
            EnvelopePayload::UpdateAttachmentMenuBots(bots) => {
                self.web_apps.attachment_menu_bots = bots;
            }
            EnvelopePayload::UpdateWebAppMessageSent { .. } => {}
            EnvelopePayload::CustomRequestResult { result } => {
                if let Some(p) = pending
                    && p.purpose == RequestPurpose::SendWebAppCustomRequest
                    && let Some(req_id) = self.web_apps.custom_requests.remove(&p.id.0)
                {
                    self.web_apps.custom_replies.push(CustomRequestReply {
                        req_id,
                        result: Ok(result),
                    });
                }
            }
            _ => {}
        }
    }

    fn finish_web_app_open(&mut self, bot_user_id: i64, launch_id: i64, url: String) {
        let pending = self.web_apps.pending.take();
        let (chat_id, source) = match pending {
            Some(p) if p.bot_user_id == bot_user_id => (p.chat_id, p.source),
            _ => (None, crate::web_app::LaunchSource::MenuButton),
        };
        self.web_apps.open_result = Some(if url.is_empty() {
            WebAppOpenResult::Failed {
                bot_user_id,
                message: "The bot returned no app address.".into(),
            }
        } else {
            WebAppOpenResult::Opened {
                bot_user_id,
                chat_id,
                source,
                launch_id,
                url,
            }
        });
    }

    /// `getGrossingWebAppBots` returns `foundUsers`, which the envelope
    /// parses as `Users`.
    pub(crate) fn apply_web_app_users(
        &mut self,
        user_ids: &[i64],
        pending: Option<&PendingRequest>,
    ) -> bool {
        if pending.map(|p| p.purpose) == Some(RequestPurpose::GetGrossingWebAppBots) {
            self.web_apps.grossing_bots = Some(user_ids.to_vec());
            self.web_apps.grossing_loading = false;
            return true;
        }
        false
    }

    pub(crate) fn apply_web_app_ok(&mut self, pending: Option<&PendingRequest>) {
        match pending.map(|p| p.purpose) {
            Some(RequestPurpose::CanBotSendMessages { bot_user_id }) => {
                self.web_apps.write_access = Some((bot_user_id, WriteAccessResult::Allowed));
            }
            Some(RequestPurpose::AllowBotToSendMessages { bot_user_id }) => {
                self.web_apps.write_access = Some((bot_user_id, WriteAccessResult::Granted));
            }
            Some(RequestPurpose::ToggleBotInAttachmentMenu { bot_user_id, added }) => {
                self.web_apps.attachment_menu_toggled = Some(Ok((bot_user_id, added)));
            }
            Some(RequestPurpose::SendWebAppData) => {
                self.web_apps.data_sent = Some(Ok(()));
            }
            _ => {}
        }
    }

    pub(crate) fn apply_web_app_error(&mut self, pending: Option<&PendingRequest>, err: &TdError) {
        let Some(p) = pending else { return };
        match p.purpose {
            RequestPurpose::OpenWebApp { bot_user_id }
            | RequestPurpose::GetWebAppUrl { bot_user_id }
            | RequestPurpose::GetMainWebApp { bot_user_id }
            | RequestPurpose::GetWebAppLinkUrl { bot_user_id } => {
                self.web_apps.pending = None;
                self.web_apps.open_result = Some(WebAppOpenResult::Failed {
                    bot_user_id,
                    message: web_app_error_text(err),
                });
            }
            RequestPurpose::SearchWebApp { .. } => {
                self.web_apps.found = Some(Err(if err.code == 404 {
                    "This bot has no app with that name.".into()
                } else {
                    web_app_error_text(err)
                }));
            }
            RequestPurpose::CanBotSendMessages { bot_user_id } => {
                self.web_apps.write_access = Some((
                    bot_user_id,
                    if err.code == 404 {
                        WriteAccessResult::NeedsConsent
                    } else {
                        WriteAccessResult::Denied
                    },
                ));
            }
            RequestPurpose::AllowBotToSendMessages { bot_user_id } => {
                self.web_apps.write_access = Some((bot_user_id, WriteAccessResult::Denied));
            }
            RequestPurpose::GetAttachmentMenuBot { .. } => {
                self.web_apps.attachment_menu_bot = Some(Err(web_app_error_text(err)));
            }
            RequestPurpose::ToggleBotInAttachmentMenu { .. } => {
                self.web_apps.attachment_menu_toggled = Some(Err(web_app_error_text(err)));
            }
            RequestPurpose::GetGrossingWebAppBots => {
                self.web_apps.grossing_loading = false;
                self.web_apps.grossing_bots.get_or_insert_with(Vec::new);
            }
            RequestPurpose::SendWebAppCustomRequest => {
                if let Some(req_id) = self.web_apps.custom_requests.remove(&p.id.0) {
                    self.web_apps.custom_replies.push(CustomRequestReply {
                        req_id,
                        result: Err(if err.code == 400 {
                            "BAD_REQUEST".into()
                        } else {
                            "UNKNOWN_ERROR".into()
                        }),
                    });
                }
            }
            RequestPurpose::SendWebAppData => {
                self.web_apps.data_sent = Some(Err(web_app_error_text(err)));
            }
            _ => {}
        }
    }
}

/// A short, secret-free line for a failed mini-app call (`TdError` keeps
/// no message text on purpose).
fn web_app_error_text(err: &TdError) -> String {
    match err.code {
        404 => "The bot has no app to open.".into(),
        400 => "The bot refused to open its app.".into(),
        _ => "The app could not be opened. Try again later.".into(),
    }
}
