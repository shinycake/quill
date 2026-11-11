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

/// The save of the link box, from the send to TDLib's answer. The box stays
/// open on its draft until it says `Saved`; a refusal goes back to `Idle`
/// with the error line, so the draft can be fixed and sent again.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ChatLinkSave {
    #[default]
    Idle,
    /// A create or edit is out; only its own answer ends it.
    Saving(RequestId),
    /// The answer arrived; the box goes back to the list and resets this.
    Saved,
}

/// One request of the bot lookup: not sent, out under its request,
/// answered, or refused. A refused leg is not an empty answer.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum LookupLeg<T> {
    #[default]
    Idle,
    Asking(RequestId),
    Found(T),
    Failed,
}

impl<T> LookupLeg<T> {
    fn waits_for(&self, request: Option<RequestId>) -> bool {
        matches!((self, request), (Self::Asking(asked), Some(id)) if *asked == id)
    }

    fn settled(&self) -> bool {
        matches!(self, Self::Found(_) | Self::Failed)
    }

    pub fn found(&self) -> Option<&T> {
        match self {
            Self::Found(value) => Some(value),
            _ => None,
        }
    }
}

/// The bot lookup of the chatbots screen for one typed username.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BotLookup {
    pub generation: u32,
    pub query: String,
    /// `Found(None)` when nothing matched the exact username.
    pub exact: LookupLeg<Option<i64>>,
    pub mine: LookupLeg<Vec<i64>>,
    pub global: LookupLeg<Vec<i64>>,
}

impl BotLookup {
    /// Every leg answered or refused.
    pub fn done(&self) -> bool {
        self.exact.settled() && self.mine.settled() && self.global.settled()
    }

    /// Some leg was refused: the results shown are incomplete.
    pub fn failed(&self) -> bool {
        [
            self.exact == LookupLeg::Failed,
            self.mine == LookupLeg::Failed,
            self.global == LookupLeg::Failed,
        ]
        .contains(&true)
    }

    /// The exact username's user, when it matched one.
    pub fn exact_user(&self) -> Option<i64> {
        self.exact.found().copied().flatten()
    }

    /// The account's own matching chats.
    pub fn mine_users(&self) -> &[i64] {
        self.mine.found().map_or(&[], Vec::as_slice)
    }

    /// The global results.
    pub fn global_users(&self) -> &[i64] {
        self.global.found().map_or(&[], Vec::as_slice)
    }

    /// Record that `part` went out as `request`.
    pub fn sent(&mut self, part: BotLookupPart, request: RequestId) {
        match part {
            BotLookupPart::Exact => self.exact = LookupLeg::Asking(request),
            BotLookupPart::Mine => self.mine = LookupLeg::Asking(request),
            BotLookupPart::Global => self.global = LookupLeg::Asking(request),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BusinessExtraState {
    /// `getBusinessChatLinks`, newest first. A failed load keeps the row
    /// list it replaced and draws a Retry.
    pub chat_links: Fetch<Vec<ChatLink>>,
    pub chat_link_save: ChatLinkSave,
    /// `getBusinessConnectedBot`: ready with `None` when no bot is
    /// connected.
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
        if let Some(id) = extra
            && state.chat_link_save == ChatLinkSave::Saving(id)
        {
            state.chat_link_save = ChatLinkSave::Saved;
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
            self.forget_written_bot();
        }
    }

    /// A bot save answered (`ok` or refused): the bot it wrote ahead is
    /// forgotten so the next visit asks TDLib, unless another save is still
    /// out (it wrote the shown bot) or a fetch started since (its answer is
    /// the truth).
    fn forget_written_bot(&mut self) {
        let other_save = self
            .requests
            .has_purpose(RequestPurpose::Settings(SettingsPurpose::SaveConnectedBot));
        let bot = &mut self.settings.business_extra.connected_bot;
        if !other_save && matches!(bot, Fetch::Ready(_)) {
            *bot = Fetch::Idle;
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
        if lookup.generation == generation && lookup.exact.waits_for(pending.map(|p| p.id)) {
            lookup.exact = LookupLeg::Found(match kind {
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
        let id = pending.map(|p| p.id);
        if lookup.generation == generation {
            match part {
                BotLookupPart::Mine if lookup.mine.waits_for(id) => {
                    lookup.mine = LookupLeg::Found(users);
                }
                BotLookupPart::Global if lookup.global.waits_for(id) => {
                    lookup.global = LookupLeg::Found(users);
                }
                _ => {}
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
                    && extra_id.is_some_and(|id| extra.chat_link_save == ChatLinkSave::Saving(id))
                {
                    extra.chat_link_save = ChatLinkSave::Idle;
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
                self.forget_written_bot();
                (
                    BusinessArea::Chatbot,
                    format!("Couldn't save the chatbot: {reason}"),
                )
            }
            SettingsPurpose::LookupBusinessBot { generation, part } => {
                let lookup = &mut extra.lookup;
                if lookup.generation == generation {
                    match part {
                        BotLookupPart::Exact if lookup.exact.waits_for(extra_id) => {
                            lookup.exact = LookupLeg::Failed;
                        }
                        BotLookupPart::Mine if lookup.mine.waits_for(extra_id) => {
                            lookup.mine = LookupLeg::Failed;
                        }
                        BotLookupPart::Global if lookup.global.waits_for(extra_id) => {
                            lookup.global = LookupLeg::Failed;
                        }
                        _ => {}
                    }
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
