use super::super::*;

/// Slice G1: member-management dialog tabs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemberTab {
    All,
    Administrators,
    Restricted,
    Banned,
}

impl MemberTab {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::All => "Members",
            Self::Administrators => "Admins",
            Self::Restricted => "Restricted",
            Self::Banned => "Banned",
        }
    }

    pub(crate) fn filter(self) -> Option<MemberListFilter> {
        match self {
            Self::All => None,
            Self::Administrators => Some(MemberListFilter::Administrators),
            Self::Restricted => Some(MemberListFilter::Restricted),
            Self::Banned => Some(MemberListFilter::Banned),
        }
    }
}

/// Slice G1: member-management dialog for a basic group or
/// supergroup. `All` browses Recent/Search pages (basic groups read
/// their full member list from `getBasicGroupFullInfo`); the other
/// tabs read the matching `getSupergroupMembers` filter. The add
/// section at the bottom picks contacts to add via `addChatMember`
/// (basic groups) / `addChatMembers` (supergroups).
pub struct MemberDialog {
    pub(crate) chat_id: ChatId,
    pub(crate) is_basic_group: bool,
    pub(crate) tab: MemberTab,
    pub(crate) search_input: Entity<TextareaState>,
    pub(crate) add_open: bool,
    pub(crate) add_search: Entity<TextareaState>,
    pub(crate) add_selected: Vec<i64>,
}

impl MemberDialog {
    pub(crate) fn new(
        window: &mut Window,
        cx: &mut Context<QuillApp>,
        chat_id: ChatId,
        is_basic_group: bool,
    ) -> Self {
        let search_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Search members")
                .auto_grow(1, 1)
                .submit_on_enter(false)
        });
        let add_search = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Search contacts to add")
                .auto_grow(1, 1)
                .submit_on_enter(false)
        });
        Self {
            chat_id,
            is_basic_group,
            tab: MemberTab::All,
            search_input,
            add_open: false,
            add_search,
            add_selected: Vec::new(),
        }
    }
}
