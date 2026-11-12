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

/// The `createCommunity` in flight and the picture its identity box chose
/// for the community it makes (uploaded once the community exists). One
/// create goes at a time, so a newer one replaces the stage; the answer,
/// the refusal of that request, or the account going away ends it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum CommunityCreate {
    #[default]
    Idle,
    Creating {
        request: RequestId,
        photo: Option<String>,
    },
}

impl CommunityCreate {
    /// The create answered: its picture, once, for the request that asked.
    pub fn answered(&mut self, answer: RequestId) -> Option<String> {
        match std::mem::take(self) {
            Self::Creating { request, photo } if request == answer => photo,
            other => {
                *self = other;
                None
            }
        }
    }

    /// The create was refused or lost: the picture waiting for it goes.
    pub fn failed(&mut self, request: Option<RequestId>) {
        if matches!(self, Self::Creating { request: asked, .. } if Some(*asked) == request) {
            *self = Self::Idle;
        }
    }
}

pub struct CommunitiesState {
    /// `getJoinedCommunities`.
    pub joined: Fetch<Vec<i64>>,
    /// `getOwnedBots`: the user ids of the bots the person owns.
    pub owned_bots: Fetch<Vec<i64>>,
    /// See [`CommunityCreate`].
    pub create: CommunityCreate,
    /// `getCommunityAddChatRequests`, by community id.
    pub add_chat_requests: HashMap<i64, Fetch<AddChatRequestList>>,
    /// Add-chat requests approved or declined, by community, since the last
    /// first page was asked: a page that was in flight meanwhile may still
    /// list them, and must not bring them back.
    pub handled_add_requests: HashMap<i64, HashSet<i64>>,
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
            create: CommunityCreate::Idle,
            add_chat_requests: HashMap::new(),
            handled_add_requests: HashMap::new(),
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
