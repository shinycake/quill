//! Telegram Business links to chat, the connected chatbot and its lookup,
//! and the limits TDLib reports as options (see `crate::business_bots`,
//! `crate::business_chat_links`, `crate::business_start_page`).
use crate::business_bots::ConnectedBot;
use crate::business_chat_links::{ChatLink, LINKS_MAX};
use crate::business_start_page::{MESSAGE_MAX, TITLE_MAX};
use crate::state::fetch::Fetch;
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

/// The latest answer to a link create or edit, with the request it answers.
/// The link box that sent the request waits for exactly that id
/// (`LinkEditDraft`), so an answer for a box that was closed matches
/// nothing and needs no reset: the next answer replaces it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ChatLinkAnswer {
    #[default]
    None,
    Saved(RequestId),
    /// Refused; the error line is in the Business screens' error.
    Refused(RequestId),
}

/// The bot lookup of the chatbots screen for one typed username.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BotLookup {
    pub generation: u32,
    pub query: String,
    /// `Ready(None)` when nothing matched; a refused search is `Failed`,
    /// not an empty result.
    pub exact: Fetch<Option<i64>>,
    pub mine: Fetch<Vec<i64>>,
    pub global: Fetch<Vec<i64>>,
}

impl BotLookup {
    /// Every search answered or failed.
    pub fn done(&self) -> bool {
        !self.exact.is_loading() && !self.mine.is_loading() && !self.global.is_loading()
    }

    /// The first refusal, when any search failed.
    pub fn error(&self) -> Option<&str> {
        self.exact
            .error()
            .or_else(|| self.mine.error())
            .or_else(|| self.global.error())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BusinessExtraState {
    /// `getBusinessChatLinks`, newest first. A failed load keeps the row
    /// list it replaced and draws a Retry.
    pub chat_links: Fetch<Vec<ChatLink>>,
    pub chat_link_answer: ChatLinkAnswer,
    /// `getBusinessConnectedBot`: ready with `None` when no bot is
    /// connected. A bot save shows its bot at once, loading under the
    /// save's request (`Fetch::written_ahead`): only that save's answer
    /// forgets it, so the next visit asks TDLib (an older save's answer, or
    /// a fetch started since, leaves the shown bot alone).
    pub connected_bot: Fetch<Option<ConnectedBot>>,
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

    pub(crate) fn apply_chat_links(&mut self, links: Vec<ChatLink>, request: Option<RequestId>) {
        self.settings
            .business_extra
            .chat_links
            .answered(request, links);
    }

    /// A created or edited link: an edit replaces its row, a new link goes
    /// first (tdesktop `prependRow`).
    pub(crate) fn apply_chat_link(&mut self, link: ChatLink, extra: Option<RequestId>) {
        let state = &mut self.settings.business_extra;
        if let Some(id) = extra {
            state.chat_link_answer = ChatLinkAnswer::Saved(id);
        }
        state.chat_links.insert_data_if_missing(Vec::new);
        let Some(list) = state.chat_links.data_mut() else {
            return;
        };
        match list.iter_mut().find(|l| l.link == link.link) {
            Some(row) => *row = link,
            None => list.insert(0, link),
        }
    }

    /// `getBusinessConnectedBot` answered: only the fetch that asked takes
    /// it (a save since then wrote the bot ahead and dropped that fetch).
    pub(crate) fn apply_connected_bot(
        &mut self,
        bot: Option<ConnectedBot>,
        pending: Option<&PendingRequest>,
    ) {
        self.settings
            .business_extra
            .connected_bot
            .answered(pending.map(|p| p.id), bot);
    }

    /// `ok` of a bot save: the bot was written ahead of the answer, which
    /// carries no data, so TDLib is asked what is connected on the next
    /// visit. (A chat link delete needs nothing: its row went at once and
    /// the answer confirms it.)
    pub(crate) fn apply_business_extra_ok(&mut self, pending: Option<&PendingRequest>) {
        if let Some(RequestPurpose::Settings(SettingsPurpose::SaveConnectedBot)) =
            pending.map(|p| p.purpose)
        {
            self.forget_written_bot(pending.map(|p| p.id));
        }
    }

    /// A bot save answered (`ok` or refused): the bot it wrote ahead is
    /// forgotten so the next visit asks TDLib, when it was the newest save
    /// (a newer one wrote the shown bot) and no fetch started since (its
    /// answer is the truth).
    fn forget_written_bot(&mut self, request: Option<RequestId>) {
        let extra = &mut self.settings.business_extra;
        let Some(request) = request else {
            return;
        };
        if extra.connected_bot.waits_for(request) {
            extra.connected_bot = Fetch::Idle;
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
            let found = match kind {
                ChatKind::Private { user_id } => Some(user_id.0),
                _ => None,
            };
            lookup.exact.answered(pending.map(|p| p.id), found);
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
            let id = pending.map(|p| p.id);
            match part {
                BotLookupPart::Mine => {
                    lookup.mine.answered(id, users);
                }
                BotLookupPart::Global => {
                    lookup.global.answered(id, users);
                }
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
        extra_id: Option<RequestId>,
    ) -> bool {
        let Some(RequestPurpose::Settings(purpose)) = purpose else {
            return false;
        };
        let reason = error_reason(err);
        let extra = &mut self.settings.business_extra;
        let (area, error) = match purpose {
            SettingsPurpose::GetChatLinks => {
                let error = format!("Couldn't load the links: {reason}");
                extra.chat_links.failed(extra_id, error.clone(), false);
                (BusinessArea::ChatLinks, error)
            }
            SettingsPurpose::MutateChatLink { op } => {
                if op != ChatLinkOp::Delete
                    && let Some(id) = extra_id
                {
                    extra.chat_link_answer = ChatLinkAnswer::Refused(id);
                }
                (
                    BusinessArea::ChatLinks,
                    format!("Couldn't save the link: {reason}"),
                )
            }
            // No connected bot answers "Not Found".
            SettingsPurpose::GetConnectedBot if err.code == 404 => {
                extra.connected_bot.answered(extra_id, None);
                return true;
            }
            SettingsPurpose::GetConnectedBot => {
                let error = format!("Couldn't load the chatbot: {reason}");
                if !extra.connected_bot.failed(extra_id, error.clone(), false) {
                    return true;
                }
                (BusinessArea::Chatbot, error)
            }
            SettingsPurpose::SaveConnectedBot => {
                // The save wrote its bot ahead of the answer: forget it, so
                // the next visit asks TDLib what is connected instead of
                // showing the refused bot as saved.
                self.forget_written_bot(extra_id);
                (
                    BusinessArea::Chatbot,
                    format!("Couldn't save the chatbot: {reason}"),
                )
            }
            SettingsPurpose::LookupBusinessBot { generation, part } => {
                let lookup = &mut extra.lookup;
                if lookup.generation == generation {
                    let message = format!("Couldn't search for bots: {reason}");
                    match part {
                        BotLookupPart::Exact => lookup.exact.failed(extra_id, message, true),
                        BotLookupPart::Mine => lookup.mine.failed(extra_id, message, true),
                        BotLookupPart::Global => lookup.global.failed(extra_id, message, true),
                    };
                }
                return true;
            }
            SettingsPurpose::SetSponsoredEnabled => (
                BusinessArea::Ads,
                format!("Couldn't change the ads setting: {reason}"),
            ),
            _ => return false,
        };
        self.settings.business.refuse(area, error);
        true
    }
}
