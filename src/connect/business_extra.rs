//! Connect driver: Telegram Business start page, links to chat, the
//! connected chatbot and its lookup, and the ads switch.
use super::*;
use crate::business_bots::{self as bots, ConnectedBot};
use crate::business_chat_links as links;
use crate::business_settings::BusinessSettingKind;
use crate::state::{BotLookup, BotLookupPart, ChatLinkOp, RequestPurpose, SettingsPurpose};

impl<S: JsonSender> ConnectDriver<S> {
    fn send_business_extra(
        &mut self,
        purpose: SettingsPurpose,
        build: impl FnOnce(&str) -> String,
    ) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::Settings(purpose), None);
        let json = build(&extra.as_extra());
        self.send_json_request(extra, &json).map(|_| ())
    }

    /// `setBusinessStartPage`; empty values remove the custom page.
    pub fn save_business_start_page(
        &mut self,
        title: &str,
        message: &str,
        sticker_file_id: Option<i32>,
    ) -> Result<(), ConnectSendError> {
        let state = &self.session.settings.business_extra;
        if title.chars().count() > state.start_page_title_max()
            || message.chars().count() > state.start_page_message_max()
        {
            return Err(ConnectSendError::InvalidRequest);
        }
        let kind = BusinessSettingKind::StartPage;
        self.session.settings.business.error = None;
        self.send_business_extra(SettingsPurpose::SaveBusinessSetting { kind }, |extra| {
            crate::business_start_page::set_start_page_request(
                extra,
                title,
                message,
                sticker_file_id,
            )
        })?;
        self.session.settings.business.saving.push(kind);
        Ok(())
    }

    /// `getBusinessChatLinks`, once unless it failed or a link was deleted.
    pub fn ensure_chat_links(&mut self) -> Result<(), ConnectSendError> {
        let state = &mut self.session.settings.business_extra;
        if state.chat_links_requested {
            return Ok(());
        }
        state.chat_links_requested = true;
        let sent =
            self.send_business_extra(SettingsPurpose::GetChatLinks, links::get_links_request);
        if sent.is_err() {
            self.session.settings.business_extra.chat_links_requested = false;
        }
        sent
    }

    pub fn create_chat_link(&mut self, markup: &str, title: &str) -> Result<(), ConnectSendError> {
        let count = self
            .session
            .settings
            .business_extra
            .chat_links
            .as_ref()
            .map_or(0, Vec::len);
        if count >= self.session.settings.business_extra.chat_links_max() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.settings.business.error = None;
        self.send_business_extra(
            SettingsPurpose::MutateChatLink {
                op: ChatLinkOp::Create,
            },
            |extra| links::create_link_request(extra, markup, title),
        )?;
        self.session.settings.business_extra.chat_links_saving += 1;
        Ok(())
    }

    pub fn edit_chat_link(
        &mut self,
        link: &str,
        markup: &str,
        title: &str,
    ) -> Result<(), ConnectSendError> {
        self.session.settings.business.error = None;
        self.send_business_extra(
            SettingsPurpose::MutateChatLink {
                op: ChatLinkOp::Edit,
            },
            |extra| links::edit_link_request(extra, link, markup, title),
        )?;
        self.session.settings.business_extra.chat_links_saving += 1;
        Ok(())
    }

    /// Deletes a link and drops its row; the list is asked for again on
    /// `ok`.
    pub fn delete_chat_link(&mut self, link: &str) -> Result<(), ConnectSendError> {
        self.session.settings.business.error = None;
        self.send_business_extra(
            SettingsPurpose::MutateChatLink {
                op: ChatLinkOp::Delete,
            },
            |extra| links::delete_link_request(extra, link),
        )?;
        if let Some(list) = self.session.settings.business_extra.chat_links.as_mut() {
            list.retain(|l| l.link != link);
        }
        Ok(())
    }

    /// `getBusinessConnectedBot`, once unless it failed or was saved.
    pub fn ensure_connected_bot(&mut self) -> Result<(), ConnectSendError> {
        let state = &mut self.session.settings.business_extra;
        if state.connected_bot_requested {
            return Ok(());
        }
        state.connected_bot_requested = true;
        let sent = self.send_business_extra(
            SettingsPurpose::GetConnectedBot,
            bots::get_connected_bot_request,
        );
        if sent.is_err() {
            self.session.settings.business_extra.connected_bot_requested = false;
        }
        sent
    }

    /// Connect or update `bot`, or (with `None`) disconnect `previous`.
    pub fn save_connected_bot(
        &mut self,
        bot: Option<&ConnectedBot>,
        previous: Option<i64>,
    ) -> Result<(), ConnectSendError> {
        self.session.settings.business.error = None;
        match (bot, previous) {
            (Some(bot), _) => self
                .send_business_extra(SettingsPurpose::SaveConnectedBot, |extra| {
                    bots::set_connected_bot_request(extra, bot)
                })?,
            (None, Some(previous)) => self
                .send_business_extra(SettingsPurpose::SaveConnectedBot, |extra| {
                    bots::delete_connected_bot_request(extra, previous)
                })?,
            (None, None) => return Ok(()),
        }
        self.session.settings.business_extra.connected_bot = Some(bot.cloned());
        Ok(())
    }

    /// Look up bots for a typed username (tdesktop `LookupBots`): the
    /// exact username, own chats and global results. An invalid query
    /// clears the lookup.
    pub fn lookup_business_bot(&mut self, text: &str) -> Result<(), ConnectSendError> {
        let generation = self
            .session
            .settings
            .business_extra
            .lookup
            .generation
            .wrapping_add(1);
        let Some(query) = bots::parse_username_query(text) else {
            self.session.settings.business_extra.lookup = BotLookup {
                generation,
                ..BotLookup::default()
            };
            return Ok(());
        };
        if self.session.settings.business_extra.lookup.query == query {
            return Ok(());
        }
        self.session.settings.business_extra.lookup = BotLookup {
            generation,
            query: query.clone(),
            ..BotLookup::default()
        };
        let part = |part| SettingsPurpose::LookupBusinessBot { generation, part };
        self.send_business_extra(part(BotLookupPart::Exact), |extra| {
            bots::search_public_chat_request(extra, &query)
        })?;
        self.send_business_extra(part(BotLookupPart::Mine), |extra| {
            bots::search_bots_on_server_request(extra, &query)
        })?;
        self.send_business_extra(part(BotLookupPart::Global), |extra| {
            bots::search_public_bots_request(extra, &query)
        })
    }

    /// `toggleHasSponsoredMessagesEnabled` ("Do Not Hide Ads").
    pub fn set_sponsored_enabled(&mut self, enabled: bool) -> Result<(), ConnectSendError> {
        self.session.settings.business.error = None;
        self.send_business_extra(SettingsPurpose::SetSponsoredEnabled, |extra| {
            crate::business_settings::toggle_sponsored_request(extra, enabled)
        })
    }
}
