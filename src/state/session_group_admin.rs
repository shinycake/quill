//! B7: group and channel admin toggles — which controls the viewer may
//! use (mirrors tdesktop's `edit_peer_info_box.cpp` / `edit_peer_type_box.cpp`
//! gates), the cached toggle values, and the follow-up steps that chain
//! after a basic group upgrade or a history change.
use super::*;
use crate::telegram::envelope::{ChatAvailableReactions, ReactionType};

/// tdesktop's `forum_upgrade_participants_min` default (200): a group with
/// fewer members can't turn topics on. The server has the final say.
pub const FORUM_MIN_MEMBERS: i32 = 200;

/// Which supergroup toggle a rollback restores.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupToggle {
    JoinToSend,
    HistoryVisible,
    HiddenMembers,
}

/// A step that runs once the request it waits for succeeds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdminFollowup {
    /// The basic group `old` was upgraded: turn topics on in the new
    /// supergroup (tdesktop `saveForum` migrates first).
    EnableTopicsAfterUpgrade { old: ChatId, tabs: bool },
    /// The basic group `old` was upgraded: set the history visibility.
    HistoryAfterUpgrade { old: ChatId, visible: bool },
    /// The group's history is now visible: link it to `channel`
    /// (TDLib: `toggleSupergroupIsAllHistoryAvailable` must come first).
    LinkAfterHistory { channel: ChatId, group: ChatId },
}

