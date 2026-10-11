//! Telegram Business links to chat, the connected chatbot and its lookup,
//! and the limits TDLib reports as options (see `crate::business_bots`,
//! `crate::business_chat_links`, `crate::business_start_page`).
use crate::business_bots::ConnectedBot;
use crate::business_chat_links::{ChatLink, LINKS_MAX};
use crate::business_start_page::{MESSAGE_MAX, TITLE_MAX};
use crate::state::*;

/// Which lookup request answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BotLookupPart {
    /// `searchPublicChat` for the exact username.
    Exact,
    /// `searchChatsOnServer`: the account's own chats.
    Mine,
    /// `searchPublicChats`: global results.
    Global,
}

/// A link-to-chat change in flight.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatLinkOp {
    Create,
    Edit,
    Delete,
}

/// The bot lookup of the chatbots screen for one typed username.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BotLookup {
    pub generation: u32,
    pub query: String,
    /// `None` while the exact lookup runs; `Some(None)` when nothing
    /// matched.
    pub exact: Option<Option<i64>>,
    pub mine: Option<Vec<i64>>,
    pub global: Option<Vec<i64>>,
}

impl BotLookup {
    pub fn done(&self) -> bool {
        self.exact.is_some() && self.mine.is_some() && self.global.is_some()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BusinessExtraState {
    /// `getBusinessChatLinks`, newest first; `None` until loaded.
    pub chat_links: Option<Vec<ChatLink>>,
    pub chat_links_requested: bool,
    /// Creates and edits in flight.
    pub chat_links_saving: u32,
    /// `getBusinessConnectedBot`: `None` until it answers, `Some(None)`
    /// when no bot is connected.
    pub connected_bot: Option<Option<ConnectedBot>>,
    pub connected_bot_requested: bool,
    pub lookup: BotLookup,
    /// TDLib options; tdesktop's defaults until reported.
    pub title_max: Option<usize>,
    pub message_max: Option<usize>,
    pub links_max: Option<usize>,
}

impl BusinessExtraState {
    pub fn start_page_title_max(&self) -> usize {
        self.title_max.unwrap_or(TITLE_MAX)
    }

    pub fn start_page_message_max(&self) -> usize {
        self.message_max.unwrap_or(MESSAGE_MAX)
    }

    pub fn chat_links_max(&self) -> usize {
        self.links_max.unwrap_or(LINKS_MAX)
    }
}

impl Session {
    /// `updateOption`: the start page and link limits.
    pub(crate) fn apply_business_option(&mut self, name: &str, value: &OptionValue) {
        let OptionValue::Integer(limit) = value else {
            return;
        };
        let limit = usize::try_from(*limit).ok().filter(|l| *l > 0);
        let state = &mut self.settings.business_extra;
        match name {
            "business_start_page_title_length_max" => state.title_max = limit,
            "business_start_page_message_length_max" => state.message_max = limit,
            "business_chat_link_count_max" => state.links_max = limit,
            _ => {}
        }
    }

    pub(crate) fn apply_chat_links(&mut self, links: Vec<ChatLink>) {
        self.settings.business_extra.chat_links = Some(links);
    }

    /// A created or edited link: an edit replaces its row, a new link goes
    /// first (tdesktop `prependRow`).
    pub(crate) fn apply_chat_link(&mut self, link: ChatLink) {
        let state = &mut self.settings.business_extra;
        state.chat_links_saving = state.chat_links_saving.saturating_sub(1);
        let list = state.chat_links.get_or_insert_with(Vec::new);
        match list.iter_mut().find(|l| l.link == link.link) {
            Some(row) => *row = link,
            None => list.insert(0, link),
        }
    }

    pub(crate) fn apply_connected_bot(&mut self, bot: Option<ConnectedBot>) {
        self.settings.business_extra.connected_bot = Some(bot);
    }

    /// `ok` of a chat link delete, a bot save or the ads switch: the lists
    /// are asked for again (the answers carry no data).
    pub(crate) fn apply_business_extra_ok(&mut self, pending: Option<&PendingRequest>) {
        let state = &mut self.settings.business_extra;
        match pending.map(|p| p.purpose) {
            Some(RequestPurpose::Settings(SettingsPurpose::MutateChatLink {
                op: ChatLinkOp::Delete,
            })) => state.chat_links_requested = false,
            Some(RequestPurpose::Settings(SettingsPurpose::SaveConnectedBot)) => {
                state.connected_bot_requested = false;
            }
            _ => {}
        }
    }

    /// `searchPublicChat` answered for the bot lookup. `true` when it
    /// was one.
    pub(crate) fn apply_business_bot_exact(
        &mut self,
        kind: &ChatKind,
        pending: Option<&PendingRequest>,
    ) -> bool {
        let Some(RequestPurpose::Settings(SettingsPurpose::LookupBusinessBot {
            generation,
            part: BotLookupPart::Exact,
        })) = pending.map(|p| p.purpose)
        else {
            return false;
        };
        let lookup = &mut self.settings.business_extra.lookup;
        if lookup.generation == generation {
            lookup.exact = Some(match kind {
                ChatKind::Private { user_id } => Some(user_id.0),
                _ => None,
            });
        }
        true
    }

    /// `searchChatsOnServer` / `searchPublicChats` answered for the bot
    /// lookup. `true` when it was one.
    pub(crate) fn apply_business_bot_search(
        &mut self,
        chat_ids: &[ChatId],
        pending: Option<&PendingRequest>,
    ) -> bool {
        let Some(RequestPurpose::Settings(SettingsPurpose::LookupBusinessBot { generation, part })) =
            pending.map(|p| p.purpose)
        else {
            return false;
        };
        let users: Vec<i64> = chat_ids
            .iter()
            .filter_map(|id| {
                self.private_chat_user_id(*id)
                    .or((id.0 > 0).then_some(id.0))
            })
            .collect();
        let lookup = &mut self.settings.business_extra.lookup;
        if lookup.generation == generation {
            match part {
                BotLookupPart::Mine => lookup.mine = Some(users),
                BotLookupPart::Global => lookup.global = Some(users),
                BotLookupPart::Exact => {}
            }
        }
        true
    }

    /// A failed links, bot or lookup request; `true` when it was one.
    pub(crate) fn apply_business_extra_error(
        &mut self,
        err: &TdError,
        purpose: Option<RequestPurpose>,
    ) -> bool {
        let Some(RequestPurpose::Settings(purpose)) = purpose else {
            return false;
        };
        let reason = error_reason(err);
        let extra = &mut self.settings.business_extra;
        let error = match purpose {
            SettingsPurpose::GetChatLinks => {
                extra.chat_links_requested = false;
                format!("Couldn't load the links: {reason}")
            }
            SettingsPurpose::MutateChatLink { op } => {
                if op != ChatLinkOp::Delete {
                    extra.chat_links_saving = extra.chat_links_saving.saturating_sub(1);
                }
                format!("Couldn't save the link: {reason}")
            }
            // No connected bot answers "Not Found".
            SettingsPurpose::GetConnectedBot if err.code == 404 => {
                extra.connected_bot = Some(None);
                return true;
            }
            SettingsPurpose::GetConnectedBot => {
                extra.connected_bot_requested = false;
                format!("Couldn't load the chatbot: {reason}")
            }
            SettingsPurpose::SaveConnectedBot => {
                extra.connected_bot_requested = false;
                format!("Couldn't save the chatbot: {reason}")
            }
            SettingsPurpose::LookupBusinessBot { generation, part } => {
                let lookup = &mut extra.lookup;
                if lookup.generation == generation {
                    match part {
                        BotLookupPart::Exact => lookup.exact = Some(None),
                        BotLookupPart::Mine => lookup.mine = Some(Vec::new()),
                        BotLookupPart::Global => lookup.global = Some(Vec::new()),
                    }
                }
                return true;
            }
            SettingsPurpose::SetSponsoredEnabled => {
                format!("Couldn't change the ads setting: {reason}")
            }
            _ => return false,
        };
        self.settings.business.error = Some(error);
        true
    }
}
