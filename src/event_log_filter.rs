//! The Recent actions filter box, as tdesktop's
//! `history/admin_log/history_admin_log_filter.cpp` lays it out: an "All
//! actions" box over per-category boxes in three sections, and "All admins"
//! over one box per administrator. Pure, so the choices are unit tested.
//!
//! TDLib has no "nothing" filter (an empty set means every type), so a draft
//! with no category or no administrator ticked cannot be saved.

use crate::telegram::requests::ChatEventLogFilterSet;

/// One tick box of the filter, in tdesktop's wording and order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterCategory {
    AdminRights,
    TagChanges,
    Restrictions,
    NewMembers,
    MembersLeaving,
    Info,
    InviteLinks,
    VideoChats,
    SubscriptionRenewals,
    Topics,
    Deleted,
    Edited,
    Pinned,
}

/// The three sections of the box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterSection {
    Members,
    Settings,
    Messages,
}

impl FilterCategory {
    /// The sections with their categories; a channel has no topics and no
    /// pinned-messages box.
    pub fn sections(is_channel: bool) -> Vec<(FilterSection, Vec<FilterCategory>)> {
        use FilterCategory::*;
        let mut settings = vec![Info, InviteLinks, VideoChats, SubscriptionRenewals];
        let mut messages = vec![Deleted, Edited];
        if !is_channel {
            settings.push(Topics);
            messages.push(Pinned);
        }
        vec![
            (
                FilterSection::Members,
                vec![
                    AdminRights,
                    TagChanges,
                    Restrictions,
                    NewMembers,
                    MembersLeaving,
                ],
            ),
            (FilterSection::Settings, settings),
            (FilterSection::Messages, messages),
        ]
    }

    fn all(is_channel: bool) -> Vec<FilterCategory> {
        Self::sections(is_channel)
            .into_iter()
            .flat_map(|(_, list)| list)
            .collect()
    }

    pub fn get(self, set: &ChatEventLogFilterSet) -> bool {
        match self {
            Self::AdminRights => set.member_promotions,
            Self::TagChanges => set.member_tag_changes,
            Self::Restrictions => set.member_restrictions,
            Self::NewMembers => set.member_joins && set.member_invites,
            Self::MembersLeaving => set.member_leaves,
            Self::Info => set.info_changes && set.setting_changes,
            Self::InviteLinks => set.invite_link_changes,
            Self::VideoChats => set.video_chat_changes,
            Self::SubscriptionRenewals => set.subscription_extensions,
            Self::Topics => set.forum_changes,
            Self::Deleted => set.message_deletions,
            Self::Edited => set.message_edits,
            Self::Pinned => set.message_pins,
        }
    }

    pub fn set(self, set: &mut ChatEventLogFilterSet, on: bool) {
        match self {
            Self::AdminRights => set.member_promotions = on,
            Self::TagChanges => set.member_tag_changes = on,
            Self::Restrictions => set.member_restrictions = on,
            Self::NewMembers => {
                set.member_joins = on;
                set.member_invites = on;
            }
            Self::MembersLeaving => set.member_leaves = on,
            Self::Info => {
                set.info_changes = on;
                set.setting_changes = on;
            }
            Self::InviteLinks => set.invite_link_changes = on,
            Self::VideoChats => set.video_chat_changes = on,
            Self::SubscriptionRenewals => set.subscription_extensions = on,
            Self::Topics => set.forum_changes = on,
            Self::Deleted => set.message_deletions = on,
            Self::Edited => set.message_edits = on,
            Self::Pinned => set.message_pins = on,
        }
    }
}

/// What the open box shows before Save.
#[derive(Debug, Clone, PartialEq)]
pub struct FilterDraft {
    ticked: ChatEventLogFilterSet,
    admins: Vec<i64>,
    is_channel: bool,
}

/// What Save stores: no filter at all is `None` / empty.
#[derive(Debug, Clone, PartialEq)]
pub struct FilterChoice {
    pub categories: Option<ChatEventLogFilterSet>,
    pub admins: Vec<i64>,
}

impl FilterDraft {
    /// Start from what the chat filters by now: no stored set means every
    /// category, no stored administrators means all of them.
    pub fn open(
        stored: Option<ChatEventLogFilterSet>,
        stored_admins: &[i64],
        all_admins: &[i64],
        is_channel: bool,
    ) -> Self {
        let mut ticked = ChatEventLogFilterSet::default();
        match stored.filter(|set| set.any_enabled()) {
            Some(set) => ticked = set,
            None => {
                for category in FilterCategory::all(is_channel) {
                    category.set(&mut ticked, true);
                }
            }
        }
        let admins = if stored_admins.is_empty() {
            all_admins.to_vec()
        } else {
            all_admins
                .iter()
                .copied()
                .filter(|id| stored_admins.contains(id))
                .collect()
        };
        Self {
            ticked,
            admins,
            is_channel,
        }
    }

