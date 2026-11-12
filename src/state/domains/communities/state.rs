//! The communities domain's share of `Session`: what the TDLib 1.8.70
//! community methods answered. The `community` objects and their full
//! info stay in `GroupsState` (`communities`, `community_full_infos`).
use crate::state::fetch::Fetch;
use crate::state::*;
use crate::telegram::envelope::{
    ChatNotificationSettings, CommunityAddChatRequest, CommunityMemberChats, OptionValue,
};
use std::collections::{HashSet, VecDeque};

/// The loaded add-chat requests of one community.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AddChatRequestList {
    pub requests: Vec<CommunityAddChatRequest>,
    /// `total_count` of the last page.
    pub total_count: i32,
    /// The offset of the next page; `None` once the list is complete.
    pub next_offset: Option<String>,
    /// Chats approved or declined while a page of this list was out: that
    /// page may still list them and must not bring them back. It rides
    /// with the list the page was asked for and goes when the page is
    /// asked (`begin_add_requests_page`) or answers.
    pub handled: HashSet<i64>,
}

/// Why a community request failed, as the screens tell it apart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommunityError {
    /// `COMMUNITY_PEERS_TOO_MUCH`: the community holds the most chats
    /// (or bots) it can.
    PeersTooMuch,
    /// Any other refusal, as one line.
    Other(String),
}

/// A finished community request, waiting for the UI to tell the person
/// (tdesktop shows a toast and closes the boxes that asked). Each one
/// names what was asked, so the UI matches it to its own box.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommunityOutcome {
    ChatAdded {
        community_id: i64,
        chat_id: i64,
    },
    /// The person can't change the list: a request went to the admins.
    AddRequestSent {
        community_id: i64,
        chat_id: i64,
    },
    AddFailed {
        community_id: i64,
        chat_id: i64,
        error: CommunityError,
    },
    ChatRemoved {
        community_id: i64,
        chat_id: i64,
    },
    RemoveFailed {
        community_id: i64,
        chat_id: i64,
        reason: String,
    },
    RequestProcessed {
        community_id: i64,
        chat_id: i64,
        approve: bool,
    },
    AllRequestsProcessed {
        community_id: i64,
        approve: bool,
        count: i32,
    },
    ProcessFailed {
        community_id: i64,
        error: CommunityError,
    },
    MemberBanned {
        community_id: i64,
        member: CommunityMember,
    },
    BanFailed {
        community_id: i64,
        reason: String,
    },
    CollapseFailed {
        community_id: i64,
        reason: String,
    },
    NotificationsFailed {
        community_id: i64,
        reason: String,
    },
    /// `createCommunity` answered with the new community, made around
    /// `chat_id`.
    Created {
        /// The `createCommunity` request this answers.
        request: RequestId,
        community_id: i64,
        chat_id: Option<i64>,
    },
    CreateFailed {
        /// The `createCommunity` request that was refused.
        request: RequestId,
        reason: String,
    },
}

pub struct CommunitiesState {
    /// `getJoinedCommunities`.
    pub joined: Fetch<Vec<i64>>,
    /// `getOwnedBots`: the user ids of the bots the person owns.
    pub owned_bots: Fetch<Vec<i64>>,
    /// The picture for the community our `createCommunity` (the request
    /// id) is making; one create goes at a time, so a newer one replaces
    /// it.
    pub new_community_photo: Option<(RequestId, String)>,
    /// `getCommunityAddChatRequests`, by community id.
    pub add_chat_requests: HashMap<i64, Fetch<AddChatRequestList>>,
    /// The community requests out (`send_communities_request`), each by its
    /// own request with its community: what the screens read as in flight.
    pub sent: crate::state::held::Holds<(i64, CommunitiesPurpose)>,
    /// `getCommunityMemberChats`, by community and member.
    pub member_chats: HashMap<(i64, CommunityMember), Fetch<CommunityMemberChats>>,
    /// The settings each in-flight `setCommunityNotificationSettings`
    /// sent; on `ok` they become the community's.
    pub notification_sends: HashMap<RequestId, (i64, ChatNotificationSettings)>,
    /// Finished requests the UI has not told about yet.
    pub outcomes: VecDeque<CommunityOutcome>,
    /// TDLib options `community_chat_count_max` and
    /// `community_bot_count_max`, for the limit toast.
    pub chat_count_max: i64,
    pub bot_count_max: i64,
}

/// TDLib's default for both community limits, until `updateOption` sends
/// one.
pub const DEFAULT_COMMUNITY_PEERS_MAX: i64 = 100;

impl CommunitiesState {
    pub(crate) fn new() -> Self {
        Self {
            joined: Fetch::Idle,
            owned_bots: Fetch::Idle,
            new_community_photo: None,
            add_chat_requests: HashMap::new(),
            sent: Default::default(),
            member_chats: HashMap::new(),
            notification_sends: HashMap::new(),
            outcomes: VecDeque::new(),
            chat_count_max: DEFAULT_COMMUNITY_PEERS_MAX,
            bot_count_max: DEFAULT_COMMUNITY_PEERS_MAX,
        }
    }

    pub(crate) fn apply_option(&mut self, name: &str, value: &OptionValue) {
        let OptionValue::Integer(number) = *value else {
            return;
        };
        match name {
            "community_chat_count_max" => self.chat_count_max = number,
            "community_bot_count_max" => self.bot_count_max = number,
            _ => {}
        }
    }

    /// The account comes back on screen: what finished for the screen it
    /// left is not reported on the new one.
    pub fn forget_outcomes(&mut self) {
        self.outcomes.clear();
    }

    /// The next finished request for the UI to report.
    pub fn take_outcome(&mut self) -> Option<CommunityOutcome> {
        self.outcomes.pop_front()
    }

    /// A page of the add-chat requests goes out as `request`: what was
    /// handled so far is behind it.
    pub fn begin_add_requests_page(&mut self, community_id: i64, request: RequestId) {
        let fetch = self.add_chat_requests.entry(community_id).or_default();
        if let Some(list) = fetch.data_mut() {
            list.handled.clear();
        }
        fetch.begin_with(request);
    }

    /// A request of the community was approved or declined: a page in
    /// flight must not bring it back.
    pub fn mark_add_request_handled(&mut self, community_id: i64, chat_id: i64) {
        if let Some(fetch) = self.add_chat_requests.get_mut(&community_id)
            && fetch.is_loading()
            && let Some(list) = fetch.data_mut()
        {
            list.handled.insert(chat_id);
        }
    }

    pub fn add_chat_requests(&self, community_id: i64) -> &Fetch<AddChatRequestList> {
        const IDLE: &Fetch<AddChatRequestList> = &Fetch::Idle;
        self.add_chat_requests.get(&community_id).unwrap_or(IDLE)
    }

    pub fn member_chats(
        &self,
        community_id: i64,
        member: CommunityMember,
    ) -> &Fetch<CommunityMemberChats> {
        const IDLE: &Fetch<CommunityMemberChats> = &Fetch::Idle;
        self.member_chats
            .get(&(community_id, member))
            .unwrap_or(IDLE)
    }
}