/// Why the Topics control can't be switched on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TopicsLock {
    /// Fewer than `FORUM_MIN_MEMBERS` members.
    TooFewMembers,
    /// The group is a channel's discussion group.
    LinkedDiscussion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TopicsControl {
    pub enabled: bool,
    /// A basic group: it is upgraded to a supergroup first.
    pub needs_upgrade: bool,
    pub locked: Option<TopicsLock>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HistoryControl {
    /// New members see the earlier messages.
    pub visible: bool,
    /// A basic group: it is upgraded to a supergroup first.
    pub needs_upgrade: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscussionControl {
    /// A channel: link, change or unlink its discussion group.
    Channel { linked: Option<ChatId> },
    /// A discussion group: unlink it from its channel.
    Group { channel: ChatId },
}

/// The controls of the group settings dialog the viewer may use. Anything
/// `None` / false is not shown at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GroupAdminControls {
    pub topics: Option<TopicsControl>,
    pub history: Option<HistoryControl>,
    /// Discussion group: members must join before they can write.
    pub join_to_send: bool,
    pub hide_members: bool,
    pub protected_content: bool,
    pub discussion: Option<DiscussionControl>,
    pub reactions: bool,
    /// Offer the explicit "Upgrade to supergroup" action.
    pub upgrade: bool,
    /// Owner of a supergroup or channel with more than one username to
    /// manage (tdesktop's "Link order" list).
    pub usernames: bool,
    /// Administrator of a supergroup or channel: boosts list and link.
    pub boosts: bool,
}

impl GroupAdminControls {
    /// Nothing to show.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

impl Session {
    fn chat_member_total(&self, chat_id: ChatId) -> i32 {
        self.chats
            .get(&chat_id.0)
            .and_then(|chat| self.group_member_counts(chat))
            .map_or(0, |(members, _)| members)
    }

    /// `supergroup.join_to_send_messages` (false while unknown).
    pub fn chat_join_to_send(&self, chat_id: ChatId) -> bool {
        self.chat_supergroup(chat_id)
            .and_then(|id| self.groups.supergroup_join_to_send.get(&id).copied())
            .unwrap_or(false)
    }

    /// The channel's linked discussion group, or the discussion group's
    /// channel (`supergroupFullInfo.linked_chat_id`); `None` when unlinked
    /// or not fetched yet.
    pub fn chat_linked_chat(&self, chat_id: ChatId) -> Option<ChatId> {
        let id = self.chat_supergroup(chat_id)?;
        self.groups
            .supergroup_full_infos
            .get(&id)
            .map(|info| info.linked_chat_id)
            .filter(|linked| *linked != 0)
            .map(ChatId)
    }

    /// The cached allowed-reactions setting; `None` until the chat arrives
    /// with one.
    pub fn chat_available_reactions(&self, chat_id: ChatId) -> Option<&ChatAvailableReactions> {
        self.chats_state.chat_available_reactions.get(&chat_id.0)
    }

    /// The emoji reactions a picker can offer: the server's active list
    /// when known, tdesktop's default row otherwise.
    pub fn reaction_picker_emoji(&self) -> Vec<String> {
        if self.active_emoji_reactions.is_empty() {
            crate::telegram::envelope::DEFAULT_EMOJI_REACTIONS
                .iter()
                .map(|emoji| (*emoji).to_string())
                .collect()
        } else {
            self.active_emoji_reactions.clone()
        }
    }

    /// Whether `chat_id` has a group-admin request in flight (the dialog
    /// disables its rows meanwhile).
    pub fn group_admin_busy(&self, chat_id: ChatId) -> bool {
        [
            RequestPurpose::ToggleSupergroupIsForum,
            RequestPurpose::ToggleSupergroupIsAllHistoryAvailable,
            RequestPurpose::ToggleSupergroupJoinToSendMessages,
            RequestPurpose::ToggleSupergroupHasHiddenMembers,
            RequestPurpose::ToggleChatHasProtectedContent,
            RequestPurpose::SetChatAvailableReactions,
            RequestPurpose::SetChatDiscussionGroup,
            RequestPurpose::UpgradeBasicGroup,
            RequestPurpose::ToggleSupergroupUsername,
            RequestPurpose::ReorderSupergroupUsernames,
        ]
        .iter()
        .any(|purpose| self.requests.has_purpose_for_chat(*purpose, chat_id))
    }

    /// The controls of the group settings dialog for `chat_id`, following
    /// tdesktop's gates (`Controller::fillManageSection`,
    /// `ChannelData::canEditPreHistoryHidden`, the type box) and TDLib's
    /// documented rights. Deny by default: unknown rights hide a control.
    pub fn group_admin_controls(&self, chat_id: ChatId) -> GroupAdminControls {
        let Some(chat) = self.chats.get(&chat_id.0) else {
            return GroupAdminControls::default();
        };
        let owner = self.chat_is_owner(chat_id);
        let can_change_info = self.chat_can_change_info(chat_id);
        let mut controls = match chat.kind {
            ChatKind::BasicGroup { basic_group_id } => {
                // An upgraded group is deactivated: nothing to manage.
                if self.groups.basic_group_active.get(&basic_group_id) == Some(&false) {
                    return GroupAdminControls::default();
                }
                let too_few = self.chat_member_total(chat_id) < FORUM_MIN_MEMBERS;
                GroupAdminControls {
                    // tdesktop `canEditForum` / `canEditPreHistoryHidden`:
                    // the creator; saving migrates the group first.
                    topics: owner.then_some(TopicsControl {
                        enabled: false,
                        needs_upgrade: true,
                        locked: too_few.then_some(TopicsLock::TooFewMembers),
                    }),
                    history: owner.then_some(HistoryControl {
                        visible: false,
                        needs_upgrade: true,
                    }),
                    protected_content: owner,
                    reactions: owner || can_change_info,
                    upgrade: owner,
                    ..GroupAdminControls::default()
                }
            }
            ChatKind::Supergroup {
                is_channel: true, ..
            } => GroupAdminControls {
                protected_content: owner,
                reactions: can_change_info,
                discussion: can_change_info.then(|| DiscussionControl::Channel {
                    linked: self.chat_linked_chat(chat_id),
                }),
                ..GroupAdminControls::default()
            },
            ChatKind::Supergroup {
                supergroup_id,
                is_channel: false,
            } => {
                let broadcast = self
                    .groups
                    .supergroup_is_broadcast
                    .get(&supergroup_id)
                    .copied()
                    .unwrap_or(false);
                let full = self.groups.supergroup_full_infos.get(&supergroup_id);
                let linked = self.chat_linked_chat(chat_id);
                let is_forum = chat.is_forum_chat();
                let public = self
                    .groups
                    .supergroup_usernames
                    .get(&supergroup_id)
                    .is_some_and(|name| !name.is_empty());
                let can_restrict = self.chat_can_restrict_members(chat_id);
                let topics = (owner && !broadcast).then(|| TopicsControl {
                    enabled: is_forum,
                    needs_upgrade: false,
                    locked: if is_forum {
                        None
                    } else if linked.is_some() {
                        Some(TopicsLock::LinkedDiscussion)
                    } else if self.chat_member_total(chat_id) < FORUM_MIN_MEMBERS {
                        Some(TopicsLock::TooFewMembers)
                    } else {
                        None
                    },
                });
                // tdesktop hides the history row for public groups, groups
                // with a location, discussion groups and forums.
                let history = (!broadcast
                    && (owner || can_restrict)
                    && !public
                    && !is_forum
                    && linked.is_none())
                .then_some(full)
                .flatten()
                .map(|info| HistoryControl {
                    visible: info.admin.is_all_history_available,
                    needs_upgrade: false,
                });
                GroupAdminControls {
                    topics,
                    history,
                    join_to_send: !broadcast && linked.is_some() && can_restrict,
                    hide_members: full.is_some_and(|info| info.admin.can_hide_members),
                    protected_content: owner,
                    // TDLib: unlinking from the group side needs the pin
                    // right, which the cache doesn't track; owners only.
                    discussion: (owner && !broadcast)
                        .then_some(linked)
                        .flatten()
                        .map(|channel| DiscussionControl::Group { channel }),
                    reactions: can_change_info,
                    upgrade: false,
                    ..GroupAdminControls::default()
                }
            }
            _ => GroupAdminControls::default(),
        };
        controls.usernames = owner
            && self
                .chat_usernames(chat_id)
                .is_some_and(|lists| lists.is_manageable());
        controls.boosts = self.chat_can_view_boosts(chat_id);
        controls
    }

    /// Optimistically set a supergroup toggle; returns the previous value
    /// for the rollback.
    pub(crate) fn set_group_toggle(
        &mut self,
        supergroup_id: i64,
        toggle: GroupToggle,
        value: bool,
    ) -> Option<bool> {
        match toggle {
            GroupToggle::JoinToSend => self
                .groups
                .supergroup_join_to_send
                .insert(supergroup_id, value),
            GroupToggle::HistoryVisible => {
                let info = self.groups.supergroup_full_infos.get_mut(&supergroup_id)?;
                let previous = info.admin.is_all_history_available;
                info.admin.is_all_history_available = value;
                Some(previous)
            }
            GroupToggle::HiddenMembers => {
                let info = self.groups.supergroup_full_infos.get_mut(&supergroup_id)?;
                let previous = info.admin.has_hidden_members;
                info.admin.has_hidden_members = value;
                Some(previous)
            }
        }
    }

    /// Undo `set_group_toggle` after the server refused the change.
    pub(crate) fn restore_group_toggle(
        &mut self,
        supergroup_id: i64,
        toggle: GroupToggle,
        previous: Option<bool>,
    ) {
        match (toggle, previous) {
            (GroupToggle::JoinToSend, Some(flag)) => {
                self.groups
                    .supergroup_join_to_send
                    .insert(supergroup_id, flag);
            }
            (GroupToggle::JoinToSend, None) => {
                self.groups.supergroup_join_to_send.remove(&supergroup_id);
            }
            (GroupToggle::HistoryVisible, Some(flag)) => {
                if let Some(info) = self.groups.supergroup_full_infos.get_mut(&supergroup_id) {
                    info.admin.is_all_history_available = flag;
                }
            }
            (GroupToggle::HiddenMembers, Some(flag)) => {
                if let Some(info) = self.groups.supergroup_full_infos.get_mut(&supergroup_id) {
                    info.admin.has_hidden_members = flag;
                }
            }
            _ => {}
        }
    }

    /// Merge the admin flags of a `supergroupFullInfo` answer for a group
    /// whose full info the main apply path didn't store (it ignores
    /// unrelated purposes).
    pub(crate) fn merge_full_admin(
        &mut self,
        supergroup_id: i64,
        admin: crate::telegram::envelope::SupergroupFullAdmin,
    ) {
        if let Some(info) = self.groups.supergroup_full_infos.get_mut(&supergroup_id) {
            info.admin = admin;
        }
    }

    /// Queue a step that runs when request `request` succeeds.
    pub fn queue_admin_followup(&mut self, request: RequestId, followup: AdminFollowup) {
        self.groups.admin_followups.push((request, followup));
    }

    /// Take the step waiting for `request`, if any (success or not: a
    /// failed request must not leave it behind).
    pub fn take_admin_followup(&mut self, request: RequestId) -> Option<AdminFollowup> {
        let index = self
            .groups
            .admin_followups
            .iter()
            .position(|(id, _)| *id == request)?;
        Some(self.groups.admin_followups.remove(index).1)
    }

    /// The supergroup chat a basic group became (recorded when
    /// `upgradeBasicGroupChatToSupergroupChat` answers).
    pub fn upgraded_chat_for(&self, old: ChatId) -> Option<ChatId> {
        self.chats_state
            .chat_upgrades
            .iter()
            .find(|(from, _)| *from == old.0)
            .map(|(_, to)| ChatId(*to))
    }

    /// Drain the finished upgrades (the UI opens the new chat).
    pub fn take_chat_upgrades(&mut self) -> Vec<(ChatId, ChatId)> {
        std::mem::take(&mut self.chats_state.chat_upgrades)
            .into_iter()
            .map(|(from, to)| (ChatId(from), ChatId(to)))
            .collect()
    }

    /// Whether `reactions` allows the emoji `emoji`.
    pub fn reaction_allowed(setting: &ChatAvailableReactions, emoji: &str) -> bool {
        match setting {
            ChatAvailableReactions::All { .. } => true,
            ChatAvailableReactions::Some { reactions, .. } => reactions
                .iter()
                .any(|reaction| reaction.emoji_text() == Some(emoji)),
        }
    }

    /// The setting with `emoji` switched in or out of a "some" list. An
    /// "all" setting turns into the explicit list first.
    pub fn toggled_reaction(
        current: Option<&ChatAvailableReactions>,
        picker: &[String],
        emoji: &str,
    ) -> ChatAvailableReactions {
        let max = current.map_or(
            crate::telegram::envelope::DEFAULT_MAX_REACTION_COUNT,
            ChatAvailableReactions::max_reaction_count,
        );
        let mut reactions: Vec<ReactionType> = match current {
            Some(ChatAvailableReactions::Some { reactions, .. }) => reactions.clone(),
            Some(ChatAvailableReactions::All { .. }) => {
                picker.iter().map(ReactionType::emoji).collect()
            }
            None => Vec::new(),
        };
        let target = ReactionType::emoji(emoji);
        if let Some(index) = reactions.iter().position(|r| *r == target) {
            reactions.remove(index);
        } else {
            reactions.push(target);
        }
        ChatAvailableReactions::Some {
            reactions,
            max_reaction_count: max,
        }
    }
}