    pub fn category(&self, category: FilterCategory) -> bool {
        category.get(&self.ticked)
    }

    pub fn toggle_category(&mut self, category: FilterCategory, on: bool) {
        category.set(&mut self.ticked, on);
    }

    pub fn all_actions(&self) -> bool {
        FilterCategory::all(self.is_channel)
            .into_iter()
            .all(|category| category.get(&self.ticked))
    }

    pub fn set_all_actions(&mut self, on: bool) {
        for category in FilterCategory::all(self.is_channel) {
            category.set(&mut self.ticked, on);
        }
    }

    pub fn admin(&self, user_id: i64) -> bool {
        self.admins.contains(&user_id)
    }

    pub fn toggle_admin(&mut self, user_id: i64, on: bool) {
        self.admins.retain(|id| *id != user_id);
        if on {
            self.admins.push(user_id);
        }
    }

    pub fn all_admins(&self, all_admins: &[i64]) -> bool {
        all_admins.iter().all(|id| self.admins.contains(id))
    }

    pub fn set_all_admins(&mut self, on: bool, all_admins: &[i64]) {
        self.admins = if on { all_admins.to_vec() } else { Vec::new() };
    }

    /// Save needs something ticked on both lists.
    pub fn can_save(&self) -> bool {
        self.ticked.any_enabled() && !self.admins.is_empty()
    }

    pub fn choice(&self, all_admins: &[i64]) -> FilterChoice {
        let categories = if self.all_actions() {
            None
        } else {
            let mut set = self.ticked;
            if self.is_channel {
                set.forum_changes = false;
                set.message_pins = false;
            }
            Some(set)
        };
        let admins = if self.all_admins(all_admins) {
            Vec::new()
        } else {
            self.admins.clone()
        };
        FilterChoice { categories, admins }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ADMINS: [i64; 3] = [1, 2, 3];

    #[test]
    fn opens_with_everything_ticked_when_nothing_filters() {
        let draft = FilterDraft::open(None, &[], &ADMINS, false);
        assert!(draft.all_actions());
        assert!(draft.all_admins(&ADMINS));
        assert_eq!(
            draft.choice(&ADMINS),
            FilterChoice {
                categories: None,
                admins: vec![]
            }
        );
    }

    #[test]
    fn a_stored_filter_opens_as_it_was() {
        let stored = ChatEventLogFilterSet {
            message_edits: true,
            ..Default::default()
        };
        let draft = FilterDraft::open(Some(stored), &[2], &ADMINS, false);
        assert!(draft.category(FilterCategory::Edited));
        assert!(!draft.category(FilterCategory::Deleted));
        assert!(!draft.all_actions());
        assert!(draft.admin(2) && !draft.admin(1));
    }

    #[test]
    fn all_actions_toggles_every_box() {
        let mut draft = FilterDraft::open(None, &[], &ADMINS, false);
        draft.set_all_actions(false);
        assert!(!draft.can_save());
        draft.toggle_category(FilterCategory::Pinned, true);
        assert!(draft.can_save());
        let set = draft.choice(&ADMINS).categories.unwrap_or_default();
        assert!(set.message_pins && !set.message_edits);
    }

    #[test]
    fn grouped_categories_flip_both_fields() {
        let mut draft = FilterDraft::open(None, &[], &ADMINS, false);
        draft.set_all_actions(false);
        draft.toggle_category(FilterCategory::NewMembers, true);
        draft.toggle_category(FilterCategory::Info, true);
        let set = draft.choice(&ADMINS).categories.unwrap_or_default();
        assert!(set.member_joins && set.member_invites);
        assert!(set.info_changes && set.setting_changes);
    }

    #[test]
    fn channels_have_no_topics_or_pins() {
        let sections = FilterCategory::sections(true);
        let all: Vec<_> = sections.into_iter().flat_map(|(_, list)| list).collect();
        assert!(!all.contains(&FilterCategory::Topics));
        assert!(!all.contains(&FilterCategory::Pinned));
        let draft = FilterDraft::open(None, &[], &ADMINS, true);
        assert!(draft.all_actions());
    }

    #[test]
    fn admin_selection_needs_one_and_collapses_when_all() {
        let mut draft = FilterDraft::open(None, &[], &ADMINS, false);
        draft.toggle_admin(2, false);
        assert!(!draft.all_admins(&ADMINS));
        assert_eq!(draft.choice(&ADMINS).admins, vec![1, 3]);
        draft.set_all_admins(false, &ADMINS);
        assert!(!draft.can_save());
        draft.set_all_admins(true, &ADMINS);
        assert!(draft.choice(&ADMINS).admins.is_empty());
    }
}
